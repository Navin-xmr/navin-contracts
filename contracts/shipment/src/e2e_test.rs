// =============================================================================
// End-to-End Integration Test: NavinShipment + NavinToken (Real Token Contract)
//
// Covers four lifecycle paths, all with real token balance verification:
//   1. HAPPY PATH          — deposit → milestones (100 %) → delivery → full release
//   2. CANCEL / REFUND     — deposit → refund_escrow → tokens returned
//   3. PARTIAL + CANCEL    — partial milestone payouts, then cancel_shipment refunds remainder
//   4. DEADLINE EXPIRY     — check_deadline auto-cancels and refunds
//
// Run: cargo test --lib e2e_test
// =============================================================================

#![cfg(test)]

extern crate std;

use crate::{NavinShipment, NavinShipmentClient, ShipmentStatus};
use navin_token::{NavinToken, NavinTokenClient};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger as _},
    Address, BytesN, Env, FromVal, String, Symbol, Vec,
};
use std::string::ToString;

// ---------------------------------------------------------------------------
// Helper: advance ledger time to bypass min_status_update_interval
// ---------------------------------------------------------------------------
fn advance_time(env: &Env, seconds: u64) {
    let current = env.ledger().timestamp();
    env.ledger().set_timestamp(current + seconds);
}

// ---------------------------------------------------------------------------
// Helper: deploy + initialise NavinToken, giving admin the initial supply
// ---------------------------------------------------------------------------
fn deploy_token<'a>(env: &'a Env, admin: &Address) -> (Address, NavinTokenClient<'a>) {
    let token_id = env.register(NavinToken, ());
    let token = NavinTokenClient::new(env, &token_id);
    token.initialize(
        admin,
        &String::from_str(env, "NavinToken"),
        &String::from_str(env, "NVN"),
        &1_000_000_i128,
    );
    (token_id, token)
}

// ---------------------------------------------------------------------------
// Helper: deploy + initialise NavinShipment
// ---------------------------------------------------------------------------
fn deploy_shipment<'a>(
    env: &'a Env,
    admin: &Address,
    token_id: &Address,
) -> NavinShipmentClient<'a> {
    let contract_id = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(env, &contract_id);
    client.initialize(admin, token_id);
    client
}

// ---------------------------------------------------------------------------
// Helper: 32-byte hash seeded from one byte
// ---------------------------------------------------------------------------
fn hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

// ---------------------------------------------------------------------------
// Helper: check whether any emitted event has `name` as its first topic.
//
// env.events().all() returns Vec<(Address, Vec<Val>, Val)>:
//   .0 = contract address that emitted the event
//   .1 = topics  (the tuple passed as first arg to publish, serialised as Vec<Val>)
//   .2 = data    (the second arg to publish)
//
// The contract always publishes with a single-element tuple topic:
//   env.events().publish((Symbol::new(env, "foo"),), data)
// so topics[0] is the Symbol we want to match.
// ---------------------------------------------------------------------------
fn has_event(env: &Env, name: &str) -> bool {
    env.events().all().iter().any(|(_contract, topics, _data)| {
        topics.iter().any(|v| {
            let s: soroban_sdk::Symbol = soroban_sdk::Symbol::from_val(env, &v);
            let sym_str: std::string::String = s.to_string();
            sym_str == name
        })
    })
}

#[test]
fn test_debug_event_structure() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    let (token_id, _token) = deploy_token(&env, &admin);
    let shipment = deploy_shipment(&env, &admin, &token_id);
    shipment.add_company(&admin, &company);
    shipment.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 86_400;
    shipment.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash(&env, 0xAA),
        &Vec::new(&env),
        &deadline,
    );

    // What does our target symbol look like as a string?
    let target = Symbol::new(&env, "shipment_created");
    let target_str: std::string::String = target.to_string();
    std::println!("TARGET string: {:?}", target_str);

    // What do the event topic symbols look like as strings?
    for (_contract, topics, _data) in env.events().all().iter() {
        for (i, v) in topics.iter().enumerate() {
            let s = soroban_sdk::Symbol::from_val(&env, &v);
            let s_str: std::string::String = s.to_string();
            std::println!("  topic[{}] string: {:?}", i, s_str);
        }
    }
}

