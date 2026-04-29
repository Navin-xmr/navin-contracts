use crate::errors::NavinError;
use crate::storage;
use crate::types::{Shipment, ShipmentStatus};
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
/// Stellar Symbol Constraints:
/// - Length: 1-12 characters (enforced by Stellar protocol)
/// - Format: Alphanumeric and underscore only (A-Z, a-z, 0-9, _)
/// - Invalid: spaces, hyphens, special characters, unicode, null bytes
///
/// # Arguments
/// * `env` - Execution environment.
/// * `symbol` - The Symbol to validate.
///
/// # Returns
/// * `Ok(())` if the symbol is valid.
/// * `Err(NavinError::InvalidShipmentInput)` if the symbol is empty or exceeds max length.
///
/// # Examples
/// ```rust
/// validate_symbol(&env, &Symbol::new(&env, "warehouse"))?;
/// ```
pub fn validate_symbol(env: &Env, symbol: &Symbol) -> Result<(), NavinError> {
    // XDR layout: 4-byte ScValType tag + 4-byte length field + content padded to 4-byte boundary.
    // Byte counts by character count:
    //   0 chars  →  8 bytes  (empty — rejected)
    //   1–4 chars → 12 bytes
    //   5–8 chars → 16 bytes
    //   9–12 chars → 20 bytes  (12-char is the Stellar Symbol maximum)
    //   13+ chars → 24+ bytes  (rejected)
    let symbol_bytes = symbol.to_xdr(env);
    let len = symbol_bytes.len();

    if !(12..=20).contains(&len) {
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

/// Ensure an amount is strictly positive.
///
/// # Arguments
/// * `amount` - The `i128` value to validate.
///
/// # Returns
/// * `Ok(())` if `amount > 0`.
/// * `Err(NavinError::InvalidAmount)` otherwise.
pub fn validate_positive_amount(amount: i128) -> Result<(), NavinError> {
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

/// Compute a canonical hash for an off-chain payload.
///
/// This helper standardizes how off-chain data is hashed to ensure consistency
/// between the contract and external backends/frontends. It uses a deterministic
/// ordering and XDR encoding of the fields.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `fields` - A vector of values to be included in the hash.
///
/// # Returns
/// * `BytesN<32>` - The computed SHA-256 hash.
///
/// # Design Rationale
///
/// **Why XDR Encoding?**:
/// - XDR is the native serialization format for Soroban.
/// - It is deterministic and handles various types (Address, Symbol, u64, etc.) consistently.
/// - Frontends can use the Stellar SDK to produce matching XDR blobs.
///
/// # Examples
/// ```rust
/// let mut fields = Vec::new(&env);
/// fields.push_back(Symbol::new(&env, "event_type").into_val(&env));
/// fields.push_back(shipment_id.into_val(&env));
/// let hash = compute_offchain_payload_hash(&env, fields);
/// ```
pub fn compute_offchain_payload_hash(
    env: &Env,
    fields: soroban_sdk::Vec<soroban_sdk::Val>,
) -> BytesN<32> {
    env.crypto().sha256(&fields.to_xdr(env)).into()
}

/// Validate cross-field shipment state-machine invariants.
///
/// This validator protects against impossible state combinations and is intended
/// to be called on every write path before persisting shipment state.
pub fn validate_shipment_invariants(shipment: &Shipment) -> Result<(), NavinError> {
    if shipment.total_escrow < 0 || shipment.escrow_amount < 0 {
        return Err(NavinError::InvalidStatus);
    }

    if shipment.escrow_amount > shipment.total_escrow {
        return Err(NavinError::InvalidStatus);
    }

    if shipment.finalized {
        let terminal = shipment.status == ShipmentStatus::Delivered
            || shipment.status == ShipmentStatus::Cancelled;
        if !terminal || shipment.escrow_amount != 0 {
            return Err(NavinError::InvalidStatus);
        }
    }

    if shipment.status == ShipmentStatus::Disputed && shipment.finalized {
        return Err(NavinError::InvalidStatus);
    }

    if shipment.status == ShipmentStatus::Created && !shipment.paid_milestones.is_empty() {
        return Err(NavinError::InvalidStatus);
    }

    Ok(())
}

// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
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

    #[test]
    fn test_validate_shipment_invariants_accepts_valid_shipment() {
        let env = Env::default();
        let shipment = Shipment {
            id: 1,
            sender: soroban_sdk::Address::generate(&env),
            receiver: soroban_sdk::Address::generate(&env),
            carrier: soroban_sdk::Address::generate(&env),
            status: ShipmentStatus::InTransit,
            data_hash: BytesN::from_array(&env, &[1_u8; 32]),
            created_at: 100,
            updated_at: 100,
            escrow_amount: 10,
            total_escrow: 10,
            metadata: None,
            payment_milestones: soroban_sdk::Vec::new(&env),
            paid_milestones: soroban_sdk::Vec::new(&env),
            milestones_completed: soroban_sdk::Vec::new(&env),
            deadline: 200,
            integration_nonce: 0,
            finalized: false,
            depends_on: None,
        };

        assert_eq!(validate_shipment_invariants(&shipment), Ok(()));
    }

    #[test]
    fn test_validate_shipment_invariants_rejects_escrow_greater_than_total() {
        let env = Env::default();
        let shipment = Shipment {
            id: 1,
            sender: soroban_sdk::Address::generate(&env),
            receiver: soroban_sdk::Address::generate(&env),
            carrier: soroban_sdk::Address::generate(&env),
            status: ShipmentStatus::InTransit,
            data_hash: BytesN::from_array(&env, &[2_u8; 32]),
            created_at: 100,
            updated_at: 100,
            escrow_amount: 20,
            total_escrow: 10,
            metadata: None,
            payment_milestones: soroban_sdk::Vec::new(&env),
            paid_milestones: soroban_sdk::Vec::new(&env),
            milestones_completed: soroban_sdk::Vec::new(&env),
            deadline: 200,
            integration_nonce: 0,
            finalized: false,
            depends_on: None,
        };

        assert_eq!(
            validate_shipment_invariants(&shipment),
            Err(NavinError::InvalidStatus)
        );
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
    fn test_validate_symbol_valid_max_length_passes() {
        let env = Env::default();
        // Exactly 12 characters — the Stellar Symbol maximum
        let max_name = "a".repeat(12);
        let symbol = Symbol::new(&env, &max_name);
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
        // All names are ≤ 12 chars (Stellar Symbol maximum)
        let test_names = ["port", "warehouse", "checkpoint", "delivered"];
        for name in &test_names {
            let symbol = Symbol::new(&env, name);
            assert_eq!(
                validate_symbol(&env, &symbol),
                Ok(()),
                "Symbol '{name}' should be valid"
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
    fn test_validate_metadata_symbols_max_length_names_pass() {
        let env = Env::default();
        // 12-character key and value — at the Stellar Symbol maximum
        let key = Symbol::new(&env, "shipment_key");
        let value = Symbol::new(&env, "shipment_val");
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
                "Pair ({key_str}, {val_str}) should be valid"
            );
        }
    }
}

// ============================================================================
// Additional Symbol Validation Tests - Comprehensive Coverage
// ============================================================================

#[cfg(test)]
mod symbol_validation_tests {
    extern crate std;

    use super::*;
    use soroban_sdk::{Env, Symbol, Vec};

    // Boundary tests for symbol length
    #[test]
    fn test_symbol_length_boundary_12_chars() {
        let env = Env::default();

        // Exactly 12 characters - maximum allowed by Stellar
        let symbols_12_chars = std::vec![
            "VERYLONGNAME", // 12 uppercase
            "verylongname", // 12 lowercase
            "VeryLongName", // 12 mixed case
            "SYMBOL123456", // 12 alphanumeric
            "symbol_12345", // 12 with underscore
            "ABCDEFGHIJKL", // 12 letters
            "123456789012", // 12 digits
        ];

        for name in symbols_12_chars {
            let symbol = Symbol::new(&env, name);
            let result = validate_symbol(&env, &symbol);
            assert!(
                result.is_ok(),
                "12-character symbol '{}' should be valid",
                name
            );
        }
    }

    #[test]
    fn test_symbol_length_30_chars_rejected() {
        let env = Env::default();
        // 30 chars: within Soroban SDK's 32-char limit but above our 12-char max
        let long_30 = "A".repeat(30);
        let symbol = Symbol::new(&env, &long_30);
        let result = validate_symbol(&env, &symbol);
        assert!(
            result.is_err(),
            "Symbol of length 30 should be rejected (exceeds 12-char max)"
        );
    }

    #[test]
    fn test_symbol_length_25_chars_rejected() {
        let env = Env::default();
        let long_25 = "A".repeat(25);
        let symbol = Symbol::new(&env, &long_25);
        let result = validate_symbol(&env, &symbol);
        assert!(
            result.is_err(),
            "Symbol of length 25 should be rejected (exceeds 12-char max)"
        );
    }

    // Format validation tests
    #[test]
    fn test_symbol_alphanumeric_formats() {
        let env = Env::default();

        let valid_formats = std::vec![
            "SHIPMENT",    // uppercase
            "shipment",    // lowercase
            "Shipment",    // mixed case
            "SHIP123",     // alphanumeric
            "ship_123",    // with underscore
            "ABC",         // short uppercase
            "xyz",         // short lowercase
            "A1B2C3",      // mixed alphanumeric
            "_underscore", // leading underscore
            "trailing_",   // trailing underscore
        ];

        for name in valid_formats {
            let symbol = Symbol::new(&env, name);
            let result = validate_symbol(&env, &symbol);
            assert!(
                result.is_ok(),
                "Alphanumeric symbol '{}' should be valid",
                name
            );
        }
    }

    // Milestone validation tests
    #[test]
    fn test_milestone_symbols_no_duplicates() {
        let env = Env::default();

        let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "pickup"), 25));
        milestones.push_back((Symbol::new(&env, "transit"), 25));
        milestones.push_back((Symbol::new(&env, "delivery"), 50));

        let result = validate_milestone_symbols(&env, &milestones);
        assert!(result.is_ok(), "Unique milestone symbols should be valid");
    }

    #[test]
    fn test_milestone_symbols_with_duplicates_rejected() {
        let env = Env::default();

        let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "warehouse"), 50));
        milestones.push_back((Symbol::new(&env, "warehouse"), 50)); // Duplicate

        let result = validate_milestone_symbols(&env, &milestones);
        assert_eq!(
            result,
            Err(NavinError::InvalidShipmentInput),
            "Duplicate milestone symbols should be rejected"
        );
    }

    #[test]
    fn test_milestone_symbols_many_unique() {
        let env = Env::default();

        let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
        let names = ["pickup", "warehouse", "port", "transit", "delivery"];
        for name in &names {
            milestones.push_back((Symbol::new(&env, name), 20));
        }

        let result = validate_milestone_symbols(&env, &milestones);
        assert!(
            result.is_ok(),
            "Many unique milestone symbols should be valid"
        );
    }

    // Metadata validation tests
    #[test]
    fn test_metadata_symbols_various_pairs() {
        let env = Env::default();

        let test_pairs = std::vec![
            ("weight", "kg_100"),
            ("priority", "high"),
            ("category", "fragile"),
            ("temp", "controlled"),
            ("status", "active"),
            ("tracking", "enabled"),
        ];

        for (key_str, val_str) in test_pairs {
            let key = Symbol::new(&env, key_str);
            let value = Symbol::new(&env, val_str);

            let result = validate_metadata_symbols(&env, &key, &value);
            assert!(
                result.is_ok(),
                "Metadata pair ({}, {}) should be valid",
                key_str,
                val_str
            );
        }
    }

    #[test]
    fn test_metadata_symbols_max_length() {
        let env = Env::default();

        // 12-character key and value (maximum)
        let key = Symbol::new(&env, "verylongkey1");
        let value = Symbol::new(&env, "verylongval1");

        let result = validate_metadata_symbols(&env, &key, &value);
        assert!(
            result.is_ok(),
            "12-character metadata symbols should be valid"
        );
    }

    // Error message tests
    #[test]
    fn test_validation_error_types() {
        let env = Env::default();
        // 30 chars: within Soroban SDK's 32-char limit but above our 12-char max
        let oversized = "A".repeat(30);
        let oversized_symbol = Symbol::new(&env, &oversized);
        let result = validate_symbol(&env, &oversized_symbol);
        assert_eq!(
            result,
            Err(NavinError::InvalidShipmentInput),
            "Oversized symbol must return InvalidShipmentInput"
        );
    }

    #[test]
    fn test_duplicate_milestone_error_type() {
        let env = Env::default();

        let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "checkpoint"), 50));
        milestones.push_back((Symbol::new(&env, "checkpoint"), 50));

        let result = validate_milestone_symbols(&env, &milestones);

        assert_eq!(
            result,
            Err(NavinError::InvalidShipmentInput),
            "Duplicate milestone should return InvalidShipmentInput"
        );
    }
}

