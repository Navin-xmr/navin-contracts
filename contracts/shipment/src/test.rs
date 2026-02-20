#![cfg(test)]

extern crate std;

use crate::{ShipmentContract, ShipmentContractClient};
use soroban_sdk::{testutils::Address as _, testutils::Events, Address, BytesN, Env};

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
fn test_scaffold() {
    let env = Env::default();
    let _client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
}

#[test]
fn test_create_shipment_success() {
    let (env, client, admin) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);

    client.initialize(&admin);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(&company, &receiver, &carrier, &data_hash);
    assert_eq!(shipment_id, 1);
    assert_eq!(client.get_shipment_counter(), 1);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.id, shipment_id);
    assert_eq!(shipment.sender, company);
    assert_eq!(shipment.receiver, receiver);
    assert_eq!(shipment.carrier, carrier);
    assert_eq!(shipment.data_hash, data_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_create_shipment_unauthorized() {
    let (env, client, admin) = setup_env();
    let outsider = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[9u8; 32]);

    client.initialize(&admin);
    client.create_shipment(&outsider, &receiver, &carrier, &data_hash);
}

#[test]
fn test_multiple_shipments_have_unique_ids() {
    let (env, client, admin) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let hash_one = BytesN::from_array(&env, &[1u8; 32]);
    let hash_two = BytesN::from_array(&env, &[2u8; 32]);
    let hash_three = BytesN::from_array(&env, &[3u8; 32]);

    client.initialize(&admin);
    client.add_company(&admin, &company);

    let id_one = client.create_shipment(&company, &receiver, &carrier, &hash_one);
    let id_two = client.create_shipment(&company, &receiver, &carrier, &hash_two);
    let id_three = client.create_shipment(&company, &receiver, &carrier, &hash_three);

    assert_eq!(id_one, 1);
    assert_eq!(id_two, 2);
    assert_eq!(id_three, 3);
    assert_eq!(client.get_shipment_counter(), 3);
}
