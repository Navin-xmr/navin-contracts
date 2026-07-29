use crate::{audit, NavinShipment, NavinShipmentClient, ShipmentStatus};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Symbol, Vec};

#[soroban_sdk::contract]
struct MockToken;
#[soroban_sdk::contractimpl]
impl MockToken {
    pub fn decimals(_env: soroban_sdk::Env) -> u32 {
        7
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

fn setup_shipment_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = crate::test_utils::setup_env();

    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));

    (env, client, admin, token_contract)
}

#[test]
fn test_finalization_on_delivery_settlement() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Initial state: not finalized
    let shipment = client.get_shipment(&shipment_id);
    assert!(!shipment.finalized);

    // Step 1: Deposit escrow
    client.deposit_escrow(&company, &shipment_id, &1000);

    // Step 2: Transition to Delivered - this should release remaining escrow and finalize
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );
    client.confirm_delivery(&receiver, &shipment_id, &data_hash);

    // Should be finalized because status is Delivered and escrow is released (cleared to 0)
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Delivered);
    assert_eq!(shipment.escrow_amount, 0);
    assert!(shipment.finalized);
}

#[test]
fn test_finalization_on_cancel_with_zero_escrow() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Initial state: not finalized
    let shipment = client.get_shipment(&shipment_id);
    assert!(!shipment.finalized);

    // Cancel without escrow should finalize immediately
    client.cancel_shipment(&company, &shipment_id, &data_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
    assert!(shipment.finalized);
}

#[test]
#[should_panic(expected = "Error(Contract, #38)")]
fn test_mutation_rejected_after_finalization() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Finalize it
    client.cancel_shipment(&company, &shipment_id, &data_hash);
    let shipment = client.get_shipment(&shipment_id);
    assert!(shipment.finalized);

    // Try to update metadata - should panic with ShipmentFinalized (38)
    client.set_shipment_metadata(
        &company,
        &shipment_id,
        &Symbol::new(&env, "key"),
        &Symbol::new(&env, "val"),
    );
}

// ── Finalization lock-out: mutating paths after finalization (issue #446) ────

/// Helper: create a shipment and cancel it (which finalizes it).
/// Returns (shipment_id, company, receiver, carrier, data_hash).
fn create_and_finalize(
    env: &Env,
    client: &NavinShipmentClient<'static>,
    admin: &Address,
    token_contract: &Address,
) -> (u64, Address, Address, Address, BytesN<32>) {
    let company = Address::generate(env);
    let receiver = Address::generate(env);
    let carrier = Address::generate(env);
    let data_hash = BytesN::from_array(env, &[0xFFu8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(admin, token_contract);
    client.add_company(admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(env),
        &deadline,
    );
    client.cancel_shipment(&company, &shipment_id, &data_hash);
    assert!(client.get_shipment(&shipment_id).finalized);
    (shipment_id, company, receiver, carrier, data_hash)
}

#[test]
fn test_update_status_rejected_after_finalization() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let (shipment_id, _company, _receiver, carrier, data_hash) =
        create_and_finalize(&env, &client, &admin, &token_contract);

    // The carrier is the authorised caller for update_status; after finalization
    // the call must return ShipmentFinalized (#38).
    let result = client.try_update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );
    assert!(
        matches!(result, Err(Ok(crate::NavinError::ShipmentFinalized))),
        "update_status must be rejected with ShipmentFinalized after finalization"
    );
}

#[test]
fn test_deposit_escrow_rejected_after_finalization() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let (shipment_id, company, _receiver, _carrier, _) =
        create_and_finalize(&env, &client, &admin, &token_contract);

    let result = client.try_deposit_escrow(&company, &shipment_id, &1000_i128);
    assert!(
        matches!(result, Err(Ok(crate::NavinError::ShipmentFinalized))),
        "deposit_escrow must be rejected with ShipmentFinalized after finalization"
    );
}

