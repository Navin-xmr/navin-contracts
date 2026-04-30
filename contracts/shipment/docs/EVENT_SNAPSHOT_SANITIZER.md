# Event Snapshot Sanitizer

## Overview

The event snapshot sanitizer normalizes non-deterministic fields in Soroban test snapshots to eliminate noisy diffs in snapshot-based regression tests. This keeps snapshot tests focused on actual contract behavior changes rather than ledger-specific test harness state.

## Problem Statement

Soroban test snapshots capture the complete ledger state, including:

- Generated contract addresses
- Ledger timestamps and sequence numbers
- Storage nonces
- Event idempotency keys (which include ledger-dependent values)

These fields change on every test run, creating massive diffs even when the actual contract behavior hasn't changed. This makes snapshot-based regression testing impractical.

## Solution

The `sanitize_json_snapshot()` function in `test_utils.rs` normalizes volatile fields to canonical values while preserving the meaningful parts of the snapshot.

## Normalized Fields

### Ledger State Fields

| Field                    | Normalized Value | Rationale                                |
| ------------------------ | ---------------- | ---------------------------------------- |
| `generators.address`     | `0`              | Test address counter - changes every run |
| `generators.nonce`       | `0`              | Test nonce counter - changes every run   |
| `ledger.timestamp`       | `86400`          | Canonical 1-day offset for readability   |
| `ledger.sequence_number` | `1`              | Canonical sequence number                |
| `ledger_key_nonce.nonce` | `0`              | Storage nonces - non-deterministic       |

### Event Fields

| Field                  | Normalized Value                    | Rationale                                           |
| ---------------------- | ----------------------------------- | --------------------------------------------------- |
| `event.contract_id`    | `"0000...0000"` (32-byte zero hash) | Generated contract addresses change every run       |
| Event idempotency keys | `"0000...0000"` (32-byte zero hash) | SHA-256 hashes that include ledger-dependent values |

**Note on idempotency keys:** Events emit idempotency keys as the last `bytes` field in their data vector. These are SHA-256 hashes computed from domain, shipment_id, event_type, and event_counter. Since they include ledger-dependent values, they're normalized to zero hashes.

## What's NOT Normalized

The sanitizer intentionally preserves:

- Event topics (e.g., `"shipment_created"`, `"escrow_released"`)
- Event data structure and ordering
- Business data (shipment IDs, amounts, addresses in event payloads)
- Data hashes (first `bytes` fields in events - these are user-provided)
- Contract state and storage values

This ensures that **true event changes still produce diffs** while eliminating noise.

## Usage

### Quick Start

```bash
# 1. Run tests to generate snapshots
cargo test --lib

# 2. Sanitize all snapshots
cargo run --example sanitize_snapshots

# 3. Commit sanitized snapshots
git add test_snapshots/
git commit -m "Sanitize test snapshots"
```

### Manual Sanitization

For individual files or custom workflows:

```rust
use std::fs;
use shipment::test_utils::sanitize_json_snapshot;

let raw = fs::read_to_string("test_snapshots/e2e_test/test_happy_path.1.json")?;
let sanitized = sanitize_json_snapshot(&raw);
fs::write("test_snapshots/e2e_test/test_happy_path.1.json", sanitized)?;
```

### In Test Code

The sanitizer is available in `test_utils` for use in custom test helpers:

```rust
use crate::test_utils::sanitize_json_snapshot;

#[test]
fn test_my_contract_behavior() {
    let env = Env::default();
    // ... test setup and execution ...

    // If you need to manually work with snapshots
    let snapshot_json = /* get snapshot somehow */;
    let sanitized = sanitize_json_snapshot(&snapshot_json);

    // Compare or save as needed
}
```

### Example: Before and After

**Before sanitization:**

```json
{
  "generators": { "address": 6, "nonce": 3 },
  "ledger": { "timestamp": 172845, "sequence_number": 42 },
  "events": [
    {
      "event": {
        "contract_id": "0000000000000000000000000000000000000000000000000000000000000006",
        "body": {
          "v0": {
            "data": {
              "vec": [
                { "u64": 1 },
                {
                  "bytes": "4d665e5885d370938b6ef4915d3e18cce2280979a315d468afc7bef8d99362b4"
                }
              ]
            }
          }
        }
      }
    }
  ]
}
```

**After sanitization:**

```json
{
  "generators": { "address": 0, "nonce": 0 },
  "ledger": { "timestamp": 86400, "sequence_number": 1 },
  "events": [
    {
      "event": {
        "contract_id": "0000000000000000000000000000000000000000000000000000000000000000",
        "body": {
          "v0": {
            "data": {
              "vec": [
                { "u64": 1 },
                {
                  "bytes": "0000000000000000000000000000000000000000000000000000000000000000"
                }
              ]
            }
          }
        }
      }
    }
  ]
}
```

## Design Rationale

### Why Normalize Instead of Ignore?

We could ignore these fields entirely, but normalization is better because:

1. **Structural validation** - We still verify the snapshot structure is correct
2. **Readability** - Canonical values (like `86400` for timestamp) are more readable than random values
3. **Diff clarity** - Changes to normalized fields are immediately visible in diffs

### Why These Specific Values?

- **Zero hashes** - Clearly indicate "this was normalized" while being valid 32-byte values
- **`86400` for timestamp** - Represents 1 day in seconds, a human-readable canonical value
- **`1` for sequence** - The natural starting point for ledger sequences
- **`0` for counters** - The natural starting point for generators and nonces

### Event Idempotency Key Detection

The sanitizer identifies idempotency keys by:

1. Looking for the last element in an event's data vector
2. Checking if it's a `bytes` field
3. Verifying it's exactly 64 hex characters (32 bytes)

This heuristic works because:

- Idempotency keys are always emitted last in event data
- They're always 32-byte SHA-256 hashes
- User-provided data hashes appear earlier in the event data

## Testing the Sanitizer

The sanitizer itself has comprehensive tests in `test_utils.rs`:

```bash
cargo test test_sanitize_json_snapshot --lib
```

Tests cover:

- Basic ledger field normalization
- Event contract_id normalization
- Event idempotency key normalization
- Preservation of non-idempotency bytes fields
- Multiple events in a single snapshot

## Maintenance

### Adding New Volatile Fields

If you discover new non-deterministic fields causing snapshot churn:

1. Add normalization logic to the `walk()` function in `sanitize_json_snapshot()`
2. Add a test case demonstrating the normalization
3. Update this documentation with the new field

### Verifying Effectiveness

To verify the sanitizer is working:

1. Run a snapshot test twice without changing code
2. Compare the generated snapshots - they should be identical
3. Make a meaningful change (e.g., add an event field)
4. Verify the diff shows only your change, not ledger noise

## Related Documentation

- [TESTING.md](./TESTING.md) - General testing guidelines
- [FRONTEND_VERIFICATION.md](./FRONTEND_VERIFICATION.md) - Event verification patterns
- [SETTLEMENT_IMPLEMENTATION_SUMMARY.md](./SETTLEMENT_IMPLEMENTATION_SUMMARY.md) - Settlement event patterns
