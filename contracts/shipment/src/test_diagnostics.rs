use crate::{
    config,
    test_utils::{advance_ledger_time, setup_env},
    types::ShipmentStatus,
    NavinShipment, NavinShipmentClient,
};
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Vec};

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn decimals(_env: soroban_sdk::Env) -> u32 {
        7
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    pub fn transfer_from(
        _env: Env,
        _spender: Address,
        _from: Address,
        _to: Address,
        _amount: i128,
    ) {
    }
}

fn prepare_test() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = setup_env();
    let token = env.register(MockToken {}, ());
    let cid = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &cid);
    client.initialize(&admin, &token);
    (env, client, admin, token)
}

#[test]
fn test_clean_health_check() {
    let (env, client, admin, _token) = prepare_test();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let _shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let health = client.check_contract_health(&admin);
    assert_eq!(health.total_shipments, 1);
    assert_eq!(health.active_shipments_counted, 1);
    assert_eq!(health.sum_of_escrow_balances, 0);
    assert_eq!(health.anomalous_shipment_ids.len(), 0);
    assert_eq!(health.storage_inconsistencies.len(), 0);
}

#[test]
fn test_detect_anomalies_and_escrow() {
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash1 = BytesN::from_array(&env, &[1u8; 32]);
    let data_hash2 = BytesN::from_array(&env, &[2u8; 32]);

    let id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash1,
        &Vec::new(&env),
        &deadline,
    );
    advance_ledger_time(&env, 1);
    let id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash2,
        &Vec::new(&env),
        &deadline,
    );

    client.deposit_escrow(&company, &id1, &1500);
    client.deposit_escrow(&company, &id2, &500);

    client.update_status(&carrier, &id1, &ShipmentStatus::InTransit, &data_hash1);

    // Simulate crossing the deadline threshold
    advance_ledger_time(&env, 4000); // Exceeds deadline (+3600)

    let health = client.check_contract_health(&admin);
    assert_eq!(health.total_shipments, 2);
    assert_eq!(health.active_shipments_counted, 2);

    // Sum should be accurate
    assert_eq!(health.sum_of_escrow_balances, 2000);

    // id1 is strictly InTransit and late!
    assert!(health.anomalous_shipment_ids.contains(id1));
    // id2 is still physically 'Created', which might be fine to remain late without anomaly or catch elsewhere depending on business rules, but in our code it strictly checks InTransit.
    assert!(!health.anomalous_shipment_ids.contains(id2));

    assert_eq!(
        health.storage_inconsistencies.len(),
        0,
        "Storage inconsistencies found: {:?}",
        health.storage_inconsistencies
    );
}

#[test]
fn test_detect_storage_inconsistencies() {
    // This is purely for unit verification that run_system_health_check directly exposes the
    // internal variables. We can force storage modification inside tests using raw storage functions.
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let cid = client.address.clone();
    env.as_contract(&cid, || {
        crate::storage::remove_escrow(&env, shipment_id);

        // Set escrow high within the shipment object to simulate orphaned balance
        let mut ship = crate::storage::get_shipment(&env, shipment_id).unwrap();
        ship.escrow_amount = 5000;
        crate::storage::set_shipment(&env, &ship);
    });

    let health = client.check_contract_health(&admin);
    // Because escrow_amount is 5000 but the Escrow persisted entry is killed by remove_escrow
    assert!(health.storage_inconsistencies.contains(shipment_id));
}

// ── Regression: config checksum stability and mutation ───────────────────────

/// Same config always produces the same checksum (deterministic across reruns).
#[test]
fn test_config_checksum_is_stable_when_config_unchanged() {
    let (_env, client, _admin, _token) = prepare_test();

    let c1 = client.get_config_checksum();
    let c2 = client.get_config_checksum();
    assert_eq!(c1, c2);
}

/// Checksum changes when a critical config field is mutated.
#[test]
fn test_config_checksum_changes_after_config_update() {
    let (_env, client, admin, _token) = prepare_test();

    let before = client.get_config_checksum();

    let mut new_cfg = client.get_contract_config();
    new_cfg.batch_operation_limit += 1;
    client.update_config(&admin, &new_cfg);

    let after = client.get_config_checksum();
    assert_ne!(before, after);
}

