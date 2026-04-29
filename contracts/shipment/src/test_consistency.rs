extern crate std;

use crate::{
    consistency::{
        check_all_consistency, check_batch_consistency, check_shipment_invariants,
        ConsistencyViolation,
    },
    test_utils,
    types::{ShipmentInput, ShipmentStatus},
    NavinShipment, NavinShipmentClient,
};
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Vec};

// ── Minimal mock token (always succeeds) ────────────────────────────────────

#[contract]
struct MockTokenConsistency;

#[contractimpl]
impl MockTokenConsistency {
    pub fn decimals(_env: soroban_sdk::Env) -> u32 {
        7
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    pub fn mint(_env: Env, _admin: Address, _to: Address, _amount: i128) {}
}

// ── Test helpers ────────────────────────────────────────────────────────────

fn setup() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = test_utils::setup_env();
    let token = env.register(MockTokenConsistency {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token);
    (env, client, admin, token)
}

fn dummy_hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn create_one(
    env: &Env,
    client: &NavinShipmentClient,
    company: &Address,
    carrier: &Address,
    seed: u8,
) -> u64 {
    let deadline = test_utils::future_deadline(env, 7200);
    client.create_shipment(
        company,
        &Address::generate(env),
        carrier,
        &dummy_hash(env, seed),
        &Vec::new(env),
        &deadline,
        &None,
    )
}

// ── Healthy state — no violations ───────────────────────────────────────────

#[test]
fn test_healthy_shipment_has_no_violations() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id = create_one(&env, &client, &company, &carrier, 1);

    env.as_contract(&client.address, || {
        let violations = check_shipment_invariants(&env, id);
        assert!(
            violations.is_empty(),
            "expected no violations: {violations:?}"
        );
    });
}

#[test]
fn test_healthy_batch_has_no_violations() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let deadline = test_utils::future_deadline(&env, 7200);
    let mut inputs: Vec<ShipmentInput> = Vec::new(&env);
    for seed in 1u8..=3 {
        inputs.push_back(ShipmentInput {
            receiver: Address::generate(&env),
            carrier: carrier.clone(),
            data_hash: dummy_hash(&env, seed),
            payment_milestones: Vec::new(&env),
            deadline,
            depends_on: None,
        });
    }
    let ids = client.create_shipments_batch(&company, &inputs);

    env.as_contract(&client.address, || {
        let violations = check_batch_consistency(&env, &ids);
        assert!(
            violations.is_empty(),
            "expected no violations: {violations:?}"
        );
    });
}

#[test]
fn test_check_all_consistency_clean_state() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    create_one(&env, &client, &company, &carrier, 1);
    create_one(&env, &client, &company, &carrier, 2);

    env.as_contract(&client.address, || {
        let violations = check_all_consistency(&env);
        assert!(
            violations.is_empty(),
            "expected no violations: {violations:?}"
        );
    });
}

// ── Artificial inconsistency detection ──────────────────────────────────────

#[test]
fn test_detects_escrow_mismatch() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id = create_one(&env, &client, &company, &carrier, 1);

    // Corrupt escrow storage so it diverges from the shipment struct.
    env.as_contract(&client.address, || {
        crate::storage::set_escrow(&env, id, 999_999);
        let violations = check_shipment_invariants(&env, id);
        assert!(
            violations
                .iter()
                .any(|v| v == ConsistencyViolation::EscrowMismatch(id)),
            "expected EscrowMismatch, got: {violations:?}"
        );
    });
}

#[test]
fn test_detects_invalid_finalization() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id = create_one(&env, &client, &company, &carrier, 1);

    // Force finalized=true on a non-terminal (Created) shipment.
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
        shipment.finalized = true;
        crate::storage::set_shipment(&env, &shipment);

        let violations = check_shipment_invariants(&env, id);
        assert!(
            violations
                .iter()
                .any(|v| v == ConsistencyViolation::InvalidFinalization(id)),
            "expected InvalidFinalization, got: {violations:?}"
        );
    });
}

#[test]
fn test_detects_milestone_violation() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id = create_one(&env, &client, &company, &carrier, 1);

    // Inject a paid milestone that doesn't exist in the payment schedule.
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
        shipment
            .paid_milestones
            .push_back(soroban_sdk::Symbol::new(&env, "ghost_milestone"));
        crate::storage::set_shipment(&env, &shipment);

        let violations = check_shipment_invariants(&env, id);
        assert!(
            violations
                .iter()
                .any(|v| v == ConsistencyViolation::MilestoneViolation(id)),
            "expected MilestoneViolation, got: {violations:?}"
        );
    });
}