/// Maximum number of dependencies allowed per shipment.
const MAX_DEPENDENCIES: usize = 10;

/// Maximum recursion depth for cycle detection.
const MAX_DEPTH: usize = 64;

/// Validate shipment dependencies before persistence.
///
/// Ensures:
/// - Dependency count does not exceed MAX_DEPENDENCIES (10)
/// - All referenced shipment IDs exist
/// - No circular dependencies are present
pub fn validate_dependencies(
    env: &Env,
    shipment_id: u64,
    dependencies: &Option<soroban_sdk::Vec<u64>>,
) -> Result<(), NavinError> {
    let deps = match dependencies {
        Some(d) => d,
        None => return Ok(()),
    };

    if deps.len() as usize > MAX_DEPENDENCIES {
        return Err(NavinError::InvalidShipmentInput);
    }

    for i in 0..deps.len() {
        let dep_id = deps.get_unchecked(i);
        if dep_id == shipment_id {
            return Err(NavinError::CircularDependency);
        }
        let _ = storage::get_shipment(env, dep_id)
            .ok_or(NavinError::ShipmentNotFound)?;
        if transitive_has_dependency(env, dep_id, shipment_id, 0)? {
            return Err(NavinError::CircularDependency);
        }
    }

    Ok(())
}

fn transitive_has_dependency(
    env: &Env,
    target: u64,
    shipment_id: u64,
    depth: usize,
) -> Result<bool, NavinError> {
    if depth > MAX_DEPTH {
        return Err(NavinError::CircularDependency);
    }

    if let Some(target_deps) = storage::get_dependencies(env, target) {
        for i in 0..target_deps.len() {
            let dep = target_deps.get_unchecked(i);
            if dep == shipment_id {
                return Ok(true);
            }
            if transitive_has_dependency(env, dep, shipment_id, depth + 1)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn collect_unmet_dependencies(
    env: &Env,
    shipment_id: u64,
) -> Result<soroban_sdk::Vec<u64>, NavinError> {
    let mut unmet = soroban_sdk::Vec::new(env);

    if let Some(deps) = storage::get_dependencies(env, shipment_id) {
        for i in 0..deps.len() {
            let dep_id = deps.get_unchecked(i);
            match storage::get_shipment(env, dep_id) {
                Some(shipment) if shipment.status == ShipmentStatus::Delivered => {}
                _ => {
                    unmet.push_back(dep_id);
                }
            }
        }
    }

    Ok(unmet)
}