#[test]
fn test_raise_dispute_rejected_after_finalization() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let (shipment_id, company, _receiver, _carrier, data_hash) =
        create_and_finalize(&env, &client, &admin, &token_contract);

    let result = client.try_raise_dispute(&company, &shipment_id, &data_hash);
    assert!(
        matches!(result, Err(Ok(crate::NavinError::ShipmentFinalized))),
        "raise_dispute must be rejected with ShipmentFinalized after finalization"
    );
}

#[test]
fn test_cancel_shipment_rejected_after_finalization() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let (shipment_id, company, _receiver, _carrier, data_hash) =
        create_and_finalize(&env, &client, &admin, &token_contract);

    // Attempting to cancel an already-finalized shipment must be rejected.
    let result = client.try_cancel_shipment(&company, &shipment_id, &data_hash);
    assert!(
        matches!(result, Err(Ok(crate::NavinError::ShipmentFinalized))),
        "cancel_shipment must be rejected with ShipmentFinalized on already-finalized shipment"
    );
}

#[test]
fn test_set_metadata_rejected_after_finalization() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let (shipment_id, company, _receiver, _carrier, _) =
        create_and_finalize(&env, &client, &admin, &token_contract);

    let result = client.try_set_shipment_metadata(
        &company,
        &shipment_id,
        &Symbol::new(&env, "key"),
        &Symbol::new(&env, "val"),
    );
    assert!(
        matches!(result, Err(Ok(crate::NavinError::ShipmentFinalized))),
        "set_shipment_metadata must be rejected after finalization"
    );
}

/// All mutating paths must consistently return ShipmentFinalized across
/// repeated calls — the lock-out is stable under reruns.
#[test]
fn test_lockout_is_stable_across_reruns() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let (shipment_id, company, _receiver, carrier, data_hash) =
        create_and_finalize(&env, &client, &admin, &token_contract);

    for _ in 0..3 {
        assert!(
            matches!(
                client.try_update_status(
                    &carrier,
                    &shipment_id,
                    &ShipmentStatus::InTransit,
                    &data_hash
                ),
                Err(Ok(crate::NavinError::ShipmentFinalized))
            ),
            "update_status lockout must be stable"
        );
        assert!(
            matches!(
                client.try_deposit_escrow(&company, &shipment_id, &500_i128),
                Err(Ok(crate::NavinError::ShipmentFinalized))
            ),
            "deposit_escrow lockout must be stable"
        );
        assert!(
            matches!(
                client.try_raise_dispute(&company, &shipment_id, &data_hash),
                Err(Ok(crate::NavinError::ShipmentFinalized))
            ),
            "raise_dispute lockout must be stable"
        );
    }
}

#[test]
fn test_archival_permitted_after_finalization() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Finalize it
    client.cancel_shipment(&company, &shipment_id, &data_hash);
    let shipment = client.get_shipment(&shipment_id);
    assert!(shipment.finalized);

    // Archiving should succeed (proving the finalize lock exception)
    client.archive_shipment(&admin, &shipment_id);

    // Verify it's still readable (fallback to temporary storage works)
    let archived = client.get_shipment(&shipment_id);
    assert_eq!(archived.id, shipment_id);
}

// ── Audit sequence continuity (issue #535) ──────────────────────────────────

