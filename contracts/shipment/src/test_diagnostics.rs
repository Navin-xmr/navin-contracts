use crate::{
    test_utils::{advance_ledger_time, setup_env},
    types::ShipmentStatus,
    NavinShipment, NavinShipmentClient,
};
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Vec};

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn decimals(_env: soroban_sdk::Env) -> u32 {
        7
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    pub fn transfer_from(
        _env: Env,
        _spender: Address,
        _from: Address,
        _to: Address,
        _amount: i128,
    ) {
    }
}

fn prepare_test() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = setup_env();
    let token = env.register(MockToken {}, ());
    let cid = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &cid);
    client.initialize(&admin, &token);
    (env, client, admin, token)
}

#[test]
fn test_clean_health_check() {
    let (env, client, admin, _token) = prepare_test();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let _shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    let health = client.check_contract_health(&admin);
    assert_eq!(health.total_shipments, 1);
    assert_eq!(health.active_shipments_counted, 1);
    assert_eq!(health.sum_of_escrow_balances, 0);
    assert_eq!(health.anomalous_shipment_ids.len(), 0);
    assert_eq!(health.storage_inconsistencies.len(), 0);
}

#[test]
fn test_detect_anomalies_and_escrow() {
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash1 = BytesN::from_array(&env, &[1u8; 32]);
    let data_hash2 = BytesN::from_array(&env, &[2u8; 32]);

    let id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash1,
        &Vec::new(&env),
        &deadline,
        &None,
    );
    advance_ledger_time(&env, 1);
    let id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash2,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    client.deposit_escrow(&company, &id1, &1500);
    client.deposit_escrow(&company, &id2, &500);

    client.update_status(&carrier, &id1, &ShipmentStatus::InTransit, &data_hash1);

    // Simulate crossing the deadline threshold
    advance_ledger_time(&env, 4000); // Exceeds deadline (+3600)

    let health = client.check_contract_health(&admin);
    assert_eq!(health.total_shipments, 2);
    assert_eq!(health.active_shipments_counted, 2);

    // Sum should be accurate
    assert_eq!(health.sum_of_escrow_balances, 2000);

    // id1 is strictly InTransit and late!
    assert!(health.anomalous_shipment_ids.contains(id1));
    // id2 is still physically 'Created', which might be fine to remain late without anomaly or catch elsewhere depending on business rules, but in our code it strictly checks InTransit.
    assert!(!health.anomalous_shipment_ids.contains(id2));

    assert_eq!(
        health.storage_inconsistencies.len(),
        0,
        "Storage inconsistencies found: {:?}",
        health.storage_inconsistencies
    );
}

#[test]
fn test_detect_storage_inconsistencies() {
    // This is purely for unit verification that run_system_health_check directly exposes the
    // internal variables. We can force storage modification inside tests using raw storage functions.
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    let cid = client.address.clone();
    env.as_contract(&cid, || {
        crate::storage::remove_escrow(&env, shipment_id);

        // Set escrow high within the shipment object to simulate orphaned balance
        let mut ship = crate::storage::get_shipment(&env, shipment_id).unwrap();
        ship.escrow_amount = 5000;
        crate::storage::set_shipment(&env, &ship);
    });

    let health = client.check_contract_health(&admin);
    // Because escrow_amount is 5000 but the Escrow persisted entry is killed by remove_escrow
    assert!(health.storage_inconsistencies.contains(shipment_id));
}
