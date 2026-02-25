#!/usr/bin/env bash
set -euo pipefail

# Source environment file
ENV_FILE=".env.testnet"
if [[ ! -f "$ENV_FILE" ]]; then
  echo "Error: $ENV_FILE not found. Run scripts/deploy-testnet.sh first."
  exit 1
fi

source "$ENV_FILE"

# Verify required variables
if [[ -z "${TOKEN_CONTRACT_ID:-}" ]] || [[ -z "${SHIPMENT_CONTRACT_ID:-}" ]]; then
  echo "Error: Contract IDs not found in $ENV_FILE"
  exit 1
fi

echo "Initializing contracts on Stellar testnet..."
echo "Token contract: $TOKEN_CONTRACT_ID"
echo "Shipment contract: $SHIPMENT_CONTRACT_ID"

# Get admin address from identity
ADMIN_ADDRESS=$(stellar keys address "$STELLAR_IDENTITY")
echo "Admin address: $ADMIN_ADDRESS"

# Initialize token contract
echo ""
echo "Initializing token contract..."
stellar contract invoke \
  --id "$TOKEN_CONTRACT_ID" \
  --source-account "$STELLAR_IDENTITY" \
  --rpc-url "$STELLAR_RPC_URL" \
  --network-passphrase "$STELLAR_NETWORK_PASSPHRASE" \
  -- \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --name "Navin Token" \
  --symbol "NAV" \
  --total_supply 10000000000000000

echo "Token contract initialized successfully"

# Initialize shipment contract
echo ""
echo "Initializing shipment contract..."
stellar contract invoke \
  --id "$SHIPMENT_CONTRACT_ID" \
  --source-account "$STELLAR_IDENTITY" \
  --rpc-url "$STELLAR_RPC_URL" \
  --network-passphrase "$STELLAR_NETWORK_PASSPHRASE" \
  -- \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --token_contract "$TOKEN_CONTRACT_ID"

echo "Shipment contract initialized successfully"

echo ""
echo "Initialization complete!"
echo "Both contracts are ready to use on testnet"
