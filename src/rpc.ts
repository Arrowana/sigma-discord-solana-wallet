import { createSolanaRpc, createSolanaRpcSubscriptions } from "@solana/kit";

export function deriveSubscriptionsUrl(rpcUrl: string): string {
  const url = new URL(rpcUrl);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  if (
    (url.hostname === "127.0.0.1" || url.hostname === "localhost") &&
    url.port === "8899"
  ) {
    url.port = "8900";
  }
  return url.toString();
}

export function createRpcClients(rpcUrl: string, subscriptionsUrl?: string) {
  return {
    rpc: createSolanaRpc(rpcUrl),
    rpcSubscriptions: createSolanaRpcSubscriptions(
      subscriptionsUrl ?? deriveSubscriptionsUrl(rpcUrl),
    ),
  };
}
