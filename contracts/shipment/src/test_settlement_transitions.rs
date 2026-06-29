#![cfg(test)]

use crate::test::*;
use crate::test_utils::dummy_hash;
use crate::types::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::Address;

/// Test that settlement state transitions are validated correctly.
#[test]
fn test_settlement_state_transitions_validation() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &dummy_hash(&env),
        &soroban_sdk::Vec::new(&env),
        &(env.ledger().timestamp() + 86400),
    );

    // Deposit escrow - creates settlement 1
    client.deposit_escrow(&company, &shipment_id, &1000);

    // Verify settlement transitioned from Pending to Completed
    let settlement = client.get_settlement(&1);
    assert_eq!(settlement.state, SettlementState::Completed);
    assert!(settlement.completed_at.is_some());
    assert!(settlement.error_code.is_none());

    // Verify no active settlement remains after completion
    assert!(client.get_active_settlement(&shipment_id).is_none());
}

/// Test settlement record timestamps are correctly set.
#[test]
fn test_settlement_timestamps() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &dummy_hash(&env),
        &soroban_sdk::Vec::new(&env),
        &(env.ledger().timestamp() + 86400),
    );

    let before_timestamp = env.ledger().timestamp();
    client.deposit_escrow(&company, &shipment_id, &5000);
    let after_timestamp = env.ledger().timestamp();

    let settlement = client.get_settlement(&1);

    // Verify timestamps are within expected range
    assert!(settlement.initiated_at >= before_timestamp);
    assert!(settlement.initiated_at <= after_timestamp);
    assert!(settlement.completed_at.is_some());
    let completed = settlement.completed_at.unwrap();
    assert!(completed >= settlement.initiated_at);
    assert!(completed <= after_timestamp);
}

/// Test that settlement records contain correct addresses.
#[test]
fn test_settlement_addresses() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &dummy_hash(&env),
        &soroban_sdk::Vec::new(&env),
        &(env.ledger().timestamp() + 86400),
    );

    // Test deposit: company → contract
    client.deposit_escrow(&company, &shipment_id, &1000);
    let deposit_settlement = client.get_settlement(&shipment_id);
    assert_eq!(deposit_settlement.from, company);
    assert_eq!(deposit_settlement.to, client.address);
    assert_eq!(deposit_settlement.operation, SettlementOperation::Deposit);

    // Test refund: contract → company
    client.refund_escrow(&company, &shipment_id);
    let refund_settlement = client.get_settlement(&2);
    assert_eq!(refund_settlement.from, client.address);
    assert_eq!(refund_settlement.to, company);
    assert_eq!(refund_settlement.operation, SettlementOperation::Refund);
}

/// Test that settlement counter increments correctly.
#[test]
fn test_settlement_counter_increments() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Initial count should be 0
    assert_eq!(client.get_settlement_count(), 0);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &dummy_hash(&env),
        &soroban_sdk::Vec::new(&env),
        &(env.ledger().timestamp() + 86400),
    );

    // After deposit, count should be 1
    client.deposit_escrow(&company, &shipment_id, &1000);
    assert_eq!(client.get_settlement_count(), 1);

    // After refund, count should be 2
    client.refund_escrow(&company, &shipment_id);
    assert_eq!(client.get_settlement_count(), 2);
}

/// Test that settlement IDs are unique and sequential.
#[test]
fn test_settlement_ids_unique_and_sequential() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &dummy_hash(&env),
        &soroban_sdk::Vec::new(&env),
        &(env.ledger().timestamp() + 86400),
    );

    // Create settlement 1: deposit
    client.deposit_escrow(&company, &shipment_id, &1000);

    // Transition to Delivered to allow release
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    // Create settlement 2: release
    client.release_escrow(&receiver, &shipment_id);

    // Verify IDs are sequential
    let settlement1 = client.get_settlement(&1);
    let settlement2 = client.get_settlement(&2);

    assert_eq!(settlement1.settlement_id, 1);
    assert_eq!(settlement2.settlement_id, 2);

    // Verify they're all associated with the same shipment
    assert_eq!(settlement1.shipment_id, shipment_id);
    assert_eq!(settlement2.shipment_id, shipment_id);

    // Verify operations are correct
    assert_eq!(settlement1.operation, SettlementOperation::Deposit);
    assert_eq!(settlement2.operation, SettlementOperation::Release);
}

