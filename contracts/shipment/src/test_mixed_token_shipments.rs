//! # #298 — Mixed-Token Shipment Integration Matrix
//!
//! Verifies that concurrent shipments backed by **different** token contracts
//! are fully isolated: no cross-token contamination in balances, escrow
//! accounting, or settlement records.
//!
//! ## Matrix
//! | Shipment | Token        | Flow                  |
//! |----------|--------------|-----------------------|
//! | A        | SAC (token1) | full escrow → deliver |
//! | B        | Custom NVN   | full escrow → deliver |
//! | C        | SAC (token1) | escrow → refund       |
//! | D        | Custom NVN   | escrow → dispute      |
//!
//! Each assertion checks that token1 balances are unaffected by token2
//! operations and vice-versa.

extern crate std;

use crate::{test_utils, types::ShipmentStatus, NavinShipment, NavinShipmentClient};
use navin_token::NavinTokenClient;
use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, Address, BytesN, Env, IntoVal, Vec,
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn dummy_hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

/// Deploy a fresh SAC and return its address.
fn deploy_sac(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract_v2(admin.clone())
        .address()
}

/// Deploy a fresh NavinToken, initialize it, and return its address.
fn deploy_nvn(env: &Env, admin: &Address) -> Address {
    let addr = env.register(navin_token::NavinToken, ());
    NavinTokenClient::new(env, &addr).initialize(
        admin,
        &soroban_sdk::String::from_str(env, "Navin Token"),
        &soroban_sdk::String::from_str(env, "NVN"),
        &1_000_000_000,
    );
    addr
}

/// Mint `amount` SAC tokens to `to` (SAC mint takes `(to, amount)` — no admin arg).
fn mint_sac(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

/// Mint `amount` NavinToken tokens to `to` (NavinToken mint takes `(admin, to, amount)`).
fn mint_nvn(env: &Env, token: &Address, admin: &Address, to: &Address, amount: i128) {
    NavinTokenClient::new(env, token).mint(admin, to, &amount);
}

fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    let mut args: Vec<soroban_sdk::Val> = Vec::new(env);
    args.push_back(who.clone().into_val(env));
    env.invoke_contract::<i128>(token, &soroban_sdk::symbol_short!("balance"), args)
}

// ── shared setup ─────────────────────────────────────────────────────────────

struct MixedCtx {
    env: Env,
    admin: Address,
    company: Address,
    carrier: Address,
    receiver: Address,
    token_sac: Address,
    token_nvn: Address,
    /// Shipment contract initialised with token_sac
    client_sac: NavinShipmentClient<'static>,
    /// Shipment contract initialised with token_nvn
    client_nvn: NavinShipmentClient<'static>,
}

fn setup() -> MixedCtx {
    let (env, admin) = test_utils::setup_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    let token_sac = deploy_sac(&env, &admin);
    let token_nvn = deploy_nvn(&env, &admin);

    // Two independent shipment contract instances, each bound to a different token
    let addr_sac = env.register(NavinShipment, ());
    let client_sac = NavinShipmentClient::new(&env, &addr_sac);
    client_sac.initialize(&admin, &token_sac);
    client_sac.add_company(&admin, &company);
    client_sac.add_carrier(&admin, &carrier);
    client_sac.add_carrier_to_whitelist(&company, &carrier);

    let addr_nvn = env.register(NavinShipment, ());
    let client_nvn = NavinShipmentClient::new(&env, &addr_nvn);
    client_nvn.initialize(&admin, &token_nvn);
    client_nvn.add_company(&admin, &company);
    client_nvn.add_carrier(&admin, &carrier);
    client_nvn.add_carrier_to_whitelist(&company, &carrier);

    MixedCtx {
        env,
        admin,
        company,
        carrier,
        receiver,
        token_sac,
        token_nvn,
        client_sac,
        client_nvn,
    }
}

// ── #298-1: SAC escrow deposit does not affect NVN balances ──────────────────

