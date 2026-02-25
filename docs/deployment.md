# Testnet Deployment Guide

This guide walks you through deploying the Navin shipment tracking contracts to Stellar testnet.

## Prerequisites

Before deploying, ensure you have:

1. **Rust toolchain** installed with the `wasm32-unknown-unknown` target:

   ```bash
   rustup target add wasm32-unknown-unknown
   ```

2. **Stellar CLI** (v22.0.1 or later):

   ```bash
   cargo install --locked stellar-cli
   ```

3. **Build tools**: Make sure you can build the contracts locally:
   ```bash
   cargo build
   ```

## Environment Variables

The deployment scripts use the following environment variables:

| Variable                     | Default                                   | Description                                        |
| ---------------------------- | ----------------------------------------- | -------------------------------------------------- |
| `STELLAR_IDENTITY`           | `navin-testnet`                           | Stellar CLI identity name for signing transactions |
| `STELLAR_RPC_URL`            | `https://soroban-testnet.stellar.org:443` | Stellar testnet RPC endpoint                       |
| `STELLAR_NETWORK_PASSPHRASE` | `Test SDF Network ; September 2015`       | Network passphrase for testnet                     |

You can override these by exporting them before running the scripts:

```bash
export STELLAR_IDENTITY="my-custom-identity"
export STELLAR_RPC_URL="https://my-rpc-endpoint.com"
```

## Deployment Steps

### 1. Build Contracts

Build both contracts to optimized WASM:

```bash
./scripts/build.sh
```

This will:

- Compile both `navin-token` and `shipment` contracts
- Verify WASM files are generated
- Display file sizes

Expected output:

```
Building Soroban contracts...
Build successful!
Token WASM: target/wasm32-unknown-unknown/release/navin_token.wasm (XXX KB)
Shipment WASM: target/wasm32-unknown-unknown/release/shipment.wasm (XXX KB)
```

### 2. Deploy Contracts

Deploy both contracts to testnet:

```bash
./scripts/deploy-testnet.sh
```

This will:

- Check if the Stellar identity exists (create and fund from friendbot if not)
- Deploy the token contract
- Deploy the shipment contract
- Save contract addresses to `.env.testnet`

Expected output:

```
Deploying contracts to Stellar testnet...
Identity: navin-testnet
RPC URL: https://soroban-testnet.stellar.org:443
Deploying token contract...
Token contract deployed: CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
Deploying shipment contract...
Shipment contract deployed: CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX

Deployment complete!
Contract addresses saved to .env.testnet
```

### 3. Initialize Contracts

Initialize both contracts with default parameters:

```bash
./scripts/init-testnet.sh
```

This will:

- Initialize the token contract with:
  - Name: "Navin Token"
  - Symbol: "NAV"
  - Total supply: 1,000,000,000.0000000 (10^16 stroops, 7 decimals)
- Initialize the shipment contract with the token contract address

Expected output:

```
Initializing contracts on Stellar testnet...
Token contract: CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
Shipment contract: CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
Admin address: GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX

Initializing token contract...
Token contract initialized successfully

Initializing shipment contract...
Shipment contract initialized successfully

Initialization complete!
Both contracts are ready to use on testnet
```

## Verification

After deployment, verify the contracts are working:

1. **Check token balance** of the admin:

   ```bash
   source .env.testnet
   stellar contract invoke \
     --id "$TOKEN_CONTRACT_ID" \
     --source-account "$STELLAR_IDENTITY" \
     --rpc-url "$STELLAR_RPC_URL" \
     --network-passphrase "$STELLAR_NETWORK_PASSPHRASE" \
     -- \
     balance \
     --id "$(stellar keys address $STELLAR_IDENTITY)"
   ```

2. **Check shipment contract admin**:
   ```bash
   stellar contract invoke \
     --id "$SHIPMENT_CONTRACT_ID" \
     --source-account "$STELLAR_IDENTITY" \
     --rpc-url "$STELLAR_RPC_URL" \
     --network-passphrase "$STELLAR_NETWORK_PASSPHRASE" \
     -- \
     get_admin
   ```

## Troubleshooting

### "stellar CLI not found"

Install the Stellar CLI:

```bash
cargo install --locked stellar-cli
```

### "WASM not found"

Run the build script first:

```bash
./scripts/build.sh
```

### "Account not funded" or transaction failures

The deploy script automatically funds new accounts from friendbot. If you're using an existing identity, ensure it has testnet XLM:

```bash
ACCOUNT_ADDRESS=$(stellar keys address "$STELLAR_IDENTITY")
curl "https://friendbot.stellar.org?addr=$ACCOUNT_ADDRESS"
```

### "Contract already initialized"

Contracts can only be initialized once. If you need to redeploy, run the deploy script again to get new contract instances.

### Permission denied when running scripts

Make scripts executable:

```bash
chmod +x scripts/*.sh
```

## Next Steps

After successful deployment:

1. Save the contract addresses from `.env.testnet` for your application
2. Test contract functionality using the Stellar CLI or your application
3. Monitor transactions on [Stellar Expert](https://stellar.expert/explorer/testnet)

For production deployment, follow similar steps but use mainnet configuration and thoroughly test all functionality on testnet first.
