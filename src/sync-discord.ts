import { upsertWalletCommands, updateInteractionEndpoint } from "./discord-api";

const botToken = requiredEnv("DISCORD_BOT_TOKEN");
const applicationId = requiredEnv("DISCORD_APPLICATION_ID");
const publicInteractionsUrl = requiredEnv("PUBLIC_INTERACTIONS_URL");
const guildId = process.env.DISCORD_GUILD_ID;

const interactionsEndpointUrl = `${publicInteractionsUrl.replace(/\/$/, "")}/interactions`;

const application = await updateInteractionEndpoint(botToken, interactionsEndpointUrl);
console.log(`Updated interactions endpoint: ${application.interactions_endpoint_url}`);

const commands = await upsertWalletCommands(botToken, applicationId, guildId);
console.log(
  `Updated ${guildId ? `guild ${guildId}` : "global"} commands: ${commands
    .map((command) => command.name)
    .join(", ")}`,
);

function requiredEnv(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`${name} is required`);
  }
  return value;
}
