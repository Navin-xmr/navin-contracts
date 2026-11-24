# Preflight Helpers for State-Changing Calls

## Overview

This document describes the preflight helper that guards state-changing (mutating)
endpoints when target shipment data is unavailable due to archival or state expiration.

> **Scope note:** this is an on-chain Rust guard (`preflight_check_shipment_available`)
> that runs inside the contract on every real invocation. For client-side
> transaction simulation — using `stellar contract invoke` or RPC
> `simulateTransaction` to preview a call's outcome before submitting it,
> without spending fees or touching ledger state — see
> [`docs/simulation.md`](simulation.md) instead.

## Problem Statement

The Navin shipment contract uses a two-tier storage model:
- **Persistent Storage**: For active shipments (expensive, long TTL)
- **Temporary Storage**: For archived shipments (cheaper, shorter TTL)

When a shipment reaches a terminal state (Delivered or Cancelled) with zero escrow,
it can be archived to temporary storage. Mutating endpoints must explicitly check for
shipment availability to avoid operating on stale data.

## Solution: Preflight Checks

### Error Variant

`ShipmentUnavailable` (code 42) is reserved in the error catalog for future use
distinguishing archived vs. expired state. The current implementation returns
`ShipmentNotFound` (4) or `ShipmentFinalized` (38) as described below.

### Preflight Helper Function

Defined in `validation.rs`:

```rust
pub fn preflight_check_shipment_available(
    env: &Env,
    shipment_id: u64,
) -> Result<Shipment, NavinError>
```

The helper performs two checks:

1. **Existence Check**: Verifies the shipment exists in persistent storage.
2. **Finalization Check**: Ensures the shipment is not finalized.

### Error Hierarchy

| Error | Code | Meaning |
|-------|------|---------|
| `ShipmentNotFound` | 4 | Shipment doesn't exist in persistent storage |
| `ShipmentFinalized` | 38 | Shipment is locked due to settlement |

## Usage Pattern

```rust
pub fn update_status(env: Env, caller: Address, shipment_id: u64, ...) -> Result<(), NavinError> {
    require_initialized(&env)?;
    caller.require_auth();
    let mut shipment = validation::preflight_check_shipment_available(&env, shipment_id)?;
    // ... rest of logic ...
}
```

## Test Coverage

Tests reside in `validation.rs` (unit) and `test_archive_restore_consistency.rs`
(integration). They cover:

- `ShipmentNotFound` when shipment doesn't exist
- `ShipmentFinalized` when shipment is locked
- Successful retrieval of an available shipment

## References

- **Storage Architecture**: See `docs/storage.md`
- **Error Handling**: See `errors.rs` for the complete error catalog
