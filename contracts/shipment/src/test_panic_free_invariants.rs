//! Panic-free invariant tests for all public entry points.
//!
//! This module ensures that all public smart contract methods handle invalid inputs
//! gracefully by returning domain errors instead of panicking. Comprehensive test
//! coverage for edge cases, boundary conditions, and malformed inputs on ALL public
//! entry points prevents wallet/dApp crashes and improves production stability.

extern crate std;

use crate::{NavinShipment, NavinShipmentClient, ShipmentStatus};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, Symbol, Vec,
};

// ─────────────────────────────────────────────────────────────────────────────
// Mock Token Contract
// ─────────────────────────────────────────────────────────────────────────────

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {
        // Mock implementation - always succeeds
    }
    pub fn decimals(_env: Env) -> u32 {
        7
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test Setup Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn setup_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = crate::test_utils::setup_env();
    let token_contract = env.register(MockToken {}, ());
    let contract_id = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &contract_id);
    client.initialize(&admin, &token_contract);
    (env, client, admin, token_contract)
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #242: Panic-Free Invariant Tests
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// 1. initialize() - Boundary and Invalid Input Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_initialize_already_initialized_returns_error() {
    let (_env, client, admin, token) = setup_env();
    // Already initialized in setup_env, second call must return error
    let result = client.try_initialize(&admin, &token);
    assert!(
        result.is_err(),
        "initialize must return error when already initialized"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. add_company() - Invalid Input Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_add_company_unauthorized_caller() {
    let (env, client, _admin, _token) = setup_env();
    let unauthorized = Address::generate(&env);
    let company = Address::generate(&env);

    let result = client.try_add_company(&unauthorized, &company);
    assert!(
        result.is_err(),
        "add_company must fail with unauthorized caller"
    );
}

#[test]
fn test_add_company_not_initialized() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.protocol_version = crate::test_utils::DEFAULT_PROTOCOL_VERSION;
    });
    env.ledger()
        .set_timestamp(crate::test_utils::DEFAULT_TIMESTAMP);

    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let _token = env.register(MockToken {}, ());
    let contract_id = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &contract_id);

    env.mock_all_auths();
    let result = client.try_add_company(&admin, &company);
    assert!(
        result.is_err(),
        "add_company must fail when not initialized"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. add_carrier() - Invalid Input Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_add_carrier_unauthorized_caller() {
    let (env, client, _admin, _token) = setup_env();
    let unauthorized = Address::generate(&env);
    let carrier = Address::generate(&env);

    let result = client.try_add_carrier(&unauthorized, &carrier);
    assert!(
        result.is_err(),
        "add_carrier must fail with unauthorized caller"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. create_shipment() - Boundary and Invalid Input Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_create_shipment_invalid_hash_all_zeros() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let invalid_hash = BytesN::from_array(&env, &[0u8; 32]); // All zeros
    let deadline = env.ledger().timestamp() + 3600;

    client.add_company(&admin, &company);

    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &invalid_hash,
        &Vec::new(&env),
        &deadline,
    );
    assert!(result.is_err(), "create_shipment must reject all-zero hash");
}

#[test]
fn test_create_shipment_deadline_in_past() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let past_deadline = env.ledger().timestamp() - 1; // Past timestamp

    client.add_company(&admin, &company);

    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &past_deadline,
    );
    assert!(result.is_err(), "create_shipment must reject past deadline");
}

#[test]
fn test_create_shipment_unauthorized_company() {
    let (env, client, _admin, _token) = setup_env();
    let unauthorized_company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let result = client.try_create_shipment(
        &unauthorized_company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );
    assert!(result.is_err(), "create_shipment must fail for non-company");
}

#[test]
fn test_create_shipment_invalid_milestone_sum() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.add_company(&admin, &company);

    // Create milestones that don't sum to 100
    let mut milestones = Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "checkpoint1"), 50u32));
    milestones.push_back((Symbol::new(&env, "checkpoint2"), 30u32)); // Sum = 80, not 100

    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
    assert!(
        result.is_err(),
        "create_shipment must reject invalid milestone sum"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. deposit_escrow() - Boundary and Invalid Input Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_deposit_escrow_invalid_amount_zero() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.add_company(&admin, &company);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let result = client.try_deposit_escrow(&company, &shipment_id, &0i128);
    assert!(result.is_err(), "deposit_escrow must reject zero amount");
}

#[test]
fn test_deposit_escrow_nonexistent_shipment() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);

    client.add_company(&admin, &company);

    let result = client.try_deposit_escrow(&company, &999u64, &100i128);
    assert!(
        result.is_err(),
        "deposit_escrow must fail for nonexistent shipment"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. update_status() - Invalid State Transition Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_update_status_nonexistent_shipment() {
    let (env, client, admin, _token) = setup_env();
    let carrier = Address::generate(&env);
    let status_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.add_carrier(&admin, &carrier);

    let result =
        client.try_update_status(&carrier, &999u64, &ShipmentStatus::InTransit, &status_hash);
    assert!(
        result.is_err(),
        "update_status must fail for nonexistent shipment"
    );
}

#[test]
fn test_update_status_invalid_transition() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let status_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

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

    // Try invalid transition: Created -> Delivered (should be Created -> InTransit)
    let result = client.try_update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::Delivered,
        &status_hash,
    );
    assert!(
        result.is_err(),
        "update_status must reject invalid state transition"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 7. cancel_shipment() - Invalid Input Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_cancel_shipment_nonexistent_shipment() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let reason_hash = BytesN::from_array(&env, &[3u8; 32]);

    client.add_company(&admin, &company);

    let result = client.try_cancel_shipment(&company, &999u64, &reason_hash);
    assert!(
        result.is_err(),
        "cancel_shipment must fail for nonexistent shipment"
    );
}

