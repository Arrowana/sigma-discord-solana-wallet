import {
  createKeyPairFromBytes,
  getPublicKeyFromAddress,
  signBytes,
  signatureBytes,
  type Address,
  verifySignature,
} from "@solana/kit";

import { bytesToHex, hexToBytes, utf8Bytes } from "./bytes";
import { DEFAULT_DISCORD_PUBLIC_KEY, DISCORD_SECRET_KEY_BYTES } from "./constants";

let discordSigningKeyPairPromise: Promise<CryptoKeyPair> | undefined;
const verificationKeyCache = new Map<Address, Promise<CryptoKey>>();

export type DiscordInteraction =
  | {
      id: string;
      type: 1;
    }
  | {
      id: string;
      type: 2;
      guild_id: string;
      member: {
        user: { id: string; username?: string };
      };
      data: {
        name: "wallet" | "wallet_init" | "set_withdrawer" | "transfer";
        options?: Array<{ name: string; value: string | number }>;
      };
    };

export async function signDiscordRequest(
  timestamp: string,
  rawBody: string,
  secretKey: Uint8Array = DISCORD_SECRET_KEY_BYTES,
): Promise<string> {
  const message = utf8Bytes(timestamp + rawBody);
  const signature = await signBytes(
    await getDiscordPrivateKey(secretKey),
    message,
  );
  return bytesToHex(signature);
}

export async function verifyDiscordRequest(
  timestamp: string | null,
  signatureHex: string | null,
  rawBody: string,
  publicKey: Address = DEFAULT_DISCORD_PUBLIC_KEY,
): Promise<boolean> {
  if (!timestamp || !signatureHex) {
    return false;
  }
  const message = utf8Bytes(timestamp + rawBody);
  const signature = signatureBytes(hexToBytes(signatureHex));
  return verifySignature(
    await getVerificationKey(publicKey),
    signature,
    message,
  );
}

export async function discordHeaders(
  timestamp: string,
  rawBody: string,
  secretKey: Uint8Array = DISCORD_SECRET_KEY_BYTES,
): Promise<Headers> {
  const headers = new Headers();
  headers.set("content-type", "application/json");
  headers.set("x-signature-timestamp", timestamp);
  headers.set(
    "x-signature-ed25519",
    await signDiscordRequest(timestamp, rawBody, secretKey),
  );
  return headers;
}

async function getDiscordPrivateKey(secretKey: Uint8Array): Promise<CryptoKey> {
  if (secretKey === DISCORD_SECRET_KEY_BYTES) {
    discordSigningKeyPairPromise ??= createKeyPairFromBytes(secretKey);
    const { privateKey } = await discordSigningKeyPairPromise;
    return privateKey;
  }
  const { privateKey } = await createKeyPairFromBytes(secretKey);
  return privateKey;
}

function getVerificationKey(publicKey: Address): Promise<CryptoKey> {
  let cached = verificationKeyCache.get(publicKey);
  if (!cached) {
    cached = getPublicKeyFromAddress(publicKey);
    verificationKeyCache.set(publicKey, cached);
  }
  return cached;
}
