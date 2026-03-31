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

3. Install `cloudflared`, or set `PUBLIC_INTERACTIONS_URL` to an already-public HTTPS endpoint.

4. Run:

```bash
bun run real:e2e
```

The helper deploys `target/deploy/discord_wallet.so` to the configured localnet with `relayer-keypair.json`, starts the local bot server, opens a `trycloudflare` tunnel if needed, attempts to update the Discord interactions endpoint, and upserts the `wallet`, `wallet_init`, `set_withdrawer`, and `transfer` commands.

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

Leave that process running while you start the bot with `bun run start` or `bun run real:e2e`.
