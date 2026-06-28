extern crate std;

use crate::{
    config,
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
    let (env, client, admin, _token) = setup();
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

// ── New regression cases ─────────────────────────────────────────────────────

/// Escrow stored in the shipment struct is non-zero but the shipment is in a
/// terminal (Cancelled) state — the cross-field invariant must fire.
#[test]
fn test_detects_escrow_nonzero_on_terminal_shipment() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id = create_one(&env, &client, &company, &carrier, 0xDE);

    env.as_contract(&client.address, || {
        // Put the shipment in Cancelled state but leave escrow_amount non-zero.
        let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
        shipment.status = ShipmentStatus::Cancelled;
        shipment.escrow_amount = 5_000;
        shipment.finalized = true; // finalized=true with non-zero escrow → InvalidFinalization
        crate::storage::set_shipment(&env, &shipment);
        // Keep the escrow storage entry in sync so EscrowMismatch doesn't fire instead.
        crate::storage::set_escrow(&env, id, 5_000);

        let violations = check_shipment_invariants(&env, id);
        assert!(
            violations
                .iter()
                .any(|v| v == ConsistencyViolation::InvalidFinalization(id)),
            "expected InvalidFinalization for terminal shipment with non-zero escrow, got: {violations:?}"
        );
    });
}

/// The escrow storage entry is absent (returns 0) while the shipment struct
/// records a positive escrow_amount — a missing persistent entry must be
/// detected as an EscrowMismatch.
#[test]
fn test_detects_missing_escrow_persistent_entry() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id = create_one(&env, &client, &company, &carrier, 0xEF);

    env.as_contract(&client.address, || {
        // Give the shipment struct a non-zero escrow_amount …
        let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
        shipment.escrow_amount = 1_000;
        crate::storage::set_shipment(&env, &shipment);
        // … but remove the dedicated escrow storage key so it is "missing".
        crate::storage::remove_escrow(&env, id);

        let violations = check_shipment_invariants(&env, id);
        assert!(
            violations
                .iter()
                .any(|v| v == ConsistencyViolation::EscrowMismatch(id)),
            "expected EscrowMismatch when escrow persistent entry is absent, got: {violations:?}"
        );
    });
}

/// Calling check_all_consistency twice on the same state must return identical
/// results — the report is deterministic and has no side-effects.
#[test]
fn test_consistency_report_is_deterministic() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id1 = create_one(&env, &client, &company, &carrier, 0x01);
    let id2 = create_one(&env, &client, &company, &carrier, 0x02);

    // Corrupt one shipment so the report is non-empty.
    env.as_contract(&client.address, || {
        crate::storage::set_escrow(&env, id1, 42);
    });

    env.as_contract(&client.address, || {
        let first = check_all_consistency(&env);
        let second = check_all_consistency(&env);

        assert_eq!(
            first.len(),
            second.len(),
            "report length must be stable across repeated calls"
        );
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(
                a, b,
                "report entries must be identical across repeated calls"
            );
        }
        // Sanity: the corruption on id1 is present, id2 is clean.
        assert!(first
            .iter()
            .any(|v| v == ConsistencyViolation::EscrowMismatch(id1)));
        assert!(!first
            .iter()
            .any(|v| v == ConsistencyViolation::EscrowMismatch(id2)));
    });
}

// ── Config checksum drift detection tests ──────────────────────────────────

/// Updating config with the same values must preserve the checksum.
#[test]
fn test_config_noop_update_preserves_checksum() {
    let (env, client, admin, _) = setup();
    let before = client.get_config_checksum();
    let current = client.get_contract_config();
    client.update_config(&admin, &current);
    let after = client.get_config_checksum();
    let _ = env;
    assert_eq!(before, after, "no-op update must preserve checksum");
}

/// Repeated queries across the checksum and config read paths must not mutate state.
#[test]
fn test_config_checksum_stable_across_queries() {
    let (_env, client, admin, _) = setup();
    let c1 = client.get_config_checksum();
    let cfg = client.get_contract_config();
    let c2 = client.get_config_checksum();
    let _ = client.get_contract_config();
    let c3 = client.get_config_checksum();

    assert_eq!(c1, c2);
    assert_eq!(c2, c3);

    // Even after a write (no-op), checksum stays identical
    client.update_config(&admin, &cfg);
    let c4 = client.get_config_checksum();
    assert_eq!(c1, c4);
}