// =============================================================================
// TEST 1 — HAPPY PATH WITH MILESTONE-BASED ESCROW RELEASE
//
// Milestone schedule (must sum to exactly 100 when non-empty):
//   "warehouse" = 30 %
//   "port"      = 30 %
//   "final"     = 40 %
//
// Token flow (escrow = 1 000):
//   deposit_escrow    : company  -1000  | contract +1000
//   warehouse reached : contract  -300  | carrier   +300
//   port reached      : contract  -300  | carrier   +600
//   confirm_delivery  : contract  -400  | carrier  +1000
// =============================================================================
#[test]
fn test_e2e_happy_path_with_milestones_and_token_balances() {
    let env = Env::default();
    env.mock_all_auths();

    // ── Actors ────────────────────────────────────────────────────────────────
    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    // ── Deploy both contracts ──────────────────────────────────────────────────
    let (token_id, token) = deploy_token(&env, &admin);
    let shipment = deploy_shipment(&env, &admin, &token_id);

    // ── Roles ──────────────────────────────────────────────────────────────────
    shipment.add_company(&admin, &company);
    shipment.add_carrier(&admin, &carrier);

    // ── Fund company ──────────────────────────────────────────────────────────
    token.mint(&admin, &company, &10_000_i128);
    assert!(has_event(&env, "mint"), "token mint event");
    assert_eq!(
        token.balance(&company),
        10_000,
        "company starts with 10 000"
    );
    assert_eq!(token.balance(&carrier), 0, "carrier starts with 0");

    // ── Milestone schedule: must sum to 100 % ─────────────────────────────────
    let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "warehouse"), 30_u32));
    milestones.push_back((Symbol::new(&env, "port"), 30_u32));
    milestones.push_back((Symbol::new(&env, "final"), 40_u32));

    // ── Create shipment ───────────────────────────────────────────────────────
    let deadline = env.ledger().timestamp() + 86_400;
    let shipment_id = shipment.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash(&env, 0xAA),
        &milestones,
        &deadline,
    );
    assert_eq!(shipment_id, 1, "first shipment id should be 1");
    assert!(
        has_event(&env, "shipment_created"),
        "shipment_created event"
    );

    let s = shipment.get_shipment(&shipment_id);
    assert_eq!(s.status, ShipmentStatus::Created);
    assert_eq!(s.escrow_amount, 0);

    // =========================================================================
    // STEP 1 — Company deposits 1 000 tokens
    // =========================================================================
    shipment.deposit_escrow(&company, &shipment_id, &1_000_i128);
    assert!(
        has_event(&env, "escrow_deposited"),
        "escrow_deposited event"
    );
    assert!(
        has_event(&env, "transfer"),
        "token transfer event on deposit"
    );

    assert_eq!(
        token.balance(&company),
        9_000,
        "company -1000 after deposit"
    );
    assert_eq!(
        token.balance(&shipment.address),
        1_000,
        "contract holds 1000 in escrow"
    );

    let s = shipment.get_shipment(&shipment_id);
    assert_eq!(s.escrow_amount, 1_000);
    assert_eq!(s.total_escrow, 1_000);

    // =========================================================================
    // STEP 2 — Carrier moves to InTransit
    // =========================================================================
    shipment.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &hash(&env, 0xBB),
    );
    assert!(has_event(&env, "status_updated"), "status_updated event");
    assert_eq!(
        shipment.get_shipment(&shipment_id).status,
        ShipmentStatus::InTransit
    );

    // =========================================================================
    // STEP 3 — "warehouse" milestone → 30 % = 300 tokens released
    // =========================================================================
    advance_time(&env, 120);
    shipment.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "warehouse"),
        &hash(&env, 0xCC),
    );
    assert!(
        has_event(&env, "milestone_recorded"),
        "milestone_recorded event"
    );
    assert!(
        has_event(&env, "escrow_released"),
        "escrow_released event after warehouse"
    );
    assert_eq!(token.balance(&carrier), 300, "carrier +300 after warehouse");
    assert_eq!(token.balance(&shipment.address), 700, "contract holds 700");
    assert_eq!(shipment.get_shipment(&shipment_id).escrow_amount, 700);
    assert_eq!(shipment.get_shipment(&shipment_id).paid_milestones.len(), 1);

    // =========================================================================
    // STEP 4 — "port" milestone → 30 % = 300 more tokens released
    // =========================================================================
    advance_time(&env, 120);
    shipment.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "port"),
        &hash(&env, 0xDD),
    );
    assert!(
        has_event(&env, "milestone_recorded"),
        "milestone_recorded event port"
    );
    assert!(
        has_event(&env, "escrow_released"),
        "escrow_released event after port"
    );
    assert_eq!(token.balance(&carrier), 600, "carrier +300 after port");
    assert_eq!(token.balance(&shipment.address), 400, "contract holds 400");
    assert_eq!(shipment.get_shipment(&shipment_id).escrow_amount, 400);
    assert_eq!(shipment.get_shipment(&shipment_id).paid_milestones.len(), 2);

    // =========================================================================
    // STEP 5 — Receiver confirms delivery → remaining 400 tokens released
    // =========================================================================
    let confirmation_hash = hash(&env, 0xEE);
    shipment.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);
    assert!(
        has_event(&env, "delivery_confirmed"),
        "delivery_confirmed event"
    );
    assert!(
        has_event(&env, "escrow_released"),
        "escrow_released event on delivery"
    );
    assert!(
        has_event(&env, "transfer"),
        "token transfer event on delivery"
    );

    assert_eq!(token.balance(&carrier), 1_000, "carrier receives full 1000");
    assert_eq!(
        token.balance(&shipment.address),
        0,
        "contract holds 0 after delivery"
    );
    assert_eq!(
        token.balance(&company),
        9_000,
        "company unchanged since deposit"
    );

    let s = shipment.get_shipment(&shipment_id);
    assert_eq!(s.status, ShipmentStatus::Delivered);
    assert_eq!(s.escrow_amount, 0);

    // =========================================================================
    // STEP 6 — Verify delivery proof
    // =========================================================================
    assert!(
        shipment.verify_delivery_proof(&shipment_id, &confirmation_hash),
        "correct hash must verify"
    );
    assert!(
        !shipment.verify_delivery_proof(&shipment_id, &hash(&env, 0xFF)),
        "wrong hash must not verify"
    );

    std::println!("✅ Happy path — all balances and events verified");
}

