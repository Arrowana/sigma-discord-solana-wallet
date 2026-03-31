#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${ENV_FILE:-$ROOT_DIR/.env}"
RELAYER_KEYPAIR_PATH="${RELAYER_KEYPAIR_PATH:-$ROOT_DIR/relayer-keypair.json}"
PROGRAM_KEYPAIR_PATH="${PROGRAM_KEYPAIR_PATH:-$ROOT_DIR/target/deploy/discord_wallet-keypair.json}"
PROGRAM_SO="${PROGRAM_SO:-$ROOT_DIR/target/deploy/discord_wallet.so}"
SOLANA_RPC_URL_DEFAULT="http://127.0.0.1:8899"
RELAYER_AIRDROP_SOL="${RELAYER_AIRDROP_SOL:-10}"

if [[ -f "$ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  set +a
fi

SOLANA_RPC_URL="${SOLANA_RPC_URL:-$SOLANA_RPC_URL_DEFAULT}"
PROGRAM_ID="${PROGRAM_ID:-}"

if [[ ! -f "$RELAYER_KEYPAIR_PATH" ]]; then
  echo "Relayer keypair not found at $RELAYER_KEYPAIR_PATH" >&2
  exit 1
fi

if [[ ! -f "$PROGRAM_KEYPAIR_PATH" ]]; then
  echo "Program keypair not found at $PROGRAM_KEYPAIR_PATH" >&2
  exit 1
fi

if [[ ! -f "$PROGRAM_SO" ]]; then
  echo "Program binary not found at $PROGRAM_SO" >&2
  echo "Build it first with cargo build-sbf." >&2
  exit 1
fi

PROGRAM_KEYPAIR_PUBKEY="$(solana address -k "$PROGRAM_KEYPAIR_PATH")"
RELAYER_PUBKEY="$(solana address -k "$RELAYER_KEYPAIR_PATH")"

if [[ -n "$PROGRAM_ID" && "$PROGRAM_ID" != "$PROGRAM_KEYPAIR_PUBKEY" ]]; then
  echo "PROGRAM_ID ($PROGRAM_ID) does not match $PROGRAM_KEYPAIR_PATH ($PROGRAM_KEYPAIR_PUBKEY)" >&2
  exit 1
fi

echo "Deploying program to $SOLANA_RPC_URL"
echo "Program ID: $PROGRAM_KEYPAIR_PUBKEY"
echo "Relayer: $RELAYER_PUBKEY"

solana airdrop "$RELAYER_AIRDROP_SOL" "$RELAYER_PUBKEY" --url "$SOLANA_RPC_URL" >/dev/null
solana program deploy "$PROGRAM_SO" \
  --program-id "$PROGRAM_KEYPAIR_PATH" \
  --keypair "$RELAYER_KEYPAIR_PATH" \
  --url "$SOLANA_RPC_URL"