#[test]
fn test_sac_deposit_does_not_affect_nvn_balance() {
    let ctx = setup();
    let amount = 1_000i128;

    mint_sac(&ctx.env, &ctx.token_sac, &ctx.company, amount);
    mint_nvn(&ctx.env, &ctx.token_nvn, &ctx.admin, &ctx.company, amount);

    let deadline = ctx.env.ledger().timestamp() + 3600;
    let id_sac = ctx.client_sac.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 1),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );

    ctx.client_sac
        .deposit_escrow(&ctx.company, &id_sac, &amount);

    // SAC balance moved into contract
    assert_eq!(balance(&ctx.env, &ctx.token_sac, &ctx.company), 0);
    assert_eq!(
        balance(&ctx.env, &ctx.token_sac, &ctx.client_sac.address),
        amount
    );

    // NVN balances completely untouched
    assert_eq!(balance(&ctx.env, &ctx.token_nvn, &ctx.company), amount);
    assert_eq!(
        balance(&ctx.env, &ctx.token_nvn, &ctx.client_nvn.address),
        0
    );
}

// ── #298-2: NVN escrow deposit does not affect SAC balances ──────────────────

#[test]
fn test_nvn_deposit_does_not_affect_sac_balance() {
    let ctx = setup();
    let amount = 500i128;

    mint_sac(&ctx.env, &ctx.token_sac, &ctx.company, amount);
    mint_nvn(&ctx.env, &ctx.token_nvn, &ctx.admin, &ctx.company, amount);

    let deadline = ctx.env.ledger().timestamp() + 3600;
    let id_nvn = ctx.client_nvn.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 2),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );

    ctx.client_nvn
        .deposit_escrow(&ctx.company, &id_nvn, &amount);

    // NVN moved into nvn contract
    assert_eq!(balance(&ctx.env, &ctx.token_nvn, &ctx.company), 0);
    assert_eq!(
        balance(&ctx.env, &ctx.token_nvn, &ctx.client_nvn.address),
        amount
    );

    // SAC completely untouched
    assert_eq!(balance(&ctx.env, &ctx.token_sac, &ctx.company), amount);
    assert_eq!(
        balance(&ctx.env, &ctx.token_sac, &ctx.client_sac.address),
        0
    );
}

// ── #298-3: Concurrent deliver flows — full matrix ───────────────────────────
//
// Shipment A (SAC) and Shipment B (NVN) run concurrently.
// After both deliveries:
//   - carrier holds amount_a in SAC and amount_b in NVN
//   - both contract escrow balances are zero
//   - no cross-token leakage

#[test]
fn test_concurrent_deliver_balance_isolation() {
    let ctx = setup();
    let amount_a = 800i128;
    let amount_b = 600i128;

    mint_sac(&ctx.env, &ctx.token_sac, &ctx.company, amount_a);
    mint_nvn(&ctx.env, &ctx.token_nvn, &ctx.admin, &ctx.company, amount_b);

    let deadline = ctx.env.ledger().timestamp() + 3600;

    // Create both shipments
    let id_a = ctx.client_sac.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 10),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );
    let id_b = ctx.client_nvn.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 11),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );

    // Deposit escrow for both
    ctx.client_sac
        .deposit_escrow(&ctx.company, &id_a, &amount_a);
    ctx.client_nvn
        .deposit_escrow(&ctx.company, &id_b, &amount_b);

    // Advance both to InTransit
    ctx.client_sac.update_status(
        &ctx.carrier,
        &id_a,
        &ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env, 12),
    );
    ctx.client_nvn.update_status(
        &ctx.carrier,
        &id_b,
        &ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env, 13),
    );

    // Confirm both deliveries
    ctx.client_sac
        .confirm_delivery(&ctx.receiver, &id_a, &dummy_hash(&ctx.env, 14));
    ctx.client_nvn
        .confirm_delivery(&ctx.receiver, &id_b, &dummy_hash(&ctx.env, 15));

    // Carrier received exactly the right token amounts
    assert_eq!(balance(&ctx.env, &ctx.token_sac, &ctx.carrier), amount_a);
    assert_eq!(balance(&ctx.env, &ctx.token_nvn, &ctx.carrier), amount_b);

    // Both contract escrows are zero
    assert_eq!(
        balance(&ctx.env, &ctx.token_sac, &ctx.client_sac.address),
        0
    );
    assert_eq!(
        balance(&ctx.env, &ctx.token_nvn, &ctx.client_nvn.address),
        0
    );

    // No cross-token leakage: SAC contract holds no NVN, NVN contract holds no SAC
    assert_eq!(
        balance(&ctx.env, &ctx.token_nvn, &ctx.client_sac.address),
        0
    );
    assert_eq!(
        balance(&ctx.env, &ctx.token_sac, &ctx.client_nvn.address),
        0
    );
}

