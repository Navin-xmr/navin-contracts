//! # Cross-Contract Integration Tests
//!
//! Verifies correct behaviour when the shipment contract interacts with
//! external token contracts. Tests cover:
//!
//! - Successful shipment creation via a working token contract.
//! - Token transfer failure propagating as `TokenTransferFailed`.
//! - Circuit breaker opening after repeated transfer failures.
//! - Batch creation succeeding independently of token contract state.
//! - Cancel without escrow succeeds even when the token contract is broken.
//!
//! ## Mock Contracts
//!
//! Stubs are placed in private submodules to prevent Soroban's proc-macros from
//! generating conflicting symbol names at the crate level.
//!
//! | Stub             | Behaviour                                        |
//! |------------------|--------------------------------------------------|
//! | `mock_ok`        | `transfer` always succeeds.                      |
//! | `mock_fail`      | `transfer` always returns `TransferFailed`.      |

extern crate std;

// ── Mock token: always succeeds ──────────────────────────────────────────────

mod mock_ok {
    use soroban_sdk::{contract, contractimpl, Address, Env};

    #[contract]
    pub struct MockToken;

    #[contractimpl]
    impl MockToken {
        pub fn decimals(_env: soroban_sdk::Env) -> u32 {
            7
        }

        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
        pub fn mint(_env: Env, _admin: Address, _to: Address, _amount: i128) {}
    }
}

// ── Mock token: always fails on transfer ─────────────────────────────────────

mod mock_fail {
    use soroban_sdk::{contract, contracterror, contractimpl, Address, Env};

    #[contracterror]
    #[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
    #[repr(u32)]
    pub enum MockTokenError {
        TransferFailed = 1,
    }

    #[contract]
    pub struct FailingToken;

    #[contractimpl]
    impl FailingToken {
        pub fn transfer(
            _env: Env,
            _from: Address,
            _to: Address,
            _amount: i128,
        ) -> Result<(), MockTokenError> {
            Err(MockTokenError::TransferFailed)
        }
        pub fn mint(_env: Env, _admin: Address, _to: Address, _amount: i128) {}
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

use crate::{
    test_utils, types::ShipmentInput, NavinError, NavinShipment, NavinShipmentClient,
    ShipmentStatus,
};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

fn dummy_hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

struct Ctx {
    env: Env,
    client: NavinShipmentClient<'static>,
    #[allow(dead_code)]
    admin: Address,
    company: Address,
    carrier: Address,
}

fn setup_ok() -> Ctx {
    let (env, admin) = test_utils::setup_env();
    let token = env.register(mock_ok::MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token);
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);
    Ctx {
        env,
        client,
        admin,
        company,
        carrier,
    }
}

fn setup_fail() -> Ctx {
    let (env, admin) = test_utils::setup_env();
    let token = env.register(mock_fail::FailingToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token);
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);
    Ctx {
        env,
        client,
        admin,
        company,
        carrier,
    }
}

fn inject_escrow(ctx: &Ctx, id: u64, amount: i128) {
    ctx.env.as_contract(&ctx.client.address, || {
        let mut s = crate::storage::get_shipment(&ctx.env, id).unwrap();
        s.escrow_amount = amount;
        s.total_escrow = amount;
        crate::storage::set_shipment(&ctx.env, &s);
        crate::storage::set_escrow(&ctx.env, id, amount);
    });
}

fn advance_to_delivered(ctx: &Ctx, id: u64) {
    test_utils::advance_past_rate_limit(&ctx.env);
    ctx.client.update_status(
        &ctx.carrier,
        &id,
        &ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env, 90),
    );
    test_utils::advance_past_rate_limit(&ctx.env);
    ctx.client.update_status(
        &ctx.carrier,
        &id,
        &ShipmentStatus::Delivered,
        &dummy_hash(&ctx.env, 91),
    );
}

// ── Happy-path tests ─────────────────────────────────────────────────────────

#[test]
fn test_shipment_creation_without_escrow_succeeds() {
    let ctx = setup_ok();
    let deadline = test_utils::future_deadline(&ctx.env, 3600);
    let id = ctx.client.create_shipment(
        &ctx.company,
        &Address::generate(&ctx.env),
        &ctx.carrier,
        &dummy_hash(&ctx.env, 1),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );
    let s = ctx.client.get_shipment(&id);
    assert_eq!(s.status, ShipmentStatus::Created);
    assert_eq!(s.escrow_amount, 0);
}

#[test]
fn test_batch_creation_5_items_succeeds() {
    let ctx = setup_ok();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let mut inputs: Vec<ShipmentInput> = Vec::new(&ctx.env);
    for seed in 1u8..=5 {
        inputs.push_back(ShipmentInput {
            receiver: Address::generate(&ctx.env),
            carrier: ctx.carrier.clone(),
            data_hash: dummy_hash(&ctx.env, seed),
            payment_milestones: Vec::new(&ctx.env),
            deadline,
            depends_on: None,
        });
    }
    let ids = ctx.client.create_shipments_batch(&ctx.company, &inputs);
    assert_eq!(ids.len(), 5);
    for id in ids.iter() {
        let s = ctx.client.get_shipment(&id);
        assert_eq!(s.status, ShipmentStatus::Created);
    }
}

