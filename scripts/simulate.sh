#!/usr/bin/env bash
# simulate.sh — Simulate Navin Shipment contract method calls before submitting
#
# Usage:
#   ./scripts/simulate.sh [METHOD]
#
#   METHOD  (optional) One of: initialize, create_shipment, update_status,
#           deposit_escrow, raise_dispute, report_condition_breach, all
#           Defaults to "all" when omitted.
#
# Prerequisites:
#   - stellar CLI installed (https://github.com/stellar/stellar-cli)
#   - Contracts built: run `./scripts/build.sh` first
#   - For testnet: set SHIPMENT_CONTRACT_ID and STELLAR_IDENTITY env vars
#     (or source .env.testnet after running ./scripts/deploy-testnet.sh)
#
# Environment variables:
#   STELLAR_NETWORK      local | testnet | mainnet   (default: local)
#   STELLAR_RPC_URL      RPC endpoint (auto-set for local/testnet)
#   STELLAR_IDENTITY     Stellar account key alias   (default: default)
#   SHIPMENT_CONTRACT_ID Deployed shipment contract address
#   TOKEN_CONTRACT_ID    Deployed token contract address
#
# What simulation does:
#   Calls simulateTransaction on the Stellar RPC without signing or submitting.
#   The output shows:
#     - Whether the call would succeed or fail
#     - Estimated resource usage (CPU instructions, memory, ledger reads/writes)
#     - Required authorisations
#     - Minimum fee recommendation
#   No ledger state is modified during simulation.
#
# Simulation flag used: `--send=no`
#   This tells the Stellar CLI to build and simulate the transaction but stop
#   before broadcasting it to the network.

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────

NETWORK="${STELLAR_NETWORK:-local}"

case "$NETWORK" in
  local)
    RPC_URL="${STELLAR_RPC_URL:-http://localhost:8000/rpc}"
    NETWORK_PASSPHRASE="${STELLAR_NETWORK_PASSPHRASE:-Standalone Network ; February 2017}"
    ;;
  testnet)
    RPC_URL="${STELLAR_RPC_URL:-https://soroban-testnet.stellar.org:443}"
    NETWORK_PASSPHRASE="${STELLAR_NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
    ;;
  mainnet)
    RPC_URL="${STELLAR_RPC_URL:-https://soroban.stellar.org:443}"
    NETWORK_PASSPHRASE="${STELLAR_NETWORK_PASSPHRASE:-Public Global Stellar Network ; September 2015}"
    ;;
  *)
    echo "ERROR: Unknown STELLAR_NETWORK='$NETWORK'. Use local | testnet | mainnet."
    exit 1
    ;;
esac

IDENTITY="${STELLAR_IDENTITY:-default}"
CONTRACT_ID="${SHIPMENT_CONTRACT_ID:-}"
TOKEN_ID="${TOKEN_CONTRACT_ID:-}"
METHOD="${1:-all}"

# ── Helpers ───────────────────────────────────────────────────────────────────

check_deps() {
  if ! command -v stellar &>/dev/null; then
    echo "ERROR: 'stellar' CLI not found."
    echo "  Install: https://github.com/stellar/stellar-cli?tab=readme-ov-file#install"
    exit 1
  fi
}

require_contract_id() {
  if [[ -z "$CONTRACT_ID" ]]; then
    echo "ERROR: SHIPMENT_CONTRACT_ID is not set."
    echo "  Run ./scripts/deploy-testnet.sh first, then source .env.testnet"
    exit 1
  fi
}

# Core simulate helper — wraps `stellar contract invoke --send=no`
simulate() {
  local label="$1"; shift
  echo ""
  echo "════════════════════════════════════════════════════════════════"
  echo "  SIMULATE: $label"
  echo "════════════════════════════════════════════════════════════════"
  stellar contract invoke \
    --id        "$CONTRACT_ID" \
    --source    "$IDENTITY" \
    --rpc-url   "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" \
    --send=no \
    -- "$@" 2>&1 || true
  echo ""
}

