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
