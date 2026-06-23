#!/usr/bin/env bash

# Bastion local environment provisioner
#
# Boots a local validator with Bastion loaded, creates an owner wallet and SPL
# token mint, funds everything, and writes the generated artifacts to:
#
#   examples/bot/temp/
#
# Usage:
#
#   pnpm -F @examples/bot provision
#
# Then, in another terminal:
#
#   pnpm -F @examples/bot demo

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

TEMP_DIR="$ROOT/examples/bot/temp"
LEDGER_DIR="${LEDGER_DIR:-/tmp/bastion-ledger}"
RPC_URL="http://127.0.0.1:8899"
WS_URL="ws://127.0.0.1:8900"

PROGRAM_KEYPAIR="target/deploy/bastion-keypair.json"
PROGRAM_SO="target/deploy/bastion.so"

DECIMALS=6
SYMBOL="DEMO"
OWNER_AIRDROP_SOL=100
TOKEN_SUPPLY=1000000

OWNER_JSON="$TEMP_DIR/owner.json"
MINT_JSON="$TEMP_DIR/mint.json"
CONFIG_JSON="$TEMP_DIR/config.json"

for bin in solana solana-keygen solana-test-validator spl-token jq; do
    command -v "$bin" >/dev/null 2>&1 || {
        echo "Missing required tool: $bin" >&2
        exit 1
    }
done

if [[ ! -f "$PROGRAM_SO" || ! -f "$PROGRAM_KEYPAIR" ]]; then
    echo "Bastion program not built. Run:"
    echo "  anchor run build-sbf"
    exit 1
fi

PROGRAM_ID="$(solana address -k "$PROGRAM_KEYPAIR")"

echo "Program ID: $PROGRAM_ID"

mkdir -p "$TEMP_DIR"

# Start validator

echo "Starting validator..."

solana-test-validator \
    --reset \
    --ledger "$LEDGER_DIR" \
    --bpf-program "$PROGRAM_ID" "$PROGRAM_SO" \
    --rpc-port 8899 \
    >/tmp/bastion-validator.log 2>&1 &

VALIDATOR_PID=$!

cleanup() {
    echo "Stopping validator..."
    kill "$VALIDATOR_PID" 2>/dev/null || true
}

trap cleanup EXIT
trap 'exit 0' INT TERM

echo "Waiting for validator..."

for i in $(seq 1 60); do
    if solana cluster-version -u "$RPC_URL" >/dev/null 2>&1; then
        echo "Validator ready after ${i}s"
        break
    fi

    if ! kill -0 "$VALIDATOR_PID" 2>/dev/null; then
        echo "Validator crashed"
        cat /tmp/bastion-validator.log
        exit 1
    fi

    sleep 1
done

# Create owner wallet

solana-keygen new \
    --no-bip39-passphrase \
    --force \
    --silent \
    -o "$OWNER_JSON"

OWNER_ADDRESS="$(solana address -k "$OWNER_JSON")"

echo "Owner: $OWNER_ADDRESS"

solana airdrop \
    "$OWNER_AIRDROP_SOL" \
    "$OWNER_ADDRESS" \
    -u "$RPC_URL" \
    >/dev/null

echo "Airdropped ${OWNER_AIRDROP_SOL} SOL"

# Create SPL mint

solana-keygen new \
    --no-bip39-passphrase \
    --force \
    --silent \
    -o "$MINT_JSON"

MINT_ADDRESS="$(solana address -k "$MINT_JSON")"

spl-token --url "$RPC_URL" --fee-payer "$OWNER_JSON" \
    create-token \
    --decimals "$DECIMALS" \
    --mint-authority "$OWNER_ADDRESS" \
    "$MINT_JSON" \
    >/dev/null

spl-token --url "$RPC_URL" --fee-payer "$OWNER_JSON" \
    create-account \
    "$MINT_ADDRESS" \
    --owner "$OWNER_ADDRESS" \
    >/dev/null

spl-token --url "$RPC_URL" --fee-payer "$OWNER_JSON" \
    mint \
    "$MINT_ADDRESS" \
    "$TOKEN_SUPPLY" \
    --mint-authority "$OWNER_JSON" \
    --recipient-owner "$OWNER_ADDRESS" \
    >/dev/null

echo "Mint: $MINT_ADDRESS (${TOKEN_SUPPLY} ${SYMBOL})"

# Write bot config

jq -n \
    --arg rpcUrl "$RPC_URL" \
    --arg wsUrl "$WS_URL" \
    --arg mint "$MINT_ADDRESS" \
    --arg symbol "$SYMBOL" \
    --arg programId "$PROGRAM_ID" \
    --arg ownerKeypair "examples/bot/temp/owner.json" \
    --argjson decimals "$DECIMALS" \
    '{
        rpcUrl: $rpcUrl,
        wsUrl: $wsUrl,
        mint: $mint,
        decimals: $decimals,
        symbol: $symbol,
        programId: $programId,
        ownerKeypair: $ownerKeypair
    }' \
    >"$CONFIG_JSON"

echo
echo "Provisioning complete"
echo
echo "Artifacts:"
echo "  owner.json   -> $OWNER_ADDRESS"
echo "  mint.json    -> $MINT_ADDRESS"
echo "  config.json  -> rpc=$RPC_URL"
echo
echo "Validator running."
echo
echo "In another terminal:"
echo "  pnpm -F @examples/bot demo"
echo

wait "$VALIDATOR_PID"
