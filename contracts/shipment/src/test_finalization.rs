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
