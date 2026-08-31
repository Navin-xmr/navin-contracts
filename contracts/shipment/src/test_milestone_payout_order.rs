//! # Milestone Payout Order Tests — Issue #450
//!
//! Verifies that milestone payouts are enforced in sequential order:
//! - Milestones must be released in the order they are declared.
//! - Attempting to release a later milestone before an earlier one is rejected.
//! - Remaining milestone state updates correctly after each payout.

#![cfg(test)]

use crate::{NavinError, NavinShipment, NavinShipmentClient, ShipmentStatus};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{contract, contractimpl, symbol_short, Address, BytesN, Env, Symbol, Vec};

// ── Mock token ────────────────────────────────────────────────────────────────

#[contract]
struct MilestoneOrderToken;

#[contractimpl]
impl MilestoneOrderToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    pub fn decimals(_env: Env) -> u32 {
        7
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, NavinShipmentClient<'static>, Address) {
    let (env, admin) = crate::test_utils::setup_env();
    let token = env.register(MilestoneOrderToken, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token);
    (env, client, admin)
}

fn data_hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

/// Build a shipment with three sequential milestones (alpha 40 %, beta 35 %, gamma 25 %)
/// and an escrow deposit. Returns (shipment_id, company, carrier).
fn create_milestone_shipment(
    env: &Env,
    client: &NavinShipmentClient<'static>,
    admin: &Address,
) -> (u64, Address, Address) {
    let company = Address::generate(env);
    let carrier = Address::generate(env);
    let receiver = Address::generate(env);

    client.add_company(admin, &company);
    client.add_carrier(admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let mut milestones = Vec::new(env);
    milestones.push_back((symbol_short!("alpha"), 40u32));
    milestones.push_back((symbol_short!("beta"), 35u32));
    milestones.push_back((symbol_short!("gamma"), 25u32));

    let deadline = env.ledger().timestamp() + 86_400;
    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash(env, 0xAB),
        &milestones,
        &deadline,
    );

    // Fund the escrow (10_000 units).
    client.deposit_escrow(&company, &id, &10_000);

    // Move to InTransit so the carrier can record milestones / release payments.
    env.as_contract(&client.address, || {
        let mut s = crate::storage::get_shipment(env, id).unwrap();
        s.status = ShipmentStatus::InTransit;
        crate::storage::set_shipment(env, &s);
    });

    (id, company, carrier)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Releasing the first milestone (index 0) always succeeds.
#[test]
fn test_release_first_milestone_succeeds() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    let result = client.try_release_milestone_payment(&carrier, &id, &symbol_short!("alpha"));
    assert!(
        result.is_ok(),
        "first milestone must be releasable: {result:?}"
    );

    let shipment = client.get_shipment(&id);
    assert_eq!(shipment.milestones_completed.len(), 1);
    assert_eq!(shipment.paid_milestones.len(), 1);
}

/// Releasing in sequential order (alpha → beta → gamma) must succeed for all steps.
#[test]
fn test_release_milestones_in_order_succeeds() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    client.release_milestone_payment(&carrier, &id, &symbol_short!("alpha"));
    client.release_milestone_payment(&carrier, &id, &symbol_short!("beta"));
    client.release_milestone_payment(&carrier, &id, &symbol_short!("gamma"));

    let shipment = client.get_shipment(&id);
    assert_eq!(shipment.milestones_completed.len(), 3);
    // After all milestones, escrow should be fully drained.
    assert_eq!(shipment.escrow_amount, 0);
}

/// Attempting to release the second milestone (beta) before the first (alpha) is rejected.
#[test]
fn test_release_out_of_order_second_before_first_fails() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    let result = client.try_release_milestone_payment(&carrier, &id, &symbol_short!("beta"));
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidStatus)),
        "releasing beta before alpha must be rejected"
    );
}

