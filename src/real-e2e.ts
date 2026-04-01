import { requiredValue } from "./bot-runtime";
import { upsertWalletCommands, updateInteractionEndpoint } from "./discord-api";

const appId = process.env.DISCORD_APPLICATION_ID;
const botToken = process.env.DISCORD_BOT_TOKEN;
const guildId = process.env.DISCORD_GUILD_ID;
const publicInteractionsUrl = requiredEnv("PUBLIC_INTERACTIONS_URL");
const args = new Set(Bun.argv.slice(2));
const skipDeploy = args.has("--skip-deploy");

if (skipDeploy) {
  console.log("Skipping localnet deploy. Using the existing program state.");
} else {
  await deployProgramToLocalnet();
}

const interactionsEndpointUrl = `${publicInteractionsUrl.replace(/\/$/, "")}/interactions`;
console.log(`Public interactions endpoint: ${interactionsEndpointUrl}`);

if (botToken && appId) {
  try {
    const application = await updateInteractionEndpoint(botToken, interactionsEndpointUrl);
    console.log(
      `Updated Discord interactions endpoint to ${application.interactions_endpoint_url}.`,
    );
  } catch (error) {
    console.error(
      `Could not update the interactions endpoint automatically: ${(error as Error).message}`,
    );
    console.error(
      "Set the endpoint manually in the Discord developer portal if your app/token does not allow this route.",
    );
  }

  const commands = await upsertWalletCommands(botToken, appId, guildId);
  console.log(
    `Updated Discord ${guildId ? `guild ${guildId}` : "global"} commands: ${commands
      .map((command) => command.name)
      .join(", ")}.`,
  );
} else {
  console.log(
    "DISCORD_APPLICATION_ID or DISCORD_BOT_TOKEN not set; skipping endpoint update and command registration.",
  );
}

console.log("Use wallet, wallet_init, set_withdrawer, and transfer in a guild channel where the app is installed.");

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
  return requiredValue(process.env, name);
}
