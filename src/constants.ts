import { address } from "@solana/kit";

export const EXECUTE_DISCRIMINATOR = 1;
export const WITHDRAW_DISCRIMINATOR = 2;
export const SYSTEM_TRANSFER_DISCRIMINATOR = 3;
export const TOKEN_TRANSFER_DISCRIMINATOR = 4;
export const EXECUTE_HEADER_LEN = 5;
export const MAX_TRANSACTION_SIZE = 4096;

export const DISCORD_SECRET_KEY_BYTES = new Uint8Array([
  243, 215, 14, 197, 86, 78, 251, 95, 181, 36, 168, 42, 69, 13, 213, 31, 29, 76,
  213, 24, 173, 16, 52, 24, 254, 35, 182, 130, 136, 167, 71, 65, 98, 198, 251,
  212, 221, 147, 204, 209, 242, 154, 244, 58, 38, 97, 65, 123, 209, 247, 77, 25,
  153, 241, 68, 65, 203, 18, 79, 150, 43, 91, 74, 76,
]);

export const DEFAULT_DISCORD_PUBLIC_KEY = address(
  "7eawKgepAhdzLrVgTwsn9zoH3EGipCzF4HBxajusY5QF",
);
export const ED25519_PROGRAM_ID = address(
  "Ed25519SigVerify111111111111111111111111111",
);

const TEXT_ENCODER = new TextEncoder();

export const WALLET_SEED = TEXT_ENCODER.encode("wallet");
export const VAULT_SEED = TEXT_ENCODER.encode("vault");
