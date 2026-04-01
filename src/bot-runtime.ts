import {
  address,
  createKeyPairSignerFromBytes,
  getBase58Encoder,
  createSolanaRpc,
  createSolanaRpcSubscriptions,
  lamports,
  sendAndConfirmTransactionFactory,
} from "@solana/kit";

import { createRpcExecutor } from "./executor";
import { requiredDiscordPublicKey } from "./discord";
import { vaultPda, walletStatePda } from "./program";
import { deriveSubscriptionsUrl } from "./rpc";
import type { BotConfig } from "./server";

const TOKEN_PROGRAM_ID = address("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const BASE58_ENCODER = getBase58Encoder();
const AIRDROP_LAMPORTS = lamports(5_000_000_000n);

export type BotRuntimeOptions = {
  rpcUrl: string;
  subscriptionsUrl?: string;
  relayerSecret: string;
  programId: string;
  discordPublicKey: string;
};

export type BotRuntime = Pick<
  BotConfig,
  "executor" | "relayer" | "programId" | "discordPublicKey" | "walletStateExists" | "walletSummary" | "airdrop"
> & {
  rpcUrl: string;
};

export async function createBotRuntime(
  options: BotRuntimeOptions,
): Promise<BotRuntime> {
  const rpc = createSolanaRpc(options.rpcUrl);
  const rpcSubscriptions = createSolanaRpcSubscriptions(
    options.subscriptionsUrl ?? deriveSubscriptionsUrl(options.rpcUrl),
  );
  const relayer = await createKeyPairSignerFromBytes(
    BASE58_ENCODER.encode(options.relayerSecret),
  );
  const programAddress = address(options.programId);
  const discordPublicKey = requiredDiscordPublicKey({
    DISCORD_PUBLIC_KEY: options.discordPublicKey,
  });
  const sendAndConfirmTransaction = sendAndConfirmTransactionFactory({
    rpc,
    rpcSubscriptions,
  });

  return {
    rpcUrl: options.rpcUrl,
    executor: createRpcExecutor({
      async getLatestBlockhash() {
        const { value } = await rpc.getLatestBlockhash().send();
        return value;
      },
      async sendAndConfirmTransaction(transaction, config) {
        await sendAndConfirmTransaction(transaction, config);
      },
    }),
    relayer,
    programId: programAddress,
    discordPublicKey,
    async walletStateExists(discordUserId) {
      const walletState = await walletStatePda(programAddress, discordUserId);
      const { value } = await rpc.getAccountInfo(walletState, { encoding: "base64" }).send();
      return value !== null;
    },
    async walletSummary(discordUserId) {
      const walletState = await walletStatePda(programAddress, discordUserId);
      const { value } = await rpc.getAccountInfo(walletState, { encoding: "base64" }).send();
      const vault = await vaultPda(programAddress, walletState);
      const [{ value: solLamports }, { value: tokenAccounts }] = await Promise.all([
        rpc.getBalance(vault).send(),
        rpc
          .getTokenAccountsByOwner(
            vault,
            { programId: TOKEN_PROGRAM_ID },
            { encoding: "jsonParsed" },
          )
          .send(),
      ]);
      const sol = formatSol(solLamports);
      const tokenLines = tokenAccounts.map((entry) => {
        const info = entry.account.data.parsed.info;
        const mint = info.mint;
        const tokenAmount = info.tokenAmount;
        const uiAmount =
          tokenAmount.uiAmountString ??
          (tokenAmount.uiAmount !== null && tokenAmount.uiAmount !== undefined
            ? String(tokenAmount.uiAmount)
            : tokenAmount.amount);
        return `${mint}: ${uiAmount}`;
      });

      const tokens = tokenLines.length > 0 ? tokenLines.join("\n") : "none";
      const initialized = value !== null ? "yes" : "no";
      return `vault: ${vault}\ninitialized: ${initialized}\nsol: ${sol}\ntokens:\n${tokens}`;
    },
    async airdrop(discordUserId) {
      const walletState = await walletStatePda(programAddress, discordUserId);
      const { value } = await rpc.getAccountInfo(walletState, { encoding: "base64" }).send();
      if (value === null) {
        throw new Error("wallet_not_initialized");
      }

      const vault = await vaultPda(programAddress, walletState);
      return rpc.requestAirdrop(vault, AIRDROP_LAMPORTS).send();
    },
  };
}

export function requiredRuntimeOptions(source: Record<string, string | undefined>): BotRuntimeOptions {
  return {
    rpcUrl: source.SOLANA_RPC_URL ?? "http://127.0.0.1:8899",
    subscriptionsUrl: source.SOLANA_WS_URL,
    relayerSecret: requiredValue(source, "RELAYER_SECRET_KEY"),
    programId: requiredValue(source, "PROGRAM_ID"),
    discordPublicKey: requiredValue(source, "DISCORD_PUBLIC_KEY"),
  };
}

export function requiredValue(
  source: Record<string, string | undefined>,
  name: string,
): string {
  const value = source[name];
  if (!value) {
    throw new Error(`${name} is required`);
  }
  return value;
}

function formatSol(lamports: bigint) {
  const whole = lamports / 1_000_000_000n;
  const fractional = lamports % 1_000_000_000n;
  if (fractional === 0n) {
    return `${whole}`;
  }
  return `${whole}.${fractional.toString().padStart(9, "0").replace(/0+$/, "")}`;
}
