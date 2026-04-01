import {
  address,
  createKeyPairFromBytes,
  getBase16Decoder,
  getBase16Encoder,
  getPublicKeyFromAddress,
  signBytes,
  signatureBytes,
  type Address,
  verifySignature,
} from "@solana/kit";

import { utf8Bytes } from "./bytes";

const HEX_ENCODER = getBase16Decoder();
const HEX_DECODER = getBase16Encoder();
const signingKeyCache = new Map<string, Promise<CryptoKeyPair>>();
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
        name: "wallet" | "airdrop" | "wallet_init" | "set_withdrawer" | "transfer";
        options?: Array<{ name: string; value: string | number }>;
      };
    };

export async function signDiscordRequest(
  timestamp: string,
  rawBody: string,
  secretKey: Uint8Array,
): Promise<string> {
  const message = utf8Bytes(timestamp + rawBody);
  const signature = await signBytes(
    await getDiscordPrivateKey(secretKey),
    message,
  );
  return HEX_ENCODER.decode(signature);
}

export async function verifyDiscordRequest(
  timestamp: string | null,
  signatureHex: string | null,
  rawBody: string,
  publicKey: Address,
): Promise<boolean> {
  if (!timestamp || !signatureHex) {
    return false;
  }
  const message = utf8Bytes(timestamp + rawBody);
  const signature = signatureBytes(HEX_DECODER.encode(signatureHex));
  return verifySignature(
    await getVerificationKey(publicKey),
    signature,
    message,
  );
}

export async function discordHeaders(
  timestamp: string,
  rawBody: string,
  secretKey: Uint8Array,
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
  const cacheKey = HEX_ENCODER.decode(secretKey);
  let cached = signingKeyCache.get(cacheKey);
  if (!cached) {
    cached = createKeyPairFromBytes(secretKey);
    signingKeyCache.set(cacheKey, cached);
  }
  const { privateKey } = await cached;
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

export function requiredDiscordPublicKey(
  source: Record<string, string | undefined>,
): Address {
  const value = source.DISCORD_PUBLIC_KEY;
  if (!value) {
    throw new Error("DISCORD_PUBLIC_KEY is required");
  }
  return address(value);
}
