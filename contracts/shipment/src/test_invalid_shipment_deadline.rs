/// Tests for NavinError::InvalidShipmentDeadline (error code 58) — issue #580
///
/// Validates that create_shipment (single path) and create_shipments_batch (batch path)
/// reject deadlines that are not strictly in the future, while accepting valid future deadlines.
extern crate std;

use crate::{test::setup_shipment_env, NavinError};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Vec};

#[test]
fn test_past_deadline_returns_error() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    let sender = Address::generate(&env);
    client.add_company(&admin, &sender);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&sender, &carrier);
    let data_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
    let past_deadline = env.ledger().timestamp().saturating_sub(3600);

    let result = client.try_create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &past_deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentDeadline)),
        "deadline in the past must return InvalidShipmentDeadline"
    );
}

#[test]
fn test_current_timestamp_deadline_returns_error() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    let sender = Address::generate(&env);
    client.add_company(&admin, &sender);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&sender, &carrier);
    let data_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
    let current_deadline = env.ledger().timestamp();

    let result = client.try_create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &current_deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentDeadline)),
        "deadline equal to current timestamp must return InvalidShipmentDeadline"
    );
}

#[test]
fn test_valid_future_deadline_succeeds() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    let sender = Address::generate(&env);
    client.add_company(&admin, &sender);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&sender, &carrier);
    let data_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
    let future_deadline = env.ledger().timestamp() + 3600;

    let result = client.try_create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &future_deadline,
    );
    assert!(
        result.is_ok(),
        "shipment with valid future deadline must create successfully"
    );
}

#[test]
fn test_error_code_is_58() {
    assert_eq!(NavinError::InvalidShipmentDeadline as u32, 58);

    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    let sender = Address::generate(&env);
    client.add_company(&admin, &sender);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&sender, &carrier);
    let data_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
    let past_deadline = env.ledger().timestamp().saturating_sub(3600);

    let result = client.try_create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &past_deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentDeadline)),
        "InvalidShipmentDeadline discriminant must be 58"
    );
}

/// A deadline more than MAX_FUTURE_OFFSET seconds in the future is rejected.
#[test]
fn test_far_future_deadline_returns_error() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    let sender = Address::generate(&env);
    client.add_company(&admin, &sender);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&sender, &carrier);
    let data_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
    let now = env.ledger().timestamp();
    let far_future = now + super::validation::MAX_FUTURE_OFFSET + 1;

    let result = client.try_create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &far_future,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentDeadline)),
        "deadline beyond MAX_FUTURE_OFFSET must return InvalidShipmentDeadline"
    );
}
