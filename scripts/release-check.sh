#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "==> release-check: formatting"
cargo fmt --all -- --check

echo "==> release-check: clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "==> release-check: tests"
cargo test --workspace

echo "==> release-check: wasm target availability"
if ! rustup target list --installed | awk '$0 == "wasm32-unknown-unknown" { found = 1 } END { exit !found }'; then
  echo "wasm32-unknown-unknown target is missing. Install with:"
  echo "  rustup target add wasm32-unknown-unknown"
  exit 1
fi

echo "==> release-check: wasm build"
cargo build --workspace --target wasm32-unknown-unknown --release

echo "==> release-check: docs consistency checks"
required_refs=(
  "get_shipments_batch"
  "get_shipments_by_sender"
  "get_shipments_by_carrier"
  "get_shipments_by_status"
  "ReentrancyLock"
  "ReentrancyDetected"
)

for ref in "${required_refs[@]}"; do
  if ! python3 - "$ref" <<'PY'
import sys
from pathlib import Path

needle = sys.argv[1]
paths = [Path("docs"), Path("README.md"), Path("contracts/shipment/src/events.rs")]

for path in paths:
    if path.is_dir():
        for child in path.rglob("*"):
            if child.is_file():
                try:
                    if needle in child.read_text(encoding="utf-8", errors="ignore"):
                        sys.exit(0)
                except OSError:
                    pass
    elif path.is_file():
        if needle in path.read_text(encoding="utf-8", errors="ignore"):
            sys.exit(0)

sys.exit(1)
PY
  then
    echo "Missing documentation/reference for: $ref"
    exit 1
  fi
done

echo "==> release-check: PASS"
