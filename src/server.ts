import {
  signTransactionMessageWithSigners,
  type Address,
  type KeyPairSigner,
} from "@solana/kit";

import { utf8ByteLength } from "./bytes";
import { DEFAULT_DISCORD_PUBLIC_KEY } from "./constants";
import type { DiscordInteraction } from "./discord";
import { verifyDiscordRequest } from "./discord";
import type { TransactionExecutor } from "./executor";
import { buildDiscordCommandTransaction } from "./program";

export type BotConfig = {
  executor: TransactionExecutor;
  relayer: KeyPairSigner;
  programId: Address;
  discordPublicKey?: Address;
  port?: number;
  walletStateExists?(discordUserId: string): Promise<boolean>;
  walletSummary?(discordUserId: string): Promise<string>;
};

export function createBotHandler(config: BotConfig) {
  return async function handleRequest(request: Request): Promise<Response> {
    if (new URL(request.url).pathname !== "/interactions") {
      return new Response("not found", { status: 404 });
    }
    if (request.method !== "POST") {
      return new Response("method not allowed", { status: 405 });
    }

    const rawBody = await request.text();
    const timestamp = request.headers.get("x-signature-timestamp");
    const signature = request.headers.get("x-signature-ed25519");
    const discordPublicKey = config.discordPublicKey ?? DEFAULT_DISCORD_PUBLIC_KEY;
    if (!(await verifyDiscordRequest(timestamp, signature, rawBody, discordPublicKey))) {
      return new Response("invalid signature", { status: 401 });
    }

    let interaction: DiscordInteraction;
    try {
      interaction = JSON.parse(rawBody);
    } catch (error) {
      return responseMessage(`invalid json: ${(error as Error).message}`);
    }

    if (interaction.type === 1) {
      return Response.json({ type: 1 });
    }

    try {
      if (interaction.data.name === "wallet" && config.walletSummary) {
        return responseMessage(
          await config.walletSummary(walletTargetUserId(interaction)),
        );
      }

      if (
        interaction.data.name === "wallet_init" &&
        config.walletStateExists &&
        (await config.walletStateExists(interaction.member.user.id))
      ) {
        return Response.json({
          type: 4,
          data: {
            content: "ok:wallet_init:already_initialized",
          },
        });
      }

      const latestBlockhash = await config.executor.getLatestBlockhash();
      const { transactionMessage } = await buildDiscordCommandTransaction({
        interaction,
        rawBody,
        timestamp: timestamp!,
        signatureHex: signature!,
        programId: config.programId,
        relayer: config.relayer,
        latestBlockhash,
        discordPublicKey,
      });

      const signedTransaction = await signTransactionMessageWithSigners(transactionMessage);
      const { serializedLength, signature: txSignature } =
        await config.executor.execute(signedTransaction);

      return Response.json({
        type: 4,
        data: {
          content: `ok:${interaction.data.name}:${serializedLength}:${txSignature}`,
        },
      });
    } catch (error) {
      await logInteractionFailure(error, interaction, rawBody, timestamp, signature);
      return responseMessage(`error:${(error as Error).message}`);
    }
  };
}

function walletTargetUserId(
  interaction: Extract<DiscordInteraction, { type: 2 }>,
): string {
  const value = interaction.data.options?.find((option) => option.name === "user")?.value;
  if (typeof value === "string" && value.length > 0) {
    return value;
  }
  return interaction.member.user.id;
}

function responseMessage(content: string): Response {
  return Response.json({
    type: 4,
    data: { content },
  });
}

async function logInteractionFailure(
  error: unknown,
  interaction: Extract<DiscordInteraction, { type: 2 }>,
  rawBody: string,
  timestamp: string | null,
  signature: string | null,
) {
  const requestMeta = {
    timestamp,
    signature,
    rawBodyLength: utf8ByteLength(rawBody),
    verifiedMessageLength: utf8ByteLength(timestamp ?? "") + utf8ByteLength(rawBody),
  };
  const lines = [
    "[discord-wallet] interaction execution failed",
    `command=${interaction.data.name}`,
    `interaction_id=${interaction.id}`,
    `user_id=${interaction.member.user.id}`,
    `message=${getErrorMessage(error)}`,
    `request_timestamp=${timestamp ?? ""}`,
    `request_signature=${signature ?? ""}`,
    `request_meta=${safeJson(requestMeta)}`,
    `raw_body=${rawBody}`,
  ];

  const context = getErrorContext(error);
  if (context) {
    lines.push(`rpc_context=${safeJson(context)}`);
    const logs = getSimulationLogs(context);
    if (logs.length > 0) {
      lines.push("simulation_logs:");
      for (const log of logs) {
        lines.push(`  ${log}`);
      }
    }
  }

  const cause = getErrorCause(error);
  if (cause) {
    lines.push(`cause=${getErrorMessage(cause)}`);
  }

  console.error(lines.join("\n"));
}

function getErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function getErrorCause(error: unknown) {
  if (!error || typeof error !== "object" || !("cause" in error)) {
    return undefined;
  }
  return error.cause;
}

function getErrorContext(error: unknown): unknown {
  if (!error || typeof error !== "object" || !("context" in error)) {
    return undefined;
  }
  return error.context;
}

function getSimulationLogs(context: unknown): string[] {
  if (!context || typeof context !== "object" || !("logs" in context)) {
    return [];
  }
  const { logs } = context;
  if (!Array.isArray(logs)) {
    return [];
  }
  return logs.filter((entry): entry is string => typeof entry === "string");
}

function safeJson(value: unknown, space = 0) {
  try {
    return JSON.stringify(
      value,
      (_key, nestedValue) => (typeof nestedValue === "bigint" ? nestedValue.toString() : nestedValue),
      space,
    );
  } catch {
    return String(value);
  }
}