#[test]
fn test_audit_sequence_continuity() {
    let env = soroban_sdk::Env::default();
    let contract_id = env.register(crate::NavinShipment, ());
    let _client = NavinShipmentClient::new(&env, &contract_id);

    // Initial count must be 0
    let initial_count = env.as_contract(&contract_id, || audit::get_audit_entry_count(&env));
    assert_eq!(initial_count, 0, "audit entry count must start at 0");

    // Insert entries and verify monotonic IDs
    let admin = Address::generate(&env);
    let actor1 = Address::generate(&env);
    let actor2 = Address::generate(&env);
    let actor3 = Address::generate(&env);

    let ids: Vec<u64> = env.as_contract(&contract_id, || {
        let id1 = audit::get_next_audit_entry_id(&env).unwrap();
        audit::store_audit_entry(
            &env,
            &audit::AuditLogEntry {
                entry_id: id1,
                event_type: audit::AuditEventType::RoleAssigned,
                actor: admin.clone(),
                target: actor1,
                timestamp: 1000,
            },
        );

        let id2 = audit::get_next_audit_entry_id(&env).unwrap();
        audit::store_audit_entry(
            &env,
            &audit::AuditLogEntry {
                entry_id: id2,
                event_type: audit::AuditEventType::RoleRevoked,
                actor: admin.clone(),
                target: actor2,
                timestamp: 2000,
            },
        );

        let id3 = audit::get_next_audit_entry_id(&env).unwrap();
        audit::store_audit_entry(
            &env,
            &audit::AuditLogEntry {
                entry_id: id3,
                event_type: audit::AuditEventType::RoleSuspended,
                actor: admin.clone(),
                target: actor3,
                timestamp: 3000,
            },
        );

        soroban_sdk::vec![&env, id1, id2, id3]
    });

    // Verify monotonic sequence: 0, 1, 2
    assert_eq!(ids.len(), 3, "must have 3 audit entries");
    assert_eq!(ids.get(0).unwrap(), 0, "first entry ID must be 0");
    assert_eq!(ids.get(1).unwrap(), 1, "second entry ID must be 1");
    assert_eq!(ids.get(2).unwrap(), 2, "third entry ID must be 2");

    // Verify count reflects 3 entries
    let final_count = env.as_contract(&contract_id, || audit::get_audit_entry_count(&env));
    assert_eq!(
        final_count, 3,
        "audit entry count must be 3 after inserting 3 entries"
    );

    // Verify entries can be read back (they exist in storage)
    let count_again = env.as_contract(&contract_id, || audit::get_audit_entry_count(&env));
    assert_eq!(count_again, 3, "count must persist between reads");
}

#[test]
fn test_recovery_history_logging() {
    use crate::types::RecoveryActionType;

    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let reason1 = BytesN::from_array(&env, &[0xA1; 32]);
    let reason2 = BytesN::from_array(&env, &[0xB2; 32]);

    // Initial history is empty
    assert_eq!(client.get_recovery_record_count(&shipment_id), 0);
    assert_eq!(client.get_recovery_history(&shipment_id).len(), 0);

    // Action 1: recover shipment status from Created to Cancelled
    client.recover_shipment(&admin, &shipment_id, &ShipmentStatus::Cancelled, &reason1);

    // Set finalized flag so clear_finalization can be invoked
    env.as_contract(&client.address, || {
        let mut s = crate::storage::get_shipment(&env, shipment_id).unwrap();
        s.finalized = true;
        crate::storage::set_shipment(&env, &s);
    });

    // Action 2: clear finalization flag
    client.clear_finalization(&admin, &shipment_id, &reason2);

    // Verify history log
    assert_eq!(client.get_recovery_record_count(&shipment_id), 2);
    let history = client.get_recovery_history(&shipment_id);
    assert_eq!(history.len(), 2);

    let rec1 = history.get(0).unwrap();
    assert_eq!(rec1.action_type, RecoveryActionType::RecoverShipment);
    assert_eq!(rec1.admin, admin);
    assert_eq!(rec1.reason_hash, reason1);

    let rec2 = history.get(1).unwrap();
    assert_eq!(rec2.action_type, RecoveryActionType::ClearFinalization);
    assert_eq!(rec2.admin, admin);
    assert_eq!(rec2.reason_hash, reason2);
}

// ── #649: No orphaned counters/indexes after archive_shipment ────────────────

