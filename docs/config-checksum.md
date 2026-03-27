# Config Checksum — Deterministic Drift Detection

## Overview

The config checksum feature exposes a deterministic SHA-256 checksum of critical configuration fields to help indexers and operators detect unintended configuration drift. This enables real-time monitoring of contract configuration state without requiring off-chain state reconstruction.

## Design

### Serialization Order

All config fields are serialized in a fixed, declaration-order sequence to ensure deterministic hashing:

| # | Field | Type | Size | Encoding |
|---|-------|------|------|----------|
| 1 | `shipment_ttl_threshold` | u32 | 4 bytes | big-endian |
| 2 | `shipment_ttl_extension` | u32 | 4 bytes | big-endian |
| 3 | `min_status_update_interval` | u64 | 8 bytes | big-endian |
| 4 | `batch_operation_limit` | u32 | 4 bytes | big-endian |
| 5 | `max_metadata_entries` | u32 | 4 bytes | big-endian |
| 6 | `default_shipment_limit` | u32 | 4 bytes | big-endian |
| 7 | `multisig_min_admins` | u32 | 4 bytes | big-endian |
| 8 | `multisig_max_admins` | u32 | 4 bytes | big-endian |
| 9 | `proposal_expiry_seconds` | u64 | 8 bytes | big-endian |
| 10 | `deadline_grace_seconds` | u64 | 8 bytes | big-endian |

**Total serialized size:** 52 bytes  
**Hash algorithm:** SHA-256 (32-byte output)

### Key Properties

- **Deterministic:** Same config always produces identical checksum
- **Sensitive:** Any single-bit change in any field produces a different checksum
- **Immutable:** Serialization order is fixed and cannot change without breaking compatibility
- **Efficient:** Computed once per config update and stored in instance storage
- **Queryable:** Exposed via `get_config_checksum()` contract method

## Implementation

### Core Functions

#### `compute_config_checksum(config: &ContractConfig) -> BytesN<32>`

Computes the SHA-256 checksum of a config struct by:
1. Serializing all fields in fixed order to a 52-byte buffer
2. Using big-endian encoding for all numeric types
3. Computing SHA-256 hash of the serialized bytes
4. Returning the 32-byte hash as `BytesN<32>`

**Location:** `contracts/shipment/src/config.rs`

#### `set_config(env: &Env, config: &ContractConfig)`

Stores config in instance storage and automatically computes/stores the checksum:
1. Stores config at `DataKey::ContractConfig`
2. Computes checksum via `compute_config_checksum()`
3. Stores checksum at `DataKey::ConfigChecksum`

This ensures the checksum is always in sync with the current config.

#### `get_config_checksum(env: &Env) -> Option<BytesN<32>>`

Retrieves the stored checksum from instance storage. Returns `None` if not yet computed.

#### `get_config_checksum(env: Env) -> Result<BytesN<32>, NavinError>` (Contract Method)

Public contract query that:
1. Requires contract initialization
2. Retrieves stored checksum or computes it from current config
3. Returns the 32-byte checksum

**Location:** `contracts/shipment/src/lib.rs`

### Storage

- **Key:** `DataKey::ConfigChecksum`
- **Tier:** Instance storage (global, cheapest)
- **Updated:** Automatically whenever config changes via `update_config()`
- **Queryable:** Via `get_config_checksum()` contract method

## Usage

### For Indexers

Query the checksum to detect config drift:

```rust
// Query current checksum
let checksum = contract.get_config_checksum()?;

// Compute expected checksum from known config
let expected = compute_config_checksum(&known_config);

// Detect drift
if checksum != expected {
    alert!("Config drift detected!");
}
```

### For Operators

Monitor checksum changes to track config updates:

```rust
// Store previous checksum
let prev_checksum = contract.get_config_checksum()?;

// After admin updates config
let new_checksum = contract.get_config_checksum()?;

// Verify change
if prev_checksum != new_checksum {
    log!("Config updated: {} -> {}", prev_checksum, new_checksum);
}
```

## Testing

### Unit Tests (config.rs)

Located in `contracts/shipment/src/config.rs::tests`:

- `test_checksum_deterministic_same_config` — Verifies same config produces identical checksums
- `test_checksum_changes_on_field_modification` — Verifies each field change produces different checksum
- `test_checksum_different_for_different_configs` — Verifies different configs produce different checksums
- `test_checksum_stable_across_multiple_runs` — Verifies checksum stability across multiple computations
- `test_checksum_serialization_order_matters` — Verifies serialization order is critical
- `test_checksum_is_32_bytes` — Verifies checksum is always 32 bytes
- `test_checksum_not_all_zeros` — Sanity check that checksum is not all zeros
- `test_checksum_boundary_values` — Tests with min/max config values
- `test_checksum_single_bit_flip_changes_hash` — Verifies sensitivity to single-bit changes

**Run:** `cargo test -p shipment --lib config::tests::test_checksum`

### Integration Tests (test.rs)

Located in `contracts/shipment/src/test.rs`:

- `test_config_checksum_exposed_via_query` — Verifies checksum is queryable
- `test_config_checksum_stable_after_initialization` — Verifies stability across queries
- `test_config_checksum_deterministic_across_instances` — Verifies same config produces same checksum across instances
- `test_config_checksum_changes_on_config_update` — Verifies checksum changes when config updates
- `test_config_checksum_not_all_zeros` — Sanity check
- `test_config_checksum_boundary_values` — Tests with boundary values
- `test_config_checksum_sequential_updates` — Verifies unique checksums for sequential updates
- `test_config_checksum_revert_produces_same_checksum` — Verifies reverting to same config produces same checksum
- `test_config_checksum_not_affected_by_shipment_operations` — Verifies shipment ops don't affect checksum
- `test_config_checksum_query_before_initialization_fails` — Verifies proper error handling

**Run:** `cargo test -p shipment test_config_checksum`

## Acceptance Criteria

✅ **Same config always yields same checksum**
- Verified by `test_checksum_deterministic_same_config`
- Verified by `test_checksum_stable_across_multiple_runs`
- Verified by `test_config_checksum_stable_after_initialization`

✅ **Config changes produce checksum change deterministically**
- Verified by `test_checksum_changes_on_field_modification` (all 10 fields tested)
- Verified by `test_checksum_different_for_different_configs`
- Verified by `test_config_checksum_changes_on_config_update`
- Verified by `test_checksum_single_bit_flip_changes_hash`

✅ **Serialization order is defined**
- Documented in this file (table above)
- Implemented in `compute_config_checksum()` with explicit comments
- Verified by `test_checksum_serialization_order_matters`

✅ **Checksum is exposed via query**
- Public contract method: `get_config_checksum(env: Env) -> Result<BytesN<32>, NavinError>`
- Verified by `test_config_checksum_exposed_via_query`

## Backward Compatibility

The checksum feature is **fully backward compatible**:

- Existing configs continue to work unchanged
- Checksum is computed on-demand if not stored
- No breaking changes to existing contract interface
- New `DataKey::ConfigChecksum` storage key is isolated

## Future Enhancements

Potential improvements for future versions:

1. **Versioned checksums** — Support multiple serialization formats
2. **Partial checksums** — Checksum subsets of config fields
3. **Checksum history** — Track historical checksums for audit trails
4. **Checksum events** — Emit events when checksum changes
5. **Multi-config checksums** — Combine multiple configs into single checksum

## References

- **Config module:** `contracts/shipment/src/config.rs`
- **Contract interface:** `contracts/shipment/src/lib.rs`
- **Tests:** `contracts/shipment/src/test.rs`
- **Storage keys:** `contracts/shipment/src/types.rs`
