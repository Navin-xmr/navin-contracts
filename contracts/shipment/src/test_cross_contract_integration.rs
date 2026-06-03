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
        pub fn decimals(_env: Env) -> u32 {
            7
        }

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
    test_utils,
    types::{SettlementOperation, SettlementState, ShipmentInput},
    NavinError, NavinShipment, NavinShipmentClient, ShipmentStatus,
};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Symbol, Vec};

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

/// Test carrier handoff event emission
#[test]
fn test_carrier_handoff_event_emitted() {
    let ctx = setup_ok();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let receiver = Address::generate(&ctx.env);

    // Create initial shipment
    let id = ctx.client.create_shipment(
        &ctx.company,
        &receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 1),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );

    // Verify no handoff events initially
    let events = ctx.env.events().all();
    assert_eq!(events.len(), 0);

    // Create a new carrier for handoff
    let new_carrier = Address::generate(&ctx.env);
    ctx.client.add_carrier(&ctx.admin, &new_carrier);

    // Perform handoff
    ctx.client
        .handoff_shipment(&ctx.carrier, &id, &new_carrier, &dummy_hash(&ctx.env, 2));

    // Verify handoff event is emitted
    let events = ctx.env.events().all();
    assert_eq!(events.len(), 1);

    // Check that the event has the correct topic and data
    let event = &events[0];
    assert_eq!(event.topics.len(), 3);
    assert_eq!(
        event.topics.get(0).unwrap(),
        Symbol::new(&ctx.env, "handoff")
    );

    // Check from/to carrier in payload
    let from_carrier = event.data.get(0).unwrap().to_address().unwrap();
    let to_carrier = event.data.get(1).unwrap().to_address().unwrap();
    assert_eq!(from_carrier, ctx.carrier);
    assert_eq!(to_carrier, new_carrier);
}

/// Test rejected handoff when caller is not current carrier
#[test]
fn test_rejected_handoff_when_caller_not_current_carrier() {
    let ctx = setup_ok();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let receiver = Address::generate(&ctx.env);

    // Create initial shipment
    let id = ctx.client.create_shipment(
        &ctx.company,
        &receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 1),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );

    // Create a new carrier for handoff
    let new_carrier = Address::generate(&ctx.env);
    ctx.client.add_carrier(&ctx.admin, &new_carrier);

    // Try handoff with unauthorized caller (not current carrier)
    let unauthorized = Address::generate(&ctx.env);

    let result =
        ctx.client
            .try_handoff_shipment(&unauthorized, &id, &new_carrier, &dummy_hash(&ctx.env, 2));

    assert!(
        result.is_err(),
        "handoff should fail when caller is not current carrier"
    );

    // Verify it returns Unauthorized error instead of panicking
    match result {
        Ok(_) => panic!("expected error but got success"),
        Err(e) => {
            assert_eq!(e, Err(Ok(crate::NavinError::Unauthorized)));
        }
    }
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
fn test_happy_and_failing_token_flows_can_run_together() {
    let ok_ctx = setup_ok();
    let fail_ctx = setup_fail();
    let deadline = test_utils::future_deadline(&ok_ctx.env, 7200);
    let receiver_ok = Address::generate(&ok_ctx.env);
    let receiver_fail = Address::generate(&fail_ctx.env);

    let ok_id = ok_ctx.client.create_shipment(
        &ok_ctx.company,
        &receiver_ok,
        &ok_ctx.carrier,
        &dummy_hash(&ok_ctx.env, 21),
        &Vec::new(&ok_ctx.env),
        &deadline,
        &None,
    );
    ok_ctx
        .client
        .deposit_escrow(&ok_ctx.company, &ok_id, &1_000);

    test_utils::advance_past_rate_limit(&ok_ctx.env);
    ok_ctx.client.update_status(
        &ok_ctx.carrier,
        &ok_id,
        &ShipmentStatus::InTransit,
        &dummy_hash(&ok_ctx.env, 22),
    );
    ok_ctx
        .client
        .confirm_delivery(&receiver_ok, &ok_id, &dummy_hash(&ok_ctx.env, 23));

    assert_eq!(ok_ctx.client.get_settlement_count(), 2);
    let deposit = ok_ctx.client.get_settlement(&1);
    assert_eq!(deposit.operation, SettlementOperation::Deposit);
    assert_eq!(deposit.state, SettlementState::Completed);
    let release = ok_ctx.client.get_settlement(&2);
    assert_eq!(release.operation, SettlementOperation::Release);
    assert_eq!(release.state, SettlementState::Completed);

    let fail_deposit_id = fail_ctx.client.create_shipment(
        &fail_ctx.company,
        &receiver_fail,
        &fail_ctx.carrier,
        &dummy_hash(&fail_ctx.env, 25),
        &Vec::new(&fail_ctx.env),
        &deadline,
        &None,
    );
    let err = fail_ctx
        .client
        .try_deposit_escrow(&fail_ctx.company, &fail_deposit_id, &500)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, NavinError::TokenTransferFailed);
    assert_eq!(fail_ctx.client.get_escrow_balance(&fail_deposit_id), 0);

    let fail_release_id = fail_ctx.client.create_shipment(
        &fail_ctx.company,
        &receiver_fail,
        &fail_ctx.carrier,
        &dummy_hash(&fail_ctx.env, 26),
        &Vec::new(&fail_ctx.env),
        &deadline,
        &None,
    );
    inject_escrow(&fail_ctx, fail_release_id, 500);
    advance_to_delivered(&fail_ctx, fail_release_id);
    let err = fail_ctx
        .client
        .try_release_escrow(&receiver_fail, &fail_release_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, NavinError::TokenTransferFailed);
    assert_eq!(fail_ctx.client.get_escrow_balance(&fail_release_id), 500);

    let fail_refund_id = fail_ctx.client.create_shipment(
        &fail_ctx.company,
        &receiver_fail,
        &fail_ctx.carrier,
        &dummy_hash(&fail_ctx.env, 27),
        &Vec::new(&fail_ctx.env),
        &deadline,
        &None,
    );
    inject_escrow(&fail_ctx, fail_refund_id, 250);
    let err = fail_ctx
        .client
        .try_refund_escrow(&fail_ctx.company, &fail_refund_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, NavinError::TokenTransferFailed);
    assert_eq!(fail_ctx.client.get_escrow_balance(&fail_refund_id), 250);
    assert_eq!(fail_ctx.client.get_settlement_count(), 0);
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
    );
    // No escrow — cancel should skip the token transfer entirely.
    ctx.client
        .cancel_shipment(&ctx.company, &id, &dummy_hash(&ctx.env, 14));
    assert_eq!(
        ctx.client.get_shipment(&id).status,
        ShipmentStatus::Cancelled
    );
}

