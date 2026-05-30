use crate::{NavinShipment, NavinShipmentClient, ShipmentStatus};
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
        &None,
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
        &None,
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
        &None,
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
        &None,
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

// ── Recovery flow regression tests ───────────────────────────────────────────

fn setup_recovery_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, client, admin, token) = setup_shipment_env();
    client.initialize(&admin, &token);
    client.add_company(&admin, &admin);
    // init_multisig requires 2–10 admins; use admin twice to satisfy the minimum.
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin.clone());
    client.init_multisig(&admin, &admins, &1);
    (env, client, admin, token)
}

fn make_shipment(
    env: &Env,
    client: &NavinShipmentClient,
    company: &Address,
    carrier: &Address,
    seed: u8,
) -> u64 {
    let data_hash = BytesN::from_array(env, &[seed; 32]);
    let deadline = env.ledger().timestamp() + 3600;
    client.create_shipment(
        company,
        &Address::generate(env),
        carrier,
        &data_hash,
        &Vec::new(env),
        &deadline,
        &None,
    )
}

/// Non-terminal shipments (Created, InTransit, Disputed) are recoverable;
/// Cancelled is the only terminal state that cannot transition to non-Cancelled targets.
/// Delivered can only go to Cancelled (the catch-all `(_, Cancelled)` arm fires first).
#[test]
fn test_recoverable_vs_unrecoverable_states() {
    use crate::recovery::is_valid_recovery_transition;

    // Any state → Cancelled is always allowed (catch-all arm in the function).
    for from in [
        ShipmentStatus::Created,
        ShipmentStatus::InTransit,
        ShipmentStatus::AtCheckpoint,
        ShipmentStatus::Disputed,
        ShipmentStatus::Delivered,
        ShipmentStatus::Cancelled,
    ] {
        assert!(
            is_valid_recovery_transition(&from, &ShipmentStatus::Cancelled),
            "{from:?} → Cancelled should be allowed"
        );
    }

    // Cancelled cannot transition to any non-Cancelled state.
    for to in [
        ShipmentStatus::Created,
        ShipmentStatus::InTransit,
        ShipmentStatus::AtCheckpoint,
        ShipmentStatus::Delivered,
        ShipmentStatus::Disputed,
    ] {
        assert!(
            !is_valid_recovery_transition(&ShipmentStatus::Cancelled, &to),
            "Cancelled → {to:?} must be unrecoverable"
        );
    }

    // Delivered cannot transition to non-Cancelled states (terminal guard fires after catch-all).
    for to in [
        ShipmentStatus::Created,
        ShipmentStatus::InTransit,
        ShipmentStatus::AtCheckpoint,
        ShipmentStatus::Disputed,
    ] {
        assert!(
            !is_valid_recovery_transition(&ShipmentStatus::Delivered, &to),
            "Delivered → {to:?} must be unrecoverable"
        );
    }
}

/// unlock_escrow zeroes the shipment's escrow_amount in storage and the
/// dedicated escrow storage entry is left as-is (the function only clears
/// the struct field — callers are responsible for the storage key).
#[test]
fn test_unlock_escrow_clears_shipment_escrow_amount() {
    let (env, client, admin, _) = setup_recovery_env();
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);

    let id = make_shipment(&env, &client, &admin, &carrier, 0xAA);
    let reason = BytesN::from_array(&env, &[0x01u8; 32]);

    env.as_contract(&client.address, || {
        // Manually set a non-zero escrow on the shipment struct.
        let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
        shipment.escrow_amount = 5_000;
        shipment.total_escrow = 5_000;
        crate::storage::set_shipment(&env, &shipment);

        crate::recovery::unlock_escrow(&env, &admin, id, &reason).unwrap();

        let after = crate::storage::get_shipment(&env, id).unwrap();
        assert_eq!(
            after.escrow_amount, 0,
            "escrow_amount must be zeroed after unlock"
        );
    });
}