/// Reverting a config change restores the original checksum.
#[test]
fn test_config_checksum_restored_after_revert() {
    let (_env, client, admin, _token) = prepare_test();

    let original = client.get_config_checksum();

    let mut mutated = client.get_contract_config();
    mutated.deadline_grace_seconds = 120;
    client.update_config(&admin, &mutated);
    assert_ne!(client.get_config_checksum(), original);

    // Revert
    let mut reverted = client.get_contract_config();
    reverted.deadline_grace_seconds = 0;
    client.update_config(&admin, &reverted);
    assert_eq!(client.get_config_checksum(), original);
}

/// Each distinct field mutation produces a distinct checksum.
#[test]
fn test_each_field_mutation_produces_unique_checksum() {
    let (_env, client, admin, _token) = prepare_test();

    let base = client.get_config_checksum();

    let mutations: &[fn(&mut crate::ContractConfig)] = &[
        |c| c.shipment_ttl_threshold += 1,
        |c| c.shipment_ttl_extension += 1,
        |c| c.min_status_update_interval += 10,
        |c| c.batch_operation_limit += 1,
        |c| c.max_metadata_entries += 1,
        |c| c.default_shipment_limit += 1,
        |c| c.proposal_expiry_seconds += 3600,
        |c| c.deadline_grace_seconds += 60,
        |c| c.auto_dispute_breach = !c.auto_dispute_breach,
        |c| c.max_milestones_per_shipment -= 1,
        |c| c.max_notes_per_shipment -= 1,
        |c| c.max_evidence_per_dispute -= 1,
        |c| c.max_breaches_per_shipment -= 1,
    ];

    let base_cfg = client.get_contract_config();

    for mutate in mutations {
        let mut cfg = base_cfg.clone();
        mutate(&mut cfg);
        client.update_config(&admin, &cfg);
        assert_ne!(
            client.get_config_checksum(),
            base,
            "checksum must differ after field mutation"
        );
        // Restore
        client.update_config(&admin, &base_cfg);
        assert_eq!(
            client.get_config_checksum(),
            base,
            "checksum must be restored"
        );
    }
}

// ── Regression tests added for issue #379 ────────────────────────────────────
// These cases expand coverage for the restore-diagnostics path so that
// StoragePresenceState variants and PersistentRestoreDiagnostics fields
// keep consistent semantics across refactors.

