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
        shipment_id,
        "shipment_created",
        event_counter,
    );
    assert_eq!(event_idempotency_key, expected_key);

    println!("Verification successful!");
}