/// Archive a cancelled shipment that has notes, dispute evidence, and a
/// recovery record, then assert:
///   1. The shipment is still readable from temporary (archived) storage.
///   2. No per-shipment counter or index key survives in persistent storage.
///   3. `check_contract_health` reports zero storage inconsistencies.
#[test]
fn test_archive_clears_all_per_shipment_counters() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[0xABu8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    // Add two notes via the public API
    let note0 = BytesN::from_array(&env, &[0x11u8; 32]);
    let note1 = BytesN::from_array(&env, &[0x22u8; 32]);
    client.append_note_hash(&company, &shipment_id, &note0);
    client.append_note_hash(&company, &shipment_id, &note1);
    assert_eq!(client.get_note_count(&shipment_id), 2);

    // Inject dispute evidence and recovery records directly via storage so
    // we don't need to go through the Disputed state machine path.  This
    // mirrors what test_detect_storage_inconsistencies does and is the
    // standard approach for seeding raw storage in this test suite.
    let cid = client.address.clone();
    env.as_contract(&cid, || {
        use crate::types::{DataKey, RecoveryActionType, RecoveryRecord};

        // Two evidence hashes (index 0 and 1)
        let evidence0 = BytesN::from_array(&env, &[0x44u8; 32]);
        let evidence1 = BytesN::from_array(&env, &[0x55u8; 32]);
        env.storage().persistent().set(&DataKey::DisputeEvidence(shipment_id, 0), &evidence0);
        env.storage().persistent().set(&DataKey::DisputeEvidence(shipment_id, 1), &evidence1);
        env.storage().persistent().set(&DataKey::DisputeEvidenceCount(shipment_id), &2u32);

        // One recovery record (index 0)
        let record = RecoveryRecord {
            action_type: RecoveryActionType::RecoverShipment,
            admin: admin.clone(),
            reason_hash: BytesN::from_array(&env, &[0x77u8; 32]),
            timestamp: env.ledger().timestamp(),
        };
        env.storage().persistent().set(&DataKey::RecoveryRecord(shipment_id, 0), &record);
        env.storage().persistent().set(&DataKey::RecoveryRecordCount(shipment_id), &1u32);
    });

    assert_eq!(client.get_dispute_evidence_count(&shipment_id), 2);
    assert_eq!(client.get_recovery_record_count(&shipment_id), 1);

    // Cancel to reach a terminal state
    let cancel_hash = BytesN::from_array(&env, &[0x66u8; 32]);
    client.cancel_shipment(&company, &shipment_id, &cancel_hash);
    assert_eq!(client.get_shipment(&shipment_id).status, ShipmentStatus::Cancelled);

    // Sanity: health is clean before archival
    let pre_health = client.check_contract_health(&admin);
    assert_eq!(
        pre_health.storage_inconsistencies.len(),
        0,
        "unexpected pre-archive inconsistencies: {:?}",
        pre_health.storage_inconsistencies
    );

    // Archive the shipment — this is the operation under test
    client.archive_shipment(&admin, &shipment_id);

    // 1. Shipment must still be readable from temporary (archived) storage
    let archived = client.get_shipment(&shipment_id);
    assert_eq!(archived.id, shipment_id);
    assert_eq!(archived.status, ShipmentStatus::Cancelled);

    // 2. Verify no orphaned persistent keys remain by inspecting storage directly
    env.as_contract(&cid, || {
        use crate::types::DataKey;

        // Primary shipment record must be gone
        assert!(
            !env.storage().persistent().has(&DataKey::Shipment(shipment_id)),
            "Shipment key must be removed after archive"
        );

        // Counters / scalar keys
        assert!(
            !env.storage().persistent().has(&DataKey::EventCount(shipment_id)),
            "EventCount must be cleared after archive"
        );
        assert!(
            !env.storage().persistent().has(&DataKey::MilestoneEventCount(shipment_id)),
            "MilestoneEventCount must be cleared after archive"
        );
        assert!(
            !env.storage().persistent().has(&DataKey::BreachEventCount(shipment_id)),
            "BreachEventCount must be cleared after archive"
        );
        assert!(
            !env.storage().persistent().has(&DataKey::LastStatusUpdate(shipment_id)),
            "LastStatusUpdate must be cleared after archive"
        );
        assert!(
            !env.storage().persistent().has(&DataKey::ConfirmationHash(shipment_id)),
            "ConfirmationHash must be cleared after archive"
        );
        assert!(
            !env.storage().persistent().has(&DataKey::Escrow(shipment_id)),
            "Escrow key must be cleared after archive"
        );
        assert!(
            !env.storage().persistent().has(&DataKey::EscrowFreezeReasonByShipment(shipment_id)),
            "EscrowFreezeReasonByShipment must be cleared after archive"
        );
        assert!(
            !env.storage().persistent().has(&DataKey::ActiveSettlement(shipment_id)),
            "ActiveSettlement must be cleared after archive"
        );

        // Note count + per-index entries
        assert!(
            !env.storage().persistent().has(&DataKey::ShipmentNoteCount(shipment_id)),
            "ShipmentNoteCount must be cleared after archive"
        );
        assert!(
            !env.storage().persistent().has(&DataKey::ShipmentNote(shipment_id, 0)),
            "ShipmentNote[0] must be cleared after archive"
        );
        assert!(
            !env.storage().persistent().has(&DataKey::ShipmentNote(shipment_id, 1)),
            "ShipmentNote[1] must be cleared after archive"
        );

        // Evidence count + per-index entries
        assert!(
            !env.storage().persistent().has(&DataKey::DisputeEvidenceCount(shipment_id)),
            "DisputeEvidenceCount must be cleared after archive"
        );
        assert!(
            !env.storage().persistent().has(&DataKey::DisputeEvidence(shipment_id, 0)),
            "DisputeEvidence[0] must be cleared after archive"
        );
        assert!(
            !env.storage().persistent().has(&DataKey::DisputeEvidence(shipment_id, 1)),
            "DisputeEvidence[1] must be cleared after archive"
        );

        // Recovery record count + per-index entries
        assert!(
            !env.storage().persistent().has(&DataKey::RecoveryRecordCount(shipment_id)),
            "RecoveryRecordCount must be cleared after archive"
        );
        assert!(
            !env.storage().persistent().has(&DataKey::RecoveryRecord(shipment_id, 0)),
            "RecoveryRecord[0] must be cleared after archive"
        );
    });

    // 3. check_contract_health must report no false-positive inconsistencies
    let post_health = client.check_contract_health(&admin);
    assert_eq!(
        post_health.storage_inconsistencies.len(),
        0,
        "check_contract_health must report no inconsistencies after archive: {:?}",
        post_health.storage_inconsistencies
    );
}

