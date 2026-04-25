//! # TTL Health Summary Tests
//!
//! Comprehensive test suite for the TTL health monitoring functionality.
//! Tests cover sampling strategies, edge cases, and deterministic behavior.
//!
//! **Note**: These tests verify persistent storage presence metrics rather than
//! direct TTL values, as TTL is not directly queryable in production Soroban contracts.

#![cfg(test)]

use crate::{NavinShipment, NavinShipmentClient};
use soroban_sdk::{
    contract, contractimpl, testutils::Address as _, xdr::ToXdr, Address, BytesN, Env,
};

#[contract]
struct TtlMockToken;

#[contractimpl]
impl TtlMockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

fn setup_shipment_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = super::test_utils::setup_env();
    let token_contract = env.register(TtlMockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    (env, client, admin, token_contract)
}

/// Helper to create a shipment with default values
fn create_test_shipment(
    client: &NavinShipmentClient,
    env: &Env,
    company: &Address,
    carrier: &Address,
) -> u64 {
    let receiver = Address::generate(env);
    let data_hash: BytesN<32> = env.crypto().sha256(&receiver.clone().to_xdr(env)).into();
    let deadline = env.ledger().timestamp() + 86400; // 1 day from now

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
    let (_env, client, admin, token_contract) = setup_shipment_env();

    // Initialize contract
    client.initialize(&admin, &token_contract);

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
    let (env, client, admin, token_contract) = setup_shipment_env();

    // Initialize contract
    client.initialize(&admin, &token_contract);

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Create a single shipment
    create_test_shipment(&client, &env, &company, &carrier);

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
    let (env, client, admin, token_contract) = setup_shipment_env();

    // Initialize contract
    client.initialize(&admin, &token_contract);

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Create 5 shipments
    for _ in 0..5 {
        create_test_shipment(&client, &env, &company, &carrier);
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
    let (env, client, admin, token_contract) = setup_shipment_env();

    // Initialize contract
    client.initialize(&admin, &token_contract);

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Create 10 shipments
    for _ in 0..10 {
        create_test_shipment(&client, &env, &company, &carrier);
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
    let (env, client, admin, token_contract) = setup_shipment_env();

    // Initialize contract
    client.initialize(&admin, &token_contract);

    // Get config to verify values
    let config = client.get_contract_config();

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Create a shipment
    create_test_shipment(&client, &env, &company, &carrier);

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
    let (env, client, admin, token_contract) = setup_shipment_env();

    // Initialize contract
    client.initialize(&admin, &token_contract);

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Create exactly 20 shipments (boundary case)
    for _ in 0..20 {
        create_test_shipment(&client, &env, &company, &carrier);
    }

    // Query TTL health
    let health = client.get_ttl_health_summary();

    assert_eq!(health.total_shipment_count, 20);
    assert_eq!(health.sampled_count, 20); // All should be sampled
    assert_eq!(health.persistent_count, 20);
    assert_eq!(health.persistent_percentage, 100);
}
