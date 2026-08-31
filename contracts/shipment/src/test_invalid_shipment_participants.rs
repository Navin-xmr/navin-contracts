/// Tests for NavinError::InvalidShipmentParticipants (error code 57) — issue #579
///
/// Validates that create_shipment (single path) rejects parameter combinations
/// where sender, receiver, and carrier are not three distinct addresses, while
/// accepting distinct participants.
extern crate std;

use crate::{test::setup_shipment_env, NavinError, NavinShipmentClient};
use soroban_sdk::{testutils::Address as _, Address, BytesN};

// ── helpers ──────────────────────────────────────────────────────────────────

fn setup_company(
    env: &soroban_sdk::Env,
    client: &NavinShipmentClient,
    admin: &Address,
) -> Address {
    let company = Address::generate(env);
    client.add_company(admin, &company);
    company
}

// ── duplicate participant combinations ────────────────────────────────────────

#[test]
fn test_sender_equals_receiver_returns_error() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    let sender = setup_company(&env, &client, &admin);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let result = client.try_create_shipment(
        &sender, &sender, &carrier, &data_hash,
        &soroban_sdk::Vec::new(&env), &deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentParticipants)),
        "sender == receiver must return InvalidShipmentParticipants"
    );
}

#[test]
fn test_sender_equals_carrier_returns_error() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    let sender = setup_company(&env, &client, &admin);
    let receiver = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let result = client.try_create_shipment(
        &sender, &receiver, &sender, &data_hash,
        &soroban_sdk::Vec::new(&env), &deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentParticipants)),
        "sender == carrier must return InvalidShipmentParticipants"
    );
}

#[test]
fn test_receiver_equals_carrier_returns_error() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    let sender = setup_company(&env, &client, &admin);
    let shared = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let result = client.try_create_shipment(
        &sender, &shared, &shared, &data_hash,
        &soroban_sdk::Vec::new(&env), &deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentParticipants)),
        "receiver == carrier must return InvalidShipmentParticipants"
    );
}

#[test]
fn test_all_participants_same_returns_error() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    let sender = setup_company(&env, &client, &admin);
    let data_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let result = client.try_create_shipment(
        &sender, &sender, &sender, &data_hash,
        &soroban_sdk::Vec::new(&env), &deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentParticipants)),
        "sender == receiver == carrier must return InvalidShipmentParticipants"
    );
}

// ── valid participants ────────────────────────────────────────────────────────

#[test]
fn test_distinct_participants_succeeds() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    let sender = setup_company(&env, &client, &admin);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&sender, &carrier);
    let data_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let result = client.try_create_shipment(
        &sender, &receiver, &carrier, &data_hash,
        &soroban_sdk::Vec::new(&env), &deadline,
    );
    assert!(
        result.is_ok(),
        "shipment with distinct participants must create successfully"
    );
}

// ── discriminant ─────────────────────────────────────────────────────────────

#[test]
fn test_error_code_is_57() {
    assert_eq!(NavinError::InvalidShipmentParticipants as u32, 57);

    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    let sender = setup_company(&env, &client, &admin);
    let shared = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let result = client.try_create_shipment(
        &sender, &shared, &shared, &data_hash,
        &soroban_sdk::Vec::new(&env), &deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidShipmentParticipants)),
        "InvalidShipmentParticipants discriminant must be 57"
    );
}
