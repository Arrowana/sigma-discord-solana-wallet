# Discord Solana Wallet Bot

## Program E2E

```bash
bun run e2e
```

This runs the Rust LiteSVM integration suite under [litesvm.rs](REDACTED/projects/experiments/discord-solana-wallet-bot/programs/discord-wallet/tests/litesvm.rs). The test:

- rebuilds the SBF program against the fixture Discord public key
- loads the compiled program into LiteSVM with Ed25519 precompile support
- verifies `wallet_init`
- verifies `set_withdrawer`
- verifies Discord-triggered SOL transfers to wallet addresses and mentioned users
- verifies Discord-triggered SPL token transfers to owner ATAs and mentioned-user vault ATAs
- verifies direct SOL and SPL token withdrawals by the configured withdrawer

## Real Discord E2E

1. Build and deploy the program against the real Discord application public key:

```bash
DISCORD_PUBLIC_KEY=<discord-app-public-key> cargo build-sbf --manifest-path programs/discord-wallet/Cargo.toml --features bpf-entrypoint
```

2. Fill in `.env.example`.

3. Start the Worker locally with Wrangler, usually:

```bash
bun run start
```

For a publicly reachable development URL, use `wrangler dev --remote` instead and set `PUBLIC_INTERACTIONS_URL` to that Worker URL.

4. Run:

```bash
bun run real:e2e
```

The helper deploys `target/deploy/discord_wallet.so` to the configured localnet with `relayer-keypair.json`, updates the Discord interactions endpoint to `PUBLIC_INTERACTIONS_URL/interactions`, and overwrites the `wallet`, `wallet_init`, `set_withdrawer`, and `transfer` commands.

For fast iteration in a test server, set `DISCORD_GUILD_ID`. That makes the sync use guild-scoped commands, which update immediately. Without it, the sync updates global commands, which can appear stale in the Discord client for a while.

To run against the program already deployed on localnet:

```bash
bun run real:e2e -- --skip-deploy
```

If you only want the deploy step:

```bash
bun run deploy:localnet
```

Notes:

- The live on-chain flow is guild-only.

## Cloudflare Worker

The Discord HTTP handler is also exposed as a Worker entry at [worker.ts](REDACTED/projects/experiments/discord-solana-wallet-bot/src/worker.ts), with config in [wrangler.jsonc](REDACTED/projects/experiments/discord-solana-wallet-bot/wrangler.jsonc).

Set these Worker secrets or vars:

- `RELAYER_SECRET_KEY`
- `PROGRAM_ID`
- `DISCORD_PUBLIC_KEY`
- `SOLANA_RPC_URL`
- `SOLANA_WS_URL` is optional and currently unused by the Worker path

Then deploy with:

```bash
bunx wrangler deploy
```

Or, if `wrangler` is installed already:

```bash
bun run deploy:worker
```

## Cloudflare Pages

A standalone landing page for Cloudflare Pages lives at [index.html](REDACTED/projects/experiments/discord-solana-wallet-bot/pages/index.html).

You can deploy that directory directly as a Pages project:

```bash
pages/
```

## Local validator helper

To boot a local validator with the compiled program preloaded at `PROGRAM_ID`, and to fund both the relayer and a generated test user:

```bash
./scripts/start-local-validator.sh
```

The script:

- loads `target/deploy/discord_wallet.so` at the `PROGRAM_ID` from `.env`
- reuses `relayer-keypair.json`
- creates `test-user-keypair.json` if it does not exist
- airdrops 10 SOL to the relayer and 10 SOL to the test user

Leave that process running while you start the Worker with `bun run start`.
