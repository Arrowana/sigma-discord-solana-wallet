import { createBotRuntime, requiredRuntimeOptions } from "./bot-runtime";
import { createBotHandler } from "./server";

export type WorkerEnv = {
  SOLANA_RPC_URL?: string;
  SOLANA_WS_URL?: string;
  RELAYER_SECRET_KEY: string;
  PROGRAM_ID: string;
  DISCORD_PUBLIC_KEY: string;
};

export default {
  async fetch(request: Request, env: WorkerEnv): Promise<Response> {
    const handler = await createWorkerHandler(env);
    return handler(request);
  },
};

async function createWorkerHandler(env: WorkerEnv) {
  const runtime = await createBotRuntime(requiredRuntimeOptions(env));
  return createBotHandler(runtime);
}
