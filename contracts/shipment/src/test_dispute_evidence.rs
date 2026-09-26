//! Tests for dispute evidence attachment (issue #884).
//!
//! `EvidenceLimitExceeded`, `EvidenceNotFound` and the `EVIDENCE_ADDED` topic
//! existed as scaffolding with no implementing entrypoint. These tests pin the
//! behaviour of `add_dispute_evidence` / `get_dispute_evidence` and, in doing
//! so, prove both error variants are now reachable from `lib.rs`.
//!
//! Covers:
//! - Evidence appends in order and is readable back by index.
//! - Reading an unwritten index yields `EvidenceNotFound`.
//! - The per-dispute cap yields `EvidenceLimitExceeded`.
//! - Evidence is rejected unless the shipment is actually `Disputed`.
//! - Non-participants are rejected.
//! - An all-zero hash is rejected.

extern crate std;

use crate::{NavinError, NavinShipment, NavinShipmentClient, ShipmentStatus};
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env};

// ── Minimal mock token ────────────────────────────────────────────────────────

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn decimals(_env: Env) -> u32 {
        7
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    pub fn mint(_env: Env, _admin: Address, _to: Address, _amount: i128) {}
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = super::test_utils::setup_env();
    let token = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    (env, client, admin, token)
}

/// Create a shipment and return `(id, company, receiver, carrier)`.
fn create_test_shipment(
    env: &Env,
    client: &NavinShipmentClient,
    admin: &Address,
    token: &Address,
) -> (u64, Address, Address, Address) {
    client.initialize(admin, token);

    let company = Address::generate(env);
    let receiver = Address::generate(env);
    let carrier = Address::generate(env);
    let data_hash = BytesN::from_array(env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.add_company(admin, &company);
    client.add_carrier(admin, &carrier);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(env),
        &deadline,
    );

    (id, company, receiver, carrier)
}

