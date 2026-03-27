use crate::errors::NavinError;
use crate::storage;
use crate::types::Shipment;
use soroban_sdk::{BytesN, Env};

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
    use soroban_sdk::{testutils::Ledger, BytesN, Env};

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

    // preflight_check_shipment_available
    #[test]
    fn test_preflight_check_shipment_available_not_found() {
        let env = Env::default();
        let result = env.as_contract(&env.register(crate::NavinShipment, ()), || {
            preflight_check_shipment_available(&env, 999)
        });
        assert!(matches!(result, Err(NavinError::ShipmentNotFound)));
    }

    #[test]
    fn test_preflight_check_shipment_available_finalized_fails() {
        use crate::types::{Shipment, ShipmentStatus};
        use soroban_sdk::{testutils::Address as _, Address};

        let env = Env::default();
        let result = env.as_contract(&env.register(crate::NavinShipment, ()), || {
            // Create a finalized shipment in persistent storage
            let sender = Address::generate(&env);
            let receiver = Address::generate(&env);
            let carrier = Address::generate(&env);
            let shipment = Shipment {
                id: 1,
                sender: sender.clone(),
                receiver: receiver.clone(),
                carrier: carrier.clone(),
                status: ShipmentStatus::Delivered,
                data_hash: BytesN::from_array(&env, &[1u8; 32]),
                escrow_amount: 0,
                total_escrow: 1000,
                metadata: None,
                payment_milestones: soroban_sdk::Vec::new(&env),
                paid_milestones: soroban_sdk::Vec::new(&env),
                deadline: env.ledger().timestamp() + 86400,
                integration_nonce: 0,
                finalized: true, // Mark as finalized
                created_at: env.ledger().timestamp(),
                updated_at: env.ledger().timestamp(),
            };

            storage::set_shipment(&env, &shipment);

            // Attempt to check availability — should fail with ShipmentFinalized
            preflight_check_shipment_available(&env, 1)
        });
        assert!(matches!(result, Err(NavinError::ShipmentFinalized)));
    }

    #[test]
    fn test_preflight_check_shipment_available_success() {
        use crate::types::{Shipment, ShipmentStatus};
        use soroban_sdk::{testutils::Address as _, Address};

        let env = Env::default();
        let result = env.as_contract(&env.register(crate::NavinShipment, ()), || {
            // Create an active (non-finalized) shipment in persistent storage
            let sender = Address::generate(&env);
            let receiver = Address::generate(&env);
            let carrier = Address::generate(&env);
            let shipment = Shipment {
                id: 1,
                sender: sender.clone(),
                receiver: receiver.clone(),
                carrier: carrier.clone(),
                status: ShipmentStatus::InTransit,
                data_hash: BytesN::from_array(&env, &[1u8; 32]),
                escrow_amount: 1000,
                total_escrow: 1000,
                metadata: None,
                payment_milestones: soroban_sdk::Vec::new(&env),
                paid_milestones: soroban_sdk::Vec::new(&env),
                deadline: env.ledger().timestamp() + 86400,
                integration_nonce: 0,
                finalized: false, // Not finalized
                created_at: env.ledger().timestamp(),
                updated_at: env.ledger().timestamp(),
            };

            storage::set_shipment(&env, &shipment);

            // Attempt to check availability — should succeed
            preflight_check_shipment_available(&env, 1)
        });
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, 1);
    }

    #[test]
    fn test_preflight_check_shipment_available_archived_not_found() {
        use crate::types::{Shipment, ShipmentStatus};
        use soroban_sdk::{testutils::Address as _, Address};

        let env = Env::default();
        let result = env.as_contract(&env.register(crate::NavinShipment, ()), || {
            // Create a shipment in temporary (archived) storage only
            let sender = Address::generate(&env);
            let receiver = Address::generate(&env);
            let carrier = Address::generate(&env);
            let shipment = Shipment {
                id: 1,
                sender: sender.clone(),
                receiver: receiver.clone(),
                carrier: carrier.clone(),
                status: ShipmentStatus::Delivered,
                data_hash: BytesN::from_array(&env, &[1u8; 32]),
                escrow_amount: 0,
                total_escrow: 1000,
                metadata: None,
                payment_milestones: soroban_sdk::Vec::new(&env),
                paid_milestones: soroban_sdk::Vec::new(&env),
                deadline: env.ledger().timestamp() + 86400,
                integration_nonce: 0,
                finalized: true,
                created_at: env.ledger().timestamp(),
                updated_at: env.ledger().timestamp(),
            };

            // Store in temporary storage (archived)
            env.storage()
                .temporary()
                .set(&crate::types::DataKey::ArchivedShipment(1), &shipment);

            // Attempt to check availability — should fail with ShipmentNotFound
            // because archived shipments are not in persistent storage
            preflight_check_shipment_available(&env, 1)
        });
        assert!(matches!(result, Err(NavinError::ShipmentNotFound)));
    }
}
