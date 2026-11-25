#![cfg(test)]

extern crate std;

use super::test_helpers::*;

#[test]
#[should_panic(expected = "Error(Contract, #59)")]
fn test_create_shipment_zero_percentage_milestone_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    // A zero percentage is an invalid milestone weight.
    milestones.push_back((Symbol::new(&env, "pickup"), 0_u32));
    milestones.push_back((Symbol::new(&env, "delivery"), 100_u32));

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
#[should_panic(expected = "Error(Contract, #59)")]
fn test_create_shipment_over_100_percentage_milestone_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 86400;
    let mut milestones = soroban_sdk::Vec::new(&env);
    // 150 % is out of bounds for a single milestone weight.
    milestones.push_back((Symbol::new(&env, "pickup"), 150_u32));

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Issue #596 — InvalidMigrationEdge (code 47)
// Issue #758 — rollback and skip-version migrations are intentionally
// unsupported. There is no runtime-configurable allowed-edges list.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_upgrade_valid_forward_migration_succeeds() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let wasm: &[u8] = crate::test_utils::UPGRADE_WASM;
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);
    client.initialize(&admin, &token_contract);

    // Version starts at 1
    assert_eq!(client.get_version(), 1);

    // Valid forward migration 1→2
    let result = client.try_upgrade(&admin, &new_wasm_hash, &2);
    assert_eq!(result, Ok(Ok(())));

    // The upgrade swaps the running wasm for real, so the version can no
    // longer be read through the (now stale) client ABI — read storage
    // directly instead, as other post-upgrade tests in this file do.
    let contract_id = client.address.clone();
    env.as_contract(&contract_id, || {
        assert_eq!(crate::storage::get_version(&env), 2);
    });
}

#[test]
fn test_upgrade_downgrade_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &token_contract);

    // Put the contract at version 2 directly — a real `upgrade` call would
    // swap the running wasm for real, which would break every subsequent
    // client call in this test (the client's ABI no longer matches the
    // deployed code). The edge-rejection check being tested here runs
    // before any wasm is touched, so a direct storage write is equivalent
    // and keeps the client usable for the second call.
    env.as_contract(&client.address, || {
        crate::storage::set_version(&env, 2);
    });

    // Now try to downgrade 2→1 — should fail with InvalidMigrationEdge
    let result = client.try_upgrade(&admin, &new_wasm_hash, &1);
    assert_eq!(result, Err(Ok(NavinError::InvalidMigrationEdge)));
}

#[test]
fn test_upgrade_skip_version_rejected_detailed() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &token_contract);

    // Skip from 1 to 3 — intentionally unsupported (no allowed-edges list)
    let result = client.try_upgrade(&admin, &new_wasm_hash, &3);
    assert_eq!(result, Err(Ok(NavinError::InvalidMigrationEdge)));
}

#[test]
fn test_upgrade_same_version_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &token_contract);

    // Staying at the current version is not a sequential forward upgrade.
    let result = client.try_upgrade(&admin, &new_wasm_hash, &1);
    assert_eq!(result, Err(Ok(NavinError::InvalidMigrationEdge)));
}

#[test]
fn test_dry_run_migration_skip_version_rejected_detailed() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Skip from 1 to 3 — should fail
    let result = client.try_dry_run_migration(&3);
    assert_eq!(result, Err(Ok(NavinError::InvalidMigrationEdge)));
}

#[test]
fn test_dry_run_migration_downgrade_rejected_detailed() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Rollback from 1 to 0 — intentionally unsupported
    let result = client.try_dry_run_migration(&0);
    assert_eq!(result, Err(Ok(NavinError::InvalidMigrationEdge)));
}

#[test]
fn test_dry_run_migration_same_version_rejected() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let result = client.try_dry_run_migration(&1);
    assert_eq!(result, Err(Ok(NavinError::InvalidMigrationEdge)));
}

#[test]
fn test_dry_run_migration_valid_forward_succeeds_detailed() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Valid forward migration 1→2
    let result = client.try_dry_run_migration(&2);
    assert!(result.is_ok());
    let report = result.unwrap().unwrap();
    assert_eq!(report.current_version, 1);
    assert_eq!(report.target_version, 2);
}

#[test]
fn test_upgrade_non_admin_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &token_contract);

    let outsider = Address::generate(&env);
    let result = client.try_upgrade(&outsider, &new_wasm_hash, &2);
    assert_eq!(result, Err(Ok(NavinError::Unauthorized)));
}

#[test]
fn test_upgrade_non_admin_dry_run_rejected() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // dry_run_migration is permissionless (no auth required), so it should
    // succeed for valid edges and fail with InvalidMigrationEdge for invalid ones.
    // This test verifies that a non-admin can still call dry_run for a valid edge.
    let result = client.try_dry_run_migration(&2);
    assert!(result.is_ok());
}

// ═══════════════════════════════════════════════════════════════════════════
// Issue #595 — MilestoneLimitExceeded (code 48)
// ═══════════════════════════════════════════════════════════════════════════

