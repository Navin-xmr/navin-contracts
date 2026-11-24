#![cfg(test)]

extern crate std;

use super::test_helpers::*;

#[test]
fn test_revoke_role_company() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);
    assert_eq!(client.get_role(&company), crate::types::Role::Company);

    client.revoke_role(&admin, &company);
    assert_eq!(client.get_role(&company), crate::types::Role::Unassigned);
}

#[test]
fn test_revoke_role_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);
    assert_eq!(client.get_role(&carrier), crate::types::Role::Carrier);

    client.revoke_role(&admin, &carrier);
    assert_eq!(client.get_role(&carrier), crate::types::Role::Unassigned);
}

#[test]
#[should_panic(expected = "Error(Contract, #32)")]
fn test_revoke_role_self_revoke_fails() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Admin cannot self-revoke (error code 32 = CannotSelfRevoke)
    client.revoke_role(&admin, &admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_revoke_role_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let non_admin = Address::generate(&env);
    let target = Address::generate(&env);
    client.add_company(&admin, &target);

    // Non-admin cannot revoke roles (error code 3 = Unauthorized)
    client.revoke_role(&non_admin, &target);
}

#[test]
fn test_revoke_role_emits_event() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);
    client.revoke_role(&admin, &company);

    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "role_revoked") {
                        found = true;
                    }
                }
            }
        }
    }
    assert!(found, "role_revoked event not found");
}

#[test]
fn test_role_changed_event_emitted_on_add_company() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);

    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "role_changed") {
                        found = true;
                    }
                }
            }
        }
    }
    assert!(found, "role_changed event not found on add_company");
}

#[test]
fn test_role_changed_event_emitted_on_add_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);

    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "role_changed") {
                        found = true;
                    }
                }
            }
        }
    }
    assert!(found, "role_changed event not found on add_carrier");
}

#[test]
fn test_role_changed_event_emitted_on_revoke_role() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);
    client.revoke_role(&admin, &company);

    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "role_changed") {
                        found = true;
                    }
                }
            }
        }
    }
    assert!(found, "role_changed event not found on revoke_role");
}

#[test]
fn test_suspend_role_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);

    // Suspend the role
    client.suspend_role(&admin, &company);

    // Verify role_changed event was emitted with Suspended action
    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "role_changed") {
                        found = true;
                    }
                }
            }
        }
    }
    assert!(found, "role_changed event not found on suspend_role");
}

#[test]
fn test_reactivate_role_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);
    client.suspend_role(&admin, &company);

    // Reactivate the role
    client.reactivate_role(&admin, &company);

    // Verify role_changed event was emitted with Reactivated action
    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "role_changed") {
                        found = true;
                    }
                }
            }
        }
    }
    assert!(found, "role_changed event not found on reactivate_role");
}

#[test]
fn test_suspended_role_cannot_perform_actions() {
    use soroban_sdk::testutils::Address as _;

    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Suspend the company role
    client.suspend_role(&admin, &company);

    // Suspended company cannot create shipment - should panic with Unauthorized
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &soroban_sdk::Vec::new(&env),
            &deadline,
        );
    }));

    assert!(
        result.is_err(),
        "Suspended company should not be able to create shipments"
    );
}

// Deadline Grace Period Tests

/// Within the grace window: deadline has passed but grace has not — must return NotExpired.
/// Exactly at the grace boundary: timestamp == deadline + grace — must succeed.
/// After the grace window: timestamp > deadline + grace — must succeed and cancel.
/// Zero grace (default): deadline passed by 1 second — must succeed immediately.
/// Validate that deadline_grace_seconds > 604_800 is rejected by update_config.
#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_update_config_rejects_grace_period_exceeding_max() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let mut config = client.get_contract_config();
    config.deadline_grace_seconds = 604_801; // 1 second over the 7-day cap
    client.update_config(&admin, &config);
}

// force_cancel_shipment tests

/// Helper: initialise contract, register a company, create one shipment, and
/// return (env, client, admin, token_contract, company, shipment_id).
fn setup_force_cancel_env() -> (
    Env,
    NavinShipmentClient<'static>,
    Address,
    Address,
    Address,
    u64,
) {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[0xABu8; 32]);
    let deadline = env.ledger().timestamp() + 7200;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    crate::test_utils::allow_carrier(&client, &company, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    (env, client, admin, token_contract, company, shipment_id)
}

/// Admin can force-cancel a shipment in Created status.
/// Admin can force-cancel a shipment that is InTransit.
/// Admin can force-cancel a Disputed shipment (bypasses normal cancel restriction).
/// Non-admin caller is rejected with Unauthorized (#3).
#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_force_cancel_shipment_unauthorized_company() {
    let (env, client, _admin, _token_contract, company, shipment_id) = setup_force_cancel_env();

    let reason_hash = BytesN::from_array(&env, &[0x06u8; 32]);
    // company is not admin — must be rejected
    client.force_cancel_shipment(&company, &shipment_id, &reason_hash);
}

/// All-zero reason_hash is rejected with ForceCancelReasonHashMissing (#34).
#[test]
#[should_panic(expected = "Error(Contract, #34)")]
fn test_force_cancel_shipment_zero_reason_hash_rejected() {
    let (env, client, admin, _token_contract, _company, shipment_id) = setup_force_cancel_env();
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
    client.force_cancel_shipment(&admin, &shipment_id, &zero_hash);
}

/// Force-cancelling a non-existent shipment returns ShipmentNotFound (#4).
/// Force-cancelling an already-Delivered shipment returns ShipmentFinalized (#38).
/// Force-cancelling an already-Cancelled shipment returns ShipmentFinalized (#38).
/// Escrow is deterministically zeroed on force-cancel (no-escrow path).
/// Verifies escrow_amount stays 0 and force_cancelled event is emitted.
/// Both force_cancelled and shipment_cancelled events are emitted on force-cancel.
/// Payload shape regression test for both events emitted on force-cancel.
// Shipment Note Tests