/// recover_shipment transitions a stuck InTransit shipment to Cancelled and
/// persists the new status.
#[test]
fn test_recover_shipment_transitions_stuck_state() {
    let (env, client, admin, _) = setup_recovery_env();
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);

    let id = make_shipment(&env, &client, &admin, &carrier, 0xBB);
    let reason = BytesN::from_array(&env, &[0x02u8; 32]);

    env.as_contract(&client.address, || {
        // Force the shipment into InTransit (a "stuck" intermediate state).
        let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
        shipment.status = ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);

        crate::recovery::recover_shipment(&env, &admin, id, ShipmentStatus::Cancelled, &reason)
            .unwrap();

        let after = crate::storage::get_shipment(&env, id).unwrap();
        assert_eq!(after.status, ShipmentStatus::Cancelled);
    });
}

/// recover_shipment on a terminal (Delivered) shipment must fail — terminal
/// states are unrecoverable.
#[test]
fn test_recover_shipment_fails_for_terminal_state() {
    let (env, client, admin, _) = setup_recovery_env();
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);

    let id = make_shipment(&env, &client, &admin, &carrier, 0xCC);
    let reason = BytesN::from_array(&env, &[0x03u8; 32]);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
        shipment.status = ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);

        let result =
            crate::recovery::recover_shipment(&env, &admin, id, ShipmentStatus::InTransit, &reason);
        assert!(
            matches!(result, Err(crate::NavinError::InvalidStatus)),
            "recovery from Delivered must return InvalidStatus"
        );
    });
}

/// unlock_escrow on a shipment with zero escrow must fail predictably.
#[test]
fn test_unlock_escrow_fails_when_escrow_already_zero() {
    let (env, client, admin, _) = setup_recovery_env();
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);

    let id = make_shipment(&env, &client, &admin, &carrier, 0xDD);
    let reason = BytesN::from_array(&env, &[0x04u8; 32]);

    env.as_contract(&client.address, || {
        // escrow_amount is 0 by default after creation.
        let result = crate::recovery::unlock_escrow(&env, &admin, id, &reason);
        assert!(
            matches!(result, Err(crate::NavinError::EscrowLocked)),
            "unlock with zero escrow must return EscrowLocked"
        );
    });
}

/// clear_finalization on a non-finalized shipment must fail predictably.
#[test]
fn test_clear_finalization_fails_when_not_finalized() {
    let (env, client, admin, _) = setup_recovery_env();
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);

    let id = make_shipment(&env, &client, &admin, &carrier, 0xEE);
    let reason = BytesN::from_array(&env, &[0x05u8; 32]);

    env.as_contract(&client.address, || {
        let result = crate::recovery::clear_finalization(&env, &admin, id, &reason);
        assert!(
            matches!(result, Err(crate::NavinError::InvalidStatus)),
            "clear_finalization on non-finalized shipment must return InvalidStatus"
        );
    });
}

/// clear_finalization on a finalized shipment clears the flag.
#[test]
fn test_clear_finalization_succeeds_on_finalized_shipment() {
    let (env, client, admin, _) = setup_recovery_env();
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);

    let id = make_shipment(&env, &client, &admin, &carrier, 0xFF);
    let reason = BytesN::from_array(&env, &[0x06u8; 32]);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
        shipment.finalized = true;
        shipment.status = ShipmentStatus::Cancelled;
        crate::storage::set_shipment(&env, &shipment);

        crate::recovery::clear_finalization(&env, &admin, id, &reason).unwrap();

        let after = crate::storage::get_shipment(&env, id).unwrap();
        assert!(!after.finalized, "finalized flag must be cleared");
    });
}