// =============================================================================
// TEST 2 — CANCEL / REFUND PATH
// =============================================================================
#[test]
fn test_e2e_cancel_refund_path_with_token_balances() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    let (token_id, token) = deploy_token(&env, &admin);
    let shipment = deploy_shipment(&env, &admin, &token_id);

    shipment.add_company(&admin, &company);
    shipment.add_carrier(&admin, &carrier);

    token.mint(&admin, &company, &5_000_i128);
    assert!(has_event(&env, "mint"), "token mint event");
    assert_eq!(token.balance(&company), 5_000);

    let deadline = env.ledger().timestamp() + 86_400;
    let shipment_id = shipment.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash(&env, 0x01),
        &Vec::new(&env),
        &deadline,
    );
    assert_eq!(shipment_id, 1);
    assert!(
        has_event(&env, "shipment_created"),
        "shipment_created event"
    );

    // =========================================================================
    // STEP 1 — Deposit 2 000 tokens
    // =========================================================================
    shipment.deposit_escrow(&company, &shipment_id, &2_000_i128);
    assert!(
        has_event(&env, "escrow_deposited"),
        "escrow_deposited event"
    );
    assert!(
        has_event(&env, "transfer"),
        "token transfer event on deposit"
    );
    assert_eq!(
        token.balance(&company),
        3_000,
        "company -2000 after deposit"
    );
    assert_eq!(
        token.balance(&shipment.address),
        2_000,
        "contract holds 2000"
    );
    assert_eq!(shipment.get_shipment(&shipment_id).escrow_amount, 2_000);

    // =========================================================================
    // STEP 2 — refund_escrow
    // =========================================================================
    shipment.refund_escrow(&company, &shipment_id);
    assert!(has_event(&env, "escrow_refunded"), "escrow_refunded event");
    assert!(
        has_event(&env, "transfer"),
        "token transfer event on refund"
    );

    assert_eq!(
        token.balance(&company),
        5_000,
        "company fully refunded to 5000"
    );
    assert_eq!(
        token.balance(&shipment.address),
        0,
        "contract holds 0 after refund"
    );
    assert_eq!(token.balance(&carrier), 0, "carrier untouched");

    let s = shipment.get_shipment(&shipment_id);
    assert_eq!(s.status, ShipmentStatus::Cancelled);
    assert_eq!(s.escrow_amount, 0);

    std::println!("✅ Cancel/refund path — balances and events verified");
}