/// Attempting to release the third milestone (gamma) before the first two is rejected.
#[test]
fn test_release_out_of_order_third_first_fails() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    let result = client.try_release_milestone_payment(&carrier, &id, &symbol_short!("gamma"));
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidStatus)),
        "releasing gamma before alpha and beta must be rejected"
    );
}

/// After alpha is paid, beta can be released but gamma is still blocked.
#[test]
fn test_release_third_blocked_after_one_paid() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    // Pay alpha first.
    client.release_milestone_payment(&carrier, &id, &symbol_short!("alpha"));

    // gamma still blocked (index 2, only 1 completed).
    let result = client.try_release_milestone_payment(&carrier, &id, &symbol_short!("gamma"));
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidStatus)),
        "gamma must still be blocked when only alpha is paid"
    );

    // beta is allowed now.
    assert!(
        client
            .try_release_milestone_payment(&carrier, &id, &symbol_short!("beta"))
            .is_ok(),
        "beta must succeed after alpha is paid"
    );
}

/// Attempting to release the same milestone twice returns MilestoneAlreadyPaid.
#[test]
fn test_release_same_milestone_twice_fails() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    client.release_milestone_payment(&carrier, &id, &symbol_short!("alpha"));
    let result = client.try_release_milestone_payment(&carrier, &id, &symbol_short!("alpha"));
    assert_eq!(
        result,
        Err(Ok(NavinError::MilestoneAlreadyPaid)),
        "releasing alpha a second time must return MilestoneAlreadyPaid"
    );
}

/// milestones_completed grows by one with each successful sequential release.
#[test]
fn test_completed_count_grows_sequentially() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    assert_eq!(client.get_shipment(&id).milestones_completed.len(), 0);

    client.release_milestone_payment(&carrier, &id, &symbol_short!("alpha"));
    assert_eq!(client.get_shipment(&id).milestones_completed.len(), 1);

    client.release_milestone_payment(&carrier, &id, &symbol_short!("beta"));
    assert_eq!(client.get_shipment(&id).milestones_completed.len(), 2);

    client.release_milestone_payment(&carrier, &id, &symbol_short!("gamma"));
    assert_eq!(client.get_shipment(&id).milestones_completed.len(), 3);
}

/// Each milestone releases its declared percentage of total_escrow.
#[test]
fn test_milestone_payout_amounts_are_proportional() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    let total = 10_000i128;

    client.release_milestone_payment(&carrier, &id, &symbol_short!("alpha"));
    // 40 % of 10_000 = 4_000 released; remaining = 6_000
    assert_eq!(client.get_shipment(&id).escrow_amount, total - 4_000);

    client.release_milestone_payment(&carrier, &id, &symbol_short!("beta"));
    // 35 % of 10_000 = 3_500 released; remaining = 2_500
    assert_eq!(
        client.get_shipment(&id).escrow_amount,
        total - 4_000 - 3_500
    );

    client.release_milestone_payment(&carrier, &id, &symbol_short!("gamma"));
    // 25 % of 10_000 = 2_500 released; remaining = 0
    assert_eq!(client.get_shipment(&id).escrow_amount, 0);
}

/// A single-milestone shipment (100 %) releases fully in one call.
#[test]
fn test_single_milestone_full_release() {
    let (env, client, admin) = setup();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let mut milestones = Vec::new(&env);
    milestones.push_back((symbol_short!("done"), 100u32));

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash(&env, 0xCC),
        &milestones,
        &(env.ledger().timestamp() + 86_400),
    );

    client.deposit_escrow(&company, &id, &5_000);

    env.as_contract(&client.address, || {
        let mut s = crate::storage::get_shipment(&env, id).unwrap();
        s.status = ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &s);
    });

    client.release_milestone_payment(&carrier, &id, &symbol_short!("done"));
    assert_eq!(client.get_shipment(&id).escrow_amount, 0);
    assert_eq!(client.get_shipment(&id).milestones_completed.len(), 1);
}

