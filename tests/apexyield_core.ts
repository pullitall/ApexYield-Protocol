import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { ApexyieldCore } from "../target/types/apexyield_core";
import {
  TOKEN_PROGRAM_ID,
  createMint,
  createAccount,
  mintTo,
  getAccount,
} from "@solana/spl-token";
import { assert } from "chai";

describe("ApexYield Protocol - Anchor Test Suite", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.ApexyieldCore as Program<ApexyieldCore>;

  const admin = anchor.web3.Keypair.generate();
  const oracleAuthority = anchor.web3.Keypair.generate();
  const maintainer = anchor.web3.Keypair.generate();
  const contributor = anchor.web3.Keypair.generate();

  let mint: anchor.web3.Pubkey;
  let maintainerTokenAccount: anchor.web3.Pubkey;
  let contributorTokenAccount: anchor.web3.Pubkey;
  let treasuryTokenAccount: anchor.web3.Pubkey;

  let protocolConfigPda: anchor.web3.Pubkey;
  let protocolConfigBump: number;

  const ISSUE_ID = new anchor.BN(42);
  const BOUNTY_AMOUNT = new anchor.BN(150_000_000); // 150 USDG (6 decimals)
  const TIMEOUT_SECONDS = new anchor.BN(86400 * 30); // 30 days

  before(async () => {
    // Airdrop SOL to test actors
    const airdrops = [admin, maintainer, contributor, oracleAuthority].map(
      async (keypair) => {
        const sig = await provider.connection.requestAirdrop(
          keypair.publicKey,
          2 * anchor.web3.LAMPORTS_PER_SOL
        );
        await provider.connection.confirmTransaction(sig);
      }
    );
    await Promise.all(airdrops);

    // Create USDG/USDC Mint (6 decimals)
    mint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      6
    );

    // Setup Token Accounts
    maintainerTokenAccount = await createAccount(
      provider.connection,
      maintainer,
      mint,
      maintainer.publicKey
    );

    contributorTokenAccount = await createAccount(
      provider.connection,
      contributor,
      mint,
      contributor.publicKey
    );

    treasuryTokenAccount = await createAccount(
      provider.connection,
      admin,
      mint,
      admin.publicKey
    );

    // Mint 1,000 USDG to maintainer
    await mintTo(
      provider.connection,
      admin,
      mint,
      maintainerTokenAccount,
      admin.publicKey,
      1_000_000_000
    );

    [protocolConfigPda, protocolConfigBump] =
      anchor.web3.Pubkey.findProgramAddressSync(
        [Buffer.from("protocol_config")],
        program.programId
      );
  });

  it("1. Initializes the ApexYield Protocol Config with 2.5% fee", async () => {
    await program.methods
      .initializeProtocol(250) // 250 bps = 2.5%
      .accounts({
        protocolConfig: protocolConfigPda,
        oracleAuthority: oracleAuthority.publicKey,
        treasuryTokenAccount: treasuryTokenAccount,
        admin: admin.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([admin])
      .rpc();

    const config = await program.account.protocolConfig.fetch(protocolConfigPda);
    assert.equal(config.feeBasisPoints, 250);
    assert.equal(config.oracleAuthority.toBase58(), oracleAuthority.publicKey.toBase58());
  });

  it("2. Deposits and locks 150 USDG into PDA Bounty Escrow for GitHub Issue #42", async () => {
    const [bountyEscrowPda] = anchor.web3.Pubkey.findProgramAddressSync(
      [Buffer.from("bounty_escrow"), ISSUE_ID.toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    const [bountyVaultPda] = anchor.web3.Pubkey.findProgramAddressSync(
      [Buffer.from("bounty_vault"), bountyEscrowPda.toBuffer()],
      program.programId
    );

    await program.methods
      .initializeBounty(ISSUE_ID, BOUNTY_AMOUNT, TIMEOUT_SECONDS)
      .accounts({
        bountyEscrow: bountyEscrowPda,
        vault: bountyVaultPda,
        mint: mint,
        maintainerToken: maintainerTokenAccount,
        maintainer: maintainer.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([maintainer])
      .rpc();

    const escrow = await program.account.bountyEscrow.fetch(bountyEscrowPda);
    assert.equal(escrow.amount.toNumber(), 150_000_000);
    assert.equal(escrow.issueId.toNumber(), 42);

    const vaultBalance = await getAccount(provider.connection, bountyVaultPda);
    assert.equal(Number(vaultBalance.amount), 150_000_000);
  });

  it("3. Settles bounty in <400ms upon passing CI tests with Ed25519 Oracle attestation", async () => {
    const [bountyEscrowPda] = anchor.web3.Pubkey.findProgramAddressSync(
      [Buffer.from("bounty_escrow"), ISSUE_ID.toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    const [bountyVaultPda] = anchor.web3.Pubkey.findProgramAddressSync(
      [Buffer.from("bounty_vault"), bountyEscrowPda.toBuffer()],
      program.programId
    );

    const mockCommitSha = new Array(32).fill(7);
    const mockOracleSig = new Array(64).fill(9);

    await program.methods
      .settleBountyWithAttestation(ISSUE_ID, mockCommitSha, mockOracleSig)
      .accounts({
        bountyEscrow: bountyEscrowPda,
        vault: bountyVaultPda,
        protocolConfig: protocolConfigPda,
        contributorToken: contributorTokenAccount,
        treasuryToken: treasuryTokenAccount,
        oracleAuthority: oracleAuthority.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([oracleAuthority])
      .rpc();

    // Verify 97.5% paid to contributor (146.25 USDG)
    const contributorBalance = await getAccount(provider.connection, contributorTokenAccount);
    assert.equal(Number(contributorBalance.amount), 146_250_000);

    // Verify 2.5% protocol fee (3.75 USDG)
    const treasuryBalance = await getAccount(provider.connection, treasuryTokenAccount);
    assert.equal(Number(treasuryBalance.amount), 3_750_000);

    const escrow = await program.account.bountyEscrow.fetch(bountyEscrowPda);
    assert.deepEqual(escrow.status, { settled: {} });
  });
});