// ── Batch query vs. single-read consistency (issue #445) ────────────────────

/// Each position in a batch result must agree with the corresponding
/// single-record read for every field the consistency checker cares about.
#[test]
fn test_batch_single_read_equivalence() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id1 = create_one(&env, &client, &company, &carrier, 0x10);
    let id2 = create_one(&env, &client, &company, &carrier, 0x20);
    let id3 = create_one(&env, &client, &company, &carrier, 0x30);

    let mut ids: soroban_sdk::Vec<u64> = soroban_sdk::Vec::new(&env);
    ids.push_back(id1);
    ids.push_back(id2);
    ids.push_back(id3);

    let batch = client.get_shipments_batch(&ids);

    for (i, expected_id) in [id1, id2, id3].iter().enumerate() {
        let single = client.get_shipment(expected_id);
        let from_batch = batch.get(i as u32).unwrap().unwrap();
        assert_eq!(
            from_batch.id, single.id,
            "batch[{i}] id must equal single-read id"
        );
        assert_eq!(
            from_batch.status, single.status,
            "batch[{i}] status must equal single-read status"
        );
        assert_eq!(
            from_batch.escrow_amount, single.escrow_amount,
            "batch[{i}] escrow_amount must equal single-read value"
        );
        assert_eq!(
            from_batch.finalized, single.finalized,
            "batch[{i}] finalized flag must equal single-read value"
        );
    }
}

/// Missing record entries must be None in the batch result, matching the
/// error behaviour of single reads on non-existent IDs.
#[test]
fn test_missing_record_batch_returns_none_stable() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let existing_id = create_one(&env, &client, &company, &carrier, 0x40);
    let missing_id: u64 = 88888;

    let mut ids: soroban_sdk::Vec<u64> = soroban_sdk::Vec::new(&env);
    ids.push_back(existing_id);
    ids.push_back(missing_id);

    // First call
    let first = client.get_shipments_batch(&ids);
    // Second call — must agree
    let second = client.get_shipments_batch(&ids);

    assert!(first.get(0).unwrap().is_some(), "existing id must be Some");
    assert!(
        first.get(1).unwrap().is_none(),
        "missing id must be None in batch"
    );
    // Stability: second call must match first
    assert_eq!(
        first.get(1).unwrap().is_none(),
        second.get(1).unwrap().is_none(),
        "missing-record None must be stable across repeated calls"
    );
    // Single read for missing id also fails
    assert!(
        client.try_get_shipment(&missing_id).is_err(),
        "single get for missing id must return an error"
    );
}

/// Repeated batch queries on the same IDs must return identical results,
/// demonstrating that the batch output is deterministic and read-only.
#[test]
fn test_batch_query_output_deterministic() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id1 = create_one(&env, &client, &company, &carrier, 0x50);
    let id2 = create_one(&env, &client, &company, &carrier, 0x51);

    let mut ids: soroban_sdk::Vec<u64> = soroban_sdk::Vec::new(&env);
    ids.push_back(id1);
    ids.push_back(77777_u64); // missing
    ids.push_back(id2);

    let r1 = client.get_shipments_batch(&ids);
    let r2 = client.get_shipments_batch(&ids);
    let r3 = client.get_shipments_batch(&ids);

    assert_eq!(r1.len(), r2.len());
    assert_eq!(r2.len(), r3.len());
    for i in 0..r1.len() {
        match (r1.get(i).unwrap(), r2.get(i).unwrap(), r3.get(i).unwrap()) {
            (Some(a), Some(b), Some(c)) => {
                assert_eq!(a.id, b.id);
                assert_eq!(b.id, c.id);
            }
            (None, None, None) => {}
            _ => panic!("slot {i} produced inconsistent presence across three calls"),
        }
    }
}

/// The config checksum is deterministic and reproducible via the raw compute function.
#[test]
fn test_config_checksum_raw_compute_matches_saved() {
    let (env, client, _admin, _) = setup();
    let saved = client.get_config_checksum();
    let cfg = client.get_contract_config();
    let recomputed = env.as_contract(&client.address, || {
        config::compute_config_checksum(&cfg, &env)
    });
    assert_eq!(saved, recomputed, "saved checksum must match recomputed");
}

// ── [ISSUE #453] Carrier reverse-lookup consistency tests ───────────────────