/// A two-milestone shipment (60/40): releasing beta before alpha must be rejected.
#[test]
fn test_two_milestone_order_enforced() {
    let (env, client, admin) = setup();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let mut milestones = Vec::new(&env);
    milestones.push_back((symbol_short!("first"), 60u32));
    milestones.push_back((symbol_short!("second"), 40u32));

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash(&env, 0xDD),
        &milestones,
        &(env.ledger().timestamp() + 86_400),
    );

    client.deposit_escrow(&company, &id, &1_000);

    env.as_contract(&client.address, || {
        let mut s = crate::storage::get_shipment(&env, id).unwrap();
        s.status = ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &s);
    });

    // second before first must fail
    let result = client.try_release_milestone_payment(&carrier, &id, &symbol_short!("second"));
    assert_eq!(result, Err(Ok(NavinError::InvalidStatus)));

    // first succeeds
    client.release_milestone_payment(&carrier, &id, &symbol_short!("first"));

    // second now allowed
    assert!(client
        .try_release_milestone_payment(&carrier, &id, &symbol_short!("second"))
        .is_ok());
}

/// Non-existent milestone name returns InvalidShipmentInput.
#[test]
fn test_unknown_milestone_rejected() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    let bogus: Symbol = Symbol::new(&env, "bogus");
    let result = client.try_release_milestone_payment(&carrier, &id, &bogus);
    assert_eq!(result, Err(Ok(NavinError::InvalidShipmentInput)));
}

// ── Issue #757: ordering must be enforced on the record-milestone siblings ─────

/// `record_milestone` must reject a later payment milestone whose earlier
/// siblings have not been completed — the same guard `release_milestone_payment`
/// applies.
#[test]
fn test_record_milestone_out_of_order_rejected() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    let result =
        client.try_record_milestone(&carrier, &id, &symbol_short!("beta"), &data_hash(&env, 0x11));
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidStatus)),
        "recording beta before alpha must be rejected on the record_milestone path"
    );

    // Nothing was paid out.
    assert_eq!(client.get_shipment(&id).milestones_completed.len(), 0);
    assert_eq!(client.get_shipment(&id).escrow_amount, 10_000);
}

/// `record_milestone` still pays out when milestones arrive in declared order.
#[test]
fn test_record_milestone_in_order_succeeds() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    client.record_milestone(&carrier, &id, &symbol_short!("alpha"), &data_hash(&env, 0x11));
    client.record_milestone(&carrier, &id, &symbol_short!("beta"), &data_hash(&env, 0x22));
    client.record_milestone(&carrier, &id, &symbol_short!("gamma"), &data_hash(&env, 0x33));

    let shipment = client.get_shipment(&id);
    assert_eq!(shipment.milestones_completed.len(), 3);
    assert_eq!(shipment.escrow_amount, 0);
}

/// After alpha is recorded, gamma is still blocked but beta is allowed —
/// mirroring `test_release_third_blocked_after_one_paid`.
#[test]
fn test_record_milestone_third_blocked_after_one_recorded() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    client.record_milestone(&carrier, &id, &symbol_short!("alpha"), &data_hash(&env, 0x11));

    let blocked =
        client.try_record_milestone(&carrier, &id, &symbol_short!("gamma"), &data_hash(&env, 0x33));
    assert_eq!(blocked, Err(Ok(NavinError::InvalidStatus)));

    assert!(client
        .try_record_milestone(&carrier, &id, &symbol_short!("beta"), &data_hash(&env, 0x22))
        .is_ok());
}

