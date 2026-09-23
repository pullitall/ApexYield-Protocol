use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("ApexYieLd11111111111111111111111111111111111");

pub const PROTOCOL_CONFIG_SEED: &[u8] = b"protocol_config";
pub const BOUNTY_ESCROW_SEED: &[u8] = b"bounty_escrow";
pub const BOUNTY_VAULT_SEED: &[u8] = b"bounty_vault";
pub const MAX_FEE_BPS: u16 = 1000; // Max 10.0% fee cap

#[program]
pub mod apexyield_core {
    use super::*;

    /// Initialize global protocol state and set authorized attestation oracle
    pub fn initialize_protocol(
        ctx: Context<InitializeProtocol>,
        fee_basis_points: u16,
    ) -> Result<()> {
        require!(fee_basis_points <= MAX_FEE_BPS, ApexYieldError::FeeTooHigh);

        let config = &mut ctx.accounts.protocol_config;
        config.admin = ctx.accounts.admin.key();
        config.oracle_authority = ctx.accounts.oracle_authority.key();
        config.treasury_token_account = ctx.accounts.treasury_token_account.key();
        config.fee_basis_points = fee_basis_points;
        config.total_bounties_settled = 0;
        config.bump = ctx.bumps.protocol_config;

        msg!("ApexYield Protocol Initialized. Oracle: {:?}", config.oracle_authority);
        Ok(())
    }

    /// Deposit USDG/USDC into a Program-Derived Address (PDA) escrow vault for a GitHub Issue
    pub fn initialize_bounty(
        ctx: Context<InitializeBounty>,
        issue_id: u64,
        amount: u64,
        timeout_seconds: i64,
    ) -> Result<()> {
        require!(amount > 0, ApexYieldError::ZeroAmount);
        require!(timeout_seconds >= 86400, ApexYieldError::TimeoutTooShort); // Min 24h

        let escrow = &mut ctx.accounts.bounty_escrow;
        escrow.maintainer = ctx.accounts.maintainer.key();
        escrow.mint = ctx.accounts.mint.key();
        escrow.vault = ctx.accounts.vault.key();
        escrow.issue_id = issue_id;
        escrow.amount = amount;
        escrow.created_at = Clock::get()?.unix_timestamp;
        escrow.timeout_seconds = timeout_seconds;
        escrow.status = BountyStatus::Open;
        escrow.contributor = Pubkey::default();
        escrow.bump = ctx.bumps.bounty_escrow;
        escrow.vault_bump = ctx.bumps.vault;

        // Transfer tokens from maintainer to PDA vault
        let cpi_accounts = Transfer {
            from: ctx.accounts.maintainer_token.to_account_info(),
            to: ctx.accounts.vault.to_account_info(),
            authority: ctx.accounts.maintainer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        emit!(BountyCreated {
            issue_id,
            maintainer: escrow.maintainer,
            mint: escrow.mint,
            amount,
            timeout_seconds,
        });

        msg!("ApexYield Bounty Created for GitHub Issue #{}! Amount: {}", issue_id, amount);
        Ok(())
    }

    /// Assign an autonomous AI agent or developer contributor to the bounty
    pub fn assign_contributor(
        ctx: Context<AssignContributor>,
        issue_id: u64,
        contributor: Pubkey,
    ) -> Result<()> {
        let escrow = &mut ctx.accounts.bounty_escrow;
        require!(escrow.status == BountyStatus::Open, ApexYieldError::BountyNotOpen);

        escrow.contributor = contributor;
        escrow.status = BountyStatus::Assigned;

        emit!(BountyAssigned {
            issue_id,
            contributor,
        });

        msg!("ApexYield Issue #{} assigned to contributor {:?}", issue_id, contributor);
        Ok(())
    }

    /// Settle bounty in < 400ms when CI tests pass and Attestation Oracle signs Ed25519 payload
    pub fn settle_bounty_with_attestation(
        ctx: Context<SettleBountyWithAttestation>,
        issue_id: u64,
        commit_sha: [u8; 32],
        _oracle_signature: [u8; 64],
    ) -> Result<()> {
        let escrow = &mut ctx.accounts.bounty_escrow;
        let config = &mut ctx.accounts.protocol_config;

        require!(
            escrow.status == BountyStatus::Open || escrow.status == BountyStatus::Assigned,
            ApexYieldError::InvalidBountyStatus
        );

        // Calculate fee split: Default 2.5% protocol fee, 97.5% to contributor
        let total_amount = escrow.amount;
        let protocol_fee = (total_amount as u128)
            .checked_mul(config.fee_basis_points as u128)
            .ok_or(ApexYieldError::MathOverflow)?
            .checked_div(10000)
            .ok_or(ApexYieldError::MathOverflow)? as u64;

        let contributor_payout = total_amount
            .checked_sub(protocol_fee)
            .ok_or(ApexYieldError::MathOverflow)?;

        let issue_id_bytes = issue_id.to_le_bytes();
        let seeds = &[
            BOUNTY_ESCROW_SEED,
            issue_id_bytes.as_ref(),
            &[escrow.bump],
        ];
        let signer_seeds = &[&seeds[..]];

        // 1. Payout to contributor (97.5%)
        let transfer_to_contributor = Transfer {
            from: ctx.accounts.vault.to_account_info(),
            to: ctx.accounts.contributor_token.to_account_info(),
            authority: escrow.to_account_info(),
        };
        let cpi_contributor_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer_to_contributor,
            signer_seeds,
        );
        token::transfer(cpi_contributor_ctx, contributor_payout)?;

        // 2. Transfer protocol fee (2.5%) to treasury
        if protocol_fee > 0 {
            let transfer_to_treasury = Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.treasury_token.to_account_info(),
                authority: escrow.to_account_info(),
            };
            let cpi_treasury_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                transfer_to_treasury,
                signer_seeds,
            );
            token::transfer(cpi_treasury_ctx, protocol_fee)?;
        }

        escrow.status = BountyStatus::Settled;
        config.total_bounties_settled = config
            .total_bounties_settled
            .checked_add(1)
            .unwrap_or(config.total_bounties_settled);

        emit!(BountySettled {
            issue_id,
            contributor: ctx.accounts.contributor_token.owner,
            payout: contributor_payout,
            protocol_fee,
            commit_sha,
        });

        msg!("🎉 ApexYield Settlement Complete! Contributor Payout: {} | Fee: {}", contributor_payout, protocol_fee);
        Ok(())
    }

    /// Refund bounty to maintainer if timeout expiration period has passed
    pub fn refund_bounty(ctx: Context<RefundBounty>, issue_id: u64) -> Result<()> {
        let escrow = &mut ctx.accounts.bounty_escrow;
        let now = Clock::get()?.unix_timestamp;

        require!(
            escrow.status == BountyStatus::Open || escrow.status == BountyStatus::Assigned,
            ApexYieldError::InvalidBountyStatus
        );
        require!(
            now >= escrow.created_at + escrow.timeout_seconds,
            ApexYieldError::TimeoutNotReached
        );

        let refund_amount = escrow.amount;
        let issue_id_bytes = issue_id.to_le_bytes();
        let seeds = &[
            BOUNTY_ESCROW_SEED,
            issue_id_bytes.as_ref(),
            &[escrow.bump],
        ];
        let signer_seeds = &[&seeds[..]];

        let transfer_refund = Transfer {
            from: ctx.accounts.vault.to_account_info(),
            to: ctx.accounts.maintainer_token.to_account_info(),
            authority: escrow.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer_refund,
            signer_seeds,
        );
        token::transfer(cpi_ctx, refund_amount)?;

        escrow.status = BountyStatus::Refunded;

        emit!(BountyRefunded {
            issue_id,
            maintainer: escrow.maintainer,
            refund_amount,
        });

        msg!("ApexYield Bounty #{} refunded to maintainer.", issue_id);
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION STRUCTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeProtocol<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + ProtocolConfig::LEN,
        seeds = [PROTOCOL_CONFIG_SEED],
        bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// CHECK: Authorized Oracle that signs Ed25519 CI test attestations
    pub oracle_authority: UncheckedAccount<'info>,

    #[account(
        constraint = treasury_token_account.owner == admin.key() @ ApexYieldError::InvalidTreasuryOwner
    )]
    pub treasury_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub admin: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(issue_id: u64)]
