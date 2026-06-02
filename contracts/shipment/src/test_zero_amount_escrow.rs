/// Tests for zero-amount escrow rejection.
///
/// This module ensures that zero-amount and negative-amount escrow operations
/// are rejected consistently across all escrow call paths, preventing silent
/// acceptance of invalid amounts and maintaining bounded, predictable behavior.
use crate::{NavinError, NavinShipment, NavinShipmentClient, ShipmentStatus};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Symbol, Vec};

#[soroban_sdk::contract]
struct MockToken;

#[soroban_sdk::contractimpl]
impl MockToken {
    pub fn decimals(_env: soroban_sdk::Env) -> u32 {
        7
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

fn setup_escrow_env() -> (
    Env,
    NavinShipmentClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let (env, admin) = crate::test_utils::setup_env();
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));

    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    (env, client, admin, company, receiver, carrier)
}

/// Test that deposit_escrow with zero amount is rejected.
#[test]
fn test_deposit_escrow_zero_amount_rejected() {
    let (env, client, _admin, company, receiver, carrier) = setup_escrow_env();
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    // Create a shipment in Created status
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Verify shipment exists and is in Created status
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Created);
    assert_eq!(shipment.escrow_amount, 0);

    // Attempt to deposit zero amount - should fail
    let result = client.try_deposit_escrow(&company, &shipment_id, &0);
    assert!(result.is_err(), "Zero-amount deposit should be rejected");
}

/// Test that deposit_escrow with negative amount is rejected.
#[test]
fn test_deposit_escrow_negative_amount_rejected() {
    let (env, client, _admin, company, receiver, carrier) = setup_escrow_env();
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Attempt to deposit negative amount - should fail
    let result = client.try_deposit_escrow(&company, &shipment_id, &-100);
    assert!(
        result.is_err(),
        "Negative-amount deposit should be rejected"
    );
}

/// Test that positive amounts are accepted in deposit_escrow.
#[test]
fn test_deposit_escrow_positive_amount_accepted() {
    let (env, client, _admin, company, receiver, carrier) = setup_escrow_env();
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Deposit a positive amount - should succeed
    let amount = 1000i128;
    let result = client.try_deposit_escrow(&company, &shipment_id, &amount);
    assert!(result.is_ok(), "Positive-amount deposit should be accepted");

    // Verify escrow was stored
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, amount);
}

/// Test that no escrow validation is bypassed for helper-based calls.
/// This ensures the rejection stays consistent regardless of call path.
#[test]
fn test_deposit_escrow_zero_rejection_consistency() {
    let (env, client, _admin, company, receiver, carrier) = setup_escrow_env();
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Multiple attempts with zero - all should fail consistently
    for _ in 0..3 {
        let result = client.try_deposit_escrow(&company, &shipment_id, &0);
        assert!(
            result.is_err(),
            "Zero-amount rejection must stay consistent"
        );
    }
}

/// Test that exceeding MAX_AMOUNT is also rejected.
#[test]
fn test_deposit_escrow_exceeds_max_amount_rejected() {
    let (env, client, _admin, company, receiver, carrier) = setup_escrow_env();
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Try to deposit an amount exceeding MAX_AMOUNT
    let max_amount = 9_223_372_036_854_775_807i128; // i128::MAX
    let result = client.try_deposit_escrow(&company, &shipment_id, &max_amount);
    assert!(
        result.is_err(),
        "Amount exceeding MAX_AMOUNT should be rejected"
    );
}

/// Test that zero-amount boundary is respected across different escrow states.
#[test]
fn test_zero_amount_rejection_multiple_shipments() {
    let (env, client, _admin, company, receiver, carrier) = setup_escrow_env();
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    // Create multiple shipments
    let shipment_1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let shipment_2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Zero-amount should be rejected for all shipments
    let result_1 = client.try_deposit_escrow(&company, &shipment_1, &0);
    let result_2 = client.try_deposit_escrow(&company, &shipment_2, &0);

    assert!(result_1.is_err(), "Zero-amount rejected for shipment 1");
    assert!(result_2.is_err(), "Zero-amount rejected for shipment 2");
}

/// Test that release_escrow rejects zero-amount escrow even after shipment delivery.
#[test]
fn test_release_escrow_zero_escrow_rejected() {
    let (env, client, _admin, company, receiver, carrier) = setup_escrow_env();
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[2u8; 32]),
    );
    client.confirm_delivery(
        &receiver,
        &shipment_id,
        &BytesN::from_array(&env, &[3u8; 32]),
    );

    let result = client.try_release_escrow(&receiver, &shipment_id);
    assert!(result.is_err(), "Release should reject zero escrow amount");
}

/// Test that refund_escrow rejects zero-amount escrow in Created state.
#[test]
fn test_refund_escrow_zero_escrow_rejected() {
    let (env, client, _admin, company, _receiver, _carrier) = setup_escrow_env();
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let shipment_id = client.create_shipment(
        &company,
        &_receiver,
        &_carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let result = client.try_refund_escrow(&company, &shipment_id);
    assert!(result.is_err(), "Refund should reject zero escrow amount");
}