// Idempotency Window Tests
// Integration Tests for Symbol and BytesN<32> Validators

#[test]
#[should_panic(expected = "Error(Contract, #60)")]
fn test_create_shipment_with_duplicate_milestone_symbols_fails() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    // Duplicate milestone names should fail validation
    milestones.push_back((Symbol::new(&env, "warehouse"), 50_u32));
    milestones.push_back((Symbol::new(&env, "warehouse"), 50_u32));

    let deadline = super::test_utils::future_deadline(&env, 86400);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
}

#[test]
fn test_operator_can_manage_roles() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let operator = Address::generate(&env);
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_operator(&admin, &operator);

    client.add_company(&operator, &company);
    client.suspend_company(&operator, &company);
    client.reactivate_company(&operator, &company);

    client.add_carrier(&operator, &carrier);
    client.suspend_carrier(&operator, &carrier);
    client.reactivate_carrier(&operator, &carrier);

    let outsider = Address::generate(&env);
    let result = client.try_add_company(&outsider, &Address::generate(&env));
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));
}

#[test]
fn test_get_canonical_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let mut fields = soroban_sdk::Vec::new(&env);
    fields.push_back(Symbol::new(&env, "test").into_val(&env));
    fields.push_back(123_u64.into_val(&env));

    let hash1 = client.get_canonical_hash(&fields);
    let hash2 = client.get_canonical_hash(&fields);

    assert_eq!(hash1, hash2);

    // Ensure different fields result in different hash
    fields.push_back(456_u64.into_val(&env));
    let hash3 = client.get_canonical_hash(&fields);
    assert_ne!(hash1, hash3);
}

#[test]
fn test_get_expected_token_decimals_policy() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    assert_eq!(
        client.get_expected_token_decimals(),
        crate::types::EXPECTED_TOKEN_DECIMALS
    );
}

#[test]
fn test_dispute_emits_escrow_frozen_event() {
    let (env, client, admin, token) = setup_shipment_env();
    client.initialize(&admin, &token);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let milestones = soroban_sdk::Vec::new(&env);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );

    client.raise_dispute(&company, &shipment_id, &data_hash);

    let events = env.events().all();

    let mut frozen_found = false;
    for (_contract_id, topic, data) in events.into_iter() {
        if let Some(topic_sym) = topic
            .get(0)
            .and_then(|v| Symbol::try_from_val(&env, &v).ok())
        {
            if topic_sym == Symbol::new(&env, crate::event_topics::ESCROW_FROZEN) {
                frozen_found = true;

                let data_vec =
                    soroban_sdk::Vec::<soroban_sdk::Val>::try_from_val(&env, &data).unwrap();
                assert_eq!(data_vec.len(), 4);

                let reason =
                    crate::types::EscrowFreezeReason::try_from_val(&env, &data_vec.get(1).unwrap())
                        .unwrap();
                let caller = Address::try_from_val(&env, &data_vec.get(2).unwrap()).unwrap();

                assert_eq!(reason, crate::types::EscrowFreezeReason::DisputeRaised);
                assert_eq!(caller, company);
            }
        }
    }

    assert!(frozen_found, "escrow_frozen event was not emitted");

    let stored_reason = client.get_escrow_freeze_reason(&shipment_id);
    assert_eq!(
        stored_reason,
        Some(crate::types::EscrowFreezeReason::DisputeRaised)
    );
}

// Finalization lock-out tests (issue #446)

// Metadata symbol boundary tests (issue #448)

// Rate-limit exhaustion and recovery tests (issue #18)

/// Test that rate limit exhaustion blocks the action as expected.
/// Test that advancing the ledger clears the rate limit window and restores the action.
/// Test that rate-limit behavior is deterministic across multiple cycles.
#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_created_at_fails_for_invalid_id() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment_created_at(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_shipment_created_at_fails_before_initialization() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    client.get_shipment_created_at(&1);
}

// [ISSUE #506] get_admin in multi-admin configurations

#[test]
fn test_get_admin_returns_primary_admin_after_multisig_init() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());
    client.init_multisig(&admin, &admins, &2);

    assert_eq!(
        client.get_admin(),
        admin,
        "get_admin must return the primary admin set during initialize"
    );
}

#[test]
fn test_get_admin_consistent_across_queries_in_multisig_mode() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admin2 = Address::generate(&env);
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &2);

    let first = client.get_admin();
    let second = client.get_admin();
    assert_eq!(
        first, second,
        "repeated get_admin calls must return the same address"
    );
    assert_eq!(first, admin);
}

#[test]
fn test_get_admin_unaffected_by_additional_multisig_admins() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let before = client.get_admin();

    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());
    client.init_multisig(&admin, &admins, &3);

    let after = client.get_admin();
    assert_eq!(
        before, after,
        "get_admin must not change after init_multisig"
    );
}

// [ISSUE #599] StatusHashNotFound error variant tests
//
// StatusHashNotFound (#44) is returned when a shipment exists but no hash was
// recorded for the requested status.
// ShipmentNotFound (#4) is returned when the shipment itself doesn't exist.

/// Error code pin: StatusHashNotFound discriminant must be 44.
#[test]
fn test_status_hash_not_found_error_code_is_44() {
    assert_eq!(
        NavinError::StatusHashNotFound as u32,
        44,
        "StatusHashNotFound discriminant must be 44"
    );
}

// Issue #581 – InvalidPaymentMilestones (code 59)
