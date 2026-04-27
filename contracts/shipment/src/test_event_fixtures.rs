extern crate std;
use std::string::ToString;

use crate::{test_utils, NavinShipment, NavinShipmentClient};
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, BytesN, Env, Symbol, TryFromVal, Vec,
};

fn fixture_env() -> (
    Env,
    NavinShipmentClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let (env, admin) = test_utils::setup_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    // Register SAC token for standard token tests
    let token_address = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    let shipment_addr = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &shipment_addr);
    client.initialize(&admin, &token_address);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    env.mock_all_auths();

    (env, client, admin, company, carrier, receiver)
}

#[test]
fn test_all_fixtures_emit_expected_topics() {
    let (env, client, _admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    // 1. create a shipment
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // 2. escrow_frozen (via raise_dispute)
    client.raise_dispute(&company, &shipment_id, &data_hash);

    let emitted_events = env.events().all();
    let mut topics_found = std::vec::Vec::new();

    for (_contract, topic, _data) in emitted_events.into_iter() {
        if let Some(topic_sym) = topic.get(0) {
            if let Ok(sym) = Symbol::try_from_val(&env, &topic_sym) {
                topics_found.push(sym);
            }
        }
    }

    let topics_strings: std::vec::Vec<std::string::String> =
        topics_found.iter().map(|s| s.to_string()).collect();
    std::println!("TARGET TOPICS = [dispute_raised, escrow_frozen]");
    std::println!("FOUND TOPICS = {:?}", topics_strings);

    assert!(topics_found.contains(&Symbol::new(&env, crate::event_topics::DISPUTE_RAISED)));
    assert!(topics_found.contains(&Symbol::new(&env, crate::event_topics::ESCROW_FROZEN)));
}

#[test]
fn test_fixture_payload_shapes_are_stable() {
    let (env, client, _admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    client.raise_dispute(&company, &shipment_id, &data_hash);

    let emitted_events = env.events().all();

    let mut saw_dispute = false;
    let mut saw_frozen = false;

    for (_contract, topic, data) in emitted_events.into_iter() {
        let topic_sym = topic
            .get(0)
            .and_then(|v| Symbol::try_from_val(&env, &v).ok());
        if topic_sym.is_none() {
            continue;
        }
        let topic_sym = topic_sym.unwrap();

        if let Ok(payload) = soroban_sdk::Vec::<soroban_sdk::Val>::try_from_val(&env, &data) {
            if topic_sym == Symbol::new(&env, crate::event_topics::DISPUTE_RAISED) {
                saw_dispute = true;
                assert_eq!(payload.len(), 3);
            }
            if topic_sym == Symbol::new(&env, crate::event_topics::ESCROW_FROZEN) {
                saw_frozen = true;
                assert_eq!(payload.len(), 4);
            }
        }
    }

    assert!(saw_dispute);
    assert!(saw_frozen);
}
