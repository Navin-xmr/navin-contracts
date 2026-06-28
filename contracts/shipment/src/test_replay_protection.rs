#![cfg(test)]

use crate::{
    NavinError, NavinShipmentClient, ShipmentStatus, NavinShipment,
};
use soroban_sdk::{
    contract, contractimpl,
    testutils::Address as _,
    Address, BytesN, Env,
};

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}

    pub fn decimals(_env: Env) -> u32 {
        crate::types::EXPECTED_TOKEN_DECIMALS
    }
}

// ── Cross-shipment idempotency isolation tests ─────────────────────────────────
//
// These tests prove that idempotency windows remain isolated per shipment and
// do not bleed across unrelated records.  The idempotency key storage is a flat
// namespace keyed by SHA-256(action_hash).  Isolation relies on the action
// payload including a shipment-unique field (shipment_id for status updates,
// sender for creation).  If that field were ever omitted two different shipments
// could produce the same action hash and incorrectly block each other.
//
// 1. test_cross_shipment_status_update_isolation
//    Two shipments updated with the same (new_status, data_hash) must both
//    succeed because the shipment_id differs in the payload.
//
// 2. test_cross_shipment_create_shipment_isolation
//    Different companies using an identical data_hash must both succeed because
//    the XDR-encoded sender address differs in the payload.
//
// 3. test_cross_shipment_no_interference
//    An idempotency window created by an action on shipment A must not prevent
//    any action on shipment B.
//
// 4. test_replay_semantics_remain_deterministic
//    Within the same shipment, replaying an identical action is always blocked
//    (proving the idempotency window itself works correctly).

const DEFAULT_DEADLINE_OFFSET: u64 = 3600;

fn setup_initialized_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = super::test_utils::setup_env();
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token_contract);
    (env, client, admin, token_contract)
}

fn create_company(env: &Env, client: &NavinShipmentClient, admin: &Address) -> Address {
    let company = Address::generate(env);
    client.add_company(admin, &company);
    company
}

fn create_carrier(env: &Env, client: &NavinShipmentClient, admin: &Address) -> Address {
    let carrier = Address::generate(env);
    client.add_carrier(admin, &carrier);
    carrier
}

fn create_shipment(
    env: &Env,
    client: &NavinShipmentClient,
    company: &Address,
    carrier: &Address,
    data_hash: &BytesN<32>,
) -> u64 {
    let receiver = Address::generate(env);
    let deadline = env.ledger().timestamp() + DEFAULT_DEADLINE_OFFSET;
    client.create_shipment(
        company,
        &receiver,
        carrier,
        data_hash,
        &soroban_sdk::Vec::new(env),
        &deadline,
    )
}

// ── Test 1: status-update isolation across two shipments ──────────────────────

#[test]
fn test_cross_shipment_status_update_isolation() {
    let (env, client, admin, _token) = setup_initialized_env();
    let company = create_company(&env, &client, &admin);
    let carrier = create_carrier(&env, &client, &admin);

    let hash_a = BytesN::from_array(&env, &[1u8; 32]);
    let hash_b = BytesN::from_array(&env, &[2u8; 32]);

    // Create two shipments with different data hashes.
    let id1 = create_shipment(&env, &client, &company, &carrier, &hash_a);
    let id2 = create_shipment(&env, &client, &company, &carrier, &hash_b);

    // Shipment 1: Created → InTransit with hash_x.
    let status_hash = BytesN::from_array(&env, &[0xAAu8; 32]);
    client.update_status(&carrier, &id1, &ShipmentStatus::InTransit, &status_hash);

    // Shipment 2: Created → InTransit with the *same* status + data_hash.
    // This must succeed because the payload includes shipment_id, producing a
    // different action hash than the one stored for shipment 1.
    super::test_utils::advance_past_rate_limit(&env);
    client.update_status(&carrier, &id2, &ShipmentStatus::InTransit, &status_hash);

    assert_eq!(
        client.get_shipment(&id1).status,
        ShipmentStatus::InTransit
    );
    assert_eq!(
        client.get_shipment(&id2).status,
        ShipmentStatus::InTransit
    );
}

// ── Test 2: create-shipment isolation across different companies ──────────────

