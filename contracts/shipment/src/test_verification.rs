extern crate std;
use std::println;

use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, BytesN, Symbol, TryIntoVal, Vec,
};

// ── [ISSUE #598] DataHashMismatch error variant tests ────────────────────────
//
// DataHashMismatch (#45) is returned by assert_delivery_hash and assert_data_hash
// when the caller-supplied hash does not match the value stored on-chain.
// These tests pin the discriminant, cover every error path, and confirm the
// happy path is unaffected.

/// Error code pin: DataHashMismatch discriminant must be exactly 45.
#[test]
fn test_data_hash_mismatch_error_code_is_45() {
    use crate::NavinError;
    assert_eq!(
        NavinError::DataHashMismatch as u32,
        45,
        "DataHashMismatch discriminant must be 45"
    );
}

/// assert_delivery_hash returns DataHashMismatch (#45) when the supplied hash
/// differs from the confirmation hash stored during confirm_delivery.
#[test]
fn test_assert_delivery_hash_wrong_hash_returns_data_hash_mismatch() {
    use crate::NavinError;
    let (env, client, admin, _token_contract) = crate::test::setup_shipment_env();
    client.initialize(&admin, &_token_contract);

    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 86400;

    client.add_company(&admin, &sender);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    crate::test_utils::advance_past_rate_limit(&env);
    let transit_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.update_status(&carrier, &shipment_id, &crate::ShipmentStatus::InTransit, &transit_hash);

    let confirmation_hash = BytesN::from_array(&env, &[3u8; 32]);
    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    // A different hash must produce DataHashMismatch.
    let wrong_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
    let result = client.try_assert_delivery_hash(&shipment_id, &wrong_hash);
    assert_eq!(
        result,
        Err(Ok(NavinError::DataHashMismatch)),
        "assert_delivery_hash with wrong hash must return DataHashMismatch (#45)"
    );
}

/// assert_delivery_hash returns Ok(()) when the correct confirmation hash is supplied.
#[test]
fn test_assert_delivery_hash_correct_hash_returns_ok() {
    let (env, client, admin, _token_contract) = crate::test::setup_shipment_env();
    client.initialize(&admin, &_token_contract);

    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 86400;

    client.add_company(&admin, &sender);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    crate::test_utils::advance_past_rate_limit(&env);
    let transit_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.update_status(&carrier, &shipment_id, &crate::ShipmentStatus::InTransit, &transit_hash);

    let confirmation_hash = BytesN::from_array(&env, &[3u8; 32]);
    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    // Correct hash must return Ok.
    let result = client.try_assert_delivery_hash(&shipment_id, &confirmation_hash);
    assert_eq!(
        result,
        Ok(()),
        "assert_delivery_hash with correct hash must return Ok(())"
    );
}

/// assert_delivery_hash returns ShipmentNotFound for a non-existent shipment.
#[test]
fn test_assert_delivery_hash_nonexistent_shipment_returns_not_found() {
    use crate::NavinError;
    let (env, client, admin, _token_contract) = crate::test::setup_shipment_env();
    client.initialize(&admin, &_token_contract);

    let hash = BytesN::from_array(&env, &[1u8; 32]);
    let result = client.try_assert_delivery_hash(&9999u64, &hash);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "assert_delivery_hash on non-existent shipment must return ShipmentNotFound (#4)"
    );
}

/// assert_delivery_hash returns StatusHashNotFound when no confirmation hash
/// has been stored yet (delivery not yet confirmed).
#[test]
fn test_assert_delivery_hash_no_confirmation_returns_status_hash_not_found() {
    use crate::NavinError;
    let (env, client, admin, _token_contract) = crate::test::setup_shipment_env();
    client.initialize(&admin, &_token_contract);

    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 86400;

    client.add_company(&admin, &sender);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Delivery not confirmed — no confirmation hash stored yet.
    let probe = BytesN::from_array(&env, &[5u8; 32]);
    let result = client.try_assert_delivery_hash(&shipment_id, &probe);
    assert_eq!(
        result,
        Err(Ok(NavinError::StatusHashNotFound)),
        "assert_delivery_hash before delivery confirmation must return StatusHashNotFound (#44)"
    );
}

/// assert_data_hash returns DataHashMismatch (#45) when the supplied hash differs
/// from the hash stored at update_status time.
#[test]
fn test_assert_data_hash_wrong_hash_returns_data_hash_mismatch() {
    use crate::NavinError;
    let (env, client, admin, _token_contract) = crate::test::setup_shipment_env();
    client.initialize(&admin, &_token_contract);

    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 86400;

    client.add_company(&admin, &sender);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let transit_hash = BytesN::from_array(&env, &[0xAAu8; 32]);
    client.update_status(&carrier, &shipment_id, &crate::ShipmentStatus::InTransit, &transit_hash);

    // A different hash must produce DataHashMismatch.
    let wrong_hash = BytesN::from_array(&env, &[0xBBu8; 32]);
    let result = client.try_assert_data_hash(&shipment_id, &crate::ShipmentStatus::InTransit, &wrong_hash);
    assert_eq!(
        result,
        Err(Ok(NavinError::DataHashMismatch)),
        "assert_data_hash with wrong hash must return DataHashMismatch (#45)"
    );
}