/// Test that completed settlements cannot be cancelled.
#[test]
fn test_cannot_cancel_completed_settlement() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &dummy_hash(&env),
        &soroban_sdk::Vec::new(&env),
        &(env.ledger().timestamp() + 86400),
    );

    // Create a successful settlement
    client.deposit_escrow(&company, &shipment_id, &1000);

    // Verify no active settlement (it was completed and cleared) —
    // once completed, no pending settlement remains to be cancelled.
    assert!(
        client.get_active_settlement(&shipment_id).is_none(),
        "No active settlement must exist after a completed deposit"
    );

    // The settlement record itself must be in Completed state.
    let settlement = client.get_settlement(&1);
    assert_eq!(
        settlement.state,
        SettlementState::Completed,
        "Completed settlement must be in Completed state, never left in a cancellable pending state"
    );
}

/// Test that release operations create correct settlement records.
#[test]
fn test_release_settlement_record() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &dummy_hash(&env),
        &soroban_sdk::Vec::new(&env),
        &(env.ledger().timestamp() + 86400),
    );

    // Deposit escrow
    client.deposit_escrow(&company, &shipment_id, &10000);

    // Transition to Delivered
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    // Release escrow
    client.release_escrow(&receiver, &shipment_id);

    // Verify release settlement
    let release_settlement = client.get_settlement(&2);
    assert_eq!(release_settlement.operation, SettlementOperation::Release);
    assert_eq!(release_settlement.state, SettlementState::Completed);
    assert_eq!(release_settlement.amount, 10000);
    assert_eq!(release_settlement.from, client.address);
    assert_eq!(release_settlement.to, carrier);
    assert!(release_settlement.completed_at.is_some());
    assert!(release_settlement.error_code.is_none());
}

// ── Issue #437: Exhaustive shipment status transition matrix tests ────────────

/// All valid transitions from ShipmentStatus::Created.
#[test]
fn test_transition_matrix_from_created() {
    assert!(ShipmentStatus::Created.is_valid_transition(&ShipmentStatus::InTransit));
    assert!(ShipmentStatus::Created.is_valid_transition(&ShipmentStatus::Cancelled));
    assert!(ShipmentStatus::Created.is_valid_transition(&ShipmentStatus::Disputed));

    // Illegal jumps from Created must be rejected.
    assert!(!ShipmentStatus::Created.is_valid_transition(&ShipmentStatus::Delivered));
    assert!(!ShipmentStatus::Created.is_valid_transition(&ShipmentStatus::AtCheckpoint));
    assert!(!ShipmentStatus::Created.is_valid_transition(&ShipmentStatus::PartiallyDelivered));
    assert!(!ShipmentStatus::Created.is_valid_transition(&ShipmentStatus::PartiallyRefunded));
}

/// All valid transitions from ShipmentStatus::InTransit.
#[test]
fn test_transition_matrix_from_in_transit() {
    assert!(ShipmentStatus::InTransit.is_valid_transition(&ShipmentStatus::AtCheckpoint));
    assert!(ShipmentStatus::InTransit.is_valid_transition(&ShipmentStatus::PartiallyDelivered));
    assert!(ShipmentStatus::InTransit.is_valid_transition(&ShipmentStatus::Delivered));
    assert!(ShipmentStatus::InTransit.is_valid_transition(&ShipmentStatus::Disputed));
    assert!(ShipmentStatus::InTransit.is_valid_transition(&ShipmentStatus::Cancelled));

    // Illegal: cannot go back to Created.
    assert!(!ShipmentStatus::InTransit.is_valid_transition(&ShipmentStatus::Created));
    assert!(!ShipmentStatus::InTransit.is_valid_transition(&ShipmentStatus::PartiallyRefunded));
}

