import {
  address,
  createKeyPairSignerFromBytes,
  sendTransactionWithoutConfirmingFactory,
} from "@solana/kit";
import bs58 from "bs58";

import { startTryCloudflare } from "./cloudflare";
import { upsertWalletCommands, updateInteractionEndpoint } from "./discord-api";
import { createPollingRpcExecutor } from "./executor";
import { vaultPda, walletStatePda } from "./program";
import { createRpcClients } from "./rpc";
import { createBotServer } from "./server";

const rpcUrl = process.env.SOLANA_RPC_URL ?? "http://127.0.0.1:8899";
const subscriptionsUrl = process.env.SOLANA_WS_URL;
const relayerSecret = requiredEnv("RELAYER_SECRET_KEY");
const programId = requiredEnv("PROGRAM_ID");
const discordPublicKey = requiredEnv("DISCORD_PUBLIC_KEY");
const port = Number(process.env.PORT ?? "3000");

const appId = process.env.DISCORD_APPLICATION_ID;
const botToken = process.env.DISCORD_BOT_TOKEN;
const publicInteractionsUrl = process.env.PUBLIC_INTERACTIONS_URL;
const args = new Set(Bun.argv.slice(2));
const skipDeploy = args.has("--skip-deploy");

if (skipDeploy) {
  console.log("Skipping localnet deploy. Using the existing program state.");
} else {
  await deployProgramToLocalnet();
}

const { rpc, rpcSubscriptions } = createRpcClients(rpcUrl, subscriptionsUrl);
void rpcSubscriptions;
const sendTransaction = sendTransactionWithoutConfirmingFactory({ rpc });
const relayer = await createKeyPairSignerFromBytes(bs58.decode(relayerSecret));
const programAddress = address(programId);
const tokenProgramId = address("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const server = createBotServer({
  executor: createPollingRpcExecutor({
    async getLatestBlockhash() {
      const { value } = await rpc.getLatestBlockhash().send();
      return value;
    },
    async sendTransaction(transaction, config) {
      await sendTransaction(transaction, config);
    },
    async getSignatureStatus(signature) {
      const { value } = await rpc
        .getSignatureStatuses([signature], { searchTransactionHistory: true })
        .send();
      return value[0] ?? null;
    },
  }),
  relayer,
  programId: programAddress,
  discordPublicKey: address(discordPublicKey),
  port,
  async walletStateExists(discordUserId) {
    const walletState = await walletStatePda(programAddress, discordUserId);
    const { value } = await rpc.getAccountInfo(walletState, { encoding: "base64" }).send();
    return value !== null;
  },
  async walletSummary(discordUserId) {
    const walletState = await walletStatePda(programAddress, discordUserId);
    const { value } = await rpc.getAccountInfo(walletState, { encoding: "base64" }).send();
    if (value === null) {
      return "error:wallet_not_initialized";
    }

    const vault = await vaultPda(programAddress, walletState);
    const [{ value: solLamports }, { value: tokenAccounts }] = await Promise.all([
      rpc.getBalance(vault).send(),
      rpc
        .getTokenAccountsByOwner(
          vault,
          { programId: tokenProgramId },
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
    return `vault: ${vault}\nsol: ${sol}\ntokens:\n${tokens}`;
  },
});

console.log(`Local bot server listening at ${server.url}`);

const tunnelUrl = publicInteractionsUrl ?? (await startTryCloudflare(port));
const interactionsEndpointUrl = `${tunnelUrl.replace(/\/$/, "")}/interactions`;
console.log(`Public interactions endpoint: ${interactionsEndpointUrl}`);

if (botToken && appId) {
  try {
    await updateInteractionEndpoint(botToken, interactionsEndpointUrl);
    console.log("Updated Discord interactions endpoint.");
  } catch (error) {
    console.error(
      `Could not update the interactions endpoint automatically: ${(error as Error).message}`,
    );
    console.error(
      "Set the endpoint manually in the Discord developer portal if your app/token does not allow this route.",
    );
  }

  await upsertWalletCommands(botToken, appId);
  console.log("Upserted wallet, wallet_init, set_withdrawer, and transfer commands.");
} else {
  console.log(
    "DISCORD_APPLICATION_ID or DISCORD_BOT_TOKEN not set; skipping endpoint update and command registration.",
  );
}

console.log("Use wallet, wallet_init, set_withdrawer, and transfer in a guild channel where the app is installed. Press Ctrl-C to stop.");
await new Promise(() => {});

async function deployProgramToLocalnet() {
  const child = Bun.spawn(["./scripts/deploy-localnet-program.sh"], {
    cwd: process.cwd(),
    env: process.env,
    stdout: "pipe",
    stderr: "pipe",
  });
  const stderr = await new Response(child.stderr).text();
  const stdout = await new Response(child.stdout).text();
  const exitCode = await child.exited;
  if (stdout.trim().length > 0) {
    console.log(stdout.trim());
  }
  if (exitCode !== 0) {
    throw new Error(
      `failed to deploy program to localnet before real:e2e\nstdout:\n${stdout}\nstderr:\n${stderr}`,
    );
  }
}

function requiredEnv(name: string): string {
  const value = process.env[name];
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