/// Test: Add forward whitelist entries and verify they can be read back.
/// This ensures that forward whitelist records are stored and retrieved correctly.
#[test]
fn test_carrier_whitelist_forward_lookup_basic() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier1 = Address::generate(&env);
    let carrier2 = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier1);
    client.add_carrier(&admin, &carrier2);

    // Add carriers to company whitelist
    client.add_carrier_to_whitelist(&company, &carrier1);
    client.add_carrier_to_whitelist(&company, &carrier2);

    // Verify forward lookups work
    assert!(
        client.is_carrier_whitelisted(&company, &carrier1),
        "carrier1 should be whitelisted for company"
    );
    assert!(
        client.is_carrier_whitelisted(&company, &carrier2),
        "carrier2 should be whitelisted for company"
    );
}

/// Test: Multiple companies can whitelist the same carrier independently.
/// This verifies that forward lookup views agree across multiple companies.
#[test]
fn test_carrier_whitelist_multiple_companies_independent() {
    let (env, client, admin, _) = setup();
    let company_a = Address::generate(&env);
    let company_b = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company_a);
    client.add_company(&admin, &company_b);
    client.add_carrier(&admin, &carrier);

    // Company A whitelists the carrier
    client.add_carrier_to_whitelist(&company_a, &carrier);

    // Verify forward lookups are company-specific
    assert!(
        client.is_carrier_whitelisted(&company_a, &carrier),
        "carrier should be whitelisted for company_a"
    );
    assert!(
        !client.is_carrier_whitelisted(&company_b, &carrier),
        "carrier should NOT be whitelisted for company_b yet"
    );

    // Company B whitelists the same carrier
    client.add_carrier_to_whitelist(&company_b, &carrier);

    // Both lookups should now succeed
    assert!(
        client.is_carrier_whitelisted(&company_a, &carrier),
        "carrier should still be whitelisted for company_a"
    );
    assert!(
        client.is_carrier_whitelisted(&company_b, &carrier),
        "carrier should now be whitelisted for company_b"
    );
}

/// Test: Delete paths clear the whitelist entry and forward lookup returns false.
/// This confirms deleted entries do not remain visible.
#[test]
fn test_carrier_whitelist_delete_clears_forward_lookup() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Add carrier to whitelist
    client.add_carrier_to_whitelist(&company, &carrier);
    assert!(
        client.is_carrier_whitelisted(&company, &carrier),
        "carrier should be whitelisted initially"
    );

    // Remove carrier from whitelist
    client.remove_carrier_from_whitelist(&company, &carrier);

    // Verify forward lookup now returns false
    assert!(
        !client.is_carrier_whitelisted(&company, &carrier),
        "carrier should NOT be whitelisted after removal"
    );
}

/// Test: Removing a carrier from one company's whitelist does not affect other companies.
/// This ensures delete paths are scoped correctly and don't corrupt other entries.
#[test]
fn test_carrier_whitelist_delete_scoped_to_company() {
    let (env, client, admin, _) = setup();
    let company_a = Address::generate(&env);
    let company_b = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company_a);
    client.add_company(&admin, &company_b);
    client.add_carrier(&admin, &carrier);

    // Both companies whitelist the carrier
    client.add_carrier_to_whitelist(&company_a, &carrier);
    client.add_carrier_to_whitelist(&company_b, &carrier);

    assert!(client.is_carrier_whitelisted(&company_a, &carrier));
    assert!(client.is_carrier_whitelisted(&company_b, &carrier));

    // Remove from company_a only
    client.remove_carrier_from_whitelist(&company_a, &carrier);

    // Verify company_a's lookup is false, but company_b is unaffected
    assert!(
        !client.is_carrier_whitelisted(&company_a, &carrier),
        "carrier should be removed from company_a"
    );
    assert!(
        client.is_carrier_whitelisted(&company_b, &carrier),
        "carrier should still be whitelisted for company_b"
    );
}

/// Test: Lookup behavior is deterministic across repeated calls.
/// This verifies that forward lookups are stable and read-only.
#[test]
fn test_carrier_whitelist_lookup_deterministic() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    // Query multiple times
    let result1 = client.is_carrier_whitelisted(&company, &carrier);
    let result2 = client.is_carrier_whitelisted(&company, &carrier);
    let result3 = client.is_carrier_whitelisted(&company, &carrier);

    assert_eq!(result1, result2, "lookup must be stable across calls");
    assert_eq!(result2, result3, "lookup must be stable across calls");
    assert!(result1, "carrier should be whitelisted");
}