/// Test archival behavior with different shipment states
#[test]
fn test_archival_behavior_with_different_states() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Test archival of Created shipment
    let created_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );
    
    client.archive_shipment(&admin, &created_id);
    let created_archived = client.get_shipment(&created_id);
    assert!(created_archived.archived);
    assert_eq!(created_archived.status, ShipmentStatus::Created);

    // Test archival of InTransit shipment
    let intransit_id = client.create_shipment(
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
        &intransit_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );
    client.archive_shipment(&admin, &intransit_id);
    let intransit_archived = client.get_shipment(&intransit_id);
    assert!(intransit_archived.archived);
    assert_eq!(intransit_archived.status, ShipmentStatus::InTransit);

    // Test archival of Delivered shipment
    let delivered_id = client.create_shipment(
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
        &delivered_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );
    client.confirm_delivery(&receiver, &delivered_id, &data_hash);
    client.archive_shipment(&admin, &delivered_id);
    let delivered_archived = client.get_shipment(&delivered_id);
    assert!(delivered_archived.archived);
    assert_eq!(delivered_archived.status, ShipmentStatus::Delivered);
}

/// Verify archived shipments are classified correctly
#[test]
fn test_archived_shipments_classified_correctly() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Create and archive multiple shipments
    let id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );
    client.archive_shipment(&admin, &id1);

    let id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );
    client.archive_shipment(&admin, &id2);

    // Check contract health to verify classification
    let health = client.check_contract_health(&admin);
    assert_eq!(health.archived_shipments_counted, 2);
    assert_eq!(health.total_shipments, 2);
    
    // Verify individual shipments are marked as archived
    let s1 = client.get_shipment(&id1);
    let s2 = client.get_shipment(&id2);
    assert!(s1.archived);
    assert!(s2.archived);
}

/// Restore paths remain observable after archival
#[test]
fn test_restore_paths_remain_observable_after_archival() {
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
        &None,
    );

    // Archive the shipment
    client.archive_shipment(&admin, &shipment_id);

    // Verify we can still get the shipment (restore path works)
    let archived = client.get_shipment(&shipment_id);
    assert!(archived.archived);
    assert_eq!(archived.id, shipment_id);
    
    // Verify we can still get metadata for archived shipments
    client.set_shipment_metadata(&company, &shipment_id, &Symbol::new(&env, "archive_test"), &Symbol::new(&env, "value"));
    let metadata = client.get_shipment_metadata(&shipment_id, &Symbol::new(&env, "archive_test"));
    assert_eq!(metadata, Symbol::new(&env, "value"));
}

/// The test covers both archived and active states
#[test]
fn test_archival_tests_cover_both_archived_and_active_states() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Create active shipment
    let active_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    // Create and archive shipment
    let archived_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );
    client.archive_shipment(&admin, &archived_id);

    // Verify both states are accessible
    let active_shipment = client.get_shipment(&active_id);
    let archived_shipment = client.get_shipment(&archived_id);
    
    assert!(!active_shipment.archived);
    assert!(archived_shipment.archived);
    
    // Verify both can be queried via batch
    let mut ids = Vec::new(&env);
    ids.push_back(active_id);
    ids.push_back(archived_id);
    
    let batch = client.get_shipments_batch(&ids);
    assert_eq!(batch.len(), 2);
    assert!(batch.get(0).unwrap().is_some());
    assert!(batch.get(1).unwrap().is_some());
}

#[test]
fn test_recovery_clear_finalization_unsets_finalized_flag() {
    let (env, client, admin, _) = setup_recovery_env();
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);

    let id = make_shipment(&env, &client, &admin, &carrier, 0xFF);
    let reason = BytesN::from_array(&env, &[0x06u8; 32]);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
        shipment.finalized = true;
        shipment.status = ShipmentStatus::Cancelled;
        crate::storage::set_shipment(&env, &shipment);

        crate::recovery::clear_finalization(&env, &admin, id, &reason).unwrap();

        let after = crate::storage::get_shipment(&env, id).unwrap();
        assert!(!after.finalized, "finalized flag must be cleared");
    });
}
