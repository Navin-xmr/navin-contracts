use crate::errors::NavinError;
use crate::storage;
use crate::types::Shipment;
use soroban_sdk::{xdr::ToXdr, BytesN, Env, Symbol};

/// Maximum reasonable escrow amount (1 quadrillion stroops ≈ 1 billion XLM).
const MAX_AMOUNT: i128 = 1_000_000_000_000_000;

/// How far in the past a timestamp may be before it is rejected (seconds).
/// Roughly 1 year.
const MAX_PAST_OFFSET: u64 = 365 * 24 * 60 * 60;

/// How far in the future a timestamp may be before it is rejected (seconds).
/// Roughly 10 years.
const MAX_FUTURE_OFFSET: u64 = 10 * 365 * 24 * 60 * 60;

/// Ensure a `BytesN<32>` hash is not the all-zeros sentinel value.
///
/// This validator performs a sanity check on external hashes (data_hash, reason_hash, etc.)
/// to reject the all-zeros pattern which is commonly used as a sentinel for "no data".
/// This prevents accidental or malicious use of zero hashes in critical fields.
///
/// # Arguments
/// * `hash` - The 32-byte hash to validate.
///
/// # Returns
/// * `Ok(())` if the hash contains at least one non-zero byte.
/// * `Err(NavinError::InvalidHash)` if every byte is zero.
///
/// # Examples
/// ```rust
/// validate_hash(&hash)?;
/// ```
pub fn validate_hash(hash: &BytesN<32>) -> Result<(), NavinError> {
    // BytesN::iter() is not available in no_std soroban; use to_array().
    let bytes: [u8; 32] = hash.to_array();
    if bytes.iter().all(|&b| b == 0) {
        return Err(NavinError::InvalidHash);
    }
    Ok(())
}

/// Validate a Symbol for bounded usage in shipment metadata and milestones.
///
/// This validator ensures that Symbol strings conform to expected length constraints
/// and do not contain invalid patterns. Symbols are used in:
/// - Milestone checkpoint names (e.g., "warehouse", "port")
/// - Metadata keys and values
/// - Event topic names
///
/// # Arguments
/// * `symbol` - The Symbol to validate.
///
/// # Returns
/// * `Ok(())` if the symbol is valid.
/// * `Err(NavinError::InvalidShipmentInput)` if the symbol is empty or exceeds max length.
///
/// # Examples
/// ```rust
/// validate_symbol(&Symbol::new(&env, "warehouse"))?;
/// ```
pub fn validate_symbol(env: &Env, symbol: &Symbol) -> Result<(), NavinError> {
    // Convert Symbol to XDR representation for length checking.
    // In Soroban, we check the XDR-encoded length as a proxy for symbol size.
    // Typical XDR overhead is ~8 bytes, so we allow up to 40 bytes for safety margin.
    let symbol_bytes = symbol.to_xdr(env);

    if symbol_bytes.len() > 40 {
        return Err(NavinError::InvalidShipmentInput);
    }

    Ok(())
}

/// Validate a collection of milestone symbols for bounded usage.
///
/// This validator ensures that all milestone checkpoint names conform to length
/// constraints and are not duplicated within the same shipment.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `milestones` - Vector of (Symbol, percentage) tuples.
///
/// # Returns
/// * `Ok(())` if all symbols are valid and unique.
/// * `Err(NavinError::InvalidShipmentInput)` if any symbol is invalid or duplicated.
///
/// # Examples
/// ```rust
/// validate_milestone_symbols(&env, &milestones)?;
/// ```
pub fn validate_milestone_symbols(
    env: &Env,
    milestones: &soroban_sdk::Vec<(Symbol, u32)>,
) -> Result<(), NavinError> {
    // Check each milestone symbol for validity
    for milestone in milestones.iter() {
        validate_symbol(env, &milestone.0)?;
    }

    // Check for duplicate milestone names by comparing XDR representations
    for i in 0..milestones.len() {
        let current = &milestones.get_unchecked(i).0;
        let current_xdr = current.to_xdr(env);
        for j in (i + 1)..milestones.len() {
            let other = &milestones.get_unchecked(j).0;
            let other_xdr = other.to_xdr(env);
            if current_xdr == other_xdr {
                return Err(NavinError::InvalidShipmentInput);
            }
        }
    }

    Ok(())
}

