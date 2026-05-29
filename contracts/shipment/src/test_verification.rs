extern crate std;
use std::println;

use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, BytesN, Symbol, TryIntoVal, Vec,
};

#[test]
fn test_frontend_verification_flow() {
    let (env, client, admin, _token_contract) = crate::test::setup_shipment_env();
    client.initialize(&admin, &_token_contract);

    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = 100000;
    let payment_milestones: Vec<(Symbol, u32)> = Vec::new(&env);

    // Register roles for sender and carrier using admin
    client.add_company(&admin, &sender);
    client.add_carrier(&admin, &carrier);

    client.create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &payment_milestones,
        &deadline,
        &None,
    );

    // 1. Get events
    let events = env.events().all();

    // Filter for the shipment_created event
    let target_topic = Symbol::new(&env, "shipment_created");
    let shipment_created_event = events
        .iter()
        .find(|e| {
            let topic_0: Result<Symbol, _> = e.1.get(0).unwrap().try_into_val(&env);
            topic_0.is_ok() && topic_0.unwrap() == target_topic
        })
        .expect("shipment_created event should be emitted");

    // Print for trace collection
    println!("--- SAMPLE EVENT TRACE ---");
    println!("Contract ID: {:?}", shipment_created_event.0);
    println!("Topics: {:?}", shipment_created_event.1);
    println!("Data: {:?}", shipment_created_event.2);
    println!("---------------------------");

    // 2. Verification Step: Verify Contract ID
    // A frontend would check if the event's contractId matches the known Navin contract address.
    assert_eq!(shipment_created_event.0, client.address);

    // 3. Verification Step: Verify Topics
    // Topic 0 should be the event type
    let topic_0: Symbol = shipment_created_event
        .1
        .get(0)
        .unwrap()
        .try_into_val(&env)
        .unwrap();
    assert_eq!(topic_0, target_topic);

    // 4. Verification Step: Verify Data Hash and Fields
    // For shipment_created data is a Vec<Val>:
    // [shipment_id, sender, receiver, data_hash, version, counter, idempotency_key]
    let event_data: Vec<soroban_sdk::Val> = shipment_created_event.2.try_into_val(&env).unwrap();

    let shipment_id: u64 = event_data.get(0).unwrap().try_into_val(&env).unwrap();
    let event_sender: Address = event_data.get(1).unwrap().try_into_val(&env).unwrap();
    let event_receiver: Address = event_data.get(2).unwrap().try_into_val(&env).unwrap();
    let event_data_hash: BytesN<32> = event_data.get(3).unwrap().try_into_val(&env).unwrap();
    let event_counter: u32 = event_data.get(5).unwrap().try_into_val(&env).unwrap();
    let event_idempotency_key: BytesN<32> = event_data.get(6).unwrap().try_into_val(&env).unwrap();

    assert_eq!(shipment_id, 1);
    assert_eq!(event_sender, sender);
    assert_eq!(event_receiver, receiver);
    assert_eq!(event_data_hash, data_hash);
    assert_eq!(event_counter, 1);

    // 5. Verification Step: Verify Idempotency Key
    // The idempotency key is a hash of (shipment_id, event_type, event_counter)
    let expected_key = crate::events::generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        shipment_id,
        "shipment_created",
        event_counter,
    );
    assert_eq!(event_idempotency_key, expected_key);

    println!("Verification successful!");
}

#[test]
fn test_delivery_event_confirmation_payload_indexer_friendly() {
    let (env, client, admin, _token_contract) = crate::test::setup_shipment_env();
    client.initialize(&admin, &_token_contract);

    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = 100000;
    let payment_milestones: Vec<(Symbol, u32)> = Vec::new(&env);

    client.add_company(&admin, &sender);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &payment_milestones,
        &deadline,
        &None,
    );

    client.deposit_escrow(&sender, &shipment_id, &1_000i128);
    client.update_status(
        &carrier,
        &shipment_id,
        &crate::types::ShipmentStatus::InTransit,
        &data_hash,
    );
    client.confirm_delivery(&receiver, &shipment_id, &data_hash);

    let events = env.events().all();
    let target_topic = Symbol::new(&env, "delivery_success");
    let delivery_event = events
        .iter()
        .find(|e| {
            let topic_0: Result<Symbol, _> = e.1.get(0).unwrap().try_into_val(&env);
            topic_0.is_ok() && topic_0.unwrap() == target_topic
        })
        .expect("delivery_success event should be emitted");

    let event_data: Vec<soroban_sdk::Val> = delivery_event.2.try_into_val(&env).unwrap();

    let event_carrier: Address = event_data.get(0).unwrap().try_into_val(&env).unwrap();
    let event_shipment_id: u64 = event_data.get(1).unwrap().try_into_val(&env).unwrap();
    let event_timestamp: u64 = event_data.get(2).unwrap().try_into_val(&env).unwrap();
    let event_schema_version: u32 = event_data.get(3).unwrap().try_into_val(&env).unwrap();
    let event_counter: u32 = event_data.get(4).unwrap().try_into_val(&env).unwrap();
    let event_idempotency_key: BytesN<32> = event_data.get(5).unwrap().try_into_val(&env).unwrap();

    assert_eq!(event_carrier, carrier, "carrier placement");
    assert_eq!(event_shipment_id, shipment_id, "shipment_id placement");
    assert!(event_timestamp > 0, "timestamp placement");
    assert_eq!(event_schema_version, 2, "schema_version metadata");
    assert_eq!(event_counter, 5, "event_counter placement");
    assert_eq!(event_idempotency_key.len(), 32, "idempotency_key stability");
}

