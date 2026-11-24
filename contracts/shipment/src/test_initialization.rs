#![cfg(test)]

extern crate std;

use super::test_helpers::*;

#[test]
fn test_successful_initialization() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_shipment_counter(), 0);
    assert_eq!(client.get_version(), 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_re_initialization_fails() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);
    // Second call must fail with AlreadyInitialized (error code 1)
    client.initialize(&admin, &token_contract);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_re_initialization_with_different_admin_fails() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let other_admin = Address::generate(&env);
    // Attempting to re-initialize with a different admin must also fail
    client.initialize(&other_admin, &token_contract);
}

#[test]
fn test_shipment_counter_starts_at_zero() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    assert_eq!(client.get_shipment_counter(), 0);
}

#[test]
fn test_admin_is_stored_correctly() {
    let (env, client, _admin, token_contract) = setup_shipment_env();

    let specific_admin = Address::generate(&env);
    client.initialize(&specific_admin, &token_contract);

    let stored_admin = client.get_admin();
    assert_eq!(stored_admin, specific_admin);
}

#[test]
fn test_scaffold() {
    let env = Env::default();
    let _client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
}

#[test]
fn test_token_mint_helper_maps_failure() {
    let env = Env::default();
    env.mock_all_auths();

    let token_contract = env.register(failing_token::FailingMockToken {}, ());
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let result = super::invoke_token_mint(&env, &token_contract, &admin, &recipient, 250);

    assert_eq!(result, Err(crate::NavinError::TokenMintFailed));
}

#[test]
#[should_panic(expected = "Error(Contract, #16)")]
fn test_create_shipments_batch_oversized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let deadline = env.ledger().timestamp() + 3600;
    let mut shipments = soroban_sdk::Vec::new(&env);
    for i in 0..11 {
        shipments.push_back(ShipmentInput {
            receiver: Address::generate(&env),
            carrier: Address::generate(&env),
            data_hash: BytesN::from_array(&env, &[i as u8; 32]),
            payment_milestones: soroban_sdk::Vec::new(&env),
            deadline,
        });
    }

    client.create_shipments_batch(&company, &shipments);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_create_shipment_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let outsider = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[9u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.create_shipment(
        &outsider,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
}

// Participant Validation Tests

/// create_shipment must reject when sender == receiver
#[test]
#[should_panic(expected = "Error(Contract, #57)")]
fn test_create_shipment_rejects_sender_equals_receiver() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[101u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.create_shipment(
        &company,
        &company, // sender == receiver (invalid)
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
}

/// create_shipment must reject when sender == carrier
#[test]
#[should_panic(expected = "Error(Contract, #57)")]
fn test_create_shipment_rejects_sender_equals_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[102u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.create_shipment(
        &company,
        &receiver,
        &company, // sender == carrier (invalid)
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
}

/// create_shipment must reject when receiver == carrier
#[test]
#[should_panic(expected = "Error(Contract, #57)")]
fn test_create_shipment_rejects_receiver_equals_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[103u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.create_shipment(
        &company,
        &receiver,
        &receiver, // receiver == carrier (invalid)
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
}

// Carrier Whitelist Tests