// =============================================================================
// TEST 3 — PARTIAL MILESTONES THEN CANCEL VIA DEADLINE
// =============================================================================
#[test]
fn test_e2e_partial_milestones_then_cancel_via_deadline() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    let (token_id, token) = deploy_token(&env, &admin);
    let shipment = deploy_shipment(&env, &admin, &token_id);

    shipment.add_company(&admin, &company);
    shipment.add_carrier(&admin, &carrier);

    token.mint(&admin, &company, &4_000_i128);
    assert!(has_event(&env, "mint"), "token mint event");
    assert_eq!(token.balance(&company), 4_000);

    let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "pickup"), 20_u32));
    milestones.push_back((Symbol::new(&env, "transit"), 30_u32));
    milestones.push_back((Symbol::new(&env, "rest"), 50_u32));

    let deadline = env.ledger().timestamp() + 3_600;
    let shipment_id = shipment.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash(&env, 0xA1),
        &milestones,
        &deadline,
    );
    assert!(
        has_event(&env, "shipment_created"),
        "shipment_created event"
    );

    // ── Deposit 2 000 tokens ──────────────────────────────────────────────────
    shipment.deposit_escrow(&company, &shipment_id, &2_000_i128);
    assert!(
        has_event(&env, "escrow_deposited"),
        "escrow_deposited event"
    );
    assert_eq!(token.balance(&company), 2_000);
    assert_eq!(token.balance(&shipment.address), 2_000);

    // ── InTransit ─────────────────────────────────────────────────────────────
    shipment.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &hash(&env, 0xA2),
    );
    assert!(has_event(&env, "status_updated"), "status_updated event");

    // ── "pickup" milestone → 20 % = 400 tokens ───────────────────────────────
    advance_time(&env, 120);
    shipment.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "pickup"),
        &hash(&env, 0xA3),
    );
    assert!(
        has_event(&env, "milestone_recorded"),
        "milestone_recorded pickup event"
    );
    assert!(
        has_event(&env, "escrow_released"),
        "escrow_released after pickup"
    );
    assert_eq!(token.balance(&carrier), 400, "carrier +400 after pickup");
    assert_eq!(
        token.balance(&shipment.address),
        1_600,
        "contract holds 1600"
    );

    // ── "transit" milestone → 30 % = 600 tokens ──────────────────────────────
    advance_time(&env, 120);
    shipment.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "transit"),
        &hash(&env, 0xA4),
    );
    assert!(
        has_event(&env, "milestone_recorded"),
        "milestone_recorded transit event"
    );
    assert!(
        has_event(&env, "escrow_released"),
        "escrow_released after transit"
    );
    assert_eq!(
        token.balance(&carrier),
        1_000,
        "carrier total 1000 after transit"
    );
    assert_eq!(
        token.balance(&shipment.address),
        1_000,
        "contract holds 1000 remaining"
    );

    // ── Advance past deadline and trigger permissionless expiry refund ────────
    advance_time(&env, 7_200);
    shipment.check_deadline(&shipment_id);
    assert!(
        has_event(&env, "shipment_expired"),
        "shipment_expired event"
    );
    assert!(has_event(&env, "escrow_refunded"), "escrow_refunded event");

    assert_eq!(
        token.balance(&company),
        3_000,
        "company recovers 1000 (2000+1000)"
    );
    assert_eq!(token.balance(&carrier), 1_000, "carrier keeps earned 1000");
    assert_eq!(token.balance(&shipment.address), 0, "contract holds 0");

    assert_eq!(
        token.balance(&company) + token.balance(&carrier),
        4_000,
        "token conservation: all 4000 minted tokens accounted for"
    );

    let s = shipment.get_shipment(&shipment_id);
    assert_eq!(s.status, ShipmentStatus::Cancelled);
    assert_eq!(s.escrow_amount, 0);

    std::println!("✅ Partial milestones + deadline expiry — conservation verified");
}