pub struct InitializeBounty<'info> {
    #[account(
        init,
        payer = maintainer,
        space = 8 + BountyEscrow::LEN,
        seeds = [BOUNTY_ESCROW_SEED, issue_id.to_le_bytes().as_ref()],
        bump
    )]
    pub bounty_escrow: Account<'info, BountyEscrow>,

    #[account(
        init,
        payer = maintainer,
        token::mint = mint,
        token::authority = bounty_escrow,
        seeds = [BOUNTY_VAULT_SEED, bounty_escrow.key().as_ref()],
        bump
    )]
    pub vault: Account<'info, TokenAccount>,

    pub mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = maintainer_token.owner == maintainer.key() @ ApexYieldError::UnauthorizedTokenAccount,
        constraint = maintainer_token.mint == mint.key() @ ApexYieldError::InvalidMint
    )]
    pub maintainer_token: Account<'info, TokenAccount>,

    #[account(mut)]
    pub maintainer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(issue_id: u64)]
pub struct AssignContributor<'info> {
    #[account(
        mut,
        seeds = [BOUNTY_ESCROW_SEED, issue_id.to_le_bytes().as_ref()],
        bump = bounty_escrow.bump,
        has_one = maintainer @ ApexYieldError::UnauthorizedMaintainer
    )]
    pub bounty_escrow: Account<'info, BountyEscrow>,

    pub maintainer: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(issue_id: u64)]
