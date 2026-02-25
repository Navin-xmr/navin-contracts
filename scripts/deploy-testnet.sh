#!/usr/bin/env bash
set -euo pipefail

# Environment variables with defaults
STELLAR_IDENTITY="${STELLAR_IDENTITY:-navin-testnet}"
STELLAR_RPC_URL="${STELLAR_RPC_URL:-https://soroban-testnet.stellar.org:443}"
STELLAR_NETWORK_PASSPHRASE="${STELLAR_NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"

echo "Deploying contracts to Stellar testnet..."
echo "Identity: $STELLAR_IDENTITY"
echo "RPC URL: $STELLAR_RPC_URL"

# Check if stellar CLI is installed
if ! command -v stellar &> /dev/null; then
  echo "Error: stellar CLI not found. Please install it first."
  exit 1
fi

# Check if identity exists, if not generate and fund it
if ! stellar keys show "$STELLAR_IDENTITY" &> /dev/null; then
  echo "Identity '$STELLAR_IDENTITY' not found. Generating new identity..."
  stellar keys generate "$STELLAR_IDENTITY" --network testnet
  
  echo "Funding account from friendbot..."
  ACCOUNT_ADDRESS=$(stellar keys address "$STELLAR_IDENTITY")
  curl -s "https://friendbot.stellar.org?addr=$ACCOUNT_ADDRESS" > /dev/null
  echo "Account funded: $ACCOUNT_ADDRESS"
fi

# Verify WASM files exist
WASM_DIR="target/wasm32-unknown-unknown/release"
TOKEN_WASM="$WASM_DIR/navin_token.wasm"
SHIPMENT_WASM="$WASM_DIR/shipment.wasm"

if [[ ! -f "$TOKEN_WASM" ]]; then
  echo "Error: Token WASM not found. Run scripts/build.sh first."
  exit 1
fi

if [[ ! -f "$SHIPMENT_WASM" ]]; then
  echo "Error: Shipment WASM not found. Run scripts/build.sh first."
  exit 1
fi

# Deploy token contract
echo "Deploying token contract..."
TOKEN_CONTRACT_ID=$(stellar contract deploy \
  --wasm "$TOKEN_WASM" \
  --source-account "$STELLAR_IDENTITY" \
  --rpc-url "$STELLAR_RPC_URL" \
  --network-passphrase "$STELLAR_NETWORK_PASSPHRASE")

echo "Token contract deployed: $TOKEN_CONTRACT_ID"

# Deploy shipment contract
echo "Deploying shipment contract..."
SHIPMENT_CONTRACT_ID=$(stellar contract deploy \
  --wasm "$SHIPMENT_WASM" \
  --source-account "$STELLAR_IDENTITY" \
  --rpc-url "$STELLAR_RPC_URL" \
  --network-passphrase "$STELLAR_NETWORK_PASSPHRASE")

echo "Shipment contract deployed: $SHIPMENT_CONTRACT_ID"

# Write addresses to .env.testnet
ENV_FILE=".env.testnet"
cat > "$ENV_FILE" << EOF
TOKEN_CONTRACT_ID=$TOKEN_CONTRACT_ID
SHIPMENT_CONTRACT_ID=$SHIPMENT_CONTRACT_ID
STELLAR_IDENTITY=$STELLAR_IDENTITY
STELLAR_RPC_URL=$STELLAR_RPC_URL
STELLAR_NETWORK_PASSPHRASE=$STELLAR_NETWORK_PASSPHRASE
EOF

echo ""
echo "Deployment complete!"
echo "Contract addresses saved to $ENV_FILE"
echo ""
echo "TOKEN_CONTRACT_ID=$TOKEN_CONTRACT_ID"
echo "SHIPMENT_CONTRACT_ID=$SHIPMENT_CONTRACT_ID"
