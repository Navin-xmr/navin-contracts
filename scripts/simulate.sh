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
#   SHIPMENT_SCHEMA      Contract schema JSON used to check flag names
#                        (default: docs/contract-schema.shipment.json)
#
# Exit status:
#   0 when every simulation succeeds, 1 when any simulation fails (including
#   argument errors). In `all` mode every method is still attempted; failures
#   are recorded and summarised at the end.
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

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCHEMA="${SHIPMENT_SCHEMA:-$SCRIPT_DIR/../docs/contract-schema.shipment.json}"

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

# Failed simulations, recorded so `all` mode can keep going and still exit
# non-zero at the end. A counter plus a newline-separated list avoids empty
# array expansion issues under `set -u` on older bash versions.
FAIL_COUNT=0
FAILED_LABELS=""

record_failure() {
  FAIL_COUNT=$((FAIL_COUNT + 1))
  FAILED_LABELS="${FAILED_LABELS}  - $1"$'\n'
}

# Check the `--flag` names passed for a method against the contract schema so
# a renamed parameter is reported before the CLI rejects it. Skipped (with a
# note) when jq or the schema file is unavailable.
check_flags() {
  local method="$1"; shift

  if ! command -v jq &>/dev/null || [[ ! -f "$SCHEMA" ]]; then
    echo "NOTE: skipping flag check (jq or $SCHEMA not available)"
    return 0
  fi

  if ! jq -e --arg m "$method" \
      'any(.[]; .function_v0? and .function_v0.name == $m)' \
      "$SCHEMA" >/dev/null; then
    echo "ERROR: '$method' is not a function in $SCHEMA"
    return 1
  fi

  local expected
  expected="$(jq -r --arg m "$method" \
    '.[] | select(.function_v0? and .function_v0.name == $m) | .function_v0.inputs[].name' \
    "$SCHEMA")"

  local arg name ok=0
  for arg in "$@"; do
    [[ "$arg" == --* ]] || continue
    name="${arg#--}"
    if ! grep -qxF -- "$name" <<<"$expected"; then
      echo "ERROR: '$method' has no parameter '--$name'"
      ok=1
    fi
  done

  local found
  while IFS= read -r name; do
    [[ -n "$name" ]] || continue
    found=0
    for arg in "$@"; do
      if [[ "$arg" == "--$name" ]]; then
        found=1
        break
      fi
    done
    if [[ "$found" -eq 0 ]]; then
      echo "ERROR: '$method' is missing required parameter '--$name'"
      ok=1
    fi
  done <<<"$expected"

  if [[ "$ok" -ne 0 ]]; then
    echo "  Expected parameters: $(tr '\n' ' ' <<<"$expected")"
  fi
  return "$ok"
}

# Core simulate helper — wraps `stellar contract invoke --send=no`.
# Never aborts the script: failures are recorded and reported at the end.
simulate() {
  local label="$1"; shift
  echo ""
  echo "════════════════════════════════════════════════════════════════"
  echo "  SIMULATE: $label"
  echo "════════════════════════════════════════════════════════════════"

  if ! check_flags "$@"; then
    echo "FAILED: $label (argument error, not sent to the CLI)"
    record_failure "$label (argument error)"
    echo ""
    return 0
  fi

  if stellar contract invoke \
    --id        "$CONTRACT_ID" \
    --source    "$IDENTITY" \
    --rpc-url   "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" \
    --send=no \
    -- "$@" 2>&1; then
    echo "OK: $label"
  else
    echo "FAILED: $label"
    record_failure "$label"
  fi
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

  simulate "create_shipment(sender, receiver, carrier, data_hash, payment_milestones, deadline)" \
    create_shipment \
    --sender             "$ADMIN_ADDR" \
    --receiver           "$receiver" \
    --carrier            "$carrier" \
    --data_hash          "$DUMMY_HASH" \
    --payment_milestones "[]" \
    --deadline           "$deadline"
}

sim_update_status() {
  require_contract_id
  local shipment_id="${SHIPMENT_ID:-1}"

  # Unit enum variants are passed as a bare JSON string.
  simulate "update_status(caller, shipment_id=$shipment_id, InTransit, data_hash)" \
    update_status \
    --caller      "$ADMIN_ADDR" \
    --shipment_id "$shipment_id" \
    --new_status  '"InTransit"' \
    --data_hash   "$DUMMY_HASH"
}

sim_deposit_escrow() {
  require_contract_id
  local shipment_id="${SHIPMENT_ID:-1}"

  simulate "deposit_escrow(from, shipment_id=$shipment_id, amount=1000000)" \
    deposit_escrow \
    --from        "$ADMIN_ADDR" \
    --shipment_id "$shipment_id" \
    --amount      1000000
}

sim_raise_dispute() {
  require_contract_id
  local shipment_id="${SHIPMENT_ID:-1}"

  simulate "raise_dispute(caller, shipment_id=$shipment_id, reason_hash)" \
    raise_dispute \
    --caller      "$ADMIN_ADDR" \
    --shipment_id "$shipment_id" \
    --reason_hash "$DUMMY_HASH"
}

sim_report_condition_breach() {
  require_contract_id
  local shipment_id="${SHIPMENT_ID:-1}"

  simulate "report_condition_breach(carrier, shipment_id=$shipment_id, TamperDetected, Critical, data_hash)" \
    report_condition_breach \
    --carrier     "$ADMIN_ADDR" \
    --shipment_id "$shipment_id" \
    --breach_type '"TamperDetected"' \
    --severity    '"Critical"' \
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

if [[ "$FAIL_COUNT" -gt 0 ]]; then
  echo "Simulation finished with $FAIL_COUNT failure(s). No transactions were submitted to the network."
  printf '%s' "$FAILED_LABELS"
  exit 1
fi

echo "Simulation complete. No transactions were submitted to the network."