/// All valid transitions from ShipmentStatus::AtCheckpoint.
#[test]
fn test_transition_matrix_from_at_checkpoint() {
    assert!(ShipmentStatus::AtCheckpoint.is_valid_transition(&ShipmentStatus::InTransit));
    assert!(ShipmentStatus::AtCheckpoint.is_valid_transition(&ShipmentStatus::PartiallyDelivered));
    assert!(ShipmentStatus::AtCheckpoint.is_valid_transition(&ShipmentStatus::Delivered));
    assert!(ShipmentStatus::AtCheckpoint.is_valid_transition(&ShipmentStatus::Disputed));
    assert!(ShipmentStatus::AtCheckpoint.is_valid_transition(&ShipmentStatus::Cancelled));

    // Illegal: cannot jump back to Created.
    assert!(!ShipmentStatus::AtCheckpoint.is_valid_transition(&ShipmentStatus::Created));
}

/// Terminal state: Delivered must not allow forward transitions.
#[test]
fn test_transition_matrix_delivered_is_terminal() {
    assert!(!ShipmentStatus::Delivered.is_valid_transition(&ShipmentStatus::Created));
    assert!(!ShipmentStatus::Delivered.is_valid_transition(&ShipmentStatus::InTransit));
    assert!(!ShipmentStatus::Delivered.is_valid_transition(&ShipmentStatus::AtCheckpoint));
    assert!(!ShipmentStatus::Delivered.is_valid_transition(&ShipmentStatus::Disputed));
    assert!(!ShipmentStatus::Delivered.is_valid_transition(&ShipmentStatus::Cancelled));
    assert!(!ShipmentStatus::Delivered.is_valid_transition(&ShipmentStatus::PartiallyDelivered));
}

/// Terminal state: Cancelled must not allow further transitions.
#[test]
fn test_transition_matrix_cancelled_is_terminal() {
    assert!(!ShipmentStatus::Cancelled.is_valid_transition(&ShipmentStatus::Created));
    assert!(!ShipmentStatus::Cancelled.is_valid_transition(&ShipmentStatus::InTransit));
    assert!(!ShipmentStatus::Cancelled.is_valid_transition(&ShipmentStatus::Delivered));
    assert!(!ShipmentStatus::Cancelled.is_valid_transition(&ShipmentStatus::Disputed));
}

/// Disputed may resolve to Cancelled or Delivered only.
#[test]
fn test_transition_matrix_from_disputed() {
    assert!(ShipmentStatus::Disputed.is_valid_transition(&ShipmentStatus::Cancelled));
    assert!(ShipmentStatus::Disputed.is_valid_transition(&ShipmentStatus::Delivered));

    // Cannot return to pre-dispute states.
    assert!(!ShipmentStatus::Disputed.is_valid_transition(&ShipmentStatus::Created));
    assert!(!ShipmentStatus::Disputed.is_valid_transition(&ShipmentStatus::InTransit));
    assert!(!ShipmentStatus::Disputed.is_valid_transition(&ShipmentStatus::AtCheckpoint));
}

/// PartiallyDelivered chain: can continue delivering, dispute, or cancel.
#[test]
fn test_transition_matrix_from_partially_delivered() {
    assert!(
        ShipmentStatus::PartiallyDelivered.is_valid_transition(&ShipmentStatus::PartiallyDelivered)
    );
    assert!(ShipmentStatus::PartiallyDelivered.is_valid_transition(&ShipmentStatus::Delivered));
    assert!(ShipmentStatus::PartiallyDelivered.is_valid_transition(&ShipmentStatus::Disputed));
    assert!(ShipmentStatus::PartiallyDelivered.is_valid_transition(&ShipmentStatus::Cancelled));

    // Illegal: cannot jump back to Created or InTransit.
    assert!(!ShipmentStatus::PartiallyDelivered.is_valid_transition(&ShipmentStatus::Created));
    assert!(!ShipmentStatus::PartiallyDelivered.is_valid_transition(&ShipmentStatus::InTransit));
}

