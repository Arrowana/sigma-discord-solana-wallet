import {
  getSignatureFromTransaction,
  type Blockhash,
  type SendableTransaction,
  type Signature,
  type Transaction,
} from "@solana/kit";

import { sleep } from "./bytes";
import { assertTransactionSize } from "./program";

export type TransactionExecution = {
  signature: string;
  serializedLength: number;
};

type SignedTransaction = SendableTransaction & Transaction;

export type TransactionExecutor = {
  getLatestBlockhash(): Promise<Readonly<{
    blockhash: Blockhash;
    lastValidBlockHeight: bigint;
  }>>;
  execute(transaction: SignedTransaction): Promise<TransactionExecution>;
};

type SignatureStatus = Readonly<{
  confirmationStatus: "processed" | "confirmed" | "finalized" | null;
  confirmations: bigint | null;
  err: unknown | null;
}> | null;

export function createRpcExecutor(
  params: {
    getLatestBlockhash(): Promise<Readonly<{
      blockhash: Blockhash;
      lastValidBlockHeight: bigint;
    }>>;
    sendAndConfirmTransaction(
      transaction: SignedTransaction,
      config: { commitment: "confirmed" },
    ): Promise<unknown>;
  },
): TransactionExecutor {
  return {
    getLatestBlockhash: params.getLatestBlockhash,
    async execute(transaction) {
      const serializedLength = assertTransactionSize(transaction);
      await params.sendAndConfirmTransaction(transaction, {
        commitment: "confirmed",
      });
      const signature = getSignatureFromTransaction(transaction);
      return { signature, serializedLength };
    },
  };
}

export function createPollingRpcExecutor(
  params: {
    getLatestBlockhash(): Promise<Readonly<{
      blockhash: Blockhash;
      lastValidBlockHeight: bigint;
    }>>;
    sendTransaction(
      transaction: SignedTransaction,
      config: { commitment: "confirmed" },
    ): Promise<void>;
    getSignatureStatus(signature: Signature): Promise<SignatureStatus>;
    pollIntervalMs?: number;
    timeoutMs?: number;
  },
): TransactionExecutor {
  return {
    getLatestBlockhash: params.getLatestBlockhash,
    async execute(transaction) {
      const serializedLength = assertTransactionSize(transaction);
      await params.sendTransaction(transaction, {
        commitment: "confirmed",
      });
      const signature = getSignatureFromTransaction(transaction);
      await waitForConfirmation({
        signature,
        getSignatureStatus: params.getSignatureStatus,
        pollIntervalMs: params.pollIntervalMs ?? 100,
        timeoutMs: params.timeoutMs ?? 30_000,
      });
      return { signature, serializedLength };
    },
  };
}

async function waitForConfirmation(params: {
  signature: Signature;
  getSignatureStatus(signature: Signature): Promise<SignatureStatus>;
  pollIntervalMs: number;
  timeoutMs: number;
}) {
  const deadline = Date.now() + params.timeoutMs;
  while (Date.now() <= deadline) {
    const status = await params.getSignatureStatus(params.signature);
    if (status) {
      if (status.err) {
        throw new Error(
          `transaction ${params.signature} failed: ${stringifyStatusError(status.err)}`,
        );
      }
      if (
        status.confirmationStatus === "confirmed" ||
        status.confirmationStatus === "finalized" ||
        status.confirmations === null
      ) {
        return;
      }
    }
    await sleep(params.pollIntervalMs);
  }

  throw new Error(
    `transaction ${params.signature} was not confirmed within ${params.timeoutMs}ms`,
  );
}

function stringifyStatusError(error: unknown) {
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}