/// Test: Add, remove, and re-add the same carrier to verify state transitions are clean.
/// This ensures no stale data remains after deletion.
#[test]
fn test_carrier_whitelist_add_remove_readd_cycle() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Add carrier
    client.add_carrier_to_whitelist(&company, &carrier);
    assert!(
        client.is_carrier_whitelisted(&company, &carrier),
        "carrier should be whitelisted after add"
    );

    // Remove carrier
    client.remove_carrier_from_whitelist(&company, &carrier);
    assert!(
        !client.is_carrier_whitelisted(&company, &carrier),
        "carrier should not be whitelisted after remove"
    );

    // Re-add carrier
    client.add_carrier_to_whitelist(&company, &carrier);
    assert!(
        client.is_carrier_whitelisted(&company, &carrier),
        "carrier should be whitelisted after re-add"
    );
}

/// Test: Verify storage keys are correctly scoped (company, carrier) and not reversed.
/// This is a low-level consistency check to ensure the storage layout is correct.
#[test]
fn test_carrier_whitelist_storage_key_correctness() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    // Verify internal storage using the canonical key order
    env.as_contract(&client.address, || {
        let canonical_forward = crate::storage::is_carrier_whitelisted(&env, &company, &carrier);
        assert!(
            canonical_forward,
            "canonical forward lookup (company, carrier) must succeed"
        );

        // The reverse key (carrier, company) should NOT exist
        let reversed = crate::storage::is_carrier_whitelisted(&env, &carrier, &company);
        assert!(
            !reversed,
            "reversed lookup (carrier, company) should not exist"
        );
    });
}

/// Test: Bulk whitelist operations maintain consistency.
/// This verifies that multiple adds/removes in succession maintain correct state.
#[test]
fn test_carrier_whitelist_bulk_operations_consistency() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carriers: soroban_sdk::Vec<Address> = {
        let mut vec = soroban_sdk::Vec::new(&env);
        for _ in 0..5 {
            vec.push_back(Address::generate(&env));
        }
        vec
    };

    client.add_company(&admin, &company);
    for carrier in carriers.iter() {
        client.add_carrier(&admin, &carrier);
    }

    // Add all carriers to whitelist
    for carrier in carriers.iter() {
        client.add_carrier_to_whitelist(&company, &carrier);
    }

    // Verify all are whitelisted
    for carrier in carriers.iter() {
        assert!(
            client.is_carrier_whitelisted(&company, &carrier),
            "all carriers should be whitelisted after bulk add"
        );
    }

    // Remove every other carrier
    for (i, carrier) in carriers.iter().enumerate() {
        if i % 2 == 0 {
            client.remove_carrier_from_whitelist(&company, &carrier);
        }
    }

    // Verify correct subset remains whitelisted
    for (i, carrier) in carriers.iter().enumerate() {
        let expected = i % 2 == 1;
        let actual = client.is_carrier_whitelisted(&company, &carrier);
        assert_eq!(
            actual, expected,
            "carrier at index {} should have whitelist status = {}",
            i, expected
        );
    }
}

/// Test: Whitelist state is unaffected by carrier role suspension.
/// This ensures whitelist and suspension states are independent.
#[test]
fn test_carrier_whitelist_independent_of_suspension() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    // Verify whitelist before suspension
    assert!(
        client.is_carrier_whitelisted(&company, &carrier),
        "carrier should be whitelisted before suspension"
    );

    // Suspend the carrier
    client.suspend_carrier(&admin, &carrier);

    // Whitelist state should be unchanged
    assert!(
        client.is_carrier_whitelisted(&company, &carrier),
        "carrier should still be whitelisted after suspension"
    );

    // Reactivate the carrier
    client.reactivate_carrier(&admin, &carrier);

    // Whitelist state should still be unchanged
    assert!(
        client.is_carrier_whitelisted(&company, &carrier),
        "carrier should still be whitelisted after reactivation"
    );
}

/// Test: Non-existent (company, carrier) pair returns false consistently.
/// This verifies that missing entries are handled correctly.
#[test]
fn test_carrier_whitelist_nonexistent_pair_returns_false() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Query without adding to whitelist
    let result1 = client.is_carrier_whitelisted(&company, &carrier);
    let result2 = client.is_carrier_whitelisted(&company, &carrier);

    assert!(!result1, "non-existent pair should return false");
    assert_eq!(
        result1, result2,
        "non-existent pair query must be deterministic"
    );
}