// ── Token transfer failure recovery (issue #447) ─────────────────────────────

/// After `release_escrow` fails due to a broken token, the shipment's
/// escrow balance must remain unchanged — no state is corrupted.
#[test]
fn test_escrow_balance_unchanged_after_failed_release() {
    let ctx = setup_fail();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let receiver = Address::generate(&ctx.env);
    let id = ctx.client.create_shipment(
        &ctx.company,
        &receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 0x60),
        &Vec::new(&ctx.env),
        &deadline,
    );
    inject_escrow(&ctx, id, 2000);
    advance_to_delivered(&ctx, id);

    let escrow_before = ctx.client.get_escrow_balance(&id);

    let _ = ctx.client.try_release_escrow(&receiver, &id);

    let escrow_after = ctx.client.get_escrow_balance(&id);
    assert_eq!(
        escrow_before, escrow_after,
        "escrow balance must be unchanged after a failed release"
    );
}

/// After `release_escrow` fails, the shipment status must remain as it was
/// before the call — the contract must not partially advance state.
#[test]
fn test_shipment_status_unchanged_after_failed_release() {
    let ctx = setup_fail();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let receiver = Address::generate(&ctx.env);
    let id = ctx.client.create_shipment(
        &ctx.company,
        &receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 0x61),
        &Vec::new(&ctx.env),
        &deadline,
    );
    inject_escrow(&ctx, id, 1500);
    advance_to_delivered(&ctx, id);

    let status_before = ctx.client.get_shipment(&id).status;

    let _ = ctx.client.try_release_escrow(&receiver, &id);

    let status_after = ctx.client.get_shipment(&id).status;
    assert_eq!(
        status_before, status_after,
        "shipment status must be unchanged after a failed token transfer"
    );
}

/// Multiple consecutive token transfer failures must not accumulate corrupt
/// state — each failure leaves storage in the same clean condition.
#[test]
fn test_multiple_release_failures_leave_state_consistent() {
    let ctx = setup_fail();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let receiver = Address::generate(&ctx.env);
    let id = ctx.client.create_shipment(
        &ctx.company,
        &receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 0x62),
        &Vec::new(&ctx.env),
        &deadline,
    );
    inject_escrow(&ctx, id, 500);
    advance_to_delivered(&ctx, id);

    let initial_escrow = ctx.client.get_escrow_balance(&id);
    let initial_status = ctx.client.get_shipment(&id).status;

    // Attempt release three times — each must fail and leave state unchanged.
    for attempt in 1..=3u32 {
        let result = ctx.client.try_release_escrow(&receiver, &id);
        assert!(
            result.is_err(),
            "attempt {attempt}: release_escrow must fail with a failing token"
        );
        assert_eq!(
            ctx.client.get_escrow_balance(&id),
            initial_escrow,
            "attempt {attempt}: escrow must be unchanged after failure"
        );
        assert_eq!(
            ctx.client.get_shipment(&id).status,
            initial_status,
            "attempt {attempt}: status must be unchanged after failure"
        );
    }
}

/// A token transfer failure must produce `TokenTransferFailed` (error #39),
/// confirming the error is mapped through the contract error layer correctly.
#[test]
fn test_token_failure_maps_to_transfer_failed_error_code() {
    let ctx = setup_fail();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let receiver = Address::generate(&ctx.env);
    let id = ctx.client.create_shipment(
        &ctx.company,
        &receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 0x63),
        &Vec::new(&ctx.env),
        &deadline,
    );
    inject_escrow(&ctx, id, 750);
    advance_to_delivered(&ctx, id);

    let err = ctx
        .client
        .try_release_escrow(&receiver, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        NavinError::TokenTransferFailed,
        "token transfer failure must surface as TokenTransferFailed"
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
        });
    }
    let ids = ctx.client.create_shipments_batch(&ctx.company, &inputs);
    assert_eq!(ids.len(), 3);
}
