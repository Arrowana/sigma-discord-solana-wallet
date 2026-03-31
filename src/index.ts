import {
  address,
  createKeyPairSignerFromBytes,
  sendTransactionWithoutConfirmingFactory,
} from "@solana/kit";
import bs58 from "bs58";

import { createPollingRpcExecutor } from "./executor";
import { vaultPda, walletStatePda } from "./program";
import { createRpcClients } from "./rpc";
import { createBotServer } from "./server";

const rpcUrl = process.env.SOLANA_RPC_URL ?? "http://127.0.0.1:8899";
const subscriptionsUrl = process.env.SOLANA_WS_URL;
const relayerSecret = process.env.RELAYER_SECRET_KEY;
const programId = process.env.PROGRAM_ID;
const discordPublicKey = process.env.DISCORD_PUBLIC_KEY;
const port = Number(process.env.PORT ?? "3000");

if (!relayerSecret || !programId || !discordPublicKey) {
  throw new Error("RELAYER_SECRET_KEY, PROGRAM_ID, and DISCORD_PUBLIC_KEY are required");
}

const { rpc, rpcSubscriptions } = createRpcClients(rpcUrl, subscriptionsUrl);
void rpcSubscriptions;
const sendTransaction = sendTransactionWithoutConfirmingFactory({ rpc });
const relayer = await createKeyPairSignerFromBytes(bs58.decode(relayerSecret));
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
  programId: address(programId),
  discordPublicKey: address(discordPublicKey),
  port,
  async walletStateExists(discordUserId) {
    const walletState = await walletStatePda(address(programId), discordUserId);
    const { value } = await rpc.getAccountInfo(walletState, { encoding: "base64" }).send();
    return value !== null;
  },
  async walletSummary(discordUserId) {
    const walletState = await walletStatePda(address(programId), discordUserId);
    const { value } = await rpc.getAccountInfo(walletState, { encoding: "base64" }).send();
    if (value === null) {
      return "error:wallet_not_initialized";
    }

    const vault = await vaultPda(address(programId), walletState);
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

console.log(server.url);

function formatSol(lamports: bigint) {
  const whole = lamports / 1_000_000_000n;
  const fractional = lamports % 1_000_000_000n;
  if (fractional === 0n) {
    return `${whole}`;
  }
  return `${whole}.${fractional.toString().padStart(9, "0").replace(/0+$/, "")}`;
}