// ── #298-4: SAC refund does not affect NVN escrow ────────────────────────────

#[test]
fn test_sac_refund_does_not_affect_nvn_escrow() {
    let ctx = setup();
    let amount = 400i128;

    mint_sac(&ctx.env, &ctx.token_sac, &ctx.company, amount);
    mint_nvn(&ctx.env, &ctx.token_nvn, &ctx.admin, &ctx.company, amount);

    let deadline = ctx.env.ledger().timestamp() + 3600;

    let id_sac = ctx.client_sac.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 20),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );
    let id_nvn = ctx.client_nvn.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 21),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );

    ctx.client_sac
        .deposit_escrow(&ctx.company, &id_sac, &amount);
    ctx.client_nvn
        .deposit_escrow(&ctx.company, &id_nvn, &amount);

    // Refund only the SAC shipment
    ctx.client_sac.refund_escrow(&ctx.company, &id_sac);

    // SAC refunded to company
    assert_eq!(balance(&ctx.env, &ctx.token_sac, &ctx.company), amount);
    assert_eq!(
        balance(&ctx.env, &ctx.token_sac, &ctx.client_sac.address),
        0
    );

    // NVN escrow completely unaffected
    assert_eq!(ctx.client_nvn.get_escrow_balance(&id_nvn), amount);
    assert_eq!(
        balance(&ctx.env, &ctx.token_nvn, &ctx.client_nvn.address),
        amount
    );
    assert_eq!(balance(&ctx.env, &ctx.token_nvn, &ctx.company), 0);
}

// ── #298-5: NVN dispute does not affect SAC escrow ───────────────────────────

#[test]
fn test_nvn_dispute_does_not_affect_sac_escrow() {
    let ctx = setup();
    let amount = 300i128;

    mint_sac(&ctx.env, &ctx.token_sac, &ctx.company, amount);
    mint_nvn(&ctx.env, &ctx.token_nvn, &ctx.admin, &ctx.company, amount);

    let deadline = ctx.env.ledger().timestamp() + 3600;

    let id_sac = ctx.client_sac.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 30),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );
    let id_nvn = ctx.client_nvn.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 31),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );

    ctx.client_sac
        .deposit_escrow(&ctx.company, &id_sac, &amount);
    ctx.client_nvn
        .deposit_escrow(&ctx.company, &id_nvn, &amount);

    // Raise dispute only on NVN shipment
    ctx.client_nvn
        .raise_dispute(&ctx.company, &id_nvn, &dummy_hash(&ctx.env, 32));

    // NVN shipment is disputed, escrow frozen
    let nvn_ship = ctx.client_nvn.get_shipment(&id_nvn);
    assert_eq!(nvn_ship.status, ShipmentStatus::Disputed);
    assert_eq!(ctx.client_nvn.get_escrow_balance(&id_nvn), amount);

    // SAC shipment completely unaffected
    let sac_ship = ctx.client_sac.get_shipment(&id_sac);
    assert_eq!(sac_ship.status, ShipmentStatus::Created);
    assert_eq!(ctx.client_sac.get_escrow_balance(&id_sac), amount);
}

// ── #298-6: Milestone payments isolated per token ────────────────────────────

