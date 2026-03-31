const DISCORD_API_BASE = "https://discord.com/api/v10";

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
}

export async function upsertWalletCommands(botToken: string, applicationId: string) {
  for (const command of commandDefinitions()) {
    const response = await fetch(
      `${DISCORD_API_BASE}/applications/${applicationId}/commands`,
      {
        method: "POST",
        headers: {
          authorization: `Bot ${botToken}`,
          "content-type": "application/json",
        },
        body: JSON.stringify(command),
      },
    );
    if (!response.ok) {
      throw new Error(
        `failed to upsert command ${command.name}: ${response.status} ${await response.text()}`,
      );
    }
  }
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
          description: "Use sol or a token mint address",
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
