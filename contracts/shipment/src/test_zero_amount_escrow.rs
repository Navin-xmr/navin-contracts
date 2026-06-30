/// Tests for zero-amount escrow rejection.
///
/// This module ensures that zero-amount and negative-amount escrow operations
/// are rejected consistently across all escrow call paths, preventing silent
/// acceptance of invalid amounts and maintaining bounded, predictable behavior.
use crate::{NavinError, NavinShipment, NavinShipmentClient, ShipmentStatus};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

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
    assert_eq!(result, Err(Ok(NavinError::InsufficientFunds)));
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
    assert_eq!(result, Err(Ok(NavinError::InsufficientFunds)));
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
        assert_eq!(result, Err(Ok(NavinError::InsufficientFunds)));
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
    let deadline = env.ledger().timestamp() + 3600;

    // Create multiple shipments
    let shipment_1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &Vec::new(&env),
        &deadline,
    );

    let shipment_2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[2u8; 32]),
        &Vec::new(&env),
        &deadline,
    );

    // Zero-amount should be rejected for all shipments
    let result_1 = client.try_deposit_escrow(&company, &shipment_1, &0);
    let result_2 = client.try_deposit_escrow(&company, &shipment_2, &0);

    assert_eq!(result_1, Err(Ok(NavinError::InsufficientFunds)));
    assert_eq!(result_2, Err(Ok(NavinError::InsufficientFunds)));
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

// ── Issue #549: duplicate refund rejection ───────────────────────────────────

/// Trigger a successful refund then immediately attempt a second refund on the
/// same shipment. The second call must be blocked because the escrow balance
/// was zeroed out by the first refund.
#[test]
fn test_refund_escrow_already_refunded_is_blocked() {
    let (env, client, _admin, company, receiver, carrier) = setup_escrow_env();
    let data_hash = BytesN::from_array(&env, &[0x10u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Deposit a positive escrow amount so the first refund succeeds.
    client.deposit_escrow(&company, &shipment_id, &1_000i128);

    let shipment_before = client.get_shipment(&shipment_id);
    assert_eq!(shipment_before.escrow_amount, 1_000i128);

    // First refund — must succeed.
    let first = client.try_refund_escrow(&company, &shipment_id);
    assert!(
        first.is_ok(),
        "first refund on a shipment with positive escrow must succeed"
    );

    // After refund the escrow balance is zero.
    let shipment_after = client.get_shipment(&shipment_id);
    assert_eq!(
        shipment_after.escrow_amount, 0,
        "escrow_amount must be zero after a successful refund"
    );

    // Second refund on the same shipment — must be blocked.
    // After a successful refund the shipment is finalized (Cancelled + escrow=0),
    // so the guard returns ShipmentFinalized before the balance check.
    let second = client.try_refund_escrow(&company, &shipment_id);
    assert!(
        second.is_err(),
        "second refund on an already-refunded shipment must be rejected"
    );
    assert_eq!(
        second,
        Err(Ok(NavinError::ShipmentFinalized)),
        "already-refunded shipment must be blocked by the finalized guard"
    );
}

/// Admin-triggered duplicate refund must also be blocked.
#[test]
fn test_admin_refund_already_refunded_is_blocked() {
    let (env, client, admin, company, receiver, carrier) = setup_escrow_env();
    let data_hash = BytesN::from_array(&env, &[0x11u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    client.deposit_escrow(&company, &shipment_id, &500i128);

    // Admin performs the first refund.
    let first = client.try_refund_escrow(&admin, &shipment_id);
    assert!(
        first.is_ok(),
        "admin first refund must succeed when escrow is positive"
    );

    // Admin attempts a second refund — must fail.
    let second = client.try_refund_escrow(&admin, &shipment_id);
    assert!(
        second.is_err(),
        "admin second refund on already-refunded shipment must be rejected"
    );
    assert_eq!(
        second,
        Err(Ok(NavinError::ShipmentFinalized)),
        "repeat admin refund must be blocked by the finalized guard"
    );
}

/// Three consecutive refund attempts must all fail after the first succeeds —
/// verifying the guard holds across multiple repeated calls.
#[test]
fn test_repeated_refund_attempts_all_blocked_after_first() {
    let (env, client, _admin, company, receiver, carrier) = setup_escrow_env();
    let data_hash = BytesN::from_array(&env, &[0x12u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    client.deposit_escrow(&company, &shipment_id, &2_000i128);

    // First call succeeds.
    assert!(client.try_refund_escrow(&company, &shipment_id).is_ok());

    // Subsequent calls must all fail.
    for _ in 0..3 {
        let result = client.try_refund_escrow(&company, &shipment_id);
        assert!(
            result.is_err(),
            "every repeat refund attempt must be rejected"
        );
        assert_eq!(result, Err(Ok(NavinError::ShipmentFinalized)));
    }
}