#[test]
fn test_milestone_payments_isolated_per_token() {
    let ctx = setup();
    let amount = 1_000i128;

    mint_sac(&ctx.env, &ctx.token_sac, &ctx.company, amount);
    mint_nvn(&ctx.env, &ctx.token_nvn, &ctx.admin, &ctx.company, amount);

    let deadline = ctx.env.ledger().timestamp() + 3600;

    let mut milestones = Vec::new(&ctx.env);
    milestones.push_back((soroban_sdk::symbol_short!("M1"), 50u32));
    milestones.push_back((soroban_sdk::symbol_short!("M2"), 50u32));

    let id_sac = ctx.client_sac.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 40),
        &milestones,
        &deadline,
        &None,
    );
    let id_nvn = ctx.client_nvn.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 41),
        &milestones,
        &deadline,
        &None,
    );

    ctx.client_sac
        .deposit_escrow(&ctx.company, &id_sac, &amount);
    ctx.client_nvn
        .deposit_escrow(&ctx.company, &id_nvn, &amount);

    // Advance both to InTransit
    ctx.client_sac.update_status(
        &ctx.carrier,
        &id_sac,
        &ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env, 42),
    );
    ctx.client_nvn.update_status(
        &ctx.carrier,
        &id_nvn,
        &ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env, 43),
    );

    // Hit M1 only on SAC shipment
    test_utils::advance_past_rate_limit(&ctx.env);
    ctx.client_sac.record_milestone(
        &ctx.carrier,
        &id_sac,
        &soroban_sdk::symbol_short!("M1"),
        &dummy_hash(&ctx.env, 44),
    );

    // SAC: 50% released to carrier
    assert_eq!(balance(&ctx.env, &ctx.token_sac, &ctx.carrier), 500);
    assert_eq!(
        balance(&ctx.env, &ctx.token_sac, &ctx.client_sac.address),
        500
    );

    // NVN: completely untouched
    assert_eq!(balance(&ctx.env, &ctx.token_nvn, &ctx.carrier), 0);
    assert_eq!(
        balance(&ctx.env, &ctx.token_nvn, &ctx.client_nvn.address),
        amount
    );

    // Hit M1 only on NVN shipment
    ctx.client_nvn.record_milestone(
        &ctx.carrier,
        &id_nvn,
        &soroban_sdk::symbol_short!("M1"),
        &dummy_hash(&ctx.env, 45),
    );

    // NVN: 50% released to carrier
    assert_eq!(balance(&ctx.env, &ctx.token_nvn, &ctx.carrier), 500);
    assert_eq!(
        balance(&ctx.env, &ctx.token_nvn, &ctx.client_nvn.address),
        500
    );

    // SAC: unchanged since last check
    assert_eq!(balance(&ctx.env, &ctx.token_sac, &ctx.carrier), 500);
    assert_eq!(
        balance(&ctx.env, &ctx.token_sac, &ctx.client_sac.address),
        500
    );
}

// ── #298-7: Escrow counters are per-contract, not shared ─────────────────────

#[test]
fn test_escrow_balance_counters_are_per_contract() {
    let ctx = setup();
    let amount_sac = 200i128;
    let amount_nvn = 700i128;

    mint_sac(&ctx.env, &ctx.token_sac, &ctx.company, amount_sac);
    mint_nvn(
        &ctx.env,
        &ctx.token_nvn,
        &ctx.admin,
        &ctx.company,
        amount_nvn,
    );

    let deadline = ctx.env.ledger().timestamp() + 3600;

    let id_sac = ctx.client_sac.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 50),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );
    let id_nvn = ctx.client_nvn.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env, 51),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );

    ctx.client_sac
        .deposit_escrow(&ctx.company, &id_sac, &amount_sac);
    ctx.client_nvn
        .deposit_escrow(&ctx.company, &id_nvn, &amount_nvn);

    // Each contract reports its own escrow balance independently
    assert_eq!(ctx.client_sac.get_escrow_balance(&id_sac), amount_sac);
    assert_eq!(ctx.client_nvn.get_escrow_balance(&id_nvn), amount_nvn);

    // Shipment IDs are independent counters per contract
    assert_eq!(id_sac, 1u64);
    assert_eq!(id_nvn, 1u64);
}
