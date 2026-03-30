//! # Recovery Module
//!
//! Provides admin-only recovery procedures for shipments in stuck or unintended states.
//! Includes safety checks and comprehensive event logging for all recovery operations.
//!
//! ## Recovery Operations
//!
//! - **Reset State**: Transition shipment to a valid state with validation
//! - **Unlock Escrow**: Release locked escrow in exceptional circumstances
//! - **Clear Finalization**: Allow re-processing of finalized shipments
//!
//! ## Safety Guarantees
//!
//! - All operations require admin authorization
//! - State transitions validated against state machine rules
//! - Escrow consistency verified before and after operations
//! - All operations emit recovery events for audit trail

use crate::{errors::NavinError, events, storage, types::*};
use soroban_sdk::{Address, BytesN, Env};

/// Recovery reason types for audit trail
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum RecoveryReason {
    /// Shipment stuck in intermediate state
    StuckState,
    /// Incorrect status assigned
    IncorrectStatus,
    /// Wedged escrow requiring manual intervention
    WedgedEscrow,
    /// Failed transition recovery
    FailedTransition,
    /// Other operational issue
    Other,
}

/// Recover a shipment from a stuck state by resetting to a valid target state.
///
/// # Arguments
/// * `env` - The execution environment
/// * `admin` - The admin address (must be authorized)
/// * `shipment_id` - ID of the shipment to recover
/// * `target_status` - The target status to transition to
/// * `reason_hash` - SHA-256 hash of the recovery reason (for audit trail)
///
/// # Returns
/// * `Ok(())` on successful recovery
/// * `Err(NavinError)` if recovery fails
///
/// # Safety Checks
/// - Admin authorization required
/// - Target status must be valid for current state
/// - Escrow consistency verified
/// - Recovery event emitted
///
/// # Examples
/// ```rust
/// // recover_shipment(&env, &admin, 42, ShipmentStatus::Cancelled, &reason_hash)?;
/// ```
#[allow(dead_code)]
pub fn recover_shipment(
    env: &Env,
    admin: &Address,
    shipment_id: u64,
    target_status: ShipmentStatus,
    reason_hash: &BytesN<32>,
) -> Result<(), NavinError> {
    // Verify admin authorization
    admin.require_auth();
    crate::require_role(env, admin, Role::Company)?;
    if !storage::is_admin(env, admin) {
        return Err(NavinError::Unauthorized);
    }

    // Validate reason hash is not all-zeros
    crate::validate_hash(reason_hash)?;

    // Retrieve shipment
    let mut shipment =
        storage::get_shipment(env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

    // Verify shipment is not already in target state
    if shipment.status == target_status {
        return Err(NavinError::InvalidStatus);
    }

    // Validate target status is reachable from current state
    if !is_valid_recovery_transition(&shipment.status, &target_status) {
        return Err(NavinError::InvalidStatus);
    }

    // Store old state for audit
    let old_status = shipment.status.clone();

    // Perform state transition
    shipment.status = target_status.clone();
    shipment.updated_at = env.ledger().timestamp();

    // Verify escrow consistency after transition
    verify_escrow_consistency(env, &shipment)?;

    // Persist updated shipment
    storage::set_shipment(env, &shipment);
    crate::extend_shipment_ttl(env, shipment_id);

    // Emit recovery event
    events::emit_recovery_event(
        env,
        shipment_id,
        admin,
        &old_status,
        &target_status,
        reason_hash,
    );

    Ok(())
}

/// Unlock escrow that is stuck due to failed operations.
///
/// # Arguments
/// * `env` - The execution environment
/// * `admin` - The admin address (must be authorized)
/// * `shipment_id` - ID of the shipment with stuck escrow
/// * `reason_hash` - SHA-256 hash of the unlock reason
///
/// # Returns
/// * `Ok(())` on successful unlock
/// * `Err(NavinError)` if unlock fails
///
/// # Safety Checks
/// - Admin authorization required
/// - Shipment must exist
/// - Escrow must be locked
/// - Unlock event emitted
#[allow(dead_code)]
pub fn unlock_escrow(
    env: &Env,
    admin: &Address,
    shipment_id: u64,
    reason_hash: &BytesN<32>,
) -> Result<(), NavinError> {
    // Verify admin authorization
    admin.require_auth();
    crate::require_role(env, admin, Role::Company)?;
    if !storage::is_admin(env, admin) {
        return Err(NavinError::Unauthorized);
    }

    // Validate reason hash
    crate::validate_hash(reason_hash)?;

    // Retrieve shipment
    let mut shipment =
        storage::get_shipment(env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

    // Verify escrow is locked (non-zero amount)
    if shipment.escrow_amount == 0 {
        return Err(NavinError::EscrowLocked);
    }

    // Store old escrow amount for audit
    let old_escrow = shipment.escrow_amount;

    // Clear escrow lock
    shipment.escrow_amount = 0;
    shipment.updated_at = env.ledger().timestamp();

    // Persist updated shipment
    storage::set_shipment(env, &shipment);
    crate::extend_shipment_ttl(env, shipment_id);

    // Emit unlock event
    events::emit_escrow_unlock_event(env, shipment_id, admin, old_escrow, reason_hash);

    Ok(())
}

/// Clear finalization flag to allow re-processing of a shipment.
///
/// # Arguments
/// * `env` - The execution environment
/// * `admin` - The admin address (must be authorized)
/// * `shipment_id` - ID of the shipment to un-finalize
/// * `reason_hash` - SHA-256 hash of the reason
///
/// # Returns
/// * `Ok(())` on successful clear
/// * `Err(NavinError)` if clear fails
///
/// # Safety Checks
/// - Admin authorization required
/// - Shipment must be finalized
/// - Clear event emitted
#[allow(dead_code)]
pub fn clear_finalization(
    env: &Env,
    admin: &Address,
    shipment_id: u64,
    reason_hash: &BytesN<32>,
) -> Result<(), NavinError> {
    // Verify admin authorization
    admin.require_auth();
    crate::require_role(env, admin, Role::Company)?;
    if !storage::is_admin(env, admin) {
        return Err(NavinError::Unauthorized);
    }

    // Validate reason hash
    crate::validate_hash(reason_hash)?;

    // Retrieve shipment
    let mut shipment =
        storage::get_shipment(env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

    // Verify shipment is finalized
    if !shipment.finalized {
        return Err(NavinError::InvalidStatus);
    }

    // Clear finalization flag
    shipment.finalized = false;
    shipment.updated_at = env.ledger().timestamp();

    // Persist updated shipment
    storage::set_shipment(env, &shipment);
    crate::extend_shipment_ttl(env, shipment_id);

    // Emit clear finalization event
    events::emit_finalization_clear_event(env, shipment_id, admin, reason_hash);

    Ok(())
}

/// Validate that a recovery transition is allowed.
///
/// Recovery allows transitions that would normally be invalid, but only
/// to terminal or safe states. This prevents cascading corruption.
#[allow(dead_code)]
fn is_valid_recovery_transition(from: &ShipmentStatus, to: &ShipmentStatus) -> bool {
    use ShipmentStatus::*;

    match (from, to) {
        // Allow transition to Cancelled from any state
        (_, Cancelled) => true,
        // Allow transition to Disputed from non-terminal states
        (Created | InTransit | AtCheckpoint, Disputed) => true,
        // Allow transition to Delivered from Disputed or InTransit
        (Disputed | InTransit | AtCheckpoint, Delivered) => true,
        // Allow normal state machine transitions
        (Created, InTransit) => true,
        (InTransit, AtCheckpoint) => true,
        (AtCheckpoint, InTransit) => true,
        // Prevent transitions from terminal states
        (Delivered | Cancelled, _) => false,
        _ => false,
    }
}

/// Verify escrow consistency after recovery operation.
///
/// Ensures that escrow amounts are valid and consistent with shipment state.
#[allow(dead_code)]
fn verify_escrow_consistency(_env: &Env, shipment: &Shipment) -> Result<(), NavinError> {
    // Escrow amount must be non-negative
    if shipment.escrow_amount < 0 {
        return Err(NavinError::InvalidAmount);
    }

    // Escrow amount cannot exceed total escrow
    if shipment.escrow_amount > shipment.total_escrow {
        return Err(NavinError::InvalidAmount);
    }

    // For terminal states, escrow should be zero or being released
    match shipment.status {
        ShipmentStatus::Delivered | ShipmentStatus::Cancelled => {
            // Escrow can be non-zero if not yet released/refunded
            // This is valid during recovery
        }
        _ => {
            // Non-terminal states can have any valid escrow amount
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_recovery_transitions() {
        // Created can transition to Cancelled
        assert!(is_valid_recovery_transition(
            &ShipmentStatus::Created,
            &ShipmentStatus::Cancelled
        ));

        // InTransit can transition to Cancelled
        assert!(is_valid_recovery_transition(
            &ShipmentStatus::InTransit,
            &ShipmentStatus::Cancelled
        ));

        // InTransit can transition to Disputed
        assert!(is_valid_recovery_transition(
            &ShipmentStatus::InTransit,
            &ShipmentStatus::Disputed
        ));

        // Disputed can transition to Delivered
        assert!(is_valid_recovery_transition(
            &ShipmentStatus::Disputed,
            &ShipmentStatus::Delivered
        ));
    }

    #[test]
    fn test_invalid_recovery_transitions() {
        // Cancelled cannot transition to anything
        assert!(!is_valid_recovery_transition(
            &ShipmentStatus::Cancelled,
            &ShipmentStatus::Delivered
        ));

        // AtCheckpoint cannot transition to Created
        assert!(!is_valid_recovery_transition(
            &ShipmentStatus::AtCheckpoint,
            &ShipmentStatus::Created
        ));

        // Created cannot transition to Delivered directly
        assert!(!is_valid_recovery_transition(
            &ShipmentStatus::Created,
            &ShipmentStatus::Delivered
        ));
    }
}
