#![cfg(test)]

extern crate std;

use super::test_helpers::*;

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_update_status_unauthorized() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let unauthorized_user = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let new_data_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // Unauthorized user trying to update status
    client.update_status(
        &unauthorized_user,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &new_data_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_update_status_nonexistent_shipment() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let new_data_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin, &token_contract);

    // Try to update a non-existent shipment
    client.update_status(&carrier, &999, &ShipmentStatus::InTransit, &new_data_hash);
}

#[test]
fn test_suspend_carrier_requires_admin() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let outsider = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    let res = client.try_suspend_carrier(&outsider, &carrier);
    assert_eq!(res, Err(Ok(crate::NavinError::Unauthorized)));
}

// Get Escrow Balance Tests

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_deposit_escrow_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let non_company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[11u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let escrow_amount: i128 = 1000;
    client.deposit_escrow(&non_company, &shipment_id, &escrow_amount);
    // No escrow deposited yet, should return 0
    assert_eq!(client.get_escrow_balance(&shipment_id), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_escrow_balance_shipment_not_found() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_escrow_balance(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_escrow_balance_fails_before_initialization() {
    let (_env, _client, _admin, _token_contract) = setup_shipment_env();

    _client.get_escrow_balance(&1);
}

// Get Shipment Count Tests

#[test]
fn test_get_shipment_count_returns_zero_on_fresh_contract() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    assert_eq!(client.get_shipment_count(), 0);
}

#[test]
fn test_get_shipment_count_returns_zero_after_initialization() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    assert_eq!(client.get_shipment_count(), 0);
}

// Role Tests

#[test]
fn test_get_role_unassigned() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let user = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    assert_eq!(client.get_role(&user), crate::Role::Unassigned);
}

#[test]
fn test_get_role_assigned() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    client.add_company(&admin, &company);
    assert_eq!(client.get_role(&company), crate::Role::Company);

    client.add_carrier(&admin, &carrier);
    assert_eq!(client.get_role(&carrier), crate::Role::Carrier);
}

// Get Shipment Tests

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_not_found() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_shipment_fails_before_initialization() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    client.get_shipment(&1);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_creator_fails_for_invalid_id() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment_creator(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_receiver_fails_for_invalid_id() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment_receiver(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_sender_fails_for_invalid_id() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment_sender(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_carrier_fails_for_invalid_id() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment_carrier(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_status_fails_for_invalid_id() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment_status(&999);
}

// Geofence Event Tests

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_report_geofence_event_unauthorized_role() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let event_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    // Note: outsider NOT added as carrier

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.report_geofence_event(
        &outsider,
        &shipment_id,
        &GeofenceEvent::ZoneEntry,
        &event_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_deposit_escrow_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let non_existent_shipment_id = 999u64;
    let escrow_amount: i128 = 1000;
    client.deposit_escrow(&company, &non_existent_shipment_id, &escrow_amount);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_report_geofence_event_non_existent_shipment() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let event_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);

    client.report_geofence_event(&carrier, &999, &GeofenceEvent::ZoneEntry, &event_hash);
}

// ETA Update Tests

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_update_eta_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let shipment_hash = BytesN::from_array(&env, &[1u8; 32]);
    let eta_hash = BytesN::from_array(&env, &[7u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &shipment_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    let eta_timestamp = env.ledger().timestamp() + 120;

    // outsider is not a registered carrier
    client.update_eta(&outsider, &shipment_id, &eta_timestamp, &eta_hash);
}

// Confirm Delivery Tests