/// Regression guard: the full matrix produces no unexpected true/false flips.
/// This test enumerates every (from, to) pair so any change to `is_valid_transition`
/// immediately breaks this test, forcing the author to update it intentionally.
#[test]
fn test_transition_matrix_exhaustive_regression() {
    use ShipmentStatus::*;
    let all = [
        Created,
        InTransit,
        AtCheckpoint,
        PartiallyDelivered,
        Delivered,
        Disputed,
        Cancelled,
        PartiallyRefunded,
    ];

    let expected_valid: &[(ShipmentStatus, ShipmentStatus)] = &[
        (Created, InTransit),
        (Created, Cancelled),
        (Created, Disputed),
        (InTransit, AtCheckpoint),
        (InTransit, PartiallyDelivered),
        (InTransit, Delivered),
        (InTransit, Disputed),
        (InTransit, Cancelled),
        (AtCheckpoint, InTransit),
        (AtCheckpoint, PartiallyDelivered),
        (AtCheckpoint, Delivered),
        (AtCheckpoint, Disputed),
        (AtCheckpoint, Cancelled),
        (PartiallyDelivered, PartiallyDelivered),
        (PartiallyDelivered, Delivered),
        (PartiallyDelivered, Disputed),
        (PartiallyDelivered, Cancelled),
        (Disputed, Cancelled),
        (Disputed, Delivered),
        (Disputed, PartiallyRefunded),
    ];

    for from in &all {
        for to in &all {
            let result = from.is_valid_transition(to);
            let is_expected = expected_valid.iter().any(|(f, t)| f == from && t == to);
            // The catch-all rules also allow any non-Delivered status -> Cancelled
            // and any non-Cancelled/non-Delivered status -> Disputed.
            // Terminal states (Delivered, Cancelled, PartiallyRefunded) cannot transition out.
            let catch_all = (to == &Cancelled
                && from != &Delivered
                && from != &Cancelled
                && from != &PartiallyRefunded)
                || (to == &Disputed
                    && from != &Cancelled
                    && from != &Delivered
                    && from != &PartiallyRefunded
                    && from != &Disputed);
            let should_be_valid = is_expected || catch_all;
            assert_eq!(
                result, should_be_valid,
                "Unexpected transition result {:?} -> {:?}: got {}, expected {}",
                from, to, result, should_be_valid
            );
        }
    }
}

/// Test that failed operations roll back completely.
#[test]
fn test_failed_operation_rollback() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env_with_failing_token();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &dummy_hash(&env),
        &soroban_sdk::Vec::new(&env),
        &(env.ledger().timestamp() + 86400),
    );

    // Attempt deposit - should fail and roll back
    let result = client.try_deposit_escrow(&company, &shipment_id, &1000);
    assert!(result.is_err());

    // Verify no settlement was persisted
    assert_eq!(client.get_settlement_count(), 0);
    assert!(client.get_active_settlement(&shipment_id).is_none());

    // Verify shipment state unchanged
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 0);
    assert_eq!(shipment.status, ShipmentStatus::Created);
}

#[test]
fn test_valid_and_invalid_transitions_guard_boundary() {
    use crate::validate_shipment_transition;

    // Valid transitions
    assert!(
        validate_shipment_transition(&ShipmentStatus::Created, &ShipmentStatus::InTransit).is_ok()
    );
    assert!(
        validate_shipment_transition(&ShipmentStatus::InTransit, &ShipmentStatus::Delivered)
            .is_ok()
    );

    // Invalid transition
    let err = validate_shipment_transition(&ShipmentStatus::Created, &ShipmentStatus::Delivered)
        .unwrap_err();
    assert_eq!(err, crate::NavinError::InvalidStatus);

    // Terminal state transition (should be invalid)
    let err = validate_shipment_transition(&ShipmentStatus::Delivered, &ShipmentStatus::InTransit)
        .unwrap_err();
    assert_eq!(err, crate::NavinError::InvalidStatus);
}
