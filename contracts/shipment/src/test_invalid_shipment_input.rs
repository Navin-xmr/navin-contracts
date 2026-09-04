/// Tests for NavinError::InvalidShipmentInput (error code 17) — issue #614
///
/// Validates that create_shipments_batch rejects parameter combinations where
/// receiver equals carrier, while accepting distinct participant addresses.
extern crate std;

use crate::{test::setup_shipment_env, NavinError, ShipmentInput};
use soroban_sdk::{testutils::Address as _, Address, BytesN};

// ── helpers ──────────────────────────────────────────────────────────────────

fn make_input(
    env: &soroban_sdk::Env,
    receiver: &Address,
    carrier: &Address,
    marker: u8,
) -> ShipmentInput {
    ShipmentInput {
        receiver: receiver.clone(),
        carrier: carrier.clone(),
        data_hash: BytesN::from_array(env, &[marker; 32]),
        payment_milestones: soroban_sdk::Vec::new(env),
        deadline: env.ledger().timestamp() + 7200,
    }
}

// ── invalid inputs ────────────────────────────────────────────────────────────

#[test]
fn test_batch_receiver_equals_carrier_returns_error() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shared = Address::generate(&env);
    let mut shipments = soroban_sdk::Vec::new(&env);
    shipments.push_back(make_input(&env, &shared, &shared, 1));

    let result = client.try_create_shipments_batch(&company, &shipments);
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentInput)),
        "receiver == carrier must return InvalidShipmentInput"
    );
}

#[test]
fn test_batch_second_entry_invalid_first_valid() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let r1 = Address::generate(&env);
    let c1 = Address::generate(&env);
    let shared = Address::generate(&env);

    let mut shipments = soroban_sdk::Vec::new(&env);
    shipments.push_back(make_input(&env, &r1, &c1, 10)); // valid
    shipments.push_back(make_input(&env, &shared, &shared, 20)); // invalid — receiver == carrier

    let result = client.try_create_shipments_batch(&company, &shipments);
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentInput)),
        "invalid entry in batch must propagate error even when preceding entries are valid"
    );
}

#[test]
fn test_batch_all_entries_invalid() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);

    let mut shipments = soroban_sdk::Vec::new(&env);
    shipments.push_back(make_input(&env, &s1, &s1, 30)); // receiver == carrier
    shipments.push_back(make_input(&env, &s2, &s2, 40)); // receiver == carrier

    let result = client.try_create_shipments_batch(&company, &shipments);
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentInput)),
        "all-invalid batch must return InvalidShipmentInput"
    );
}

// ── valid inputs ──────────────────────────────────────────────────────────────

#[test]
fn test_batch_distinct_participants_succeeds() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    let mut shipments = soroban_sdk::Vec::new(&env);
    shipments.push_back(make_input(&env, &receiver, &carrier, 50));

    let result = client.try_create_shipments_batch(&company, &shipments);
    assert!(
        result.is_ok(),
        "distinct receiver and carrier must create shipment successfully"
    );
}

#[test]
fn test_batch_multiple_valid_entries_all_succeed() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let mut shipments = soroban_sdk::Vec::new(&env);
    for i in 0u8..3 {
        let receiver = Address::generate(&env);
        let carrier = Address::generate(&env);
        shipments.push_back(make_input(&env, &receiver, &carrier, i + 60));
    }

    let ids = client.try_create_shipments_batch(&company, &shipments);
    assert!(ids.is_ok(), "all-valid batch must succeed");
    assert_eq!(ids.unwrap().unwrap().len(), 3);
}

#[test]
fn test_create_single_shipment_rejects_sender_receiver_duplicate() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[0xA1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let result = client.try_create_shipment(
        &company,
        &company,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentParticipants)),
        "sender == receiver must return InvalidShipmentParticipants"
    );
}

#[test]
fn test_create_single_shipment_rejects_sender_carrier_duplicate() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let receiver = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[0xA2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let company = Address::generate(&env);
    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let result = client.try_create_shipment(
        &company,
        &receiver,
        &company,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentParticipants)),
        "sender == carrier must return InvalidShipmentParticipants"
    );
}

#[test]
fn test_create_single_shipment_rejects_receiver_carrier_duplicate() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let shared = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[0xA3u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let result = client.try_create_shipment(
        &company,
        &shared,
        &shared,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentParticipants)),
        "receiver == carrier must return InvalidShipmentParticipants"
    );
}

#[test]
fn test_create_single_shipment_distinct_participants_succeeds() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    assert!(
        result.is_ok(),
        "single shipment with valid participants must succeed"
    );
}

#[test]
fn test_error_code_is_17() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shared = Address::generate(&env);
    let mut shipments = soroban_sdk::Vec::new(&env);
    shipments.push_back(make_input(&env, &shared, &shared, 99));

    let result = client.try_create_shipments_batch(&company, &shipments);
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentInput)),
        "InvalidShipmentInput discriminant must be 17"
    );
    assert_eq!(NavinError::InvalidShipmentInput as u32, 17);
}