/// Create a shipment and move it into `Disputed`.
fn disputed_shipment(
    env: &Env,
    client: &NavinShipmentClient,
    admin: &Address,
    token: &Address,
) -> (u64, Address, Address, Address) {
    let (id, company, receiver, carrier) = create_test_shipment(env, client, admin, token);
    client.raise_dispute(&receiver, &id, &BytesN::from_array(env, &[7u8; 32]));
    assert_eq!(client.get_shipment(&id).status, ShipmentStatus::Disputed);
    (id, company, receiver, carrier)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Evidence is appended in submission order and reads back by index.
#[test]
fn test_evidence_appends_in_order_and_reads_back() {
    let (env, client, admin, token) = setup();
    let (id, company, receiver, carrier) = disputed_shipment(&env, &client, &admin, &token);

    let first = BytesN::from_array(&env, &[11u8; 32]);
    let second = BytesN::from_array(&env, &[12u8; 32]);
    let third = BytesN::from_array(&env, &[13u8; 32]);

    assert_eq!(client.add_dispute_evidence(&receiver, &id, &first), 0);
    assert_eq!(client.add_dispute_evidence(&carrier, &id, &second), 1);
    assert_eq!(client.add_dispute_evidence(&company, &id, &third), 2);

    assert_eq!(client.get_dispute_evidence_count(&id), 3);
    assert_eq!(client.get_dispute_evidence(&id, &0), first);
    assert_eq!(client.get_dispute_evidence(&id, &1), second);
    assert_eq!(client.get_dispute_evidence(&id, &2), third);
}

/// A shipment with no evidence reports a count of zero.
#[test]
fn test_evidence_count_defaults_to_zero() {
    let (env, client, admin, token) = setup();
    let (id, _company, _receiver, _carrier) = create_test_shipment(&env, &client, &admin, &token);

    assert_eq!(client.get_dispute_evidence_count(&id), 0);
}

/// Reading an index that was never written yields `EvidenceNotFound`.
#[test]
fn test_reading_unwritten_index_is_evidence_not_found() {
    let (env, client, admin, token) = setup();
    let (id, _company, receiver, _carrier) = disputed_shipment(&env, &client, &admin, &token);

    client.add_dispute_evidence(&receiver, &id, &BytesN::from_array(&env, &[21u8; 32]));

    // Index 0 exists; index 1 is one past the end.
    assert_eq!(
        client.try_get_dispute_evidence(&id, &1),
        Err(Ok(NavinError::EvidenceNotFound))
    );
}

/// Reading evidence for a shipment that has none yields `EvidenceNotFound`.
#[test]
fn test_reading_evidence_of_undisputed_shipment_is_not_found() {
    let (env, client, admin, token) = setup();
    let (id, _company, _receiver, _carrier) = create_test_shipment(&env, &client, &admin, &token);

    assert_eq!(
        client.try_get_dispute_evidence(&id, &0),
        Err(Ok(NavinError::EvidenceNotFound))
    );
}

/// Filling the dispute to `max_evidence_per_dispute` succeeds; the next
/// submission is rejected and leaves the stored count untouched.
#[test]
fn test_evidence_cap_is_enforced() {
    let (env, client, admin, token) = setup();
    let (id, _company, receiver, _carrier) = disputed_shipment(&env, &client, &admin, &token);

    // Lower the cap so the test does not have to write the 255 default.
    const CAP: u32 = 3;
    let mut cfg = client.get_contract_config();
    cfg.max_evidence_per_dispute = CAP;
    client.update_config(&admin, &cfg);

    for i in 0..CAP {
        // Vary the hash so no two entries are identical.
        let hash = BytesN::from_array(&env, &[(i + 1) as u8; 32]);
        assert_eq!(client.add_dispute_evidence(&receiver, &id, &hash), i);
    }

    assert_eq!(client.get_dispute_evidence_count(&id), CAP);

    let overflow = BytesN::from_array(&env, &[0xAAu8; 32]);
    assert_eq!(
        client.try_add_dispute_evidence(&receiver, &id, &overflow),
        Err(Ok(NavinError::EvidenceLimitExceeded))
    );

    // The rejected submission must not have been recorded.
    assert_eq!(client.get_dispute_evidence_count(&id), CAP);
}

/// Evidence may only be attached while the dispute is open.
#[test]
fn test_evidence_rejected_when_not_disputed() {
    let (env, client, admin, token) = setup();
    let (id, _company, receiver, _carrier) = create_test_shipment(&env, &client, &admin, &token);

    assert_eq!(
        client.try_add_dispute_evidence(&receiver, &id, &BytesN::from_array(&env, &[31u8; 32])),
        Err(Ok(NavinError::InvalidStatus))
    );
}

/// An address uninvolved in the shipment cannot attach evidence.
#[test]
fn test_evidence_rejected_for_non_participant() {
    let (env, client, admin, token) = setup();
    let (id, _company, _receiver, _carrier) = disputed_shipment(&env, &client, &admin, &token);

    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_add_dispute_evidence(&stranger, &id, &BytesN::from_array(&env, &[41u8; 32])),
        Err(Ok(NavinError::Unauthorized))
    );
}

/// An all-zero hash is not a valid SHA-256 commitment.
#[test]
fn test_evidence_rejects_zero_hash() {
    let (env, client, admin, token) = setup();
    let (id, _company, receiver, _carrier) = disputed_shipment(&env, &client, &admin, &token);

    assert_eq!(
        client.try_add_dispute_evidence(&receiver, &id, &BytesN::from_array(&env, &[0u8; 32])),
        Err(Ok(NavinError::InvalidHash))
    );
}

/// A missing shipment is reported as such, not as missing evidence.
#[test]
fn test_evidence_on_missing_shipment_is_shipment_not_found() {
    let (env, client, admin, token) = setup();
    let (_id, _company, receiver, _carrier) = disputed_shipment(&env, &client, &admin, &token);

    assert_eq!(
        client.try_add_dispute_evidence(&receiver, &9_999, &BytesN::from_array(&env, &[51u8; 32])),
        Err(Ok(NavinError::ShipmentNotFound))
    );
}
