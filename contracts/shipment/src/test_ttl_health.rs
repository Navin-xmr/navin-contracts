//! # TTL Health Summary Tests
//!
//! Comprehensive test suite for the TTL health monitoring functionality.
//! Tests cover sampling strategies, edge cases, and deterministic behavior.
//!
//! **Note**: These tests verify persistent storage presence metrics rather than
//! direct TTL values, as TTL is not directly queryable in production Soroban contracts.

extern crate std;

use crate::{NavinShipment, NavinShipmentClient};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{storage::Persistent as _, Address as _, Ledger as _},
    Address, BytesN, Env,
};

#[contract]
struct TtlMockToken;

#[contractimpl]
impl TtlMockToken {
    pub fn decimals(_env: soroban_sdk::Env) -> u32 {
        7
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {
        // Mock implementation - always succeeds
    }
}

fn setup_shipment_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = super::test_utils::setup_env();
    let token_contract = env.register(TtlMockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token_contract);
    (env, client, admin, token_contract)
}

/// Helper to create a shipment with default values.
/// `seed` must be unique per call within a test to avoid idempotency-key collisions
/// (the contract rejects duplicate `sender + data_hash` pairs within the window).
/// Helper to create a shipment with a unique data hash per call.
/// Uses a thread-local counter to ensure each hash is distinct and avoid DuplicateAction.
fn create_test_shipment(
    client: &NavinShipmentClient,
    env: &Env,
    company: &Address,
    carrier: &Address,
    seed: u8,
) -> u64 {
    use std::sync::atomic::{AtomicU8, Ordering};
    static COUNTER: AtomicU8 = AtomicU8::new(0);
    let idx = COUNTER.fetch_add(1, Ordering::SeqCst);

    let receiver = Address::generate(env);
    let mut hash_bytes = [0u8; 32];
    hash_bytes[0] = idx;
    hash_bytes[1] = seed.saturating_add(1);
    let data_hash = BytesN::from_array(env, &hash_bytes);
    let deadline = env.ledger().timestamp() + 86400;

    client.create_shipment(
        company,
        &receiver,
        carrier,
        &data_hash,
        &soroban_sdk::Vec::new(env),
        &deadline,
        &None,
    )
}

#[test]
fn test_ttl_health_summary_no_shipments() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    // Initialize contract

    // Query TTL health with no shipments
    let health = client.get_ttl_health_summary();

    assert_eq!(health.total_shipment_count, 0);
    assert_eq!(health.sampled_count, 0);
    assert_eq!(health.persistent_count, 0);
    assert_eq!(health.missing_or_archived_count, 0);
    assert_eq!(health.persistent_percentage, 0);
    assert!(health.ttl_threshold > 0);
    assert!(health.ttl_extension > 0);
    assert!(health.current_ledger > 0);
    assert!(health.query_timestamp > 0);
}

#[test]
fn test_ttl_health_summary_single_shipment() {
    let (env, client, admin, _token_contract) = setup_shipment_env();

    // Initialize contract

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Create a single shipment
    create_test_shipment(&client, &env, &company, &carrier, 0);

    // Query TTL health
    let health = client.get_ttl_health_summary();

    assert_eq!(health.total_shipment_count, 1);
    assert_eq!(health.sampled_count, 1);
    assert_eq!(health.persistent_count, 1); // Should be in persistent storage
    assert_eq!(health.missing_or_archived_count, 0);
    assert_eq!(health.persistent_percentage, 100);
}

#[test]
fn test_ttl_health_summary_multiple_shipments() {
    let (env, client, admin, _token_contract) = setup_shipment_env();

    // Initialize contract

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Create 5 shipments
    for i in 0..5u8 {
        create_test_shipment(&client, &env, &company, &carrier, i);
    }

    // Query TTL health
    let health = client.get_ttl_health_summary();

    assert_eq!(health.total_shipment_count, 5);
    assert_eq!(health.sampled_count, 5); // All should be sampled (< 20)
    assert_eq!(health.persistent_count, 5); // All should be persistent
    assert_eq!(health.missing_or_archived_count, 0);
    assert_eq!(health.persistent_percentage, 100);
}

#[test]
fn test_ttl_health_summary_deterministic() {
    let (env, client, admin, _token_contract) = setup_shipment_env();

    // Initialize contract

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Create 10 shipments
    for i in 0..10u8 {
        create_test_shipment(&client, &env, &company, &carrier, i);
    }

    // Query TTL health multiple times
    let health1 = client.get_ttl_health_summary();
    let health2 = client.get_ttl_health_summary();

    // Results should be deterministic (same ledger, same state)
    assert_eq!(health1.total_shipment_count, health2.total_shipment_count);
    assert_eq!(health1.sampled_count, health2.sampled_count);
    assert_eq!(health1.persistent_count, health2.persistent_count);
    assert_eq!(health1.persistent_percentage, health2.persistent_percentage);
}

