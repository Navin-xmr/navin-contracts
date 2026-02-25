#!/usr/bin/env bash
set -euo pipefail

echo "Building Soroban contracts..."

# Build both contracts
stellar contract build

# Verify WASM files exist
WASM_DIR="target/wasm32-unknown-unknown/release"
TOKEN_WASM="$WASM_DIR/navin_token.wasm"
SHIPMENT_WASM="$WASM_DIR/shipment.wasm"

if [[ ! -f "$TOKEN_WASM" ]]; then
  echo "Error: Token WASM not found at $TOKEN_WASM"
  exit 1
fi

if [[ ! -f "$SHIPMENT_WASM" ]]; then
  echo "Error: Shipment WASM not found at $SHIPMENT_WASM"
  exit 1
fi

# Print file sizes
echo "Build successful!"
echo "Token WASM: $TOKEN_WASM ($(du -h "$TOKEN_WASM" | cut -f1))"
echo "Shipment WASM: $SHIPMENT_WASM ($(du -h "$SHIPMENT_WASM" | cut -f1))"
