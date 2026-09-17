import { address, type Address } from "@solana/kit";

export type TokenRegistryEntry = Readonly<{
  symbol: string;
  name: string;
  mint: Address;
}>;

export type TokenChoice = Readonly<{
  name: string;
  value: string;
}>;

const DEFAULT_TOKEN_REGISTRY: readonly TokenRegistryEntry[] = [
  {
    symbol: "USDC",
    name: "USD Coin",
    mint: address("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"),
  },
  {
    symbol: "USDT",
    name: "Tether USD",
    mint: address("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB"),
  },
  {
    symbol: "JUP",
    name: "Jupiter",
    mint: address("JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN"),
  },
];

export function createTokenRegistry(json?: string): readonly TokenRegistryEntry[] {
  if (!json) {
    return DEFAULT_TOKEN_REGISTRY;
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch {
    throw new Error("TOKEN_REGISTRY_JSON must be valid JSON");
  }
  if (!Array.isArray(parsed)) {
    throw new Error("TOKEN_REGISTRY_JSON must be an array");
  }

  const entries = parsed.map((value, index) => parseRegistryEntry(value, index));
  const seenMints = new Set<string>();
  for (const entry of entries) {
    if (seenMints.has(entry.mint)) {
      throw new Error(`TOKEN_REGISTRY_JSON contains duplicate mint ${entry.mint}`);
    }
    seenMints.add(entry.mint);
  }
  return entries;
}

export function tokenAutocompleteChoices(
  query: string,
  registry: readonly TokenRegistryEntry[],
): readonly TokenChoice[] {
  const normalizedQuery = query.trim().toLowerCase();
  const entries: TokenChoice[] = [
    { name: "SOL — Solana", value: "sol" },
    ...registry.map((entry) => {
      const mintSuffix = ` (${entry.mint})`;
      const label = `${entry.symbol} — ${entry.name}`;
      return {
        name: `${label.slice(0, 100 - mintSuffix.length)}${mintSuffix}`,
        value: entry.mint,
      };
    }),
  ];

  return entries
    .filter((entry) => {
      if (normalizedQuery.length === 0) {
        return true;
      }
      return (
        entry.name.toLowerCase().includes(normalizedQuery) ||
        entry.value.toLowerCase().includes(normalizedQuery)
      );
    })
    .slice(0, 25);
}

function parseRegistryEntry(value: unknown, index: number): TokenRegistryEntry {
  if (!value || typeof value !== "object") {
    throw new Error(`TOKEN_REGISTRY_JSON entry ${index} must be an object`);
  }
  const record = value as Record<string, unknown>;
  const symbol = requiredShortString(record.symbol, `entry ${index} symbol`).toUpperCase();
  if (!/^[A-Z0-9._+-]{1,20}$/.test(symbol)) {
    throw new Error(
      `TOKEN_REGISTRY_JSON entry ${index} symbol must use 1-20 letters, numbers, or ._+-`,
    );
  }
  if (symbol === "SOL") {
    throw new Error(`TOKEN_REGISTRY_JSON entry ${index} symbol SOL is reserved`);
  }
  const name = requiredShortString(record.name, `entry ${index} name`);
  const mintValue = requiredShortString(record.mint, `entry ${index} mint`);

  let mint: Address;
  try {
    mint = address(mintValue);
  } catch {
    throw new Error(`TOKEN_REGISTRY_JSON entry ${index} mint is invalid`);
  }
  return { symbol, name, mint };
}

function requiredShortString(value: unknown, label: string): string {
  if (typeof value !== "string" || value.trim().length === 0 || value.length > 64) {
    throw new Error(`TOKEN_REGISTRY_JSON ${label} must be a string of 1-64 characters`);
  }
  return value.trim();
}
