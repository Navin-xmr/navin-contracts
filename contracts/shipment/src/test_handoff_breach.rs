#![cfg(test)]

use crate::{test::setup_shipment_env, NavinError, ShipmentStatus};
use soroban_sdk::{testutils::Address as _, Address, BytesN};

#[test]
fn disputed_shipment_cannot_be_handed_off() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[2u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[3u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &current_carrier);
    client.add_carrier(&admin, &new_carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &current_carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.raise_dispute(&receiver, &shipment_id, &reason_hash);

    assert_eq!(
        client.get_shipment(&shipment_id).status,
        ShipmentStatus::Disputed
    );

    let result =
        client.try_handoff_shipment(&current_carrier, &new_carrier, &shipment_id, &handoff_hash);
    assert_eq!(result, Err(Ok(NavinError::ShipmentAlreadyCompleted)));

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.carrier, current_carrier);
}
