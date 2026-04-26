//! # Token Compatibility Integration Suite
//!
//! Validates the shipment contract's escrow and payment flows against both
//! Stellar Asset Contract (SAC) tokens and custom token contracts (NavinToken).
#![allow(deprecated)]

use crate::{test_utils, types::ShipmentStatus, NavinError, NavinShipment, NavinShipmentClient};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, IntoVal, Vec};

// Import custom token client
use navin_token::NavinTokenClient;

/// Matrix of token variants to test against.
#[derive(Clone, Copy)]
enum TokenVariant {
    StellarAsset,
    Custom,
}

struct TestContext {
    env: Env,
    shipment_client: NavinShipmentClient<'static>,
    token_address: Address,
    variant: TokenVariant,
    admin: Address,
    company: Address,
    carrier: Address,
    receiver: Address,
}

fn setup_test(variant: TokenVariant) -> TestContext {
    let (env, admin) = test_utils::setup_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    let token_address = match variant {
        TokenVariant::StellarAsset => {
            // Register SAC
            env.register_stellar_asset_contract_v2(admin.clone())
                .address()
        }
        TokenVariant::Custom => {
            // Register NavinToken
            let token_addr = env.register(navin_token::NavinToken, ());
            let token_client = NavinTokenClient::new(&env, &token_addr);
            token_client.initialize(
                &admin,
                &soroban_sdk::String::from_str(&env, "Navin Token"),
                &soroban_sdk::String::from_str(&env, "NVN"),
                &1_000_000_000,
            );
            token_addr
        }
    };

    let shipment_addr = env.register(NavinShipment, ());
    let shipment_client = NavinShipmentClient::new(&env, &shipment_addr);
    shipment_client.initialize(&admin, &token_address);

    // Roles setup
    shipment_client.add_company(&admin, &company);
    shipment_client.add_carrier(&admin, &carrier);
    shipment_client.add_carrier_to_whitelist(&company, &carrier);

    TestContext {
        env,
        shipment_client,
        token_address,
        variant,
        admin,
        company,
        carrier,
        receiver,
    }
}

fn mint_tokens(ctx: &TestContext, to: &Address, amount: i128) {
    let token_addr = ctx.token_address.clone();
    let mut args: Vec<soroban_sdk::Val> = Vec::new(&ctx.env);

    match ctx.variant {
        TokenVariant::StellarAsset => {
            // SAC mint(to, amount)
            args.push_back(to.clone().into_val(&ctx.env));
            args.push_back(amount.into_val(&ctx.env));
        }
        TokenVariant::Custom => {
            // NavinToken mint(admin, to, amount)
            args.push_back(ctx.admin.clone().into_val(&ctx.env));
            args.push_back(to.clone().into_val(&ctx.env));
            args.push_back(amount.into_val(&ctx.env));
        }
    }

    ctx.env
        .invoke_contract::<()>(&token_addr, &soroban_sdk::symbol_short!("mint"), args);
}

fn get_balance(ctx: &TestContext, address: &Address) -> i128 {
    let token_addr = ctx.token_address.clone();
    let mut args: Vec<soroban_sdk::Val> = Vec::new(&ctx.env);
    args.push_back(address.clone().into_val(&ctx.env));

    ctx.env
        .invoke_contract::<i128>(&token_addr, &soroban_sdk::symbol_short!("balance"), args)
}

pub fn dummy_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[1u8; 32])
}

// ── Integration Matrix Tests ─────────────────────────────────────────────────

#[test]
fn test_sac_escrow_flow() {
    run_escrow_flow_test(TokenVariant::StellarAsset);
}

#[test]
fn test_custom_token_escrow_flow() {
    run_escrow_flow_test(TokenVariant::Custom);
}

fn run_escrow_flow_test(variant: TokenVariant) {
    let ctx = setup_test(variant);
    let amount = 1000i128;

    mint_tokens(&ctx, &ctx.company, amount);
    assert_eq!(get_balance(&ctx, &ctx.company), amount);

    let deadline = ctx.env.ledger().timestamp() + 3600;
    let shipment_id = ctx.shipment_client.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env),
        &Vec::new(&ctx.env),
        &deadline,
    );

    ctx.shipment_client
        .deposit_escrow(&ctx.company, &shipment_id, &amount);

    assert_eq!(get_balance(&ctx, &ctx.company), 0);
    assert_eq!(get_balance(&ctx, &ctx.shipment_client.address), amount);
    assert_eq!(ctx.shipment_client.get_escrow_balance(&shipment_id), amount);

    // Transition to InTransit (required for Delivered)
    ctx.shipment_client.update_status(
        &ctx.carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env),
    );

    ctx.shipment_client
        .confirm_delivery(&ctx.receiver, &shipment_id, &dummy_hash(&ctx.env));

    assert_eq!(get_balance(&ctx, &ctx.shipment_client.address), 0);
    assert_eq!(get_balance(&ctx, &ctx.carrier), amount);
    assert_eq!(ctx.shipment_client.get_escrow_balance(&shipment_id), 0);

    let s = ctx.shipment_client.get_shipment(&shipment_id);
    assert_eq!(s.status, ShipmentStatus::Delivered);
}

