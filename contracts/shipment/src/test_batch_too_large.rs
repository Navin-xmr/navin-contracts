/// Tests for NavinError::BatchTooLarge (error code 16) — issue #615
///
/// Covers:
///   - create_shipments_batch exceeding the configured batch limit → BatchTooLarge
///   - create_shipments_batch within the limit → success
///   - get_shipments_batch exceeding the hard 50-item query cap → BatchTooLarge
///   - get_shipments_batch within the cap → success
extern crate std;

use crate::{test::setup_shipment_env, NavinError, ShipmentInput};
use soroban_sdk::{testutils::Address as _, Address, BytesN};

// ── helpers ──────────────────────────────────────────────────────────────────

fn make_shipment_input(env: &soroban_sdk::Env, marker: u8) -> ShipmentInput {
    ShipmentInput {
        receiver: Address::generate(env),
        carrier: Address::generate(env),
        data_hash: BytesN::from_array(env, &[marker; 32]),
        payment_milestones: soroban_sdk::Vec::new(env),
        deadline: env.ledger().timestamp() + 7200,
    }
}

fn push_shipments(env: &soroban_sdk::Env, n: usize) -> soroban_sdk::Vec<ShipmentInput> {
    let mut v = soroban_sdk::Vec::new(env);
    for i in 0..n {
        v.push_back(make_shipment_input(env, (i % 255) as u8 + 1));
    }
    v
}

// ── create_shipments_batch — over limit ──────────────────────────────────────

#[test]
fn test_create_batch_11_returns_batch_too_large() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipments = push_shipments(&env, 11); // default limit is 10
    let result = client.try_create_shipments_batch(&company, &shipments);
    assert_eq!(
        result,
        Err(Ok(NavinError::BatchTooLarge)),
        "11 shipments must return BatchTooLarge"
    );
}

#[test]
fn test_create_batch_20_returns_batch_too_large() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipments = push_shipments(&env, 20);
    let result = client.try_create_shipments_batch(&company, &shipments);
    assert_eq!(
        result,
        Err(Ok(NavinError::BatchTooLarge)),
        "20 shipments must return BatchTooLarge"
    );
}

// ── create_shipments_batch — within limit ────────────────────────────────────

#[test]
fn test_create_batch_1_succeeds() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipments = push_shipments(&env, 1);
    let result = client.try_create_shipments_batch(&company, &shipments);
    assert!(result.is_ok(), "single-item batch must succeed");
    assert_eq!(result.unwrap().len(), 1);
}

#[test]
fn test_create_batch_at_limit_succeeds() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // Default limit is 10; batch of exactly 10 must succeed
    let shipments = push_shipments(&env, 10);
    let result = client.try_create_shipments_batch(&company, &shipments);
    assert!(result.is_ok(), "batch of exactly 10 must succeed");
    assert_eq!(result.unwrap().len(), 10);
}

#[test]
fn test_create_batch_5_succeeds_and_assigns_sequential_ids() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipments = push_shipments(&env, 5);
    let ids = client.create_shipments_batch(&company, &shipments);
    assert_eq!(ids.len(), 5, "batch of 5 must return 5 ids");
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(id, (i + 1) as u64, "ids must be sequential from 1");
    }
}

// ── get_shipments_batch — over the 50-item query cap ─────────────────────────

#[test]
fn test_query_batch_51_returns_batch_too_large() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let mut ids = soroban_sdk::Vec::new(&env);
    for i in 0..51_u64 {
        ids.push_back(i + 1);
    }

    let result = client.try_get_shipments_batch(&ids);
    assert_eq!(
        result,
        Err(Ok(NavinError::BatchTooLarge)),
        "query of 51 ids must return BatchTooLarge"
    );
}

#[test]
fn test_query_batch_50_is_accepted() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let mut ids = soroban_sdk::Vec::new(&env);
    for i in 0..50_u64 {
        ids.push_back(i + 1); // IDs that don't exist will return None in the result
    }

    // Should not error with BatchTooLarge — all 50 slots return None for missing ids
    let result = client.try_get_shipments_batch(&ids);
    assert!(
        result.is_ok(),
        "query of exactly 50 ids must not return BatchTooLarge"
    );
}

// ── error code assertion ──────────────────────────────────────────────────────

#[test]
fn test_error_code_is_16() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipments = push_shipments(&env, 11);
    let result = client.try_create_shipments_batch(&company, &shipments);
    assert_eq!(
        result,
        Err(Ok(NavinError::BatchTooLarge)),
        "BatchTooLarge discriminant must be 16"
    );
    assert_eq!(NavinError::BatchTooLarge as u32, 16);
}