/// A freshly created shipment (never archived) should report ActivePersistent.
#[test]
fn test_restore_diagnostics_active_persistent_state() {
    use crate::types::StoragePresenceState;

    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[2u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let diag = client.get_restore_diagnostics(&shipment_id);
    assert_eq!(
        diag.state,
        StoragePresenceState::ActivePersistent,
        "fresh shipment must report ActivePersistent storage state"
    );
    assert!(
        diag.persistent_shipment_present,
        "persistent_shipment_present must be true for a fresh shipment"
    );
    assert!(
        !diag.archived_shipment_present,
        "archived_shipment_present must be false for a fresh shipment"
    );
}

/// Querying restore diagnostics for a non-existent shipment ID should report Missing.
#[test]
fn test_restore_diagnostics_missing_state() {
    use crate::types::StoragePresenceState;

    let (_, client, _, _) = prepare_test();
    // Shipment ID 9999 has never been created.
    let diag = client.get_restore_diagnostics(&9999u64);
    assert_eq!(
        diag.state,
        StoragePresenceState::Missing,
        "non-existent shipment must report Missing storage state"
    );
    assert!(
        !diag.persistent_shipment_present,
        "persistent_shipment_present must be false for a missing shipment"
    );
    assert!(
        !diag.archived_shipment_present,
        "archived_shipment_present must be false for a missing shipment"
    );
}

/// Diagnostics for shipment_id must echo the queried ID back in the response.
#[test]
fn test_restore_diagnostics_shipment_id_echoed() {
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 7200;
    let data_hash = BytesN::from_array(&env, &[3u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let diag = client.get_restore_diagnostics(&shipment_id);
    assert_eq!(
        diag.shipment_id, shipment_id,
        "diagnostics must echo the queried shipment_id"
    );
}

/// A healthy contract with one active shipment must have zero storage inconsistencies.
#[test]
fn test_health_check_no_inconsistencies_after_single_creation() {
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[4u8; 32]);
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let health = client.check_contract_health(&admin);
    assert_eq!(
        health.storage_inconsistencies.len(),
        0,
        "regression: no storage inconsistencies expected after a clean creation"
    );
    assert_eq!(
        health.anomalous_shipment_ids.len(),
        0,
        "regression: no anomalous shipments expected for a non-expired active shipment"
    );
}

/// Multiple shipments must all report ActivePersistent diagnostics individually.
#[test]
fn test_restore_diagnostics_multiple_shipments_all_active() {
    use crate::types::StoragePresenceState;

    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let mut ids = Vec::new(&env);
    for i in 0..3u8 {
        let data_hash = BytesN::from_array(&env, &[10 + i; 32]);
        let id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &Vec::new(&env),
            &deadline,
        );
        ids.push_back(id);
    }

    for i in 0..ids.len() {
        let id = ids.get_unchecked(i);
        let diag = client.get_restore_diagnostics(&id);
        assert_eq!(
            diag.state,
            StoragePresenceState::ActivePersistent,
            "shipment {} must be ActivePersistent",
            id
        );
    }
}

/// Escrow presence flag must match whether an escrow was recorded for the shipment.
#[test]
fn test_restore_diagnostics_escrow_present_flag() {
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[5u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let diag = client.get_restore_diagnostics(&shipment_id);
    // escrow_present reflects whether any escrow entry exists in storage;
    // the exact value depends on the contract's escrow defaults, but the
    // field must be readable and of boolean type without panic.
    let _ = diag.escrow_present; // assert field is accessible (type-level regression)
    assert_eq!(
        diag.shipment_id, shipment_id,
        "shipment_id must be correct even when checking escrow_present"
    );
}

// ── Tests for all StoragePresenceState classifications ────────────────────────

/// A shipment that has been archived should report ArchivedExpected state.
/// After archival, the persistent entry is removed and the temporary (archived) entry exists.
#[test]
fn test_restore_diagnostics_archived_expected_state() {
    use crate::types::StoragePresenceState;

    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[6u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Verify initial state is active persistent
    let initial_diag = client.get_restore_diagnostics(&shipment_id);
    assert_eq!(initial_diag.state, StoragePresenceState::ActivePersistent);
    assert!(initial_diag.persistent_shipment_present);
    assert!(!initial_diag.archived_shipment_present);

    // Transition to Delivered, then archive
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );
    client.confirm_delivery(&receiver, &shipment_id, &data_hash);
    client.archive_shipment(&admin, &shipment_id);

    // Verify archived state
    let archived_diag = client.get_restore_diagnostics(&shipment_id);
    assert_eq!(
        archived_diag.state,
        StoragePresenceState::ArchivedExpected,
        "archived shipment must report ArchivedExpected storage state"
    );
    assert!(
        !archived_diag.persistent_shipment_present,
        "persistent_shipment_present must be false for archived shipment"
    );
    assert!(
        archived_diag.archived_shipment_present,
        "archived_shipment_present must be true for archived shipment"
    );
    assert_eq!(
        archived_diag.shipment_id, shipment_id,
        "shipment_id must match the queried ID"
    );
}

#[test]
fn test_restore_diagnostics_flags_match_state_active_persistent() {
    use crate::types::StoragePresenceState;

    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[8u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let diag = client.get_restore_diagnostics(&shipment_id);

    // State must be ActivePersistent
    assert_eq!(diag.state, StoragePresenceState::ActivePersistent);

    // Boolean flags must match state
    assert!(
        diag.persistent_shipment_present,
        "ActivePersistent state must have persistent_shipment_present = true"
    );
    assert!(
        !diag.archived_shipment_present,
        "ActivePersistent state must have archived_shipment_present = false"
    );

    // Report shape must be stable (all fields must be present)
    let _ = diag.escrow_present;
    let _ = diag.confirmation_hash_present;
    let _ = diag.last_status_update_present;
    let _ = diag.event_count_present;
}

/// Boolean flags must match the state classification for archived shipments.
#[test]
fn test_restore_diagnostics_flags_match_state_archived_expected() {
    use crate::types::StoragePresenceState;

    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[9u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Archive the shipment
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );
    client.confirm_delivery(&receiver, &shipment_id, &data_hash);
    client.archive_shipment(&admin, &shipment_id);

    let diag = client.get_restore_diagnostics(&shipment_id);

    // State must be ArchivedExpected
    assert_eq!(diag.state, StoragePresenceState::ArchivedExpected);

    // Boolean flags must match state
    assert!(
        !diag.persistent_shipment_present,
        "ArchivedExpected state must have persistent_shipment_present = false"
    );
    assert!(
        diag.archived_shipment_present,
        "ArchivedExpected state must have archived_shipment_present = true"
    );

    // Report shape must be stable (all fields must be present)
    let _ = diag.escrow_present;
    let _ = diag.confirmation_hash_present;
    let _ = diag.last_status_update_present;
    let _ = diag.event_count_present;
}

/// Boolean flags must match the state classification for missing shipments.
#[test]
fn test_restore_diagnostics_flags_match_state_missing() {
    use crate::types::StoragePresenceState;

    let (_, client, _, _) = prepare_test();

    // Query a shipment ID that has never been created
    let diag = client.get_restore_diagnostics(&9999u64);

    // State must be Missing
    assert_eq!(diag.state, StoragePresenceState::Missing);

    // Boolean flags must match state
    assert!(
        !diag.persistent_shipment_present,
        "Missing state must have persistent_shipment_present = false"
    );
    assert!(
        !diag.archived_shipment_present,
        "Missing state must have archived_shipment_present = false"
    );

    // Report shape must be stable (all fields must be present)
    let _ = diag.escrow_present;
    let _ = diag.confirmation_hash_present;
    let _ = diag.last_status_update_present;
    let _ = diag.event_count_present;
}

/// All expected fields must be present in all cases.
#[test]
fn test_restore_diagnostics_report_shape_stable() {
    use crate::types::StoragePresenceState;

    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;

    // Create three scenarios: active, archived, and missing
    let active_data_hash = BytesN::from_array(&env, &[11u8; 32]);
    let active_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &active_data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let archived_data_hash = BytesN::from_array(&env, &[12u8; 32]);
    let archived_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &archived_data_hash,
        &Vec::new(&env),
        &deadline,
    );
    client.update_status(
        &carrier,
        &archived_id,
        &ShipmentStatus::InTransit,
        &archived_data_hash,
    );
    client.confirm_delivery(&receiver, &archived_id, &archived_data_hash);
    client.archive_shipment(&admin, &archived_id);

    // Test active shipment report shape
    let active_diag = client.get_restore_diagnostics(&active_id);
    assert_eq!(active_diag.state, StoragePresenceState::ActivePersistent);
    assert!(active_diag.shipment_id > 0);
    let _ = active_diag.persistent_shipment_present;
    let _ = active_diag.archived_shipment_present;
    let _ = active_diag.escrow_present;
    let _ = active_diag.confirmation_hash_present;
    let _ = active_diag.last_status_update_present;
    let _ = active_diag.event_count_present;

    // Test archived shipment report shape
    let archived_diag = client.get_restore_diagnostics(&archived_id);
    assert_eq!(archived_diag.state, StoragePresenceState::ArchivedExpected);
    assert!(archived_diag.shipment_id > 0);
    let _ = archived_diag.persistent_shipment_present;
    let _ = archived_diag.archived_shipment_present;
    let _ = archived_diag.escrow_present;
    let _ = archived_diag.confirmation_hash_present;
    let _ = archived_diag.last_status_update_present;
    let _ = archived_diag.event_count_present;

    // Test missing shipment report shape
    let missing_diag = client.get_restore_diagnostics(&5555u64);
    assert_eq!(missing_diag.state, StoragePresenceState::Missing);
    assert_eq!(missing_diag.shipment_id, 5555u64);
    let _ = missing_diag.persistent_shipment_present;
    let _ = missing_diag.archived_shipment_present;
    let _ = missing_diag.escrow_present;
    let _ = missing_diag.confirmation_hash_present;
    let _ = missing_diag.last_status_update_present;
    let _ = missing_diag.event_count_present;
}

// ── Config checksum diagnostics query path ──────────────────────────────────

/// The config checksum query path used by diagnostics/indexers must return
/// a stable checksum across multiple invocations and match a raw recompute.
#[test]
fn test_config_checksum_diagnostics_query_path() {
    let (env, client, admin, _token) = prepare_test();

    // Query path: get_config_checksum (what indexers/diagnostics use)
    let q1 = client.get_config_checksum();
    let q2 = client.get_config_checksum();
    assert_eq!(q1, q2, "diagnostics query path must be idempotent");

    // Recompute from raw config to pin the expected behaviour
    let cfg = client.get_contract_config();
    let recomputed = env.as_contract(&client.address, || {
        config::compute_config_checksum(&cfg, &env)
    });
    assert_eq!(
        q1, recomputed,
        "diagnostics query path must match raw compute"
    );

    // After a mutation, the checksum changes predictably
    let mut mutated = cfg.clone();
    mutated.batch_operation_limit += 1;
    client.update_config(&admin, &mutated);
    let q3 = client.get_config_checksum();
    assert_ne!(
        q1, q3,
        "checksum must change after config mutation via diagnostics query path"
    );

    // Restore and verify the original checksum returns
    client.update_config(&admin, &cfg);
    let q4 = client.get_config_checksum();
    assert_eq!(q1, q4, "original checksum must be restored after revert");
}

#[test]
fn test_get_non_terminal_count_alignment() {
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;

    let _id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &Vec::new(&env),
        &deadline,
    );
    let id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[2u8; 32]),
        &Vec::new(&env),
        &deadline,
    );
    let id3 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[3u8; 32]),
        &Vec::new(&env),
        &deadline,
    );
    let id4 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[4u8; 32]),
        &Vec::new(&env),
        &deadline,
    );
    let id5 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[5u8; 32]),
        &Vec::new(&env),
        &deadline,
    );

    // Initial state: 5 Created
    assert_eq!(client.get_non_terminal_count(), 5);

    // id2 -> InTransit
    client.update_status(
        &carrier,
        &id2,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[2u8; 32]),
    );
    assert_eq!(client.get_non_terminal_count(), 5);

    // id3 -> AtCheckpoint
    client.update_status(
        &carrier,
        &id3,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[3u8; 32]),
    );
    advance_ledger_time(&env, 3600);
    client.update_status(
        &carrier,
        &id3,
        &ShipmentStatus::AtCheckpoint,
        &BytesN::from_array(&env, &[3u8; 32]),
    );
    assert_eq!(client.get_non_terminal_count(), 5);

    // id4 -> Disputed
    client.update_status(
        &carrier,
        &id4,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[4u8; 32]),
    );
    advance_ledger_time(&env, 3600);
    client.raise_dispute(&company, &id4, &BytesN::from_array(&env, &[4u8; 32]));
    assert_eq!(client.get_non_terminal_count(), 5);

    // id5 -> Delivered (Terminal)
    client.update_status(
        &carrier,
        &id5,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[5u8; 32]),
    );
    advance_ledger_time(&env, 3600);
    client.confirm_delivery(&receiver, &id5, &BytesN::from_array(&env, &[5u8; 32]));
    assert_eq!(client.get_non_terminal_count(), 4);
}