/// `record_milestones_batch` must reject an out-of-order batch atomically —
/// no leg is committed when a later milestone precedes its siblings.
#[test]
fn test_record_milestones_batch_out_of_order_rejected() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    let mut milestones = Vec::new(&env);
    milestones.push_back((symbol_short!("alpha"), data_hash(&env, 0x11)));
    milestones.push_back((symbol_short!("gamma"), data_hash(&env, 0x33)));

    let result = client.try_record_milestones_batch(&carrier, &id, &milestones);
    assert_eq!(
        result,
        Err(Ok(NavinError::InvalidStatus)),
        "a batch that skips beta must be rejected"
    );

    // Atomic: alpha's leg must not have been applied either.
    let shipment = client.get_shipment(&id);
    assert_eq!(shipment.milestones_completed.len(), 0);
    assert_eq!(shipment.escrow_amount, 10_000);
}

/// A single-element batch for a later milestone is rejected, same as the
/// single-milestone entrypoint.
#[test]
fn test_record_milestones_batch_single_out_of_order_rejected() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    let mut milestones = Vec::new(&env);
    milestones.push_back((symbol_short!("beta"), data_hash(&env, 0x22)));

    let result = client.try_record_milestones_batch(&carrier, &id, &milestones);
    assert_eq!(result, Err(Ok(NavinError::InvalidStatus)));
}

/// An in-order batch pays every leg and fully drains the escrow.
#[test]
fn test_record_milestones_batch_in_order_succeeds() {
    let (env, client, admin) = setup();
    let (id, _company, carrier) = create_milestone_shipment(&env, &client, &admin);

    let mut milestones = Vec::new(&env);
    milestones.push_back((symbol_short!("alpha"), data_hash(&env, 0x11)));
    milestones.push_back((symbol_short!("beta"), data_hash(&env, 0x22)));
    milestones.push_back((symbol_short!("gamma"), data_hash(&env, 0x33)));

    let released = client.record_milestones_batch(&carrier, &id, &milestones);
    assert_eq!(released.len(), 3);

    let shipment = client.get_shipment(&id);
    assert_eq!(shipment.milestones_completed.len(), 3);
    assert_eq!(shipment.escrow_amount, 0);
}

/// The three entrypoints agree: beta-before-alpha is rejected identically
/// whether it is attempted through `release_milestone_payment`,
/// `record_milestone`, or `record_milestones_batch`.
#[test]
fn test_ordering_consistent_across_all_three_entrypoints() {
    let (env, client, admin) = setup();

    let via_release = {
        let (id, _c, carrier) = create_milestone_shipment(&env, &client, &admin);
        client.try_release_milestone_payment(&carrier, &id, &symbol_short!("beta"))
    };
    let via_record = {
        let (id, _c, carrier) = create_milestone_shipment(&env, &client, &admin);
        client
            .try_record_milestone(&carrier, &id, &symbol_short!("beta"), &data_hash(&env, 0x22))
            .map(|r| r.map(|_| ()))
    };
    let via_batch = {
        let (id, _c, carrier) = create_milestone_shipment(&env, &client, &admin);
        let mut milestones = Vec::new(&env);
        milestones.push_back((symbol_short!("beta"), data_hash(&env, 0x22)));
        client
            .try_record_milestones_batch(&carrier, &id, &milestones)
            .map(|r| r.map(|_| ()))
    };

    assert_eq!(via_release, Err(Ok(NavinError::InvalidStatus)));
    assert_eq!(via_record, Err(Ok(NavinError::InvalidStatus)));
    assert_eq!(via_batch, Err(Ok(NavinError::InvalidStatus)));
}

#[test]
fn test_validation_rejects_zero_percentage_milestone() {
    let (env, client, admin) = setup();
    let company = soroban_sdk::Address::generate(&env);
    let carrier = soroban_sdk::Address::generate(&env);
    let receiver = soroban_sdk::Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((soroban_sdk::symbol_short!("zero"), 0u32));
    milestones.push_back((soroban_sdk::symbol_short!("rest"), 100u32));

    let deadline = env.ledger().timestamp() + 86_400;
    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash(&env, 0xEE),
        &milestones,
        &deadline,
    );

    assert_eq!(
        result,
        Err(Ok(crate::errors::NavinError::InvalidPaymentMilestones))
    );
}