#[test]
fn test_cross_shipment_create_shipment_isolation() {
    let (env, client, admin, _token) = setup_initialized_env();

    // Two different companies.
    let company_a = create_company(&env, &client, &admin);
    let company_b = create_company(&env, &client, &admin);
    let carrier = create_carrier(&env, &client, &admin);
    let receiver = Address::generate(&env);
    let deadline = env.ledger().timestamp() + DEFAULT_DEADLINE_OFFSET;

    // Both companies use the **same** data_hash.
    let data_hash = BytesN::from_array(&env, &[0xBBu8; 32]);

    // First company creates a shipment — succeeds.
    let id_a = client.create_shipment(
        &company_a,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // Second company creates a shipment with the same data_hash.
    // Must succeed because XDR(company_a) != XDR(company_b), so the action
    // hashes differ.
    let id_b = client.create_shipment(
        &company_b,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    assert_ne!(id_a, id_b, "shipments must have distinct IDs");
    assert_eq!(client.get_shipment(&id_a).sender, company_a);
    assert_eq!(client.get_shipment(&id_b).sender, company_b);
}

// ── Test 3: idempotency windows do not interfere across unrelated shipments ──

#[test]
fn test_cross_shipment_no_interference() {
    let (env, client, admin, _token) = setup_initialized_env();
    let company = create_company(&env, &client, &admin);
    let carrier = create_carrier(&env, &client, &admin);

    let hash_a = BytesN::from_array(&env, &[3u8; 32]);
    let hash_b = BytesN::from_array(&env, &[4u8; 32]);

    // Two shipments with different data hashes, same company & carrier.
    let id_a = create_shipment(&env, &client, &company, &carrier, &hash_a);
    let id_b = create_shipment(&env, &client, &company, &carrier, &hash_b);

    // Update shipment A: Created → InTransit.
    let status_hash_a = BytesN::from_array(&env, &[0xCCu8; 32]);
    client.update_status(&carrier, &id_a, &ShipmentStatus::InTransit, &status_hash_a);

    // Update shipment B: Created → InTransit with a different status_hash.
    // This is an entirely unrelated record — must not be blocked.
    super::test_utils::advance_past_rate_limit(&env);
    let status_hash_b = BytesN::from_array(&env, &[0xDDu8; 32]);
    client.update_status(&carrier, &id_b, &ShipmentStatus::InTransit, &status_hash_b);

    // Now verify that replay on shipment A is still blocked (its idempotency
    // window from the first update is still active within the 300 s window).
    let res = client.try_update_status(
        &carrier,
        &id_a,
        &ShipmentStatus::InTransit,
        &status_hash_a,
    );
    assert_eq!(
        res,
        Err(Ok(NavinError::DuplicateAction)),
        "replay of identical action on same shipment must be rejected"
    );

    // Shipment B's own identical replay must also be rejected.
    let res = client.try_update_status(
        &carrier,
        &id_b,
        &ShipmentStatus::InTransit,
        &status_hash_b,
    );
    assert_eq!(
        res,
        Err(Ok(NavinError::DuplicateAction)),
        "replay on shipment B must also be rejected"
    );

    // Cross-shipment replay: try to use shipment A's action on shipment B.
    // This must succeed because the payload includes shipment_id.
    super::test_utils::advance_past_rate_limit(&env);
    client.update_status(&carrier, &id_b, &ShipmentStatus::AtCheckpoint, &status_hash_a);

    assert_eq!(
        client.get_shipment(&id_b).status,
        ShipmentStatus::AtCheckpoint,
        "shipment B must transit to AtCheckpoint unaffected by A's idempotency window"
    );
}

// ── Test 4: replay semantics remain deterministic ─────────────────────────────

#[test]
fn test_replay_semantics_remain_deterministic() {
    let (env, client, admin, _token) = setup_initialized_env();
    let company = create_company(&env, &client, &admin);
    let carrier = create_carrier(&env, &client, &admin);

    let data_hash = BytesN::from_array(&env, &[5u8; 32]);
    let shipment_id = create_shipment(&env, &client, &company, &carrier, &data_hash);

    // First status update: must succeed.
    let status_hash = BytesN::from_array(&env, &[0xEEu8; 32]);
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &status_hash);

    // Immediate replay with identical arguments: must be blocked deterministically.
    let res = client.try_update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &status_hash,
    );
    assert_eq!(
        res,
        Err(Ok(NavinError::DuplicateAction)),
        "identical replay must always produce DuplicateAction"
    );

    // Different data_hash on the same shipment & same status: different action
    // hash → passes idempotency (different hash). Must advance past rate limit
    // so the rate-limit check does not mask the state-machine result.
    super::test_utils::advance_past_rate_limit(&env);
    let res = client.try_update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[0xFFu8; 32]),
    );
    assert_eq!(
        res,
        Err(Ok(NavinError::InvalidStatus)),
        "different hash on same status must pass idempotency but fail state machine"
    );

    // Create a fresh shipment and verify that creating with the same company +
    // same data_hash is blocked (create-shipment idempotency).
    let res = client.try_create_shipment(
        &company,
        &Address::generate(&env),
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &(env.ledger().timestamp() + DEFAULT_DEADLINE_OFFSET),
    );
    assert_eq!(
        res,
        Err(Ok(NavinError::DuplicateAction)),
        "create_shipment replay with same (sender, data_hash) must be rejected"
    );
}
