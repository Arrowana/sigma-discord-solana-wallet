import {
  signTransactionMessageWithSigners,
  type Address,
  type KeyPairSigner,
} from "@solana/kit";

import { DEFAULT_DISCORD_PUBLIC_KEY } from "./constants";
import type { DiscordInteraction } from "./discord";
import { verifyDiscordRequest } from "./discord";
import type { TransactionExecutor } from "./executor";
import { buildDiscordCommandTransaction } from "./program";

const LAST_REQUEST_BODY_DUMP_PATH = "/tmp/discord-wallet-bot-last-request-body.json";
const LAST_REQUEST_META_DUMP_PATH = "/tmp/discord-wallet-bot-last-request-meta.json";

export type BotConfig = {
  executor: TransactionExecutor;
  relayer: KeyPairSigner;
  programId: Address;
  discordPublicKey?: Address;
  port?: number;
  walletStateExists?(discordUserId: string): Promise<boolean>;
  walletSummary?(discordUserId: string): Promise<string>;
};

export type BotServer = {
  url: string;
  stop(): Promise<void>;
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
    await dumpDiscordRequest(rawBody, timestamp, signature);
    const discordPublicKey = config.discordPublicKey ?? DEFAULT_DISCORD_PUBLIC_KEY;
    if (!verifyDiscordRequest(timestamp, signature, rawBody, discordPublicKey)) {
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
        await config.walletStateExists(interaction.member.user.id)
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

export function createBotServer(config: BotConfig): BotServer {
  const handleRequest = createBotHandler(config);
  const server = Bun.serve({
    port: config.port ?? 3000,
    fetch: handleRequest,
  });

  return {
    url: server.url.toString(),
    async stop() {
      server.stop(true);
    },
  };
}

async function dumpDiscordRequest(
  rawBody: string,
  timestamp: string | null,
  signature: string | null,
) {
  const verifiedMessageLength = Buffer.byteLength(timestamp ?? "", "utf8") + Buffer.byteLength(rawBody, "utf8");
  await Bun.write(Bun.file(LAST_REQUEST_BODY_DUMP_PATH), rawBody);
  await Bun.write(
    Bun.file(LAST_REQUEST_META_DUMP_PATH),
    JSON.stringify(
      {
        timestamp,
        signature,
        rawBodyLength: Buffer.byteLength(rawBody, "utf8"),
        verifiedMessageLength,
      },
      null,
      2,
    ),
  );
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
  const failureDump = await dumpFailedDiscordRequest({
    interaction,
    rawBody,
    timestamp,
    signature,
    error,
  });
  const lines = [
    "[discord-wallet] interaction execution failed",
    `command=${interaction.data.name}`,
    `interaction_id=${interaction.id}`,
    `user_id=${interaction.member.user.id}`,
    `message=${getErrorMessage(error)}`,
    `request_timestamp=${timestamp ?? ""}`,
    `request_signature=${signature ?? ""}`,
    `request_body_path=${LAST_REQUEST_BODY_DUMP_PATH}`,
    `request_meta_path=${LAST_REQUEST_META_DUMP_PATH}`,
    `failure_body_path=${failureDump.bodyPath}`,
    `failure_meta_path=${failureDump.metaPath}`,
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

async function dumpFailedDiscordRequest(params: {
  interaction: Extract<DiscordInteraction, { type: 2 }>;
  rawBody: string;
  timestamp: string | null;
  signature: string | null;
  error: unknown;
}) {
  const safeInteractionId = params.interaction.id.replace(/[^0-9A-Za-z_-]/g, "_");
  const bodyPath = `/tmp/discord-wallet-bot-failed-${safeInteractionId}-body.json`;
  const metaPath = `/tmp/discord-wallet-bot-failed-${safeInteractionId}-meta.json`;
  const verifiedMessageLength =
    Buffer.byteLength(params.timestamp ?? "", "utf8") + Buffer.byteLength(params.rawBody, "utf8");

  await Bun.write(Bun.file(bodyPath), params.rawBody);
  await Bun.write(
    Bun.file(metaPath),
    safeJson(
      {
        interactionId: params.interaction.id,
        command: params.interaction.data.name,
        userId: params.interaction.member.user.id,
        timestamp: params.timestamp,
        signature: params.signature,
        rawBodyLength: Buffer.byteLength(params.rawBody, "utf8"),
        verifiedMessageLength,
        errorMessage: getErrorMessage(params.error),
        errorContext: getErrorContext(params.error),
        errorCause: getErrorCause(params.error),
      },
      2,
    ),
  );

  return { bodyPath, metaPath };
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