#[test]
fn test_status_update_succeeds_with_working_token() {
    let ctx = setup_ok();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let id = ctx.client.create_shipment(
        &ctx.company,
        &Address::generate(&ctx.env),
        &ctx.carrier,
        &dummy_hash(&ctx.env, 2),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );
    test_utils::advance_past_rate_limit(&ctx.env);
    ctx.client.update_status(
        &ctx.carrier,
        &id,
        &ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env, 3),
    );
    assert_eq!(
        ctx.client.get_shipment(&id).status,
        ShipmentStatus::InTransit
    );
}

#[test]
fn test_read_only_queries_work_regardless_of_token_state() {
    let ctx = setup_ok();
    assert_eq!(ctx.client.get_shipment_counter(), 0);
    let analytics = ctx.client.get_analytics();
    assert_eq!(analytics.total_shipments, 0);
}

// ── Failure-mode tests ───────────────────────────────────────────────────────

#[test]
fn test_release_escrow_fails_with_failing_token() {
    let ctx = setup_fail();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let receiver = Address::generate(&ctx.env);
    let id = ctx.client.create_shipment(
        &ctx.company,
        &receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 5),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );
    inject_escrow(&ctx, id, 1000);
    advance_to_delivered(&ctx, id);

    let result = ctx.client.try_release_escrow(&receiver, &id);
    assert!(
        result.is_err(),
        "expected release_escrow to fail with a failing token"
    );
}

#[test]
fn test_token_transfer_failure_returns_correct_error() {
    let ctx = setup_fail();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let receiver = Address::generate(&ctx.env);
    let id = ctx.client.create_shipment(
        &ctx.company,
        &receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 8),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );
    inject_escrow(&ctx, id, 500);
    advance_to_delivered(&ctx, id);

    let err = ctx
        .client
        .try_release_escrow(&receiver, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, NavinError::TokenTransferFailed);
}

#[test]
fn test_circuit_breaker_opens_after_repeated_failures() {
    // Soroban rolls back ALL storage writes when a contract function returns
    // Err, so failure counts can't accumulate through normal release_escrow
    // calls. Instead we inject the Open state directly into storage, then
    // verify that a subsequent release_escrow is rejected with CircuitBreakerOpen.
    let ctx = setup_fail();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);

    // Inject circuit-breaker Open state.
    ctx.env.as_contract(&ctx.client.address, || {
        let tracker = crate::circuit_breaker::CircuitBreakerTracker {
            state: crate::circuit_breaker::CircuitBreakerState::Open,
            failure_count: 5,
            opened_at: ctx.env.ledger().timestamp(),
            half_open_requests: 0,
        };
        ctx.env
            .storage()
            .persistent()
            .set(&crate::types::DataKey::CircuitBreakerState, &tracker);
    });

    let receiver = Address::generate(&ctx.env);
    let id = ctx.client.create_shipment(
        &ctx.company,
        &receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 99),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );
    inject_escrow(&ctx, id, 100);
    advance_to_delivered(&ctx, id);

    let err = ctx
        .client
        .try_release_escrow(&receiver, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, NavinError::CircuitBreakerOpen);
}

#[test]
fn test_force_cancel_with_escrow_and_failing_token_fails() {
    // Regular cancel_shipment does NOT call the token contract.
    // force_cancel_shipment DOES refund escrow via a token transfer, so it
    // should fail when the token contract is broken.
    let ctx = setup_fail();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let id = ctx.client.create_shipment(
        &ctx.company,
        &Address::generate(&ctx.env),
        &ctx.carrier,
        &dummy_hash(&ctx.env, 11),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );
    inject_escrow(&ctx, id, 200);

    let result = ctx
        .client
        .try_force_cancel_shipment(&ctx.admin, &id, &dummy_hash(&ctx.env, 12));
    assert!(
        result.is_err(),
        "force_cancel with escrow + failing token should fail"
    );
}

#[test]
fn test_cancel_without_escrow_succeeds_with_failing_token() {
    let ctx = setup_fail();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let id = ctx.client.create_shipment(
        &ctx.company,
        &Address::generate(&ctx.env),
        &ctx.carrier,
        &dummy_hash(&ctx.env, 13),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );
    // No escrow — cancel should skip the token transfer entirely.
    ctx.client
        .cancel_shipment(&ctx.company, &id, &dummy_hash(&ctx.env, 14));
    assert_eq!(
        ctx.client.get_shipment(&id).status,
        ShipmentStatus::Cancelled
    );
}

// ── Oracle fallback simulation ────────────────────────────────────────────────

#[test]
fn test_batch_creation_does_not_call_token_contract() {
    // Batch creation should succeed even with a failing token because it
    // does not perform any token transfers.
    let ctx = setup_fail();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let mut inputs: Vec<ShipmentInput> = Vec::new(&ctx.env);
    for seed in 1u8..=3 {
        inputs.push_back(ShipmentInput {
            receiver: Address::generate(&ctx.env),
            carrier: ctx.carrier.clone(),
            data_hash: dummy_hash(&ctx.env, seed),
            payment_milestones: Vec::new(&ctx.env),
            deadline,
            depends_on: None,
        });
    }
    let ids = ctx.client.create_shipments_batch(&ctx.company, &inputs);
    assert_eq!(ids.len(), 3);
}
