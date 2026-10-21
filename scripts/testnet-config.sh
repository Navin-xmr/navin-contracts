STELLAR_IDENTITY="${STELLAR_IDENTITY:-navin-testnet}"
STELLAR_RPC_URL="${STELLAR_RPC_URL:-https://soroban-testnet.stellar.org:443}"
STELLAR_NETWORK_PASSPHRASE="${STELLAR_NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"

require_stellar_cli() {
  if ! command -v stellar > /dev/null 2>&1; then
    echo "Error: stellar CLI not found. Please install it first." >&2
    return 1
  fi
}