#[test]
fn test_status_updated_event_payload_locked_down() {
    let (env, client, admin, _token_contract) = crate::test::setup_shipment_env();
    client.initialize(&admin, &_token_contract);

    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = 100000;
    let payment_milestones: Vec<(Symbol, u32)> = Vec::new(&env);

    client.add_company(&admin, &sender);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &payment_milestones,
        &deadline,
        &None,
    );

    client.update_status(
        &carrier,
        &shipment_id,
        &crate::types::ShipmentStatus::InTransit,
        &data_hash,
    );

    let events = env.events().all();
    let target_topic = Symbol::new(&env, "status_updated");
    let status_event = events
        .iter()
        .find(|e| {
            let topic_0: Result<Symbol, _> = e.1.get(0).unwrap().try_into_val(&env);
            topic_0.is_ok() && topic_0.unwrap() == target_topic
        })
        .expect("status_updated event should be emitted");

    let event_data: Vec<soroban_sdk::Val> = status_event.2.try_into_val(&env).unwrap();

    let event_shipment_id: u64 = event_data.get(0).unwrap().try_into_val(&env).unwrap();
    let _event_old_status: soroban_sdk::Val = event_data.get(1).unwrap();
    let _event_new_status: soroban_sdk::Val = event_data.get(2).unwrap();
    let event_data_hash: BytesN<32> = event_data.get(3).unwrap().try_into_val(&env).unwrap();
    let event_schema_version: u32 = event_data.get(4).unwrap().try_into_val(&env).unwrap();
    let event_counter: u32 = event_data.get(5).unwrap().try_into_val(&env).unwrap();
    let event_idempotency_key: BytesN<32> = event_data.get(6).unwrap().try_into_val(&env).unwrap();

    assert_eq!(event_shipment_id, shipment_id, "shipment_id position");
    assert_eq!(event_data_hash, data_hash, "hash position");
    assert_eq!(event_schema_version, 2, "schema_version stability");
    assert_eq!(event_counter, 2, "event_counter stability");
    assert_eq!(event_idempotency_key.len(), 32, "idempotency_key stability");
}

#[test]
fn test_created_event_payload_pinned() {
    let (env, client, admin, _token_contract) = crate::test::setup_shipment_env();
    client.initialize(&admin, &_token_contract);

    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[3u8; 32]);
    let deadline = 100000;
    let payment_milestones: Vec<(Symbol, u32)> = Vec::new(&env);

    client.add_company(&admin, &sender);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &payment_milestones,
        &deadline,
        &None,
    );

    let events = env.events().all();
    let target_topic = Symbol::new(&env, "shipment_created");
    let created_event = events
        .iter()
        .find(|e| {
            let topic_0: Result<Symbol, _> = e.1.get(0).unwrap().try_into_val(&env);
            topic_0.is_ok() && topic_0.unwrap() == target_topic
        })
        .expect("shipment_created event should be emitted");

    let event_data: Vec<soroban_sdk::Val> = created_event.2.try_into_val(&env).unwrap();

    let event_shipment_id: u64 = event_data.get(0).unwrap().try_into_val(&env).unwrap();
    let event_sender: Address = event_data.get(1).unwrap().try_into_val(&env).unwrap();
    let event_receiver: Address = event_data.get(2).unwrap().try_into_val(&env).unwrap();
    let event_data_hash: BytesN<32> = event_data.get(3).unwrap().try_into_val(&env).unwrap();
    let event_schema_version: u32 = event_data.get(4).unwrap().try_into_val(&env).unwrap();
    let event_counter: u32 = event_data.get(5).unwrap().try_into_val(&env).unwrap();
    let event_idempotency_key: BytesN<32> = event_data.get(6).unwrap().try_into_val(&env).unwrap();

    assert_eq!(event_shipment_id, shipment_id, "shipment_id at index 0");
    assert_eq!(event_sender, sender, "sender at index 1");
    assert_eq!(event_receiver, receiver, "receiver at index 2");
    assert_eq!(event_data_hash, data_hash, "data_hash at index 3");
    assert_eq!(event_schema_version, 2, "schema_version at index 4");
    assert_eq!(event_counter, 1, "event_counter at index 5");
    assert_eq!(
        event_idempotency_key.len(),
        32,
        "idempotency_key at index 6"
    );
}
