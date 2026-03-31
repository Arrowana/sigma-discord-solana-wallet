import nacl from "tweetnacl";
import { getAddressEncoder, type Address } from "@solana/kit";

import { DEFAULT_DISCORD_PUBLIC_KEY, DISCORD_SECRET_KEY_BYTES } from "./constants";

const ADDRESS_ENCODER = getAddressEncoder();

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

export function signDiscordRequest(
  timestamp: string,
  rawBody: string,
  secretKey: Uint8Array = DISCORD_SECRET_KEY_BYTES,
): string {
  const message = new TextEncoder().encode(timestamp + rawBody);
  const signature = nacl.sign.detached(message, secretKey);
  return Buffer.from(signature).toString("hex");
}

export function verifyDiscordRequest(
  timestamp: string | null,
  signatureHex: string | null,
  rawBody: string,
  publicKey: Address = DEFAULT_DISCORD_PUBLIC_KEY,
): boolean {
  if (!timestamp || !signatureHex) {
    return false;
  }
  const message = new TextEncoder().encode(timestamp + rawBody);
  const signature = new Uint8Array(Buffer.from(signatureHex, "hex"));
  return nacl.sign.detached.verify(
    message,
    signature,
    new Uint8Array(ADDRESS_ENCODER.encode(publicKey)),
  );
}

export function discordHeaders(
  timestamp: string,
  rawBody: string,
  secretKey: Uint8Array = DISCORD_SECRET_KEY_BYTES,
): Headers {
  const headers = new Headers();
  headers.set("content-type", "application/json");
  headers.set("x-signature-timestamp", timestamp);
  headers.set("x-signature-ed25519", signDiscordRequest(timestamp, rawBody, secretKey));
  return headers;
}