/// assert_data_hash returns Ok(()) when the correct status hash is supplied.
#[test]
fn test_assert_data_hash_correct_hash_returns_ok() {
    let (env, client, admin, _token_contract) = crate::test::setup_shipment_env();
    client.initialize(&admin, &_token_contract);

    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 86400;

    client.add_company(&admin, &sender);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let transit_hash = BytesN::from_array(&env, &[0xCCu8; 32]);
    client.update_status(&carrier, &shipment_id, &crate::ShipmentStatus::InTransit, &transit_hash);

    let result = client.try_assert_data_hash(&shipment_id, &crate::ShipmentStatus::InTransit, &transit_hash);
    assert_eq!(
        result,
        Ok(()),
        "assert_data_hash with correct hash must return Ok(())"
    );
}

/// assert_data_hash returns ShipmentNotFound for a non-existent shipment.
#[test]
fn test_assert_data_hash_nonexistent_shipment_returns_not_found() {
    use crate::NavinError;
    let (env, client, admin, _token_contract) = crate::test::setup_shipment_env();
    client.initialize(&admin, &_token_contract);

    let hash = BytesN::from_array(&env, &[1u8; 32]);
    let result = client.try_assert_data_hash(&9999u64, &crate::ShipmentStatus::InTransit, &hash);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "assert_data_hash on non-existent shipment must return ShipmentNotFound (#4)"
    );
}

/// assert_data_hash returns StatusHashNotFound for a status that was never set.
#[test]
fn test_assert_data_hash_unset_status_returns_status_hash_not_found() {
    use crate::NavinError;
    let (env, client, admin, _token_contract) = crate::test::setup_shipment_env();
    client.initialize(&admin, &_token_contract);

    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 86400;

    client.add_company(&admin, &sender);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // AtCheckpoint was never set.
    let probe = BytesN::from_array(&env, &[7u8; 32]);
    let result = client.try_assert_data_hash(&shipment_id, &crate::ShipmentStatus::AtCheckpoint, &probe);
    assert_eq!(
        result,
        Err(Ok(NavinError::StatusHashNotFound)),
        "assert_data_hash for unset status must return StatusHashNotFound (#44)"
    );
}

/// DataHashMismatch (#45) and StatusHashNotFound (#44) are distinct error codes.
#[test]
fn test_data_hash_mismatch_is_distinct_from_status_hash_not_found() {
    use crate::NavinError;
    assert_ne!(
        NavinError::DataHashMismatch as u32,
        NavinError::StatusHashNotFound as u32,
    );
    assert_eq!(NavinError::DataHashMismatch as u32, 45);
    assert_eq!(NavinError::StatusHashNotFound as u32, 44);
}

/// A hash that differs by a single byte still triggers DataHashMismatch —
/// no partial-match behaviour.
#[test]
fn test_assert_delivery_hash_single_byte_difference_returns_mismatch() {
    use crate::NavinError;
    let (env, client, admin, _token_contract) = crate::test::setup_shipment_env();
    client.initialize(&admin, &_token_contract);

    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 86400;

    client.add_company(&admin, &sender);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    crate::test_utils::advance_past_rate_limit(&env);
    let transit_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.update_status(&carrier, &shipment_id, &crate::ShipmentStatus::InTransit, &transit_hash);

    let mut conf_bytes = [0x10u8; 32];
    client.confirm_delivery(&receiver, &shipment_id, &BytesN::from_array(&env, &conf_bytes));

    // Flip a single byte.
    conf_bytes[31] ^= 0x01;
    let off_by_one = BytesN::from_array(&env, &conf_bytes);

    let result = client.try_assert_delivery_hash(&shipment_id, &off_by_one);
    assert_eq!(
        result,
        Err(Ok(NavinError::DataHashMismatch)),
        "single-byte difference must trigger DataHashMismatch"
    );
}

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
    assert_eq!(shipment_created_event.0, client.address);

    // 3. Verification Step: Verify Topics
    let topic_0: Symbol = shipment_created_event
        .1
        .get(0)
        .unwrap()
        .try_into_val(&env)
        .unwrap();
    assert_eq!(topic_0, target_topic);

    // 4. Verification Step: Verify Data Hash and Fields
    let event_data: Vec<soroban_sdk::Val> = shipment_created_event.2.try_into_val(&env).unwrap();

    let shipment_id: u64 = event_data.get(0).unwrap().try_into_val(&env).unwrap();
    let event_sender: Address = event_data.get(1).unwrap().try_into_val(&env).unwrap();
    let event_receiver: Address = event_data.get(2).unwrap().try_into_val(&env).unwrap();
    let _event_token: Address = event_data.get(3).unwrap().try_into_val(&env).unwrap();
    let event_data_hash: BytesN<32> = event_data.get(4).unwrap().try_into_val(&env).unwrap();
    let event_counter: u32 = event_data.get(6).unwrap().try_into_val(&env).unwrap();
    let event_idempotency_key: BytesN<32> = event_data.get(7).unwrap().try_into_val(&env).unwrap();

    assert_eq!(shipment_id, 1);
    assert_eq!(event_sender, sender);
    assert_eq!(event_receiver, receiver);
    assert_eq!(event_data_hash, data_hash);
    assert_eq!(event_counter, 1);

    // 5. Verification Step: Verify Idempotency Key
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
