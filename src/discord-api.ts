const DISCORD_API_BASE = "https://discord.com/api/v10";

type DiscordApplication = {
  id: string;
  interactions_endpoint_url: string | null;
};

type DiscordCommand = {
  id: string;
  name: string;
  type: number;
};

export async function getCurrentApplication(botToken: string): Promise<DiscordApplication> {
  const response = await fetch(`${DISCORD_API_BASE}/applications/@me`, {
    headers: {
      authorization: `Bot ${botToken}`,
    },
  });
  if (!response.ok) {
    throw new Error(
      `failed to get current application: ${response.status} ${await response.text()}`,
    );
  }
  return response.json() as Promise<DiscordApplication>;
}

export async function updateInteractionEndpoint(
  botToken: string,
  interactionsEndpointUrl: string,
) {
  const response = await fetch(`${DISCORD_API_BASE}/applications/@me`, {
    method: "PATCH",
    headers: {
      authorization: `Bot ${botToken}`,
      "content-type": "application/json",
    },
    body: JSON.stringify({
      interactions_endpoint_url: interactionsEndpointUrl,
    }),
  });

  if (!response.ok) {
    throw new Error(
      `failed to update interactions endpoint: ${response.status} ${await response.text()}`,
    );
  }

  const application = await getCurrentApplication(botToken);
  if (application.interactions_endpoint_url !== interactionsEndpointUrl) {
    throw new Error(
      `Discord kept interactions endpoint ${application.interactions_endpoint_url ?? "null"} instead of ${interactionsEndpointUrl}`,
    );
  }

  return application;
}

export async function upsertWalletCommands(
  botToken: string,
  applicationId: string,
  guildId?: string,
): Promise<DiscordCommand[]> {
  const commands = commandDefinitions();
  const url = guildId
    ? `${DISCORD_API_BASE}/applications/${applicationId}/guilds/${guildId}/commands`
    : `${DISCORD_API_BASE}/applications/${applicationId}/commands`;
  const response = await fetch(
    url,
    {
      method: "PUT",
      headers: {
        authorization: `Bot ${botToken}`,
        "content-type": "application/json",
      },
      body: JSON.stringify(commands),
    },
  );
  if (!response.ok) {
    throw new Error(
      `failed to overwrite commands: ${response.status} ${await response.text()}`,
    );
  }

  const updatedCommands = (await response.json()) as DiscordCommand[];
  const expectedNames = new Set(commands.map((command) => command.name));
  const actualNames = new Set(updatedCommands.map((command) => command.name));
  if (
    updatedCommands.length !== commands.length ||
    commands.some((command) => !actualNames.has(command.name))
  ) {
    throw new Error(
      `Discord returned command set [${updatedCommands.map((command) => command.name).join(", ")}] instead of [${[...expectedNames].join(", ")}]`,
    );
  }

  return updatedCommands;
}

function commandDefinitions() {
  const integration_types = [0];
  const contexts = [0];

  return [
    {
      name: "wallet",
      description: "Show your vault address and balances",
      type: 1,
      integration_types,
      contexts,
      options: [
        {
          type: 6,
          name: "user",
          description: "Optional Discord user to inspect",
          required: false,
        },
      ],
    },
    {
      name: "airdrop",
      description: "Airdrop 5 SOL to your vault on localnet",
      type: 1,
      integration_types,
      contexts,
    },
    {
      name: "wallet_init",
      description: "Create your Discord-bound wallet",
      type: 1,
      integration_types,
      contexts,
    },
    {
      name: "set_withdrawer",
      description: "Authorize a wallet to withdraw from your vault",
      type: 1,
      integration_types,
      contexts,
      options: [
        {
          type: 3,
          name: "wallet",
          description: "Base58 wallet address allowed to withdraw",
          required: true,
        },
      ],
    },
    {
      name: "transfer",
      description: "Transfer SOL or SPL tokens from your vault",
      type: 1,
      integration_types,
      contexts,
      options: [
        {
          type: 3,
          name: "tkn",
          description: "Use sol, usdc, usdt, jup, or a token mint address",
          required: true,
        },
        {
          type: 10,
          name: "amt",
          description: "UI amount to transfer",
          required: true,
        },
        {
          type: 3,
          name: "to",
          description: "Destination wallet address or Discord mention",
          required: true,
        },
      ],
    },
  ];
}
