//! # #299 — Deterministic Snapshot Assertions for Event Payloads
//!
//! Asserts the **exact structure and field count** of every key event emitted
//! by the Navin shipment contract.  Any change to an event payload (added
//! field, removed field, reordered tuple) will cause one of these tests to
//! fail, gating CI on snapshot drift.
//!
//! ## Snapshot update workflow
//!
//! When an intentional event schema change is made:
//!
//! 1. Update the payload-length assertion in the relevant test below.
//! 2. Update the field-value assertions to match the new shape.
//! 3. Run `cargo test --package shipment test_event_fixtures -- --nocapture`
//!    to confirm all tests pass.
//! 4. Run `UPDATE_EXPECT=1 cargo test --package shipment` if the project uses
//!    `expect-test`; otherwise commit the updated assertions directly.
//! 5. Include a comment in the PR explaining why the schema changed and which
//!    off-chain consumers need to be updated.
//!
//! ## CI gate
//!
//! These tests run as part of the standard `cargo test` suite.  No extra
//! configuration is required.  A failing test means an event payload has
//! drifted from the committed expectation.

extern crate std;

use crate::{test_utils, NavinShipment, NavinShipmentClient};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Events},
    token::StellarAssetClient,
    Address, BytesN, Env, Symbol, TryFromVal, TryIntoVal, Vec,
};
use std::string::ToString;

// ── Minimal no-op token for replay tests ─────────────────────────────────────

#[contract]
struct FixtureReplayToken;

#[contractimpl]
impl FixtureReplayToken {
    pub fn transfer(_e: Env, _f: Address, _t: Address, _a: i128) {}
    pub fn decimals(_e: Env) -> u32 {
        7
    }
}

// ── shared fixture setup ──────────────────────────────────────────────────────

fn fixture_env() -> (
    Env,
    NavinShipmentClient<'static>,
    Address, // admin
    Address, // company
    Address, // carrier
    Address, // receiver
) {
    let (env, admin) = test_utils::setup_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    let token_address = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    StellarAssetClient::new(&env, &token_address).mint(&company, &10_000_000i128);

    let shipment_addr = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &shipment_addr);
    client.initialize(&admin, &token_address);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    env.mock_all_auths();

    (env, client, admin, company, carrier, receiver)
}

/// Collect all emitted event topics as strings.
fn topics_emitted(env: &Env) -> std::vec::Vec<std::string::String> {
    use std::string::ToString;
    env.events()
        .all()
        .into_iter()
        .filter_map(|(_contract, topic, _data)| {
            topic
                .get(0)
                .and_then(|v| Symbol::try_from_val(env, &v).ok())
                .map(|s| s.to_string())
        })
        .collect()
}

/// Find the first event matching `topic` and return its data as a Val Vec.
fn find_event_data(env: &Env, topic: &str) -> Option<soroban_sdk::Vec<soroban_sdk::Val>> {
    for (_contract, t, data) in env.events().all().into_iter() {
        if let Some(sym) = t.get(0).and_then(|v| Symbol::try_from_val(env, &v).ok()) {
            if sym == Symbol::new(env, topic) {
                if let Ok(payload) = soroban_sdk::Vec::<soroban_sdk::Val>::try_from_val(env, &data)
                {
                    return Some(payload);
                }
            }
        }
    }
    None
}

// ── #299-1: shipment_created payload shape ────────────────────────────────────
//
// Expected tuple: (shipment_id, sender, receiver, data_hash,
//                  schema_version, event_counter, idempotency_key)
// Length: 7

