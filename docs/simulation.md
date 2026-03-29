# Transaction Simulation Guide

Simulate Navin Shipment contract calls to estimate success/failure and resource
usage **before** submitting real transactions. No ledger state is modified and
no fees are charged during simulation.

---

## Why simulate?

| Concern | What simulation tells you |
|---|---|
| Will the call succeed? | Returns the same error the live call would return |
| How much will it cost? | Minimum fee recommendation in stroops |
| What auth is required? | Lists all required signers |
| Resource budget | CPU instructions, memory bytes, ledger reads/writes |

---

## Prerequisites

1. **Stellar CLI** installed
   ```
   cargo install --locked stellar-cli --features opt
   ```
   Or follow the [official install guide](https://github.com/stellar/stellar-cli?tab=readme-ov-file#install).

2. **Contracts built**
   ```bash
   ./scripts/build.sh
   ```

3. For **testnet/mainnet**: contracts deployed and `SHIPMENT_CONTRACT_ID` set
   ```bash
   ./scripts/deploy-testnet.sh
   source .env.testnet
   ```

---

## Running the simulation script

```bash
# Simulate all core methods in one shot (uses defaults)
./scripts/simulate.sh

# Simulate a single method
./scripts/simulate.sh initialize
./scripts/simulate.sh create_shipment
./scripts/simulate.sh update_status
./scripts/simulate.sh deposit_escrow
./scripts/simulate.sh raise_dispute
./scripts/simulate.sh report_condition_breach
./scripts/simulate.sh get_contract_config
```

### Environment variables

| Variable | Default | Description |
|---|---|---|
| `STELLAR_NETWORK` | `local` | `local` \| `testnet` \| `mainnet` |
| `STELLAR_IDENTITY` | `default` | Stellar CLI key alias used as the source account |
| `SHIPMENT_CONTRACT_ID` | _(required for live networks)_ | Deployed shipment contract address |
| `TOKEN_CONTRACT_ID` | _(required for `initialize`)_ | Deployed token contract address |
| `SHIPMENT_ID` | `1` | Shipment ID used in method-specific simulations |
| `RECEIVER_ADDR` | Same as identity | Receiver address for `create_shipment` |
| `CARRIER_ADDR` | Same as identity | Carrier address for `create_shipment` |

**Example — testnet with custom shipment ID:**
```bash
export STELLAR_NETWORK=testnet
source .env.testnet
SHIPMENT_ID=42 ./scripts/simulate.sh raise_dispute
```

---

## Local development (standalone node)

Start a local Stellar node with Docker:
```bash
docker run --rm -d \
  -p 8000:8000 \
  --name stellar-local \
  stellar/quickstart:soroban-dev \
  --standalone \
  --enable-soroban-rpc
```

Wait ~10 seconds for the node to be ready, then:
```bash
# Generate a funded local identity
stellar keys generate local-dev --network local

# Deploy contracts to the local node
STELLAR_IDENTITY=local-dev \
STELLAR_NETWORK=local \
  ./scripts/deploy-testnet.sh

source .env.testnet

# Simulate all methods against the local node
STELLAR_NETWORK=local ./scripts/simulate.sh
```

---

## How simulation works under the hood

`--send=no` causes the Stellar CLI to:

1. Build the transaction XDR from the provided arguments.
2. Call the RPC `simulateTransaction` endpoint with the unsigned transaction.
3. Parse and print the simulation response (success/error, resource usage, auth).
4. **Stop before signing or broadcasting.**

The same path is used internally when you run `stellar contract invoke` without
`--send=no` — the CLI always simulates first, then prompts before submitting.

---

## Interpreting simulation output

### Successful simulation

```
════════════════════════════════════════════════════════════════
  SIMULATE: create_shipment(company, receiver, carrier, ...)
════════════════════════════════════════════════════════════════
ℹ️  Simulating...

🔒 Authorization required:
  1. company (address: GC...)
     └─ create_shipment(company: GC..., ...)

⛽ Resources:
  CPU instructions : 1_234_567
  Memory bytes     :    56_789
  Ledger reads     :         8
  Ledger writes    :         4

💰 Min recommended fee: 1234 stroops

✅ Simulation success
```

### Failed simulation

```
════════════════════════════════════════════════════════════════
  SIMULATE: raise_dispute(caller, shipment_id=1, reason_hash)
════════════════════════════════════════════════════════════════
ℹ️  Simulating...

❌ Simulation failed: Error(Contract, #4) — ShipmentNotFound
```

Error codes map directly to `NavinError` variants in
[`contracts/shipment/src/errors.rs`](../contracts/shipment/src/errors.rs).

### Resource fields explained

| Field | Description |
|---|---|
| CPU instructions | Compute budget consumed. Exceeding the limit causes a `Resources` error. |
| Memory bytes | Memory budget. Soroban enforces a per-transaction cap. |
| Ledger reads | Number of ledger entries read. Each read costs a fee. |
| Ledger writes | Number of ledger entries written/created. Higher cost than reads. |
| Min fee (stroops) | Minimum base fee + resource fee. Use this as `--fee` when submitting. |

---

## Simulating without the script

You can simulate any method directly with the Stellar CLI:

```bash
stellar contract invoke \
  --id         "$SHIPMENT_CONTRACT_ID" \
  --source     "$STELLAR_IDENTITY" \
  --network    testnet \
  --send=no \
  -- get_contract_config
```

To simulate with a specific fee budget:
```bash
stellar contract invoke \
  --id         "$SHIPMENT_CONTRACT_ID" \
  --source     "$STELLAR_IDENTITY" \
  --network    testnet \
  --fee        10000 \
  --send=no \
  -- raise_dispute \
  --caller      "$CALLER_ADDR" \
  --shipment_id 1 \
  --reason_hash "0101010101010101010101010101010101010101010101010101010101010101"
```

---

## Core methods reference

| Method | Who can call | Key parameters |
|---|---|---|
| `initialize` | Deployer (once) | `admin`, `token_contract` |
| `create_shipment` | Company | `receiver`, `carrier`, `data_hash`, `milestones`, `deadline` |
| `update_status` | Carrier | `shipment_id`, `new_status`, `data_hash` |
| `deposit_escrow` | Company | `shipment_id`, `amount` |
| `release_escrow` | Carrier | `shipment_id`, `data_hash` |
| `raise_dispute` | Sender / Receiver / Carrier | `shipment_id`, `reason_hash` |
| `resolve_dispute` | Admin | `shipment_id`, `resolution`, `reason_hash` |
| `report_condition_breach` | Carrier (assigned) | `shipment_id`, `breach_type`, `severity`, `data_hash` |
| `get_contract_config` | Anyone | — |
| `update_config` | Admin | `new_config` (see `ContractConfig`) |

For a full list of methods and their parameters, see the
[Integration Guide](integration-guide.md).

---

## Troubleshooting

**`stellar` CLI not found**
Install the CLI — see [Prerequisites](#prerequisites).

**`SHIPMENT_CONTRACT_ID is not set`**
Deploy first with `./scripts/deploy-testnet.sh` and `source .env.testnet`.

**Simulation returns `NotInitialized (#2)`**
The contract has not been initialised. Run `simulate.sh initialize` first.

**Simulation returns `ShipmentNotFound (#4)`**
The `SHIPMENT_ID` doesn't exist on the network you're targeting. Create a
shipment first or set `SHIPMENT_ID` to a valid ID.

**`Error(Host, ...)`**
This is a host-level error (e.g., insufficient auth, budget exceeded). Add
`--verbose` to the Stellar CLI call for detailed diagnostic output.
