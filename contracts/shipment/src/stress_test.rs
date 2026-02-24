#![cfg(test)]

extern crate std;

use crate::{NavinShipment, NavinShipmentClient, ShipmentStatus};
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env};

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

fn setup_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    env.mock_all_auths();
    (env, client, admin, token_contract)
}

#[test]
fn test_create_50_shipments_sequentially() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    for i in 1..=50 {
        let receiver = Address::generate(&env);
        let carrier = Address::generate(&env);
        let data_hash = BytesN::from_array(&env, &[i as u8; 32]);

        let shipment_id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &soroban_sdk::Vec::new(&env),
            &deadline,
        );
        assert_eq!(shipment_id, i);
    }

    assert_eq!(client.get_shipment_counter(), 50);
}

#[test]
fn test_20_concurrent_status_updates() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let mut carriers = soroban_sdk::Vec::new(&env);
    for _ in 0..20 {
        let carrier = Address::generate(&env);
        client.add_carrier(&admin, &carrier);
        carriers.push_back(carrier);
    }

    for i in 0..20 {
        let receiver = Address::generate(&env);
        let carrier = carriers.get(i).unwrap();
        let data_hash = BytesN::from_array(&env, &[(i + 1) as u8; 32]);

        client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &soroban_sdk::Vec::new(&env),
            &deadline,
        );
    }

    for i in 0..20 {
        let shipment_id = (i + 1) as u64;
        let carrier = carriers.get(i).unwrap();
        let update_hash = BytesN::from_array(&env, &[(i + 100) as u8; 32]);

        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &update_hash,
        );
    }

    for i in 1..=20 {
        let shipment = client.get_shipment(&i);
        assert_eq!(shipment.status, ShipmentStatus::InTransit);
    }
}

#[test]
fn test_verify_shipment_count_after_mass_operations() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    for i in 1..=75 {
        let receiver = Address::generate(&env);
        let carrier = Address::generate(&env);
        let data_hash = BytesN::from_array(&env, &[i as u8; 32]);

        client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &soroban_sdk::Vec::new(&env),
            &deadline,
        );
    }

    assert_eq!(client.get_shipment_counter(), 75);
    assert_eq!(client.get_shipment_count(), 75);

    let analytics = client.get_analytics();
    assert_eq!(analytics.total_shipments, 75);
    assert_eq!(analytics.created_count, 75);
}

#[test]
fn test_no_data_corruption_between_shipments() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let mut expected_data = soroban_sdk::Vec::new(&env);

    for i in 1..=60 {
        let receiver = Address::generate(&env);
        let carrier = Address::generate(&env);
        let data_hash = BytesN::from_array(&env, &[i as u8; 32]);

        expected_data.push_back((receiver.clone(), carrier.clone(), data_hash.clone()));

        client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &soroban_sdk::Vec::new(&env),
            &deadline,
        );
    }

    for i in 1..=60 {
        let shipment = client.get_shipment(&i);
        let (expected_receiver, expected_carrier, expected_hash) =
            expected_data.get((i - 1) as u32).unwrap();

        assert_eq!(shipment.id, i);
        assert_eq!(shipment.sender, company);
        assert_eq!(shipment.receiver, expected_receiver);
        assert_eq!(shipment.carrier, expected_carrier);
        assert_eq!(shipment.data_hash, expected_hash);
        assert_eq!(shipment.status, ShipmentStatus::Created);
    }
}