/// Test: Whitelist state persists across multiple operations on other entities.
/// This ensures whitelist data is not corrupted by unrelated operations.
#[test]
fn test_carrier_whitelist_state_persistence() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier1 = Address::generate(&env);
    let carrier2 = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier1);
    client.add_carrier(&admin, &carrier2);

    // Whitelist carrier1
    client.add_carrier_to_whitelist(&company, &carrier1);
    assert!(client.is_carrier_whitelisted(&company, &carrier1));

    // Perform unrelated operations (create shipment, whitelist another carrier)
    let shipment_id = create_one(&env, &client, &company, &carrier1, 0xF1);
    client.add_carrier_to_whitelist(&company, &carrier2);

    // Verify carrier1's whitelist state persists
    assert!(
        client.is_carrier_whitelisted(&company, &carrier1),
        "carrier1 whitelist state should persist after other operations"
    );
    assert!(
        client.is_carrier_whitelisted(&company, &carrier2),
        "carrier2 should also be whitelisted"
    );
    assert!(shipment_id > 0, "shipment should be created successfully");
}

/// Test: Whitelist queries with swapped parameters return different results.
/// This verifies key order matters and parameters are not commutative.
#[test]
fn test_carrier_whitelist_parameter_order_matters() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Add (company, carrier) to whitelist
    client.add_carrier_to_whitelist(&company, &carrier);

    // Forward lookup should succeed
    assert!(
        client.is_carrier_whitelisted(&company, &carrier),
        "(company, carrier) should be whitelisted"
    );

    // Swapped parameters should fail (carrier as company, company as carrier)
    assert!(
        !client.is_carrier_whitelisted(&carrier, &company),
        "(carrier, company) should NOT be whitelisted - parameters are not commutative"
    );
}

/// Test: Repeated add operations are idempotent.
/// This ensures adding the same carrier multiple times has no adverse effects.
#[test]
fn test_carrier_whitelist_add_idempotent() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Add carrier multiple times
    client.add_carrier_to_whitelist(&company, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    // Should still be whitelisted (no error, state is correct)
    assert!(
        client.is_carrier_whitelisted(&company, &carrier),
        "carrier should be whitelisted after multiple adds"
    );

    // Remove once should clear it
    client.remove_carrier_from_whitelist(&company, &carrier);
    assert!(
        !client.is_carrier_whitelisted(&company, &carrier),
        "carrier should not be whitelisted after single remove"
    );
}

/// Test: Repeated remove operations are idempotent.
/// This ensures removing a non-existent entry has no adverse effects.
#[test]
fn test_carrier_whitelist_remove_idempotent() {
    let (env, client, admin, _) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Remove without adding (no-op)
    client.remove_carrier_from_whitelist(&company, &carrier);
    assert!(!client.is_carrier_whitelisted(&company, &carrier));

    // Add, then remove multiple times
    client.add_carrier_to_whitelist(&company, &carrier);
    assert!(client.is_carrier_whitelisted(&company, &carrier));

    client.remove_carrier_from_whitelist(&company, &carrier);
    client.remove_carrier_from_whitelist(&company, &carrier);
    client.remove_carrier_from_whitelist(&company, &carrier);

    // Should still be not whitelisted
    assert!(
        !client.is_carrier_whitelisted(&company, &carrier),
        "carrier should not be whitelisted after multiple removes"
    );
}

#[test]
fn test_upgrade_preserves_analytics_counters() {
    let (env, client, admin, _token) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;

    let id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    let id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[2u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.deposit_escrow(&company, &id1, &1000);
    client.deposit_escrow(&company, &id2, &500);

    let health_before = client.check_contract_health(&admin);
    assert_eq!(health_before.total_shipments, 2);
    assert_eq!(health_before.sum_of_escrow_balances, 1500);

    let target_version = client.get_version() + 1;
    let wasm: &[u8] = include_bytes!("../test_wasms/upgrade_test.wasm");
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    let contract_id = client.address.clone();
    client.upgrade(&admin, &new_wasm_hash, &target_version);

    env.as_contract(&contract_id, || {
        let count = crate::storage::get_shipment_counter(&env);
        assert_eq!(count, 2, "shipment counter should be preserved");

        let escrow1 = crate::storage::get_escrow(&env, id1);
        assert_eq!(escrow1, 1000);

        let escrow2 = crate::storage::get_escrow(&env, id2);
        assert_eq!(escrow2, 500);
    });
}
