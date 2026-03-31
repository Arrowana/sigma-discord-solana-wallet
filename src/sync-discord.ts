import { upsertWalletCommands, updateInteractionEndpoint } from "./discord-api";

const botToken = requiredEnv("DISCORD_BOT_TOKEN");
const applicationId = requiredEnv("DISCORD_APPLICATION_ID");
const publicInteractionsUrl = requiredEnv("PUBLIC_INTERACTIONS_URL");

const interactionsEndpointUrl = `${publicInteractionsUrl.replace(/\/$/, "")}/interactions`;

await updateInteractionEndpoint(botToken, interactionsEndpointUrl);
console.log(`Updated interactions endpoint: ${interactionsEndpointUrl}`);

await upsertWalletCommands(botToken, applicationId);
console.log("Upserted wallet, wallet_init, set_withdrawer, and transfer commands.");

function requiredEnv(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`${name} is required`);
  }
  return value;
}