# Generate a 32-byte hex hash placeholder for demo invocations
DUMMY_HASH="0101010101010101010101010101010101010101010101010101010101010101"
ADMIN_ADDR=$(stellar keys address "$IDENTITY" 2>/dev/null || echo "GAAA...")

# ── Simulation methods ────────────────────────────────────────────────────────

sim_initialize() {
  require_contract_id
  simulate "initialize(admin, token)" \
    initialize \
    --admin "$ADMIN_ADDR" \
    --token_contract "$TOKEN_ID"
}

sim_create_shipment() {
  require_contract_id
  local receiver="${RECEIVER_ADDR:-${ADMIN_ADDR}}"
  local carrier="${CARRIER_ADDR:-${ADMIN_ADDR}}"
  local deadline=$(( $(date +%s) + 86400 ))  # 24 h from now

  simulate "create_shipment(company, receiver, carrier, data_hash, milestones, deadline)" \
    create_shipment \
    --company     "$ADMIN_ADDR" \
    --receiver    "$receiver" \
    --carrier     "$carrier" \
    --data_hash   "$DUMMY_HASH" \
    --milestones  "[]" \
    --deadline    "$deadline"
}

sim_update_status() {
  require_contract_id
  local shipment_id="${SHIPMENT_ID:-1}"

  simulate "update_status(carrier, shipment_id=1, InTransit, data_hash)" \
    update_status \
    --carrier     "$ADMIN_ADDR" \
    --shipment_id "$shipment_id" \
    --new_status  '{"InTransit":null}' \
    --data_hash   "$DUMMY_HASH"
}

sim_deposit_escrow() {
  require_contract_id
  local shipment_id="${SHIPMENT_ID:-1}"

  simulate "deposit_escrow(company, shipment_id=1, amount=1000000)" \
    deposit_escrow \
    --company     "$ADMIN_ADDR" \
    --shipment_id "$shipment_id" \
    --amount      1000000
}

sim_raise_dispute() {
  require_contract_id
  local shipment_id="${SHIPMENT_ID:-1}"

  simulate "raise_dispute(caller, shipment_id=1, reason_hash)" \
    raise_dispute \
    --caller      "$ADMIN_ADDR" \
    --shipment_id "$shipment_id" \
    --reason_hash "$DUMMY_HASH"
}

sim_report_condition_breach() {
  require_contract_id
  local shipment_id="${SHIPMENT_ID:-1}"

  simulate "report_condition_breach(carrier, shipment_id=1, TamperDetected, Critical, data_hash)" \
    report_condition_breach \
    --carrier     "$ADMIN_ADDR" \
    --shipment_id "$shipment_id" \
    --breach_type '{"TamperDetected":null}' \
    --severity    '{"Critical":null}' \
    --data_hash   "$DUMMY_HASH"
}

sim_get_contract_config() {
  require_contract_id
  simulate "get_contract_config()" \
    get_contract_config
}

# ── Entry point ───────────────────────────────────────────────────────────────

check_deps

echo ""
echo "Navin Shipment Contract — Transaction Simulation"
echo "Network  : $NETWORK ($RPC_URL)"
echo "Identity : $IDENTITY"
echo "Contract : ${CONTRACT_ID:-<not set>}"
echo ""

case "$METHOD" in
  initialize)             sim_initialize ;;
  create_shipment)        sim_create_shipment ;;
  update_status)          sim_update_status ;;
  deposit_escrow)         sim_deposit_escrow ;;
  raise_dispute)          sim_raise_dispute ;;
  report_condition_breach) sim_report_condition_breach ;;
  get_contract_config)    sim_get_contract_config ;;
  all)
    sim_initialize
    sim_create_shipment
    sim_update_status
    sim_deposit_escrow
    sim_raise_dispute
    sim_report_condition_breach
    sim_get_contract_config
    ;;
  *)
    echo "Unknown method: $METHOD"
    echo "Valid methods: initialize create_shipment update_status deposit_escrow"
    echo "              raise_dispute report_condition_breach get_contract_config all"
    exit 1
    ;;
esac

echo "Simulation complete. No transactions were submitted to the network."