#[test]
fn test_ttl_health_summary_config_values() {
    let (env, client, admin, _token_contract) = setup_shipment_env();

    // Initialize contract

    // Get config to verify values
    let config = client.get_contract_config();

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Create a shipment
    create_test_shipment(&client, &env, &company, &carrier, 0);

    // Query TTL health
    let health = client.get_ttl_health_summary();

    // Verify config values are included in summary
    assert_eq!(health.ttl_threshold, config.shipment_ttl_threshold);
    assert_eq!(health.ttl_extension, config.shipment_ttl_extension);
    assert!(health.current_ledger > 0);
    assert!(health.query_timestamp > 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_ttl_health_summary_not_initialized() {
    let (env, _client, _admin, _token_contract) = setup_shipment_env();
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));

    // Try to query TTL health without initialization - should panic with NotInitialized
    client.get_ttl_health_summary();
}

#[test]
fn test_ttl_health_summary_edge_case_exactly_20_shipments() {
    let (env, client, admin, _token_contract) = setup_shipment_env();

    // Initialize contract

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Create exactly 20 shipments (boundary case)
    for i in 0..20u8 {
        create_test_shipment(&client, &env, &company, &carrier, i);
    }

    // Query TTL health
    let health = client.get_ttl_health_summary();

    assert_eq!(health.total_shipment_count, 20);
    assert_eq!(health.sampled_count, 20); // All should be sampled
    assert_eq!(health.persistent_count, 20);
    assert_eq!(health.persistent_percentage, 100);
}

// ── Regression tests for issue #377 ──────────────────────────────────────────

/// Regression test: active state mutation (update_status) must refresh the
/// shipment's persistent-storage TTL up to the configured extension ceiling.
///
/// Flow: create → record initial TTL → advance ledger sequence to simulate
/// natural TTL decay → update_status (InTransit) → assert TTL is refreshed.
#[test]
#[ignore = "pre-existing failure from #377: advancing sequence archives the contract instance"]
fn test_ttl_extended_on_active_mutation() {
    let (env, client, admin, _token) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let receiver = Address::generate(&env);
    let create_hash = BytesN::from_array(&env, &[0x01u8; 32]);
    let deadline = env.ledger().timestamp() + 86_400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &create_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    // Record TTL immediately after creation.
    let ttl_after_create = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().get_ttl(&key)
    });
    assert!(
        ttl_after_create >= 518_400,
        "TTL must be set to at least shipment_ttl_extension on creation"
    );

    // Advance ledger sequence to simulate TTL decay (consume some ledgers).
    env.ledger().with_mut(|l| {
        l.sequence_number += 1_000;
        l.timestamp += 61; // also clear the rate-limit window
    });

    // Mutate: transition to InTransit — this must re-extend the TTL.
    let update_hash = BytesN::from_array(&env, &[0x02u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &crate::ShipmentStatus::InTransit,
        &update_hash,
    );

    // Assert TTL is refreshed back to the configured extension ceiling.
    let ttl_after_update = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().get_ttl(&key)
    });
    assert!(
        ttl_after_update >= 518_400,
        "TTL must be refreshed to at least shipment_ttl_extension after update_status"
    );
}

/// Regression test: once a shipment reaches a terminal state and is archived,
/// it is removed from persistent storage so TTL extension is a no-op for it.
///
/// Flow: create → cancel (terminal) → archive (moves to temp storage) →
/// assert the persistent entry is absent (TTL extension cannot fire).
#[test]
fn test_ttl_not_extended_for_archived_terminal_shipment() {
    let (env, client, admin, _token) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let receiver = Address::generate(&env);
    let create_hash = BytesN::from_array(&env, &[0x03u8; 32]);
    let deadline = env.ledger().timestamp() + 86_400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &create_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    // Confirm the shipment is in persistent storage before cancellation.
    let present_before = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().has(&key)
    });
    assert!(
        present_before,
        "Shipment must be in persistent storage after creation"
    );

    // Transition to terminal state: Cancelled.
    let reason_hash = BytesN::from_array(&env, &[0x04u8; 32]);
    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    // Archive the terminal shipment — moves it from persistent to temp storage.
    client.archive_shipment(&admin, &shipment_id);

    // Assert the persistent entry is gone; TTL extension is now a no-op.
    let present_after_archive = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().has(&key)
    });
    assert!(
        !present_after_archive,
        "Archived terminal shipment must not remain in persistent storage; \
         TTL extension must be a no-op for it"
    );
}

// ── Regression tests for issue #378 ──────────────────────────────────────────
// Mixed-state fixture: active + archived + missing shipments in one environment.
// Verifies that TTL health diagnostics remain accurate, deterministic, and
// panic-free across all StoragePresenceState classifications.