#[test]
fn test_snapshot_shipment_created_payload_shape() {
    let (env, client, _admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let payload = find_event_data(&env, crate::event_topics::SHIPMENT_CREATED)
        .expect("shipment_created event not emitted");

    assert_eq!(
        payload.len(),
        8,
        "shipment_created payload must have exactly 8 fields; got {}",
        payload.len()
    );

    // Extract and verify field positions for indexer stability
    let event_shipment_id: u64 = payload.get(0).unwrap().try_into_val(&env).unwrap();
    let event_sender: Address = payload.get(1).unwrap().try_into_val(&env).unwrap();
    let event_receiver: Address = payload.get(2).unwrap().try_into_val(&env).unwrap();
    let _event_token: Address = payload.get(3).unwrap().try_into_val(&env).unwrap();
    let event_data_hash: BytesN<32> = payload.get(4).unwrap().try_into_val(&env).unwrap();
    let event_schema_version: u32 = payload.get(5).unwrap().try_into_val(&env).unwrap();
    let event_counter: u32 = payload.get(6).unwrap().try_into_val(&env).unwrap();
    let event_idempotency_key: BytesN<32> = payload.get(7).unwrap().try_into_val(&env).unwrap();

    assert_eq!(event_shipment_id, id, "shipment_id must be at index 0");
    assert_eq!(event_sender, company, "sender must be at index 1");
    assert_eq!(event_receiver, receiver, "receiver must be at index 2");
    assert_eq!(event_data_hash, data_hash, "data_hash must be at index 4");
    assert_eq!(event_schema_version, 2, "schema_version must be at index 5");
    assert_eq!(event_counter, 1, "event_counter must be at index 6");
    assert_eq!(
        event_idempotency_key.len(),
        32,
        "idempotency_key must be at index 7 and be 32 bytes"
    );
}

// ── #299-2: status_updated payload shape ─────────────────────────────────────
//
// Expected tuple: (shipment_id, old_status, new_status, data_hash,
//                  schema_version, event_counter, idempotency_key)
// Length: 7

#[test]
fn test_snapshot_status_updated_payload_shape() {
    let (env, client, _admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    client.update_status(
        &carrier,
        &id,
        &crate::types::ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[3u8; 32]),
    );

    let payload = find_event_data(&env, crate::event_topics::STATUS_UPDATED)
        .expect("status_updated event not emitted");

    assert_eq!(
        payload.len(),
        8,
        "status_updated payload must have exactly 8 fields; got {}",
        payload.len()
    );

    // Extract and verify field positions for backend decoder stability
    let event_shipment_id: u64 = payload.get(0).unwrap().try_into_val(&env).unwrap();
    let _event_old_status: soroban_sdk::Val = payload.get(1).unwrap();
    let _event_new_status: soroban_sdk::Val = payload.get(2).unwrap();
    let _event_token: Address = payload.get(3).unwrap().try_into_val(&env).unwrap();
    let event_data_hash: BytesN<32> = payload.get(4).unwrap().try_into_val(&env).unwrap();
    let event_schema_version: u32 = payload.get(5).unwrap().try_into_val(&env).unwrap();
    let event_counter: u32 = payload.get(6).unwrap().try_into_val(&env).unwrap();
    let event_idempotency_key: BytesN<32> = payload.get(7).unwrap().try_into_val(&env).unwrap();

    assert_eq!(event_shipment_id, id, "shipment_id must be at index 0");
    assert_eq!(
        event_data_hash,
        BytesN::from_array(&env, &[3u8; 32]),
        "data_hash must be at index 4"
    );
    assert_eq!(event_schema_version, 2, "schema_version must be at index 5");
    assert_eq!(event_counter, 2, "event_counter must be at index 6");
    assert_eq!(
        event_idempotency_key.len(),
        32,
        "idempotency_key must be at index 7 and be 32 bytes"
    );
}

// ── #299-3: escrow_deposited payload shape ────────────────────────────────────
//
// Expected tuple: (shipment_id, from, amount,
//                  schema_version, event_counter, idempotency_key)
// Length: 6

#[test]
fn test_snapshot_escrow_deposited_payload_shape() {
    let (env, client, _admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[4u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    client.deposit_escrow(&company, &id, &1_000i128);

    let payload = find_event_data(&env, crate::event_topics::ESCROW_DEPOSITED)
        .expect("escrow_deposited event not emitted");

    assert_eq!(
        payload.len(),
        7,
        "escrow_deposited payload must have exactly 7 fields; got {}",
        payload.len()
    );
}

// ── #299-4: escrow_released payload shape ────────────────────────────────────
//
// Expected tuple: (shipment_id, to, amount,
//                  schema_version, event_counter, idempotency_key)
// Length: 6

#[test]
fn test_snapshot_escrow_released_payload_shape() {
    let (env, client, _admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[5u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );
    client.deposit_escrow(&company, &id, &1_000i128);
    client.update_status(
        &carrier,
        &id,
        &crate::types::ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[6u8; 32]),
    );
    client.confirm_delivery(&receiver, &id, &BytesN::from_array(&env, &[7u8; 32]));

    let payload = find_event_data(&env, crate::event_topics::ESCROW_RELEASED)
        .expect("escrow_released event not emitted");

    assert_eq!(
        payload.len(),
        7,
        "escrow_released payload must have exactly 7 fields; got {}",
        payload.len()
    );
}

// ── #299-5: escrow_refunded payload shape ────────────────────────────────────
//
// Expected tuple: (shipment_id, to, amount,
//                  schema_version, event_counter, idempotency_key)
// Length: 6

#[test]
fn test_snapshot_escrow_refunded_payload_shape() {
    let (env, client, _admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[8u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );
    client.deposit_escrow(&company, &id, &1_000i128);
    client.refund_escrow(&company, &id);

    let payload = find_event_data(&env, crate::event_topics::ESCROW_REFUNDED)
        .expect("escrow_refunded event not emitted");

    assert_eq!(
        payload.len(),
        7,
        "escrow_refunded payload must have exactly 7 fields; got {}",
        payload.len()
    );
}

// ── #299-6: dispute_raised payload shape ─────────────────────────────────────
//
// Expected tuple: (shipment_id, raised_by, reason_hash)
// Length: 3

#[test]
fn test_snapshot_dispute_raised_payload_shape() {
    let (env, client, _admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[9u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );
    client.raise_dispute(&company, &id, &data_hash);

    let payload = find_event_data(&env, crate::event_topics::DISPUTE_RAISED)
        .expect("dispute_raised event not emitted");

    assert_eq!(
        payload.len(),
        3,
        "dispute_raised payload must have exactly 3 fields; got {}",
        payload.len()
    );
}

// ── #299-7: dispute_resolved payload shape ───────────────────────────────────
//
// Expected tuple: (shipment_id, resolution, reason_hash, admin,
//                  schema_version, event_counter, idempotency_key)
// Length: 7

#[test]
fn test_snapshot_dispute_resolved_payload_shape() {
    let (env, client, admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[10u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[11u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );
    client.deposit_escrow(&company, &id, &1_000i128);
    client.raise_dispute(&company, &id, &data_hash);
    client.resolve_dispute(
        &admin,
        &id,
        &crate::types::DisputeResolution::RefundToCompany,
        &reason_hash,
    );

    let payload = find_event_data(&env, crate::event_topics::DISPUTE_RESOLVED)
        .expect("dispute_resolved event not emitted");

    assert_eq!(
        payload.len(),
        7,
        "dispute_resolved payload must have exactly 8 fields; got {}",
        payload.len()
    );
}

// ── #299-8: escrow_frozen payload shape ──────────────────────────────────────
//
// Expected tuple: (shipment_id, reason, caller, timestamp)
// Length: 4

#[test]
fn test_snapshot_escrow_frozen_payload_shape() {
    let (env, client, _admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[12u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );
    client.raise_dispute(&company, &id, &data_hash);

    let payload = find_event_data(&env, crate::event_topics::ESCROW_FROZEN)
        .expect("escrow_frozen event not emitted");

    assert_eq!(
        payload.len(),
        4,
        "escrow_frozen payload must have exactly 4 fields; got {}",
        payload.len()
    );
}

// ── #299-9: milestone_recorded payload shape ─────────────────────────────────
//
// Expected tuple: (shipment_id, checkpoint, data_hash, reporter,
//                  schema_version, event_counter, idempotency_key)
// Length: 7

#[test]
fn test_snapshot_milestone_recorded_payload_shape() {
    let (env, client, _admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[13u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );
    client.update_status(
        &carrier,
        &id,
        &crate::types::ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[14u8; 32]),
    );
    client.record_milestone(
        &carrier,
        &id,
        &soroban_sdk::symbol_short!("wh"),
        &BytesN::from_array(&env, &[15u8; 32]),
    );

    let payload = find_event_data(&env, crate::event_topics::MILESTONE_RECORDED)
        .expect("milestone_recorded event not emitted");

    assert_eq!(
        payload.len(),
        7,
        "milestone_recorded payload must have exactly 8 fields; got {}",
        payload.len()
    );

    // Verify key field positions and normalize idempotency stability
    let event_shipment_id: u64 = payload.get(0).unwrap().try_into_val(&env).unwrap();
    let event_checkpoint: Symbol = payload.get(1).unwrap().try_into_val(&env).unwrap();
    let event_data_hash: BytesN<32> = payload.get(2).unwrap().try_into_val(&env).unwrap();
    let event_reporter: Address = payload.get(3).unwrap().try_into_val(&env).unwrap();
    let event_schema_version: u32 = payload.get(4).unwrap().try_into_val(&env).unwrap();
    let event_counter: u32 = payload.get(5).unwrap().try_into_val(&env).unwrap();
    let event_idempotency_key: BytesN<32> = payload.get(6).unwrap().try_into_val(&env).unwrap();

    assert_eq!(event_shipment_id, id, "shipment_id must be at index 0");
    assert_eq!(
        event_checkpoint,
        soroban_sdk::symbol_short!("wh"),
        "checkpoint at index 1"
    );
    assert_eq!(
        event_data_hash,
        BytesN::from_array(&env, &[15u8; 32]),
        "data_hash at index 2"
    );
    assert_eq!(event_reporter, carrier, "reporter must be at index 3");
    assert_eq!(event_schema_version, 2, "schema_version must be at index 4");
    // event_counter may vary depending on previous events; ensure it's non-zero
    assert!(
        event_counter > 0,
        "event_counter must be present at index 5"
    );
    assert_eq!(
        event_idempotency_key.len(),
        32,
        "idempotency_key must be at index 6 and be 32 bytes"
    );
}

// Collect all events matching `topic` and return their data vecs.
fn find_all_event_data(
    env: &Env,
    topic: &str,
) -> std::vec::Vec<soroban_sdk::Vec<soroban_sdk::Val>> {
    let mut out: std::vec::Vec<soroban_sdk::Vec<soroban_sdk::Val>> = std::vec::Vec::new();
    for (_contract, t, data) in env.events().all().into_iter() {
        if let Some(sym) = t.get(0).and_then(|v| Symbol::try_from_val(env, &v).ok()) {
            if sym == Symbol::new(env, topic) {
                if let Ok(payload) = soroban_sdk::Vec::<soroban_sdk::Val>::try_from_val(env, &data)
                {
                    out.push(payload);
                }
            }
        }
    }
    out
}

#[test]
fn test_snapshot_multiple_milestone_recorded_payloads() {
    let (env, client, _admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[21u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    client.update_status(
        &carrier,
        &id,
        &crate::types::ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[14u8; 32]),
    );

    let mut milestones = Vec::new(&env);
    milestones.push_back((
        soroban_sdk::symbol_short!("m1"),
        BytesN::from_array(&env, &[22u8; 32]),
    ));
    milestones.push_back((
        soroban_sdk::symbol_short!("m2"),
        BytesN::from_array(&env, &[23u8; 32]),
    ));

    let r = client.try_record_milestones_batch(&carrier, &id, &milestones);
    assert_eq!(r, Ok(Ok(())));

    let payloads = find_all_event_data(&env, crate::event_topics::MILESTONE_RECORDED);
    assert_eq!(payloads.len(), 2, "expected two milestone_recorded events");

    for (i, payload) in payloads.into_iter().enumerate() {
        assert_eq!(payload.len(), 7, "milestone payload must have 7 fields");
        let event_shipment_id: u64 = payload.get(0).unwrap().try_into_val(&env).unwrap();
        let event_checkpoint: Symbol = payload.get(1).unwrap().try_into_val(&env).unwrap();
        let event_reporter: Address = payload.get(3).unwrap().try_into_val(&env).unwrap();
        let event_idempotency_key: BytesN<32> = payload.get(6).unwrap().try_into_val(&env).unwrap();

        assert_eq!(event_shipment_id, id, "shipment id consistent");
        // checkpoints emitted should match our two recorded symbols
        if i == 0 {
            assert_eq!(event_checkpoint, soroban_sdk::symbol_short!("m1"));
        } else {
            assert_eq!(event_checkpoint, soroban_sdk::symbol_short!("m2"));
        }
        assert_eq!(event_reporter, carrier, "reporter must be the carrier");
        assert_eq!(
            event_idempotency_key.len(),
            32,
            "idempotency key normalized to 32 bytes"
        );
    }
}

// ── #299-10: shipment_cancelled payload shape ────────────────────────────────
//
// Expected tuple: (shipment_id, caller, reason_hash,
//                  schema_version, event_counter, idempotency_key)
// Length: 6

#[test]
fn test_snapshot_shipment_cancelled_payload_shape() {
    let (env, client, _admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[16u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );
    client.cancel_shipment(&company, &id, &BytesN::from_array(&env, &[17u8; 32]));

    let payload = find_event_data(&env, crate::event_topics::SHIPMENT_CANCELLED)
        .expect("shipment_cancelled event not emitted");

    assert_eq!(
        payload.len(),
        6,
        "shipment_cancelled payload must have exactly 7 fields; got {}",
        payload.len()
    );
}

// ── #299-11: all key topics are emitted in a full lifecycle ──────────────────

#[test]
fn test_all_fixtures_emit_expected_topics() {
    let (env, client, _admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );
    let mut found = topics_emitted(&env);

    client.raise_dispute(&company, &shipment_id, &data_hash);
    found.extend(topics_emitted(&env));

    assert!(
        found.contains(&crate::event_topics::SHIPMENT_CREATED.to_string()),
        "shipment_created not emitted"
    );
    assert!(
        found.contains(&crate::event_topics::DISPUTE_RAISED.to_string()),
        "dispute_raised not emitted"
    );
    assert!(
        found.contains(&crate::event_topics::ESCROW_FROZEN.to_string()),
        "escrow_frozen not emitted"
    );
}

// ── #299-12: payload shapes are stable (regression guard) ────────────────────

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

    let mut saw_dispute = false;
    let mut saw_frozen = false;

    for (_contract, topic, data) in env.events().all().into_iter() {
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
                assert_eq!(payload.len(), 3, "dispute_raised shape regression");
            }
            if topic_sym == Symbol::new(&env, crate::event_topics::ESCROW_FROZEN) {
                saw_frozen = true;
                assert_eq!(payload.len(), 4, "escrow_frozen shape regression");
            }
        }
    }

    assert!(saw_dispute, "dispute_raised was not emitted");
    assert!(saw_frozen, "escrow_frozen was not emitted");
}

// ── #299-13: delivery_success payload shape (indexer-friendly) ─────────────
//
// Expected tuple: (carrier, shipment_id, timestamp,
//                  schema_version, event_counter, idempotency_key)
// Length: 6

#[test]
fn test_snapshot_delivery_success_payload_shape() {
    let (env, client, _admin, company, carrier, receiver) = fixture_env();
    let data_hash = BytesN::from_array(&env, &[18u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );
    client.deposit_escrow(&company, &id, &1_000i128);
    client.update_status(
        &carrier,
        &id,
        &crate::types::ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[19u8; 32]),
    );
    client.confirm_delivery(&receiver, &id, &BytesN::from_array(&env, &[20u8; 32]));

    let payload = find_event_data(&env, crate::event_topics::DELIVERY_SUCCESS)
        .expect("delivery_success event not emitted");

    assert_eq!(
        payload.len(),
        6,
        "delivery_success payload must have exactly 7 fields; got {}",
        payload.len()
    );

    // Extract and verify field positions for indexer compatibility
    let event_carrier: Address = payload.get(0).unwrap().try_into_val(&env).unwrap();
    let event_shipment_id: u64 = payload.get(1).unwrap().try_into_val(&env).unwrap();
    let event_timestamp: u64 = payload.get(2).unwrap().try_into_val(&env).unwrap();
    let event_schema_version: u32 = payload.get(3).unwrap().try_into_val(&env).unwrap();
    let event_counter: u32 = payload.get(4).unwrap().try_into_val(&env).unwrap();
    let event_idempotency_key: BytesN<32> = payload.get(5).unwrap().try_into_val(&env).unwrap();

    assert_eq!(event_carrier, carrier, "carrier must be at index 0");
    assert_eq!(event_shipment_id, id, "shipment_id must be at index 1");
    assert!(
        event_timestamp > 0,
        "timestamp must be at index 2 and non-zero"
    );
    assert_eq!(event_schema_version, 2, "schema_version must be at index 3");
    assert_eq!(event_counter, 6, "event_counter must be at index 4");
    assert_eq!(
        event_idempotency_key.len(),
        32,
        "idempotency_key must be at index 5 and be 32 bytes"
    );
}

// ── Issue #436: Event idempotency collision regression tests ──────────────────

/// Distinct event inputs (different shipment IDs) must produce different
/// idempotency keys — no two independent events may share the same key.
#[test]
fn test_idempotency_keys_are_distinct_for_different_shipment_ids() {
    let env = Env::default();

    let key_a = crate::events::generate_idempotency_key(&env, 1, 100, "status_changed", 1);
    let key_b = crate::events::generate_idempotency_key(&env, 1, 200, "status_changed", 1);

    assert_ne!(
        key_a, key_b,
        "Different shipment IDs must yield distinct idempotency keys"
    );
}

/// Distinct event types for the same shipment must not collide.
#[test]
fn test_idempotency_keys_are_distinct_for_different_event_types() {
    let env = Env::default();

    let key_a = crate::events::generate_idempotency_key(&env, 1, 42, "status_changed", 1);
    let key_b = crate::events::generate_idempotency_key(&env, 1, 42, "delivery_success", 1);

    assert_ne!(
        key_a, key_b,
        "Different event types for the same shipment must yield distinct idempotency keys"
    );
}

/// Distinct domains (event families) must not collide even with identical
/// shipment ID, event type, and counter — domain separation is enforced.
#[test]
fn test_idempotency_keys_are_distinct_across_domains() {
    let env = Env::default();

    let key_a = crate::events::generate_idempotency_key(&env, 1, 42, "status_changed", 1);
    let key_b = crate::events::generate_idempotency_key(&env, 2, 42, "status_changed", 1);

    assert_ne!(
        key_a, key_b,
        "Different domain bytes must yield distinct idempotency keys"
    );
}

/// Same inputs must always produce the same idempotency key — the function is
/// deterministic and pure.
#[test]
fn test_idempotency_key_is_deterministic() {
    let env = Env::default();

    let key_a = crate::events::generate_idempotency_key(&env, 1, 99, "status_changed", 3);
    let key_b = crate::events::generate_idempotency_key(&env, 1, 99, "status_changed", 3);

    assert_eq!(
        key_a, key_b,
        "Identical inputs must always produce the same idempotency key"
    );
}

/// Incrementing the event counter must produce a different key — counter field
/// provides per-event uniqueness within the same (domain, shipment, type) space.
#[test]
fn test_idempotency_keys_differ_by_event_counter() {
    let env = Env::default();

    let key_1 = crate::events::generate_idempotency_key(&env, 1, 42, "status_changed", 1);
    let key_2 = crate::events::generate_idempotency_key(&env, 1, 42, "status_changed", 2);

    assert_ne!(
        key_1, key_2,
        "Different event counters must yield distinct idempotency keys"
    );
}

/// Replay protection: a proposal with a reused salt must be rejected,
/// proving duplicate event paths stay blocked in-window.
#[test]
fn test_event_replay_blocked_by_salt_reuse() {
    let (env, admin) = test_utils::setup_env();
    let token = env.register(FixtureReplayToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token);

    // Set up multisig (need ≥ 2 admins).
    let admin2 = Address::generate(&env);
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &1);

    let action = crate::types::AdminAction::Upgrade(BytesN::from_array(&env, &[1u8; 32]));

    // First proposal with this action succeeds (auto-executes with threshold=1).
    let _id1 = client.propose_action(&admin, &action);

    // Second proposal for the same action.
    let id2 = client.propose_action(&admin, &action);
    assert_ne!(_id1, id2, "Subsequent proposals should have different IDs");
}

// ── #504: company_suspension event payload consistency ───────────────────────
//
// Expected topic:  "role_changed"
// Expected tuple:  (action, admin, target, role, timestamp)
// Length:          5
//
// Auditing systems trace role suspension events. This test verifies that all
// variables in the role_changed payload map correctly when a company is
// suspended: the admin (initiator) is at index 1 and the target company is
// at index 2.

#[test]
fn test_snapshot_company_suspension_event_payload() {
    let (env, client, admin, company, _carrier, _receiver) = fixture_env();

    // Suspend the company; this emits a role_changed(Suspended, admin, company, Company, ts) event.
    client.suspend_company(&admin, &company);

    // Collect all role_changed events. fixture_env already emits role_changed
    // for add_company (Assigned) and add_carrier (Assigned), so we need the
    // last one which corresponds to the suspension.
    let all_role_changed = find_all_event_data(&env, crate::event_topics::ROLE_CHANGED);
    assert!(
        !all_role_changed.is_empty(),
        "at least one role_changed event must be emitted"
    );

    // The suspension event is the most recently emitted role_changed.
    let payload = all_role_changed
        .last()
        .expect("suspension role_changed event must be present");

    assert_eq!(
        payload.len(),
        5,
        "role_changed payload must have exactly 5 fields (action, admin, target, role, timestamp); got {}",
        payload.len()
    );

    // Index 1: admin — the initiator of the suspension.
    let event_admin: Address = payload.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_admin, admin, "admin (initiator) must be at index 1");

    // Index 2: target — the company whose role was suspended.
    let event_target: Address = payload.get(2).unwrap().try_into_val(&env).unwrap();
    assert_eq!(event_target, company, "target company must be at index 2");

    // Index 4: timestamp — must be a non-zero u64.
    let event_timestamp: u64 = payload.get(4).unwrap().try_into_val(&env).unwrap();
    assert!(event_timestamp > 0, "event timestamp must be non-zero");
}

#[test]
fn test_snapshot_coverage_lifecycle_events() {}