#[test]
fn test_sac_refund_flow() {
    run_refund_flow_test(TokenVariant::StellarAsset);
}

#[test]
fn test_custom_token_refund_flow() {
    run_refund_flow_test(TokenVariant::Custom);
}

fn run_refund_flow_test(variant: TokenVariant) {
    let ctx = setup_test(variant);
    let amount = 500i128;

    mint_tokens(&ctx, &ctx.company, amount);
    let deadline = ctx.env.ledger().timestamp() + 3600;
    let shipment_id = ctx.shipment_client.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env),
        &Vec::new(&ctx.env),
        &deadline,
    );

    ctx.shipment_client
        .deposit_escrow(&ctx.company, &shipment_id, &amount);

    ctx.shipment_client
        .refund_escrow(&ctx.company, &shipment_id);

    assert_eq!(get_balance(&ctx, &ctx.company), amount);
    assert_eq!(get_balance(&ctx, &ctx.shipment_client.address), 0);

    let s = ctx.shipment_client.get_shipment(&shipment_id);
    assert_eq!(s.status, ShipmentStatus::Cancelled);
}

#[test]
fn test_sac_milestone_payment_flow() {
    run_milestone_payment_flow_test(TokenVariant::StellarAsset);
}

#[test]
fn test_custom_token_milestone_payment_flow() {
    run_milestone_payment_flow_test(TokenVariant::Custom);
}

fn run_milestone_payment_flow_test(variant: TokenVariant) {
    let ctx = setup_test(variant);
    let amount = 1000i128;

    mint_tokens(&ctx, &ctx.company, amount);
    let deadline = ctx.env.ledger().timestamp() + 3600;

    let mut milestones = Vec::new(&ctx.env);
    milestones.push_back((soroban_sdk::symbol_short!("M1"), 50)); // 50%
    milestones.push_back((soroban_sdk::symbol_short!("M2"), 50)); // 50%

    let shipment_id = ctx.shipment_client.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env),
        &milestones,
        &deadline,
    );

    ctx.shipment_client
        .deposit_escrow(&ctx.company, &shipment_id, &amount);

    ctx.shipment_client.update_status(
        &ctx.carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env),
    );

    test_utils::advance_past_rate_limit(&ctx.env);
    ctx.shipment_client.record_milestone(
        &ctx.carrier,
        &shipment_id,
        &soroban_sdk::symbol_short!("M1"),
        &dummy_hash(&ctx.env),
    );

    assert_eq!(get_balance(&ctx, &ctx.carrier), 500);
    assert_eq!(get_balance(&ctx, &ctx.shipment_client.address), 500);

    test_utils::advance_past_rate_limit(&ctx.env);
    ctx.shipment_client.record_milestone(
        &ctx.carrier,
        &shipment_id,
        &soroban_sdk::symbol_short!("M2"),
        &dummy_hash(&ctx.env),
    );

    assert_eq!(get_balance(&ctx, &ctx.carrier), 1000);
    assert_eq!(get_balance(&ctx, &ctx.shipment_client.address), 0);
}

#[test]
fn test_sac_insufficient_funds() {
    run_insufficient_funds_test(TokenVariant::StellarAsset);
}

#[test]
fn test_custom_token_insufficient_funds() {
    run_insufficient_funds_test(TokenVariant::Custom);
}

fn run_insufficient_funds_test(variant: TokenVariant) {
    let ctx = setup_test(variant);
    let amount = 1000i128;

    mint_tokens(&ctx, &ctx.company, 500);

    let deadline = ctx.env.ledger().timestamp() + 3600;
    let shipment_id = ctx.shipment_client.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env),
        &Vec::new(&ctx.env),
        &deadline,
    );

    let result = ctx
        .shipment_client
        .try_deposit_escrow(&ctx.company, &shipment_id, &amount);

    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, NavinError::TokenTransferFailed);
}

// ── Behavioral Assumptions ──────────────────────────────────────────────────
//
// 1. Interface Compliance: All supported tokens must implement the standard Soroban
//    Token Interface (specifically transfer, balance, and mint for tests).
//
// 2. Authentication: The contract assumes that calling transfer(from, to, amount)
//    on the token contract will trigger from.require_auth(). This is true for both
//    SAC and standard-compliant custom tokens.
//
// 3. Error Mapping: Any failure in the token's transfer method (due to
//    insufficient balance, frozen accounts, etc.) is captured by the shipment
//    contract and returned as NavinError::TokenTransferFailed.
//
// 4. Escrow Custody: The shipment contract acts as the custodian of escrowed tokens.
//    It must have been authorized (via approve or being the target of transfer)
//    to hold and later move these tokens. In this implementation, deposit_escrow
//    directly transfers tokens from the sender to the shipment contract.
//
// 5. Atomic Releases: Milestone payments and final releases are atomic. If a
//    token transfer fails, the entire transaction (including status updates)
//    is rolled back by Soroban.
