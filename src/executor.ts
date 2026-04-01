import {
  getSignatureFromTransaction,
  type Blockhash,
  type SendableTransaction,
  type Transaction,
} from "@solana/kit";
import type { TransactionWithLastValidBlockHeight } from "@solana/transaction-confirmation";

import { assertTransactionSize } from "./program";

export type TransactionExecution = {
  signature: string;
  serializedLength: number;
};

type SignedTransaction = SendableTransaction & Transaction & {
  lifetimeConstraint: unknown;
};

export type TransactionExecutor = {
  getLatestBlockhash(): Promise<Readonly<{
    blockhash: Blockhash;
    lastValidBlockHeight: bigint;
  }>>;
  execute(transaction: SignedTransaction): Promise<TransactionExecution>;
};

export function createRpcExecutor(
  params: {
    getLatestBlockhash(): Promise<Readonly<{
      blockhash: Blockhash;
      lastValidBlockHeight: bigint;
    }>>;
    sendAndConfirmTransaction(
      transaction: SendableTransaction & Transaction & TransactionWithLastValidBlockHeight,
      config: { commitment: "confirmed" },
    ): Promise<unknown>;
  },
): TransactionExecutor {
  return {
    getLatestBlockhash: params.getLatestBlockhash,
    async execute(transaction) {
      const serializedLength = assertTransactionSize(transaction);
      await params.sendAndConfirmTransaction(
        toTransactionWithLastValidBlockHeight(transaction),
        {
          commitment: "confirmed",
        },
      );
      const signature = getSignatureFromTransaction(transaction);
      return { signature, serializedLength };
    },
  };
}

function toTransactionWithLastValidBlockHeight(
  transaction: SignedTransaction,
): SendableTransaction & Transaction & TransactionWithLastValidBlockHeight {
  const lifetimeConstraint = transaction.lifetimeConstraint;
  if (
    !lifetimeConstraint ||
    typeof lifetimeConstraint !== "object" ||
    !("lastValidBlockHeight" in lifetimeConstraint)
  ) {
    throw new Error("transaction is missing lastValidBlockHeight");
  }

  const { lastValidBlockHeight } = lifetimeConstraint;
  if (typeof lastValidBlockHeight !== "bigint") {
    throw new Error("transaction lastValidBlockHeight is invalid");
  }
  return {
    ...transaction,
    lifetimeConstraint: {
      lastValidBlockHeight,
    },
  };
}
