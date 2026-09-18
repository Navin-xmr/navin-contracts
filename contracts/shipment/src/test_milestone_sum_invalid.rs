/// Tests for NavinError::MilestoneSumInvalid (error code 18) — issue #613
///
/// Verifies that milestone percentage sum validation is enforced on shipment
/// creation: a sum != 100 must return MilestoneSumInvalid, while a sum of
/// exactly 100 must succeed.
extern crate std;

use crate::{test::setup_shipment_env, NavinError};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Symbol};

// ── helpers ──────────────────────────────────────────────────────────────────

fn base_shipment_parts(env: &soroban_sdk::Env) -> (Address, Address, Address, BytesN<32>, u64) {
    let company = Address::generate(env);
    let receiver = Address::generate(env);
    let carrier = Address::generate(env);
    let data_hash = BytesN::from_array(env, &[0xABu8; 32]);
    let deadline = env.ledger().timestamp() + 7200;
    (company, receiver, carrier, data_hash, deadline)
}

// ── invalid sums ─────────────────────────────────────────────────────────────

#[test]
fn test_milestone_sum_over_100_returns_error() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let (company, receiver, carrier, data_hash, deadline) = base_shipment_parts(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // 60 + 60 = 120 — over 100
    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "stage1"), 60u32));
    milestones.push_back((Symbol::new(&env, "stage2"), 60u32));

    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::MilestoneSumInvalid)),
        "sum 120 must return MilestoneSumInvalid"
    );
}

#[test]
fn test_milestone_sum_under_100_returns_error() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let (company, receiver, carrier, data_hash, deadline) = base_shipment_parts(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // 30 + 30 = 60 — under 100
    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "alpha"), 30u32));
    milestones.push_back((Symbol::new(&env, "beta"), 30u32));

    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::MilestoneSumInvalid)),
        "sum 60 must return MilestoneSumInvalid"
    );
}

#[test]
fn test_milestone_single_entry_not_100_returns_error() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let (company, receiver, carrier, data_hash, deadline) = base_shipment_parts(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // Single milestone at 99 — not 100
    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "only"), 99u32));

    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::MilestoneSumInvalid)),
        "single milestone at 99% must return MilestoneSumInvalid"
    );
}

#[test]
fn test_milestone_zero_sum_returns_error() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let (company, receiver, carrier, data_hash, deadline) = base_shipment_parts(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // Two milestones both at 0 — sum = 0. A per-item percentage of 0 is
    // rejected by the individual-value guard (InvalidPaymentMilestones)
    // before the sum is ever computed, so this never reaches
    // MilestoneSumInvalid — unlike the over/under/single-entry cases above,
    // which all use in-range (1-100) individual percentages.
    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "zero1"), 0u32));
    milestones.push_back((Symbol::new(&env, "zero2"), 0u32));

    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidPaymentMilestones)),
        "a zero-percent milestone must be rejected as InvalidPaymentMilestones"
    );
}

#[test]
fn test_error_code_is_18() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let (company, receiver, carrier, data_hash, deadline) = base_shipment_parts(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "m1"), 50u32));
    milestones.push_back((Symbol::new(&env, "m2"), 60u32));

    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
    assert_eq!(
        result,
        Err(Ok(NavinError::MilestoneSumInvalid)),
        "MilestoneSumInvalid discriminant must be 18"
    );
    assert_eq!(NavinError::MilestoneSumInvalid as u32, 18);
}