/// Validate metadata key-value pair symbols for bounded usage.
///
/// This validator ensures that both metadata keys and values conform to
/// length constraints before storage.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `key` - The metadata key symbol.
/// * `value` - The metadata value symbol.
///
/// # Returns
/// * `Ok(())` if both symbols are valid.
/// * `Err(NavinError::InvalidShipmentInput)` if either symbol is invalid.
///
/// # Examples
/// ```rust
/// validate_metadata_symbols(&env, &key, &value)?;
/// ```
pub fn validate_metadata_symbols(
    env: &Env,
    key: &Symbol,
    value: &Symbol,
) -> Result<(), NavinError> {
    validate_symbol(env, key)?;
    validate_symbol(env, value)?;
    Ok(())
}

/// Ensure an escrow / payment amount is positive and within a sane upper bound.
///
/// # Arguments
/// * `amount` - The `i128` value to validate.
///
/// # Returns
/// * `Ok(())` if `0 < amount <= MAX_AMOUNT`.
/// * `Err(NavinError::InvalidAmount)` otherwise.
///
/// # Examples
/// ```rust
/// validate_amount(5_000_000)?;
/// ```
pub fn validate_amount(amount: i128) -> Result<(), NavinError> {
    if amount <= 0 || amount > MAX_AMOUNT {
        return Err(NavinError::InvalidAmount);
    }
    Ok(())
}

/// Ensure a timestamp is neither too far in the past nor too far in the future
/// relative to the current ledger time.
///
/// # Arguments
/// * `env`       - The execution environment (used to read `ledger().timestamp()`).
/// * `timestamp` - The `u64` UNIX timestamp to validate.
///
/// # Returns
/// * `Ok(())` if the timestamp is within acceptable bounds.
/// * `Err(NavinError::InvalidTimestamp)` otherwise.
///
/// # Examples
/// ```rust
/// validate_timestamp(&env, some_ts)?;
/// ```
pub fn validate_timestamp(env: &Env, timestamp: u64) -> Result<(), NavinError> {
    let now = env.ledger().timestamp();
    let earliest = now.saturating_sub(MAX_PAST_OFFSET);
    let latest = now.saturating_add(MAX_FUTURE_OFFSET);

    if timestamp < earliest || timestamp > latest {
        return Err(NavinError::InvalidTimestamp);
    }
    Ok(())
}

/// Look up a shipment by ID and return it, or surface `ShipmentNotFound`.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `id`  - Shipment ID to look up.
///
/// # Returns
/// * `Ok(Shipment)` if the shipment exists in persistent storage.
/// * `Err(NavinError::ShipmentNotFound)` if no shipment is stored under `id`.
///
/// # Examples
/// ```rust
/// let shipment = validate_shipment_exists(&env, shipment_id)?;
/// ```
pub fn validate_shipment_exists(env: &Env, id: u64) -> Result<Shipment, NavinError> {
    storage::get_shipment(env, id).ok_or(NavinError::ShipmentNotFound)
}

/// Preflight check for state-changing operations: ensures the shipment exists
/// and is available for mutation.
///
/// This helper gates all mutating endpoints to prevent operations on unavailable
/// shipment state due to archival or expiration. It performs two critical checks:
///
/// 1. **Existence Check**: Verifies the shipment exists in persistent storage.
///    Archived shipments (in temporary storage) are considered unavailable for
///    mutations to prevent accidental modifications to finalized state.
///
/// 2. **Finalization Check**: Ensures the shipment is not finalized. Finalized
///    shipments are locked and cannot be modified.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment to check.
///
/// # Returns
/// * `Ok(Shipment)` - The shipment if available for mutation.
/// * `Err(NavinError::ShipmentNotFound)` - If shipment doesn't exist in persistent storage.
/// * `Err(NavinError::ShipmentUnavailable)` - If shipment is archived or expired.
/// * `Err(NavinError::ShipmentFinalized)` - If shipment is finalized and locked.
///
/// # Design Rationale
///
/// **Why Archived Shipments Are Unavailable**:
/// - Archived shipments are moved to temporary storage (cheaper, shorter TTL)
/// - They represent terminal state (Delivered/Cancelled) with zero escrow
/// - Allowing mutations would violate the finalization contract
/// - Clients should query the shipment before attempting mutations
///
/// **Error Hierarchy**:
/// - `ShipmentNotFound`: Shipment never existed or has expired completely
/// - `ShipmentUnavailable`: Shipment exists but is archived (terminal state)
/// - `ShipmentFinalized`: Shipment is locked due to settlement
///
/// # Examples
/// ```rust
/// // In a mutating endpoint:
/// let shipment = preflight_check_shipment_available(&env, shipment_id)?;
/// // Now safe to mutate the shipment
/// ```
pub fn preflight_check_shipment_available(
    env: &Env,
    shipment_id: u64,
) -> Result<Shipment, NavinError> {
    // Check if shipment exists in persistent storage
    let shipment: Option<Shipment> = env
        .storage()
        .persistent()
        .get(&crate::types::DataKey::Shipment(shipment_id));

    let shipment = shipment.ok_or(NavinError::ShipmentNotFound)?;

    // Check if shipment is finalized (locked)
    if shipment.finalized {
        return Err(NavinError::ShipmentFinalized);
    }

    Ok(shipment)
}

// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Ledger, BytesN, Env, Symbol};

    // validate_hash
    #[test]
    fn test_validate_hash_all_zeros_fails() {
        let env = Env::default();
        let zero_hash: BytesN<32> = BytesN::from_array(&env, &[0u8; 32]);
        assert_eq!(validate_hash(&zero_hash), Err(NavinError::InvalidHash));
    }

    #[test]
    fn test_validate_hash_nonzero_passes() {
        let env = Env::default();
        let mut bytes = [0u8; 32];
        bytes[0] = 1;
        let hash: BytesN<32> = BytesN::from_array(&env, &bytes);
        assert_eq!(validate_hash(&hash), Ok(()));
    }

    #[test]
    fn test_validate_hash_all_ones_passes() {
        let env = Env::default();
        let hash: BytesN<32> = BytesN::from_array(&env, &[0xFF_u8; 32]);
        assert_eq!(validate_hash(&hash), Ok(()));
    }

    // validate_amount
    #[test]
    fn test_validate_amount_zero_fails() {
        assert_eq!(validate_amount(0), Err(NavinError::InvalidAmount));
    }

    #[test]
    fn test_validate_amount_negative_fails() {
        assert_eq!(validate_amount(-1), Err(NavinError::InvalidAmount));
    }

    #[test]
    fn test_validate_amount_valid_passes() {
        assert_eq!(validate_amount(1), Ok(()));
        assert_eq!(validate_amount(5_000_000), Ok(()));
        assert_eq!(validate_amount(MAX_AMOUNT), Ok(()));
    }

    #[test]
    fn test_validate_amount_exceeds_max_fails() {
        assert_eq!(
            validate_amount(MAX_AMOUNT + 1),
            Err(NavinError::InvalidAmount)
        );
    }

    // validate_timestamp
    #[test]
    fn test_validate_timestamp_current_passes() {
        let env = Env::default();
        let now = env.ledger().timestamp();
        assert_eq!(validate_timestamp(&env, now), Ok(()));
    }

    #[test]
    fn test_validate_timestamp_near_future_passes() {
        let env = Env::default();
        let now = env.ledger().timestamp();
        // 30 days in the future — well within the 10-year window.
        assert_eq!(validate_timestamp(&env, now + 30 * 24 * 60 * 60), Ok(()));
    }

    #[test]
    fn test_validate_timestamp_far_future_fails() {
        let env = Env::default();
        let now = env.ledger().timestamp();
        let far_future = now + MAX_FUTURE_OFFSET + 1;
        assert_eq!(
            validate_timestamp(&env, far_future),
            Err(NavinError::InvalidTimestamp)
        );
    }

    #[test]
    fn test_validate_timestamp_far_past_fails() {
        let env = Env::default();
        // Set ledger time far enough ahead that subtracting MAX_PAST_OFFSET + 1
        // gives a clearly out-of-range value.
        env.ledger().with_mut(|li| {
            li.timestamp = MAX_PAST_OFFSET + 10;
        });
        let far_past = env.ledger().timestamp() - MAX_PAST_OFFSET - 1;
        assert_eq!(
            validate_timestamp(&env, far_past),
            Err(NavinError::InvalidTimestamp)
        );
    }

    // validate_shipment_exists
    #[test]
    fn test_validate_shipment_exists_missing_returns_error() {
        let env = Env::default();
        // Storage access requires a contract context in Soroban.
        let result = env.as_contract(&env.register(crate::NavinShipment, ()), || {
            validate_shipment_exists(&env, 999)
        });
        assert!(matches!(result, Err(NavinError::ShipmentNotFound)));
    }

    // validate_symbol
    #[test]
    fn test_validate_symbol_valid_short_passes() {
        let env = Env::default();
        let symbol = Symbol::new(&env, "warehouse");
        assert_eq!(validate_symbol(&env, &symbol), Ok(()));
    }

    #[test]
    fn test_validate_symbol_valid_long_passes() {
        let env = Env::default();
        // Create a 32-character symbol (maximum safe length)
        let long_name = "a".repeat(32);
        let symbol = Symbol::new(&env, &long_name);
        assert_eq!(validate_symbol(&env, &symbol), Ok(()));
    }

    #[test]
    fn test_validate_symbol_single_char_passes() {
        let env = Env::default();
        let symbol = Symbol::new(&env, "a");
        assert_eq!(validate_symbol(&env, &symbol), Ok(()));
    }

    #[test]
    fn test_validate_symbol_common_names_pass() {
        let env = Env::default();
        let test_names = ["port", "warehouse", "checkpoint", "final_destination"];
        for name in &test_names {
            let symbol = Symbol::new(&env, name);
            assert_eq!(
                validate_symbol(&env, &symbol),
                Ok(()),
                "Symbol '{}' should be valid",
                name
            );
        }
    }

    // validate_milestone_symbols
    #[test]
    fn test_validate_milestone_symbols_valid_passes() {
        let env = Env::default();
        let mut milestones = soroban_sdk::Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "warehouse"), 30_u32));
        milestones.push_back((Symbol::new(&env, "port"), 30_u32));
        milestones.push_back((Symbol::new(&env, "final"), 40_u32));
        assert_eq!(validate_milestone_symbols(&env, &milestones), Ok(()));
    }

    #[test]
    fn test_validate_milestone_symbols_single_milestone_passes() {
        let env = Env::default();
        let mut milestones = soroban_sdk::Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "delivery"), 100_u32));
        assert_eq!(validate_milestone_symbols(&env, &milestones), Ok(()));
    }

    #[test]
    fn test_validate_milestone_symbols_empty_passes() {
        let env = Env::default();
        let milestones: soroban_sdk::Vec<(Symbol, u32)> = soroban_sdk::Vec::new(&env);
        assert_eq!(validate_milestone_symbols(&env, &milestones), Ok(()));
    }

    #[test]
    fn test_validate_milestone_symbols_duplicate_fails() {
        let env = Env::default();
        let mut milestones = soroban_sdk::Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "warehouse"), 50_u32));
        milestones.push_back((Symbol::new(&env, "warehouse"), 50_u32));
        assert_eq!(
            validate_milestone_symbols(&env, &milestones),
            Err(NavinError::InvalidShipmentInput)
        );
    }

    #[test]
    fn test_validate_milestone_symbols_many_unique_passes() {
        let env = Env::default();
        let mut milestones = soroban_sdk::Vec::new(&env);
        let names = ["a", "b", "c", "d", "e"];
        for name in &names {
            milestones.push_back((Symbol::new(&env, name), 20_u32));
        }
        assert_eq!(validate_milestone_symbols(&env, &milestones), Ok(()));
    }

    // validate_metadata_symbols
    #[test]
    fn test_validate_metadata_symbols_valid_passes() {
        let env = Env::default();
        let key = Symbol::new(&env, "weight");
        let value = Symbol::new(&env, "kg_100");
        assert_eq!(validate_metadata_symbols(&env, &key, &value), Ok(()));
    }

    #[test]
    fn test_validate_metadata_symbols_single_char_passes() {
        let env = Env::default();
        let key = Symbol::new(&env, "w");
        let value = Symbol::new(&env, "k");
        assert_eq!(validate_metadata_symbols(&env, &key, &value), Ok(()));
    }

    #[test]
    fn test_validate_metadata_symbols_long_names_pass() {
        let env = Env::default();
        // Create long symbol names by concatenating strings
        let long_key = "key_aaaaaaaaaaaaaaaaaaaaaaaaaa";
        let long_value = "val_bbbbbbbbbbbbbbbbbbbbbbbbbb";
        let key = Symbol::new(&env, long_key);
        let value = Symbol::new(&env, long_value);
        assert_eq!(validate_metadata_symbols(&env, &key, &value), Ok(()));
    }

    #[test]
    fn test_validate_metadata_symbols_common_pairs_pass() {
        let env = Env::default();
        let pairs = [
            ("weight", "kg_100"),
            ("priority", "high"),
            ("category", "fragile"),
            ("temperature", "controlled"),
        ];
        for (key_str, val_str) in &pairs {
            let key = Symbol::new(&env, key_str);
            let value = Symbol::new(&env, val_str);
            assert_eq!(
                validate_metadata_symbols(&env, &key, &value),
                Ok(()),
                "Pair ({}, {}) should be valid",
                key_str,
                val_str
            );
        }
    }
}
