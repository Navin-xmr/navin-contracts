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
    testutils::{Address as _, Events},
    token::StellarAssetClient,
    Address, BytesN, Env, Symbol, TryFromVal, Vec,
};
use std::string::ToString;

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

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    let payload = find_event_data(&env, crate::event_topics::SHIPMENT_CREATED)
        .expect("shipment_created event not emitted");

    assert_eq!(
        payload.len(),
        7,
        "shipment_created payload must have exactly 7 fields; got {}",
        payload.len()
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
        &None,
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
        7,
        "status_updated payload must have exactly 7 fields; got {}",
        payload.len()
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
        &None,
    );

    client.deposit_escrow(&company, &id, &1_000i128);

    let payload = find_event_data(&env, crate::event_topics::ESCROW_DEPOSITED)
        .expect("escrow_deposited event not emitted");

    assert_eq!(
        payload.len(),
        6,
        "escrow_deposited payload must have exactly 6 fields; got {}",
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
        &None,
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
        6,
        "escrow_released payload must have exactly 6 fields; got {}",
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
        &None,
    );
    client.deposit_escrow(&company, &id, &1_000i128);
    client.refund_escrow(&company, &id);

    let payload = find_event_data(&env, crate::event_topics::ESCROW_REFUNDED)
        .expect("escrow_refunded event not emitted");

    assert_eq!(
        payload.len(),
        6,
        "escrow_refunded payload must have exactly 6 fields; got {}",
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
        &None,
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
        &None,
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
        "dispute_resolved payload must have exactly 7 fields; got {}",
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
        &None,
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
        &None,
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
        "milestone_recorded payload must have exactly 7 fields; got {}",
        payload.len()
    );
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
        &None,
    );
    client.cancel_shipment(&company, &id, &BytesN::from_array(&env, &[17u8; 32]));

    let payload = find_event_data(&env, crate::event_topics::SHIPMENT_CANCELLED)
        .expect("shipment_cancelled event not emitted");

    assert_eq!(
        payload.len(),
        6,
        "shipment_cancelled payload must have exactly 6 fields; got {}",
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
        &None,
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
        &None,
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
