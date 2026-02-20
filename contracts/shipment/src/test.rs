#![cfg(test)]

extern crate std;

use crate::{ShipmentContract, ShipmentContractClient};
use soroban_sdk::{testutils::Address as _, testutils::Events, Address, Env};

fn setup_env() -> (Env, ShipmentContractClient<'static>, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let client = ShipmentContractClient::new(&env, &env.register(ShipmentContract {}, ()));
    env.mock_all_auths();
    (env, client, admin)
}

#[test]
fn test_successful_initialization() {
    let (_env, client, admin) = setup_env();

    client.initialize(&admin);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_shipment_counter(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_re_initialization_fails() {
    let (_env, client, admin) = setup_env();

    client.initialize(&admin);
    // Second call must fail with AlreadyInitialized (error code 1)
    client.initialize(&admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_re_initialization_with_different_admin_fails() {
    let (env, client, admin) = setup_env();

    client.initialize(&admin);

    let other_admin = Address::generate(&env);
    // Attempting to re-initialize with a different admin must also fail
    client.initialize(&other_admin);
}

#[test]
fn test_shipment_counter_starts_at_zero() {
    let (_env, client, admin) = setup_env();

    client.initialize(&admin);

    assert_eq!(client.get_shipment_counter(), 0);
}

#[test]
fn test_admin_is_stored_correctly() {
    let (env, client, _admin) = setup_env();

    let specific_admin = Address::generate(&env);
    client.initialize(&specific_admin);

    let stored_admin = client.get_admin();
    assert_eq!(stored_admin, specific_admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_admin_before_initialization_fails() {
    let (_env, client, _admin) = setup_env();

    // Must fail with NotInitialized (error code 2)
    client.get_admin();
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_counter_before_initialization_fails() {
    let (_env, client, _admin) = setup_env();

    // Must fail with NotInitialized (error code 2)
    client.get_shipment_counter();
}

#[test]
fn test_initialization_emits_event() {
    let (env, client, admin) = setup_env();

    client.initialize(&admin);

    let events = env.events().all();
    // Verify at least one event was published
    assert!(!events.is_empty());
}
// ============= Carrier Whitelist Tests =============

#[test]
fn test_add_carrier_to_whitelist() {
    let (_env, client, admin) = setup_env();
    client.initialize(&admin);

    let company = Address::generate(&_env);
    let carrier = Address::generate(&_env);

    // Add carrier to whitelist
    client.add_carrier_to_whitelist(&company, &carrier);

    // Verify carrier is whitelisted
    assert!(client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_remove_carrier_from_whitelist() {
    let (_env, client, admin) = setup_env();
    client.initialize(&admin);

    let company = Address::generate(&_env);
    let carrier = Address::generate(&_env);

    // Add carrier to whitelist
    client.add_carrier_to_whitelist(&company, &carrier);
    assert!(client.is_carrier_whitelisted(&company, &carrier));

    // Remove carrier from whitelist
    client.remove_carrier_from_whitelist(&company, &carrier);

    // Verify carrier is no longer whitelisted
    assert!(!client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_is_carrier_whitelisted_returns_false_for_non_whitelisted() {
    let (_env, client, admin) = setup_env();
    client.initialize(&admin);

    let company = Address::generate(&_env);
    let carrier = Address::generate(&_env);

    // Verify carrier is not whitelisted by default
    assert!(!client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_multiple_carriers_whitelist() {
    let (_env, client, admin) = setup_env();
    client.initialize(&admin);

    let company = Address::generate(&_env);
    let carrier1 = Address::generate(&_env);
    let carrier2 = Address::generate(&_env);
    let carrier3 = Address::generate(&_env);

    // Add multiple carriers
    client.add_carrier_to_whitelist(&company, &carrier1);
    client.add_carrier_to_whitelist(&company, &carrier2);

    // Verify added carriers are whitelisted
    assert!(client.is_carrier_whitelisted(&company, &carrier1));
    assert!(client.is_carrier_whitelisted(&company, &carrier2));

    // Verify carrier3 is not whitelisted
    assert!(!client.is_carrier_whitelisted(&company, &carrier3));

    // Remove one carrier
    client.remove_carrier_from_whitelist(&company, &carrier1);

    // Verify carrier1 is removed but carrier2 is still whitelisted
    assert!(!client.is_carrier_whitelisted(&company, &carrier1));
    assert!(client.is_carrier_whitelisted(&company, &carrier2));
}

#[test]
fn test_whitelist_per_company() {
    let (_env, client, admin) = setup_env();
    client.initialize(&admin);

    let company1 = Address::generate(&_env);
    let company2 = Address::generate(&_env);
    let carrier = Address::generate(&_env);

    // Add carrier to company1's whitelist only
    client.add_carrier_to_whitelist(&company1, &carrier);

    // Verify carrier is whitelisted for company1 but not for company2
    assert!(client.is_carrier_whitelisted(&company1, &carrier));
    assert!(!client.is_carrier_whitelisted(&company2, &carrier));

    // Add same carrier to company2's whitelist
    client.add_carrier_to_whitelist(&company2, &carrier);

    // Verify carrier is now whitelisted for both companies
    assert!(client.is_carrier_whitelisted(&company1, &carrier));
    assert!(client.is_carrier_whitelisted(&company2, &carrier));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_whitelist_functions_fail_before_initialization() {
    let (_env, client, _admin) = setup_env();

    let company = Address::generate(&_env);
    let carrier = Address::generate(&_env);

    // Must fail with NotInitialized (error code 2)
    client.is_carrier_whitelisted(&company, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_add_whitelist_fails_before_initialization() {
    let (_env, client, _admin) = setup_env();

    let company = Address::generate(&_env);
    let carrier = Address::generate(&_env);

    // Must fail with NotInitialized (error code 2)
    client.add_carrier_to_whitelist(&company, &carrier);
}
