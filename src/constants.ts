import { address } from "@solana/kit";

export const EXECUTE_DISCRIMINATOR = 1;
export const WITHDRAW_DISCRIMINATOR = 2;
export const SYSTEM_TRANSFER_DISCRIMINATOR = 3;
export const TOKEN_TRANSFER_DISCRIMINATOR = 4;
export const EXECUTE_HEADER_LEN = 5;
export const MAX_TRANSACTION_SIZE = 4096;
export const ED25519_PROGRAM_ID = address(
  "Ed25519SigVerify111111111111111111111111111",
);

const TEXT_ENCODER = new TextEncoder();

export const WALLET_SEED = TEXT_ENCODER.encode("wallet");
export const VAULT_SEED = TEXT_ENCODER.encode("vault");