// =============================================================================
// TEST 4 — DEADLINE EXPIRY AUTO-CANCEL AND REFUND
// =============================================================================
#[test]
fn test_e2e_deadline_expiry_auto_cancel_and_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    let (token_id, token) = deploy_token(&env, &admin);
    let shipment = deploy_shipment(&env, &admin, &token_id);

    shipment.add_company(&admin, &company);
    shipment.add_carrier(&admin, &carrier);

    token.mint(&admin, &company, &3_000_i128);
    assert!(has_event(&env, "mint"), "token mint event");
    assert_eq!(token.balance(&company), 3_000);

    let deadline = env.ledger().timestamp() + 3_600;
    let shipment_id = shipment.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash(&env, 0xB1),
        &Vec::new(&env),
        &deadline,
    );
    assert!(
        has_event(&env, "shipment_created"),
        "shipment_created event"
    );

    // ── Deposit 3 000 tokens ──────────────────────────────────────────────────
    shipment.deposit_escrow(&company, &shipment_id, &3_000_i128);
    assert!(
        has_event(&env, "escrow_deposited"),
        "escrow_deposited event"
    );
    assert!(
        has_event(&env, "transfer"),
        "token transfer event on deposit"
    );
    assert_eq!(
        token.balance(&company),
        0,
        "company has 0 after full deposit"
    );
    assert_eq!(
        token.balance(&shipment.address),
        3_000,
        "contract holds 3000"
    );

    // ── Advance 2 hours past the 1-hour deadline ──────────────────────────────
    advance_time(&env, 7_200);

    // ── Any caller triggers expiry ────────────────────────────────────────────
    shipment.check_deadline(&shipment_id);
    assert!(
        has_event(&env, "shipment_expired"),
        "shipment_expired event"
    );
    assert!(
        has_event(&env, "escrow_refunded"),
        "escrow_refunded event on expiry"
    );
    assert!(
        has_event(&env, "transfer"),
        "token transfer on expiry refund"
    );

    assert_eq!(token.balance(&company), 3_000, "company fully refunded");
    assert_eq!(token.balance(&shipment.address), 0, "contract holds 0");
    assert_eq!(token.balance(&carrier), 0, "carrier untouched");

    let s = shipment.get_shipment(&shipment_id);
    assert_eq!(s.status, ShipmentStatus::Cancelled);
    assert_eq!(s.escrow_amount, 0);

    std::println!("✅ Deadline expiry — balances and events verified");
}