/// Archiving a shipment that has *no* optional counters (no notes, evidence,
/// or recovery records) must still succeed cleanly — the purge helpers are
/// no-ops when counts are zero.
#[test]
fn test_archive_with_no_optional_counters_is_clean() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[0x01u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    client.cancel_shipment(&company, &shipment_id, &data_hash);

    // Archive with no extra state accumulated
    client.archive_shipment(&admin, &shipment_id);

    // Shipment is readable
    let archived = client.get_shipment(&shipment_id);
    assert_eq!(archived.id, shipment_id);

    // No inconsistencies
    let health = client.check_contract_health(&admin);
    assert_eq!(
        health.storage_inconsistencies.len(),
        0,
        "no inconsistencies expected after bare archive: {:?}",
        health.storage_inconsistencies
    );
}

// ── #677-#680: recovery wrappers return NotInitialized instead of panicking ──

#[test]
fn test_recover_shipment_before_initialize_returns_error() {
    let (env, client, admin, _token) = setup_shipment_env();
    let reason = BytesN::from_array(&env, &[0xC3; 32]);

    assert_eq!(
        client.try_recover_shipment(&admin, &1u64, &ShipmentStatus::Cancelled, &reason),
        Err(Ok(crate::NavinError::NotInitialized))
    );
}

#[test]
fn test_unlock_escrow_before_initialize_returns_error() {
    let (env, client, admin, _token) = setup_shipment_env();
    let reason = BytesN::from_array(&env, &[0xC4; 32]);

    assert_eq!(
        client.try_unlock_escrow(&admin, &1u64, &reason),
        Err(Ok(crate::NavinError::NotInitialized))
    );
}