#[test]
fn test_detects_timestamp_anomaly() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id = create_one(&env, &client, &company, &carrier, 1);

    // Set updated_at to a time before created_at.
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
        shipment.updated_at = shipment.created_at.saturating_sub(10);
        crate::storage::set_shipment(&env, &shipment);

        let violations = check_shipment_invariants(&env, id);
        assert!(
            violations
                .iter()
                .any(|v| v == ConsistencyViolation::TimestampAnomaly(id)),
            "expected TimestampAnomaly, got: {violations:?}"
        );
    });
}

#[test]
fn test_detects_deadline_anomaly() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id = create_one(&env, &client, &company, &carrier, 1);

    // Backdoor: force deadline to equal created_at.
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
        shipment.deadline = shipment.created_at; // <= created_at → anomaly
        crate::storage::set_shipment(&env, &shipment);

        let violations = check_shipment_invariants(&env, id);
        assert!(
            violations
                .iter()
                .any(|v| v == ConsistencyViolation::DeadlineAnomaly(id)),
            "expected DeadlineAnomaly, got: {violations:?}"
        );
    });
}

#[test]
fn test_detects_missing_shipment() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id = create_one(&env, &client, &company, &carrier, 1);

    // Remove the shipment from storage to simulate a missing entry.
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .remove(&crate::types::DataKey::Shipment(id));

        let violations = check_shipment_invariants(&env, id);
        assert!(
            violations
                .iter()
                .any(|v| v == ConsistencyViolation::MissingShipment(id)),
            "expected MissingShipment, got: {violations:?}"
        );
    });
}

// ── Batch cross-shipment invariant violations ────────────────────────────────

#[test]
fn test_detects_batch_sender_mismatch() {
    let (env, client, admin, _) = setup();

    let company1 = Address::generate(&env);
    let company2 = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company1);
    client.add_company(&admin, &company2);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company1, &carrier);
    client.add_carrier_to_whitelist(&company2, &carrier);

    let id1 = create_one(&env, &client, &company1, &carrier, 1);
    let id2 = create_one(&env, &client, &company2, &carrier, 1);

    let mut ids: Vec<u64> = Vec::new(&env);
    ids.push_back(id1);
    ids.push_back(id2);

    env.as_contract(&client.address, || {
        let violations = check_batch_consistency(&env, &ids);
        assert!(
            violations
                .iter()
                .any(|v| v == ConsistencyViolation::BatchSenderMismatch(id2)),
            "expected BatchSenderMismatch for id2, got: {violations:?}"
        );
    });
}

#[test]
fn test_detects_batch_timestamp_mismatch() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id1 = create_one(&env, &client, &company, &carrier, 1);

    // Advance time so the second shipment has a different created_at.
    test_utils::advance_ledger_time(&env, 120);
    let id2 = create_one(&env, &client, &company, &carrier, 2);

    let mut ids: Vec<u64> = Vec::new(&env);
    ids.push_back(id1);
    ids.push_back(id2);

    env.as_contract(&client.address, || {
        let violations = check_batch_consistency(&env, &ids);
        assert!(
            violations
                .iter()
                .any(|v| v == ConsistencyViolation::BatchTimestampMismatch(id2)),
            "expected BatchTimestampMismatch for id2, got: {violations:?}"
        );
    });
}

// ── Admin contract query ─────────────────────────────────────────────────────

#[test]
fn test_admin_query_returns_violations_for_corrupted_state() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id = create_one(&env, &client, &company, &carrier, 1);

    // Corrupt the escrow to trigger a violation detectable by the admin query.
    env.as_contract(&client.address, || {
        crate::storage::set_escrow(&env, id, 1);
    });

    let violations = client.check_consistency_violations(&admin);
    assert!(
        !violations.is_empty(),
        "admin query should report at least one violation"
    );
    assert!(
        violations
            .iter()
            .any(|v| v == ConsistencyViolation::EscrowMismatch(id)),
        "expected EscrowMismatch in admin query result"
    );
}

#[test]
fn test_admin_query_returns_empty_for_clean_state() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    create_one(&env, &client, &company, &carrier, 1);
    create_one(&env, &client, &company, &carrier, 2);

    let violations = client.check_consistency_violations(&admin);
    assert!(
        violations.is_empty(),
        "expected no violations in clean state"
    );
}

// ── Status-specific invariants ───────────────────────────────────────────────

#[test]
fn test_delivered_finalized_with_zero_escrow_is_healthy() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id = create_one(&env, &client, &company, &carrier, 1);

    // Simulate a properly finalized delivered shipment.
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
        shipment.status = ShipmentStatus::Delivered;
        shipment.escrow_amount = 0;
        shipment.finalized = true;
        crate::storage::set_shipment(&env, &shipment);

        let violations = check_shipment_invariants(&env, id);
        assert!(
            violations.is_empty(),
            "properly finalized delivered shipment should have no violations: {violations:?}"
        );
    });
}
