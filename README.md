# sigma

sigma is a Discord-native Solana wallet.

The core trust assumption is simple: wallet actions only execute when the command was signed by the Discord app. The bot or relayer does not invent authority; it just forwards the request and pays fees.

At a high level:

- each Discord user gets a program-derived vault
- Discord slash commands become on-chain instructions
- SOL and token transfers can be initiated from Discord
- a configured withdrawer can always exit funds on-chain

It works only with surfpool for real Discord payloads, this project is designed to work with the `tx v1` support in [`Arrowana/surfpool`](https://github.com/Arrowana/surfpool/tree/chore/solana-4). The signed Discord raw body is typically around 1.7 kB, so carrying it atomically alongside Ed25519 verification is only possible with the larger `v1` transaction envelope. That avoids compromising the design by splitting verification away from execution.

More info about larger TX [SIMD-0296](https://github.com/solana-foundation/solana-improvement-documents/blob/main/proposals/0296-larger-transactions.md)

## How It Works

1. A user issues a slash command in Discord.
2. Discord signs the raw request.
3. The relayer submits a Solana transaction carrying that signed payload.
4. The program verifies the Discord signature, checks the wallet binding, and executes the action.

## Main Pieces

- On-chain program: [lib.rs](REDACTED/projects/experiments/discord-solana-wallet-bot/programs/discord-wallet/src/lib.rs)
- Signature verification helper: [sigverify.rs](REDACTED/projects/experiments/discord-solana-wallet-bot/programs/discord-wallet/src/sigverify.rs)
- Worker entrypoint: [worker.ts](REDACTED/projects/experiments/discord-solana-wallet-bot/src/worker.ts)
- Landing page: [index.html](REDACTED/projects/experiments/discord-solana-wallet-bot/pages/index.html)

## Test

Run the program integration suite:

```bash
bun run test
```

This uses LiteSVM and covers the main happy paths plus at least one signature failure path.

## Discord Flow

Build the program with the real Discord app public key:

```bash
DISCORD_PUBLIC_KEY=<discord-app-public-key> cargo build-sbf --manifest-path programs/discord-wallet/Cargo.toml --features bpf-entrypoint
```

Deploy the program

```bash
bun run deploy:localnet
```

Airdrop the relayer keypair then start the Worker locally:

```bash
bun run start
```

For a publicly reachable dev URL, use `wrangler dev --remote` and set:

- `PUBLIC_INTERACTIONS_URL`

Then sync Discord and optionally deploy localnet state:

```bash
bun run real:e2e
```

Useful variants:

```bash
bun run real:e2e -- --skip-deploy
bun run deploy:localnet
bun run sync:discord
```
