//! # TTL Health Summary Tests
//!
//! Comprehensive test suite for the TTL health monitoring functionality.
//! Tests cover sampling strategies, edge cases, and deterministic behavior.
//!
//! **Note**: These tests verify persistent storage presence metrics rather than
//! direct TTL values, as TTL is not directly queryable in production Soroban contracts.

#![cfg(test)]

use crate::test_utils;
use crate::types::*;
use crate::NavinShipment;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

/// Helper to create a shipment with default values
fn create_test_shipment(
    env: &Env,
    contract_id: &Address,
    company: &Address,
    carrier: &Address,
) -> u64 {
    let receiver = Address::generate(env);
    let data_hash = BytesN::from_array(env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 86400; // 1 day from now

    let shipment_id = NavinShipment::create_shipment(
        env.clone(),
        company.clone(),
        receiver,
        carrier.clone(),
        data_hash,
        soroban_sdk::Vec::new(env),
        deadline,
    )
    .unwrap();

    shipment_id
}

#[test]
fn test_ttl_health_summary_no_shipments() {
    let (env, contract_id, admin, token_contract) = test_utils::setup();

    // Initialize contract
    NavinShipment::initialize(env.clone(), admin.clone(), token_contract).unwrap();

    // Query TTL health with no shipments
    let health = NavinShipment::get_ttl_health_summary(env.clone()).unwrap();

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
    let (env, contract_id, admin, token_contract) = test_utils::setup();

    // Initialize contract
    NavinShipment::initialize(env.clone(), admin.clone(), token_contract).unwrap();

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    NavinShipment::add_company(env.clone(), admin.clone(), company.clone()).unwrap();
    NavinShipment::add_carrier(env.clone(), admin.clone(), carrier.clone()).unwrap();

    // Create a single shipment
    create_test_shipment(&env, &contract_id, &company, &carrier);

    // Query TTL health
    let health = NavinShipment::get_ttl_health_summary(env.clone()).unwrap();

    assert_eq!(health.total_shipment_count, 1);
    assert_eq!(health.sampled_count, 1);
    assert_eq!(health.persistent_count, 1); // Should be in persistent storage
    assert_eq!(health.missing_or_archived_count, 0);
    assert_eq!(health.persistent_percentage, 100);
}

#[test]
fn test_ttl_health_summary_multiple_shipments() {
    let (env, contract_id, admin, token_contract) = test_utils::setup();

    // Initialize contract
    NavinShipment::initialize(env.clone(), admin.clone(), token_contract).unwrap();

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    NavinShipment::add_company(env.clone(), admin.clone(), company.clone()).unwrap();
    NavinShipment::add_carrier(env.clone(), admin.clone(), carrier.clone()).unwrap();

    // Create 5 shipments
    for _ in 0..5 {
        create_test_shipment(&env, &contract_id, &company, &carrier);
    }

    // Query TTL health
    let health = NavinShipment::get_ttl_health_summary(env.clone()).unwrap();

    assert_eq!(health.total_shipment_count, 5);
    assert_eq!(health.sampled_count, 5); // All should be sampled (< 20)
    assert_eq!(health.persistent_count, 5); // All should be persistent
    assert_eq!(health.missing_or_archived_count, 0);
    assert_eq!(health.persistent_percentage, 100);
}

#[test]
fn test_ttl_health_summary_deterministic() {
    let (env, contract_id, admin, token_contract) = test_utils::setup();

    // Initialize contract
    NavinShipment::initialize(env.clone(), admin.clone(), token_contract).unwrap();

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    NavinShipment::add_company(env.clone(), admin.clone(), company.clone()).unwrap();
    NavinShipment::add_carrier(env.clone(), admin.clone(), carrier.clone()).unwrap();

    // Create 10 shipments
    for _ in 0..10 {
        create_test_shipment(&env, &contract_id, &company, &carrier);
    }

    // Query TTL health multiple times
    let health1 = NavinShipment::get_ttl_health_summary(env.clone()).unwrap();
    let health2 = NavinShipment::get_ttl_health_summary(env.clone()).unwrap();

    // Results should be deterministic (same ledger, same state)
    assert_eq!(health1.total_shipment_count, health2.total_shipment_count);
    assert_eq!(health1.sampled_count, health2.sampled_count);
    assert_eq!(health1.persistent_count, health2.persistent_count);
    assert_eq!(health1.persistent_percentage, health2.persistent_percentage);
}

#[test]
fn test_ttl_health_summary_config_values() {
    let (env, contract_id, admin, token_contract) = test_utils::setup();

    // Initialize contract
    NavinShipment::initialize(env.clone(), admin.clone(), token_contract).unwrap();

    // Get config to verify values
    let config = NavinShipment::get_contract_config(env.clone()).unwrap();

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    NavinShipment::add_company(env.clone(), admin.clone(), company.clone()).unwrap();
    NavinShipment::add_carrier(env.clone(), admin.clone(), carrier.clone()).unwrap();

    // Create a shipment
    create_test_shipment(&env, &contract_id, &company, &carrier);

    // Query TTL health
    let health = NavinShipment::get_ttl_health_summary(env.clone()).unwrap();

    // Verify config values are included in summary
    assert_eq!(health.ttl_threshold, config.shipment_ttl_threshold);
    assert_eq!(health.ttl_extension, config.shipment_ttl_extension);
    assert!(health.current_ledger > 0);
    assert!(health.query_timestamp > 0);
}

#[test]
fn test_ttl_health_summary_not_initialized() {
    let env = Env::default();
    let contract_id = env.register(NavinShipment, ());

    env.as_contract(&contract_id, || {
        // Try to query TTL health without initialization
        let result = NavinShipment::get_ttl_health_summary(env.clone());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), NavinError::NotInitialized);
    });
}

#[test]
fn test_ttl_health_summary_edge_case_exactly_20_shipments() {
    let (env, contract_id, admin, token_contract) = test_utils::setup();

    // Initialize contract
    NavinShipment::initialize(env.clone(), admin.clone(), token_contract).unwrap();

    // Add company and carrier
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    NavinShipment::add_company(env.clone(), admin.clone(), company.clone()).unwrap();
    NavinShipment::add_carrier(env.clone(), admin.clone(), carrier.clone()).unwrap();

    // Create exactly 20 shipments (boundary case)
    for _ in 0..20 {
        create_test_shipment(&env, &contract_id, &company, &carrier);
    }

    // Query TTL health
    let health = NavinShipment::get_ttl_health_summary(env.clone()).unwrap();

    assert_eq!(health.total_shipment_count, 20);
    assert_eq!(health.sampled_count, 20); // All should be sampled
    assert_eq!(health.persistent_count, 20);
    assert_eq!(health.persistent_percentage, 100);
}