/// ShipmentNotFound (#4) and StatusHashNotFound (#44) must be distinct error codes —
/// callers must be able to tell the difference.
#[test]
fn test_status_hash_not_found_is_distinct_from_shipment_not_found() {
    assert_ne!(
        NavinError::ShipmentNotFound as u32,
        NavinError::StatusHashNotFound as u32,
        "ShipmentNotFound and StatusHashNotFound must have different discriminants"
    );
    assert_eq!(NavinError::ShipmentNotFound as u32, 4);
    assert_eq!(NavinError::StatusHashNotFound as u32, 44);
}

// [ISSUE #598] DataHashMismatch error variant tests
//
// DataHashMismatch (#45) is returned by assert_delivery_hash
// when the provided hash does not match the value stored on-chain.

/// Error code pin: DataHashMismatch discriminant must be exactly 45.
#[test]
fn test_data_hash_mismatch_error_code_is_45() {
    assert_eq!(
        NavinError::DataHashMismatch as u32,
        45,
        "DataHashMismatch discriminant must be 45"
    );
}

/// assert_delivery_hash returns ShipmentNotFound for a non-existent shipment.
#[test]
fn test_assert_delivery_hash_nonexistent_shipment_returns_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let hash = BytesN::from_array(&env, &[1u8; 32]);
    let result = client.try_assert_delivery_hash(&9999u64, &hash);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "assert_delivery_hash for non-existent shipment must return ShipmentNotFound (#4)"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Issue #593 — CreationQuotaExceeded (code 53)
// ═══════════════════════════════════════════════════════════════════════════

/// DataHashMismatch (#45), StatusHashNotFound (#44), and ShipmentNotFound (#4)
/// must all be distinct error codes.
#[test]
fn test_data_hash_mismatch_is_distinct_from_other_hash_errors() {
    assert_ne!(
        NavinError::DataHashMismatch as u32,
        NavinError::StatusHashNotFound as u32
    );
    assert_ne!(
        NavinError::DataHashMismatch as u32,
        NavinError::ShipmentNotFound as u32
    );
    assert_eq!(NavinError::DataHashMismatch as u32, 45);
}

// [ISSUE #597] CircuitBreakerOpen error variant tests
//
// CircuitBreakerOpen (#46) is returned by invoke_token_transfer when the
// circuit breaker is in Open state. The breaker is injected directly into
// storage (env.as_contract) because Soroban rolls back all writes on Err,
// preventing failure-count accumulation through normal contract calls.

/// Error code pin: CircuitBreakerOpen discriminant must be exactly 46.
#[test]
fn test_circuit_breaker_open_error_code_is_46() {
    assert_eq!(
        NavinError::CircuitBreakerOpen as u32,
        46,
        "CircuitBreakerOpen discriminant must be 46"
    );
}

/// With the circuit breaker injected into Open state, release_escrow must
/// return CircuitBreakerOpen (#46) — the token transfer is blocked.
/// With the circuit breaker closed (default state), token transfers succeed
/// — no CircuitBreakerOpen error is raised.
// Issue #582 – DuplicatePaymentMilestone (code 60)

#[test]
#[should_panic(expected = "Error(Contract, #60)")]
fn test_create_shipment_duplicate_milestone_names_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[4u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "pickup"), 50_u32));
    milestones.push_back((Symbol::new(&env, "pickup"), 50_u32)); // duplicate

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
}

// Issue #583 – InvalidTokenAddress (code 61)

#[test]
#[should_panic(expected = "Error(Contract, #61)")]
fn test_initialize_with_admin_as_token_address_rejected() {
    let (_env, client, admin, _) = setup_shipment_env();
    // Passing the admin's own address as the token contract is invalid.
    client.initialize(&admin, &admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #61)")]
fn test_initialize_with_contract_address_as_token_rejected() {
    let (_env, client, admin, _) = setup_shipment_env();
    // Passing the shipment contract's own address as the token is invalid.
    let contract_addr = client.address.clone();
    client.initialize(&admin, &contract_addr);
}

#[test]
fn test_initialize_with_valid_token_address_succeeds() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    // A real distinct token contract should be accepted.
    client.initialize(&admin, &token_contract);
    assert_eq!(client.get_admin(), admin);
}

// Issue #584 – InvalidPaymentMilestoneName (code 62)

#[test]
#[should_panic(expected = "Error(Contract, #62)")]
fn test_create_shipment_empty_milestone_name_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[6u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    // An empty symbol name is an invalid milestone checkpoint name.
    milestones.push_back((Symbol::new(&env, ""), 100_u32));

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
#[should_panic(expected = "Error(Contract, #62)")]
fn test_create_shipment_too_long_milestone_name_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    // A 13-character name exceeds the Stellar Symbol 12-char maximum.
    milestones.push_back((Symbol::new(&env, "toolongname_x"), 100_u32));

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
}

// get_circuit_breaker_status tests (issue #640)