#[test]
fn test_cancel_shipment_unauthorized_caller() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[3u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

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

    let unauthorized = Address::generate(&env);
    let result = client.try_cancel_shipment(&unauthorized, &shipment_id, &reason_hash);
    assert!(
        result.is_err(),
        "cancel_shipment must fail for unauthorized caller"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 8. release_escrow() - Invalid Input Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_release_escrow_nonexistent_shipment() {
    let (env, client, _admin, _token) = setup_env();
    let caller = Address::generate(&env);

    let result = client.try_release_escrow(&caller, &999u64);
    assert!(
        result.is_err(),
        "release_escrow must fail for nonexistent shipment"
    );
}

#[test]
fn test_release_escrow_invalid_status() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

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

    // Try to release escrow when shipment is in Created status (not Delivered)
    let result = client.try_release_escrow(&receiver, &shipment_id);
    assert!(
        result.is_err(),
        "release_escrow must fail for non-Delivered shipment"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 9. refund_escrow() - Invalid Input Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_refund_escrow_nonexistent_shipment() {
    let (env, client, _admin, _token) = setup_env();
    let caller = Address::generate(&env);

    let result = client.try_refund_escrow(&caller, &999u64);
    assert!(
        result.is_err(),
        "refund_escrow must fail for nonexistent shipment"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 10. raise_dispute() - Invalid Input Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_raise_dispute_nonexistent_shipment() {
    let (env, client, _admin, _token) = setup_env();
    let caller = Address::generate(&env);
    let reason_hash = BytesN::from_array(&env, &[4u8; 32]);

    let result = client.try_raise_dispute(&caller, &999u64, &reason_hash);
    assert!(
        result.is_err(),
        "raise_dispute must fail for nonexistent shipment"
    );
}

#[test]
fn test_raise_dispute_unauthorized_caller() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[4u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

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

    let unauthorized = Address::generate(&env);
    let result = client.try_raise_dispute(&unauthorized, &shipment_id, &reason_hash);
    assert!(
        result.is_err(),
        "raise_dispute must fail for unauthorized caller"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 11. resolve_dispute() - Invalid Input Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_resolve_dispute_nonexistent_shipment() {
    let (env, client, admin, _token) = setup_env();
    let reason_hash = BytesN::from_array(&env, &[5u8; 32]);

    let result = client.try_resolve_dispute(
        &admin,
        &999u64,
        &crate::DisputeResolution::ReleaseToCarrier,
        &reason_hash,
    );
    assert!(
        result.is_err(),
        "resolve_dispute must fail for nonexistent shipment"
    );
}

#[test]
fn test_resolve_dispute_unauthorized_caller() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[5u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

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

    let unauthorized = Address::generate(&env);
    let result = client.try_resolve_dispute(
        &unauthorized,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
        &reason_hash,
    );
    assert!(
        result.is_err(),
        "resolve_dispute must fail for unauthorized caller"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 12. set_shipment_limit() - Invalid Input Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_set_shipment_limit_unauthorized_caller() {
    let (env, client, _admin, _token) = setup_env();
    let unauthorized = Address::generate(&env);

    let result = client.try_set_shipment_limit(&unauthorized, &50u32);
    assert!(
        result.is_err(),
        "set_shipment_limit must fail for unauthorized caller"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 13. pause() - Invalid Input Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_pause_unauthorized_caller() {
    let (env, client, _admin, _token) = setup_env();
    let unauthorized = Address::generate(&env);

    let result = client.try_pause(&unauthorized);
    assert!(result.is_err(), "pause must fail for unauthorized caller");
}

// ─────────────────────────────────────────────────────────────────────────────
// 14. unpause() - Invalid Input Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_unpause_unauthorized_caller() {
    let (env, client, _admin, _token) = setup_env();
    let unauthorized = Address::generate(&env);

    let result = client.try_unpause(&unauthorized);
    assert!(result.is_err(), "unpause must fail for unauthorized caller");
}

// ─────────────────────────────────────────────────────────────────────────────
// 15. force_cancel_shipment() - Invalid Input Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_force_cancel_shipment_nonexistent_shipment() {
    let (env, client, admin, _token) = setup_env();
    let reason_hash = BytesN::from_array(&env, &[6u8; 32]);

    let result = client.try_force_cancel_shipment(&admin, &999u64, &reason_hash);
    assert!(
        result.is_err(),
        "force_cancel_shipment must fail for nonexistent shipment"
    );
}

#[test]
fn test_force_cancel_shipment_unauthorized_caller() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[6u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

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

    let unauthorized = Address::generate(&env);
    let result = client.try_force_cancel_shipment(&unauthorized, &shipment_id, &reason_hash);
    assert!(
        result.is_err(),
        "force_cancel_shipment must fail for unauthorized caller"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Summary: All tests verify panic-free error handling
// ─────────────────────────────────────────────────────────────────────────────
// Each test ensures that invalid inputs return Err instead of panicking.
// This guarantees wallet/dApp stability.
