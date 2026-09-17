import { describe, expect, test } from "bun:test";

import {
  createTokenRegistry,
  tokenAutocompleteChoices,
} from "../src/token-registry";

describe("token registry", () => {
  test("shows symbols but submits exact mint addresses", () => {
    const choices = tokenAutocompleteChoices("usdc", createTokenRegistry());

    expect(choices).toHaveLength(1);
    expect(choices[0]?.name).toContain("USDC");
    expect(choices[0]?.name).toContain(
      "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
    );
    expect(choices[0]?.value).toBe(
      "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
    );
  });

  test("allows symbol collisions because the signed values remain distinct", () => {
    const registry = createTokenRegistry(
      JSON.stringify([
        {
          symbol: "USD",
          name: "Dollar One",
          mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        },
        {
          symbol: "USD",
          name: "Dollar Two",
          mint: "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB",
        },
      ]),
    );
    const choices = tokenAutocompleteChoices("usd", registry);

    expect(choices).toHaveLength(2);
    expect(new Set(choices.map((choice) => choice.value)).size).toBe(2);
  });

  test("rejects duplicate mint entries", () => {
    const duplicateMint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
    expect(() =>
      createTokenRegistry(
        JSON.stringify([
          { symbol: "A", name: "A", mint: duplicateMint },
          { symbol: "B", name: "B", mint: duplicateMint },
        ]),
      ),
    ).toThrow("duplicate mint");
  });

  test("reserves SOL for the native asset", () => {
    expect(() =>
      createTokenRegistry(
        JSON.stringify([
          {
            symbol: "SOL",
            name: "Not native SOL",
            mint: "So11111111111111111111111111111111111111112",
          },
        ]),
      ),
    ).toThrow("symbol SOL is reserved");
  });
});