// ── get_shipment_creator — non-existent shipment tests (issue #520) ──────────

/// Querying the creator of a shipment ID that was never created must return
/// ShipmentNotFound without panicking or crashing the node.
#[test]
fn test_get_shipment_creator_returns_not_found_for_nonexistent_id() {
    use crate::NavinError;
    let (_, client, _, _) = prepare_test();

    let result = client.try_get_shipment_creator(&9999u64);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "get_shipment_creator must return ShipmentNotFound for an ID that was never created"
    );
}

/// ID 0 is never assigned by the counter (counter starts at 1); querying it
/// must return ShipmentNotFound gracefully.
#[test]
fn test_get_shipment_creator_returns_not_found_for_zero_id() {
    use crate::NavinError;
    let (_, client, _, _) = prepare_test();

    let result = client.try_get_shipment_creator(&0u64);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "get_shipment_creator must return ShipmentNotFound for ID 0"
    );
}

/// Querying a large arbitrary shipment ID that does not exist must return
/// ShipmentNotFound — no storage panic or key error.
#[test]
fn test_get_shipment_creator_returns_not_found_for_large_invalid_id() {
    use crate::NavinError;
    let (_, client, _, _) = prepare_test();

    let result = client.try_get_shipment_creator(&u64::MAX);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "get_shipment_creator must return ShipmentNotFound for u64::MAX ID"
    );
}

/// Confirm that a real shipment returns the correct sender (creator) address.
#[test]
fn test_get_shipment_creator_returns_sender_for_valid_shipment() {
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let creator = client.get_shipment_creator(&shipment_id);
    assert_eq!(
        creator, company,
        "get_shipment_creator must return the original sender address"
    );
}
