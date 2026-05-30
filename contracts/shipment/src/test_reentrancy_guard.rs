extern crate std;

use crate::{test::setup_shipment_env, types::DataKey, NavinError};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Vec};

fn setup_single_shipment() -> (
    soroban_sdk::Env,
    crate::NavinShipmentClient<'static>,
    Address,
    Address,
    Address,
    Address,
    u64,
) {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let data_hash = BytesN::from_array(&env, &[9u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    (env, client, admin, company, receiver, carrier, shipment_id)
}

#[test]
fn test_deposit_escrow_rejected_when_reentrancy_lock_is_preheld() {
    let (env, client, _admin, company, _receiver, _carrier, shipment_id) = setup_single_shipment();

    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &true);
    });

    let result = client.try_deposit_escrow(&company, &shipment_id, &1000);
    assert_eq!(result, Err(Ok(NavinError::ReentrancyDetected)));
}

#[test]
fn test_release_escrow_rejected_when_reentrancy_lock_is_preheld() {
    let (env, client, _admin, company, receiver, _carrier, shipment_id) = setup_single_shipment();

    client.deposit_escrow(&company, &shipment_id, &1000);
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &true);
    });

    let result = client.try_release_escrow(&receiver, &shipment_id);
    assert_eq!(result, Err(Ok(NavinError::ReentrancyDetected)));
}

#[test]
fn test_refund_escrow_rejected_when_reentrancy_lock_is_preheld() {
    let (env, client, _admin, company, _receiver, _carrier, shipment_id) = setup_single_shipment();

    client.deposit_escrow(&company, &shipment_id, &1000);
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &true);
    });

    let result = client.try_refund_escrow(&company, &shipment_id);
    assert_eq!(result, Err(Ok(NavinError::ReentrancyDetected)));
}

#[test]
fn test_reentrancy_lock_is_released_after_successful_operation() {
    let (env, client, _admin, company, _receiver, _carrier, shipment_id) = setup_single_shipment();

    client.deposit_escrow(&company, &shipment_id, &1000);

    env.as_contract(&client.address, || {
        let locked = env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::ReentrancyLock)
            .unwrap_or(false);
        assert!(!locked, "reentrancy lock should be cleared after operation");
    });
}
