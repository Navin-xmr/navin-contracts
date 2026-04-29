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
    contract, contractimpl, testutils::Address as _, xdr::ToXdr, Address, BytesN, Env,
};

#[contract]
struct TtlMockToken;

#[contractimpl]
impl MockToken {
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
    let data_hash = BytesN::from_array(env, &[seed.saturating_add(1); 32]);
    let deadline = env.ledger().timestamp() + 86400; // 1 day from now
    let mut hash_bytes = [0u8; 32];
    hash_bytes[0] = idx;
    hash_bytes[1] = 0xAB;
    let data_hash = BytesN::from_array(env, &hash_bytes);
    let deadline = env.ledger().timestamp() + 86400;

    client.create_shipment(
        company,
        &receiver,
        carrier,
        &data_hash,
        &soroban_sdk::Vec::new(env),
        &deadline,
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