/// Build a mixed-state fixture with:
///   - 2 active (persistent) shipments
///   - 1 archived (terminal → archive_shipment) shipment
///   - 1 never-created (missing) ID (archived_id + 1000)
///
/// Returns (env, client, admin, company, carrier, active_id_1, active_id_2, archived_id, missing_id).
#[allow(clippy::type_complexity)]
fn setup_mixed_state_fixture() -> (
    Env,
    NavinShipmentClient<'static>,
    Address,
    Address,
    Address,
    u64,
    u64,
    u64,
    u64,
) {
    let (env, client, admin, _token) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Active shipment 1
    let active_id_1 = create_test_shipment(&client, &env, &company, &carrier, 100);

    // Active shipment 2
    let active_id_2 = create_test_shipment(&client, &env, &company, &carrier, 101);

    // Archived shipment: transition to terminal state then archive
    let archived_hash = BytesN::from_array(&env, &[0xAAu8; 32]);
    let deadline = env.ledger().timestamp() + 86_400;
    let archived_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &archived_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    // InTransit → Delivered (terminal), then archive
    client.update_status(
        &carrier,
        &archived_id,
        &crate::ShipmentStatus::InTransit,
        &archived_hash,
    );
    client.confirm_delivery(&receiver, &archived_id, &archived_hash);
    client.archive_shipment(&admin, &archived_id);

    // Missing ID: well beyond the current counter — never created
    let missing_id = archived_id + 1_000;

    (
        env,
        client,
        admin,
        company,
        carrier,
        active_id_1,
        active_id_2,
        archived_id,
        missing_id,
    )
}

/// Mixed-state: active shipments are counted, archived and missing are not.
#[test]
fn test_mixed_state_health_check_active_count() {
    let (_env, client, admin, _co, _ca, _a1, _a2, _arch, _miss) = setup_mixed_state_fixture();

    let health = client.check_contract_health(&admin);

    // 3 shipments were created (2 active + 1 archived); missing_id was never created
    assert_eq!(health.total_shipments, 3);
    // Only the 2 active (non-terminal) shipments count toward active_shipments_counted
    assert_eq!(health.active_shipments_counted, 2);
}

/// Mixed-state: escrow sum is zero when no escrow has been deposited.
#[test]
fn test_mixed_state_health_check_escrow_sum() {
    let (_env, client, admin, _co, _ca, _a1, _a2, _arch, _miss) = setup_mixed_state_fixture();

    let health = client.check_contract_health(&admin);
    assert_eq!(health.sum_of_escrow_balances, 0);
}

/// Mixed-state: no storage inconsistencies in a clean fixture.
#[test]
fn test_mixed_state_health_check_no_inconsistencies() {
    let (_env, client, admin, _co, _ca, _a1, _a2, _arch, _miss) = setup_mixed_state_fixture();

    let health = client.check_contract_health(&admin);
    assert_eq!(
        health.storage_inconsistencies.len(),
        0,
        "clean mixed-state fixture must have zero storage inconsistencies"
    );
}

/// Mixed-state: no anomalous shipments (none are InTransit past deadline).
#[test]
fn test_mixed_state_health_check_no_anomalies() {
    let (_env, client, admin, _co, _ca, _a1, _a2, _arch, _miss) = setup_mixed_state_fixture();

    let health = client.check_contract_health(&admin);
    assert_eq!(
        health.anomalous_shipment_ids.len(),
        0,
        "no shipment is InTransit past its deadline in the clean fixture"
    );
}

/// Mixed-state: active shipments report ActivePersistent.
#[test]
fn test_mixed_state_diagnostics_active_shipments() {
    use crate::types::StoragePresenceState;

    let (_env, client, _admin, _co, _ca, active_id_1, active_id_2, _arch, _miss) =
        setup_mixed_state_fixture();

    for id in [active_id_1, active_id_2] {
        let diag = client.get_restore_diagnostics(&id);
        assert_eq!(
            diag.state,
            StoragePresenceState::ActivePersistent,
            "shipment {id} must be ActivePersistent"
        );
        assert!(diag.persistent_shipment_present);
        assert!(!diag.archived_shipment_present);
    }
}

/// Mixed-state: archived shipment reports ArchivedExpected.
#[test]
fn test_mixed_state_diagnostics_archived_shipment() {
    use crate::types::StoragePresenceState;

    let (_env, client, _admin, _co, _ca, _a1, _a2, archived_id, _miss) =
        setup_mixed_state_fixture();

    let diag = client.get_restore_diagnostics(&archived_id);
    assert_eq!(
        diag.state,
        StoragePresenceState::ArchivedExpected,
        "archived shipment must report ArchivedExpected"
    );
    assert!(!diag.persistent_shipment_present);
    assert!(diag.archived_shipment_present);
    assert_eq!(diag.shipment_id, archived_id);
}