pub struct SettleBountyWithAttestation<'info> {
    #[account(
        mut,
        seeds = [BOUNTY_ESCROW_SEED, issue_id.to_le_bytes().as_ref()],
        bump = bounty_escrow.bump,
        has_one = vault @ ApexYieldError::InvalidVault
    )]
    pub bounty_escrow: Account<'info, BountyEscrow>,

    #[account(
        mut,
        seeds = [BOUNTY_VAULT_SEED, bounty_escrow.key().as_ref()],
        bump = bounty_escrow.vault_bump
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        seeds = [PROTOCOL_CONFIG_SEED],
        bump = protocol_config.bump,
        has_one = oracle_authority @ ApexYieldError::UnauthorizedOracle
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(mut)]
    pub contributor_token: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = treasury_token.key() == protocol_config.treasury_token_account @ ApexYieldError::InvalidTreasury
    )]
    pub treasury_token: Account<'info, TokenAccount>,

    pub oracle_authority: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(issue_id: u64)]
pub struct RefundBounty<'info> {
    #[account(
        mut,
        seeds = [BOUNTY_ESCROW_SEED, issue_id.to_le_bytes().as_ref()],
        bump = bounty_escrow.bump,
        has_one = maintainer @ ApexYieldError::UnauthorizedMaintainer,
        has_one = vault @ ApexYieldError::InvalidVault
    )]
    pub bounty_escrow: Account<'info, BountyEscrow>,

    #[account(
        mut,
        seeds = [BOUNTY_VAULT_SEED, bounty_escrow.key().as_ref()],
        bump = bounty_escrow.vault_bump
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = maintainer_token.owner == maintainer.key() @ ApexYieldError::UnauthorizedTokenAccount
    )]
    pub maintainer_token: Account<'info, TokenAccount>,

    pub maintainer: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

// -----------------------------------------------------------------------------
// STATE ACCOUNTS & ENUMS
// -----------------------------------------------------------------------------

#[account]
pub struct ProtocolConfig {
    pub admin: Pubkey,                  // 32
    pub oracle_authority: Pubkey,       // 32
    pub treasury_token_account: Pubkey, // 32
    pub fee_basis_points: u16,          // 2 (250 = 2.5%)
    pub total_bounties_settled: u64,    // 8
    pub bump: u8,                       // 1
}

impl ProtocolConfig {
    pub const LEN: usize = 32 + 32 + 32 + 2 + 8 + 1;
}

#[account]
pub struct BountyEscrow {
    pub maintainer: Pubkey,         // 32
    pub mint: Pubkey,               // 32 (USDG or USDC)
    pub vault: Pubkey,              // 32 (PDA Token Account)
    pub contributor: Pubkey,        // 32 (Assigned developer/agent)
    pub issue_id: u64,              // 8
    pub amount: u64,                // 8
    pub created_at: i64,            // 8
    pub timeout_seconds: i64,       // 8
    pub status: BountyStatus,       // 1
    pub bump: u8,                   // 1
    pub vault_bump: u8,             // 1
}

impl BountyEscrow {
    pub const LEN: usize = 32 + 32 + 32 + 32 + 8 + 8 + 8 + 8 + 1 + 1 + 1;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum BountyStatus {
    Open = 0,
    Assigned = 1,
    Settled = 2,
    Refunded = 3,
}

// -----------------------------------------------------------------------------
// EVENTS
// -----------------------------------------------------------------------------

#[event]
pub struct BountyCreated {
    pub issue_id: u64,
    pub maintainer: Pubkey,
    pub mint: Pubkey,
    pub amount: u64,
    pub timeout_seconds: i64,
}

#[event]
pub struct BountyAssigned {
    pub issue_id: u64,
    pub contributor: Pubkey,
}

#[event]
pub struct BountySettled {
    pub issue_id: u64,
    pub contributor: Pubkey,
    pub payout: u64,
    pub protocol_fee: u64,
    pub commit_sha: [u8; 32],
}

#[event]
pub struct BountyRefunded {
    pub issue_id: u64,
    pub maintainer: Pubkey,
    pub refund_amount: u64,
}

// -----------------------------------------------------------------------------
// ERROR CODES
// -----------------------------------------------------------------------------

#[error_code]
pub enum ApexYieldError {
    #[msg("Protocol fee exceeds maximum allowable cap (1000 bps)")]
    FeeTooHigh,
    #[msg("Bounty amount must be greater than zero")]
    ZeroAmount,
    #[msg("Timeout must be at least 24 hours")]
    TimeoutTooShort,
    #[msg("Bounty is not open for assignment or settlement")]
    BountyNotOpen,
    #[msg("Bounty status does not permit this operation")]
    InvalidBountyStatus,
    #[msg("Math calculation overflow")]
    MathOverflow,
    #[msg("Timeout duration has not elapsed yet")]
    TimeoutNotReached,
    #[msg("Unauthorized signer attempting maintainer action")]
    UnauthorizedMaintainer,
    #[msg("Oracle authority signature verification failed")]
    UnauthorizedOracle,
    #[msg("Invalid vault PDA provided")]
    InvalidVault,
    #[msg("Invalid token mint")]
    InvalidMint,
    #[msg("Provided token account does not match protocol treasury")]
    InvalidTreasury,
    #[msg("Treasury token account owner must match admin")]
    InvalidTreasuryOwner,
    #[msg("Provided token account does not belong to expected owner")]
    UnauthorizedTokenAccount,
}
