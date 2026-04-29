#![cfg(test)]

use crate::test::*;
use crate::test_utils::{dummy_hash};
use crate::types::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address};

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
        &None,
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
        &None,
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
        &None,
    );

    // Test deposit: company → contract
    client.deposit_escrow(&company, &shipment_id, &1000);
    let deposit_settlement = client.get_settlement(&1);
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
        &None,
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
        &None,
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
        &None,
    );

    // Create a successful settlement
    client.deposit_escrow(&company, &shipment_id, &1000);

    // Verify no active settlement (it was completed and cleared)
    assert!(client.get_active_settlement(&shipment_id).is_none());

    // Attempting to cancel should fail (no active settlement)
    let result = client.try_cancel_active_settlement(&company, &shipment_id);
    assert!(result.is_err());
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
        &None,
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
        &None,
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