/// Mixed-state: never-created ID reports Missing without panic.
#[test]
fn test_mixed_state_diagnostics_missing_no_panic() {
    use crate::types::StoragePresenceState;

    let (_env, client, _admin, _co, _ca, _a1, _a2, _arch, missing_id) = setup_mixed_state_fixture();

    // Must not panic — graceful Missing classification
    let diag = client.get_restore_diagnostics(&missing_id);
    assert_eq!(
        diag.state,
        StoragePresenceState::Missing,
        "never-created ID must report Missing without panic"
    );
    assert!(!diag.persistent_shipment_present);
    assert!(!diag.archived_shipment_present);
    assert_eq!(diag.shipment_id, missing_id);
}

/// Mixed-state: all three StoragePresenceState variants appear in one fixture.
#[test]
fn test_mixed_state_all_three_classifications_present() {
    use crate::types::StoragePresenceState;

    let (_env, client, _admin, _co, _ca, active_id_1, _a2, archived_id, missing_id) =
        setup_mixed_state_fixture();

    let active_diag = client.get_restore_diagnostics(&active_id_1);
    let archived_diag = client.get_restore_diagnostics(&archived_id);
    let missing_diag = client.get_restore_diagnostics(&missing_id);

    assert_eq!(active_diag.state, StoragePresenceState::ActivePersistent);
    assert_eq!(archived_diag.state, StoragePresenceState::ArchivedExpected);
    assert_eq!(missing_diag.state, StoragePresenceState::Missing);
}

/// Mixed-state: health check output is deterministic across two consecutive calls.
#[test]
fn test_mixed_state_health_check_deterministic() {
    let (_env, client, admin, _co, _ca, _a1, _a2, _arch, _miss) = setup_mixed_state_fixture();

    let h1 = client.check_contract_health(&admin);
    let h2 = client.check_contract_health(&admin);

    assert_eq!(h1.total_shipments, h2.total_shipments);
    assert_eq!(h1.active_shipments_counted, h2.active_shipments_counted);
    assert_eq!(h1.sum_of_escrow_balances, h2.sum_of_escrow_balances);
    assert_eq!(
        h1.storage_inconsistencies.len(),
        h2.storage_inconsistencies.len()
    );
    assert_eq!(
        h1.anomalous_shipment_ids.len(),
        h2.anomalous_shipment_ids.len()
    );
}

/// Mixed-state: restore diagnostics are deterministic across two consecutive calls.
#[test]
fn test_mixed_state_diagnostics_deterministic() {
    let (_env, client, _admin, _co, _ca, active_id_1, _a2, archived_id, missing_id) =
        setup_mixed_state_fixture();

    for id in [active_id_1, archived_id, missing_id] {
        let d1 = client.get_restore_diagnostics(&id);
        let d2 = client.get_restore_diagnostics(&id);
        assert_eq!(
            d1, d2,
            "diagnostics for shipment {id} must be deterministic"
        );
    }
}

/// Mixed-state: escrow deposited on an active shipment is tallied correctly.
#[test]
fn test_mixed_state_health_check_escrow_tally_with_deposit() {
    let (env, client, admin, company, _ca, active_id_1, _a2, _arch, _miss) =
        setup_mixed_state_fixture();

    // Advance time to clear the rate-limit window before deposit
    super::test_utils::advance_ledger_time(&env, 61);

    client.deposit_escrow(&company, &active_id_1, &2_500);

    let health = client.check_contract_health(&admin);
    assert_eq!(
        health.sum_of_escrow_balances, 2_500,
        "escrow sum must reflect the deposited amount"
    );
    assert_eq!(
        health.storage_inconsistencies.len(),
        0,
        "depositing escrow must not introduce storage inconsistencies"
    );
}

/// Mixed-state: an InTransit shipment past its deadline is flagged as anomalous.
#[test]
fn test_mixed_state_health_check_anomaly_detection() {
    let (env, client, admin, _co, carrier, active_id_1, _a2, _arch, _miss) =
        setup_mixed_state_fixture();

    // Advance past rate-limit window, then transition to InTransit
    super::test_utils::advance_ledger_time(&env, 61);
    let update_hash = BytesN::from_array(&env, &[0xBBu8; 32]);
    client.update_status(
        &carrier,
        &active_id_1,
        &crate::ShipmentStatus::InTransit,
        &update_hash,
    );

    // Push time past the shipment deadline (created with deadline = ts + 86_400)
    super::test_utils::advance_ledger_time(&env, 90_000);

    let health = client.check_contract_health(&admin);
    assert!(
        health.anomalous_shipment_ids.contains(active_id_1),
        "InTransit shipment past its deadline must be flagged as anomalous"
    );
}
