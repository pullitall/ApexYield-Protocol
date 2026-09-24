import {
  Connection,
  PublicKey,
  Transaction,
  SystemProgram,
  Keypair,
  sendAndConfirmTransaction,
  clusterApiUrl
} from '@solana/web3.js';
import { BigNumber } from 'bignumber.js';

export interface SolanaPayPaymentRequest {
  recipient: string;
  amount: number;
  splToken?: string;
  reference?: string;
  label?: string;
  message?: string;
  memo?: string;
}

export interface VoiceConfirmationResult {
  approved: boolean;
  transcript: string;
  confidence: number;
  timestamp: number;
}

export interface SettlementReceipt {
  success: boolean;
  signature?: string;
  explorerUrl?: string;
  amount: number;
  recipient: string;
  settlementTimeMs: number;
  error?: string;
}

/**
 * ApexGlass: Wearable & AI Smart Glasses Solana Pay Settlement Engine
 * Enables hands-free optical QR detection and voice-confirmed micro-settlements
 * for Omi, Meta Ray-Ban, and Android-based smart wearables.
 */
export class ApexGlassPayEngine {
  private connection: Connection;
  private network: 'devnet' | 'mainnet-beta';

  constructor(network: 'devnet' | 'mainnet-beta' = 'devnet', rpcUrl?: string) {
    this.network = network;
    this.connection = new Connection(rpcUrl || clusterApiUrl(network), 'confirmed');
  }

  /**
   * Parse optical OCR / QR code text captured by smart glasses camera.
   * Matches standard Solana Pay specification (solana:<recipient>?amount=...&label=...)
   */
  public parseSolanaPayUri(uri: string): SolanaPayPaymentRequest {
    if (!uri.startsWith('solana:')) {
      throw new Error('Invalid URI: Must start with solana: protocol scheme');
    }

    const clean = uri.replace('solana:', '');
    const [recipientStr, queryStr] = clean.split('?');

    if (!recipientStr) {
      throw new Error('Missing recipient public key in Solana Pay URI');
    }

    // Validate Solana public key
    new PublicKey(recipientStr);

    const params = new URLSearchParams(queryStr || '');
    const amountStr = params.get('amount');
    const amount = amountStr ? parseFloat(amountStr) : 0;
    const splToken = params.get('spl-token') || undefined;
    const reference = params.get('reference') || undefined;
    const label = params.get('label') || undefined;
    const message = params.get('message') || undefined;
    const memo = params.get('memo') || undefined;

    return {
      recipient: recipientStr,
      amount,
      splToken,
      reference,
      label,
      message,
      memo
    };
  }

  /**
   * Format audio feedback string for the smart glasses text-to-speech (TTS) speaker.
   */
  public generateAudioPrompt(request: SolanaPayPaymentRequest): string {
    const merchant = request.label || 'Merchant';
    const currency = request.splToken ? 'USDC' : 'SOL';
    return `Detected payment to ${merchant} for ${request.amount} ${currency}. Say 'Approve' to confirm or 'Cancel' to reject.`;
  }

  /**
   * Evaluate voice confirmation captured by glasses microphone.
   * Recognizes affirmative phrases: 'approve', 'confirm', 'pay now', 'yes', 'send it'.
   */
  public evaluateVoiceConfirmation(transcript: string, confidence: number = 0.9): VoiceConfirmationResult {
    const normalized = transcript.trim().toLowerCase();
    const approvedWords = ['approve', 'confirm', 'pay', 'pay now', 'yes', 'send it', 'proceed'];
    const rejectedWords = ['cancel', 'stop', 'reject', 'no', 'abort'];

    const isApproved = approvedWords.some(w => normalized.includes(w));
    const isRejected = rejectedWords.some(w => normalized.includes(w));

    return {
      approved: isApproved && !isRejected,
      transcript,
      confidence,
      timestamp: Date.now()
    };
  }

  /**
   * Execute immediate on-chain settlement on Solana Devnet/Mainnet.
   * Reaches finality in < 400ms.
   */
  public async executeSettlement(
    request: SolanaPayPaymentRequest,
    payerKeypair: Keypair
  ): Promise<SettlementReceipt> {
    const startTime = Date.now();
    try {
      const recipientPubkey = new PublicKey(request.recipient);
      const lamports = Math.round(request.amount * 1_000_000_000);

      const transaction = new Transaction();

      // Add transfer instruction
      transaction.add(
        SystemProgram.transfer({
          fromPubkey: payerKeypair.publicKey,
          toPubkey: recipientPubkey,
          lamports
        })
      );

      // Add optional reference key for merchant POS indexing
      if (request.reference) {
        const refPubkey = new PublicKey(request.reference);
        transaction.instructions[0].keys.push({
          pubkey: refPubkey,
          isWritable: false,
          isSigner: false
        });
      }

      const signature = await sendAndConfirmTransaction(
        this.connection,
        transaction,
        [payerKeypair],
        { commitment: 'confirmed' }
      );

      const elapsed = Date.now() - startTime;
      const explorer = `https://explorer.solana.com/tx/${signature}?cluster=${this.network}`;

      return {
        success: true,
        signature,
        explorerUrl: explorer,
        amount: request.amount,
        recipient: request.recipient,
        settlementTimeMs: elapsed
      };
    } catch (err: any) {
      return {
        success: false,
        amount: request.amount,
        recipient: request.recipient,
        settlementTimeMs: Date.now() - startTime,
        error: err.message || String(err)
      };
    }
  }
}
