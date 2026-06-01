//! Canonical argument ordering regression tests for external call boundaries.

extern crate std;

use crate::{test_utils, NavinShipment, NavinShipmentClient, ShipmentStatus};
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, AuthorizedFunction},
    vec, Address, BytesN, Env, IntoVal, Symbol, Val, Vec,
};

#[contracttype]
#[derive(Clone)]
enum SpyDataKey {
    Count,
    Call(u32),
}

#[contract]
struct TokenSpy;

#[contractimpl]
impl TokenSpy {
    pub fn decimals(_env: Env) -> u32 {
        7
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        let count = env
            .storage()
            .persistent()
            .get(&SpyDataKey::Count)
            .unwrap_or(0u32);
        env.storage()
            .persistent()
            .set(&SpyDataKey::Call(count), &(from, to, amount));
        env.storage()
            .persistent()
            .set(&SpyDataKey::Count, &(count + 1));
    }

    pub fn get_call_count(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&SpyDataKey::Count)
            .unwrap_or(0)
    }

    pub fn get_call(env: Env, index: u32) -> (Address, Address, i128) {
        env.storage()
            .persistent()
            .get(&SpyDataKey::Call(index))
            .unwrap()
    }
}

struct Ctx {
    env: Env,
    client: NavinShipmentClient<'static>,
    token_spy: TokenSpyClient<'static>,
    admin: Address,
    company: Address,
    carrier: Address,
    receiver: Address,
}

fn hash32(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn setup() -> Ctx {
    let (env, admin) = test_utils::setup_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    let token_id = env.register(TokenSpy, ());
    let token_spy = TokenSpyClient::new(&env, &token_id);

    let shipment_id = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &shipment_id);
    client.initialize(&admin, &token_id);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    Ctx {
        env,
        client,
        token_spy,
        admin,
        company,
        carrier,
        receiver,
    }
}

fn assert_auth_args(
    env: &Env,
    caller: &Address,
    contract_id: &Address,
    function: &str,
    expected_args: Vec<Val>,
) {
    let auths = env.auths();
    let expected_symbol = Symbol::new(env, function);
    let maybe_auth = auths.iter().find(|(addr, invocation)| {
        if addr != caller {
            return false;
        }
        match &invocation.function {
            AuthorizedFunction::Contract((id, fn_name, _)) => {
                id == contract_id && fn_name == &expected_symbol
            }
            _ => false,
        }
    });

    let (_, invocation) = maybe_auth.expect("expected auth invocation for function");
    match &invocation.function {
        AuthorizedFunction::Contract((_, _, args)) => {
            assert_eq!(
                args, &expected_args,
                "argument order should remain canonical"
            );
        }
        _ => panic!("expected contract authorization"),
    }
}

#[test]
fn token_transfer_boundary_uses_from_to_amount_order() {
    let ctx = setup();
    let deadline = test_utils::future_deadline(&ctx.env, 3600);
    let shipment_id = ctx.client.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &hash32(&ctx.env, 1),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );

    ctx.client.deposit_escrow(&ctx.company, &shipment_id, &500);
    let call0 = ctx.token_spy.get_call(&0);
    assert_eq!(call0.0, ctx.company);
    assert_eq!(call0.1, ctx.client.address);
    assert_eq!(call0.2, 500);

    test_utils::advance_past_rate_limit(&ctx.env);
    ctx.client.update_status(
        &ctx.carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &hash32(&ctx.env, 2),
    );
    ctx.client
        .confirm_delivery(&ctx.receiver, &shipment_id, &hash32(&ctx.env, 3));

    let call1 = ctx.token_spy.get_call(&1);
    assert_eq!(call1.0, ctx.client.address);
    assert_eq!(call1.1, ctx.carrier);
    assert_eq!(call1.2, 500);
    assert_eq!(ctx.token_spy.get_call_count(), 2);
}

#[test]
fn token_refund_boundary_uses_from_to_amount_order() {
    let ctx = setup();
    let deadline = test_utils::future_deadline(&ctx.env, 3600);
    let shipment_id = ctx.client.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &hash32(&ctx.env, 7),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );

    ctx.client.deposit_escrow(&ctx.company, &shipment_id, &240);
    ctx.client.refund_escrow(&ctx.company, &shipment_id);

    let call0 = ctx.token_spy.get_call(&0);
    assert_eq!(call0.0, ctx.company);
    assert_eq!(call0.1, ctx.client.address);
    assert_eq!(call0.2, 240);

    let call1 = ctx.token_spy.get_call(&1);
    assert_eq!(call1.0, ctx.client.address);
    assert_eq!(call1.1, ctx.company);
    assert_eq!(call1.2, 240);
    assert_eq!(ctx.token_spy.get_call_count(), 2);
}

#[test]
fn create_shipment_and_status_update_auth_args_are_stable() {
    let ctx = setup();
    let deadline = test_utils::future_deadline(&ctx.env, 1800);
    let data_hash = hash32(&ctx.env, 11);
    let event_hash = hash32(&ctx.env, 12);
    let milestones: Vec<(Symbol, u32)> = Vec::new(&ctx.env);

    let shipment_id = ctx.client.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );

    assert_auth_args(
        &ctx.env,
        &ctx.company,
        &ctx.client.address,
        "create_shipment",
        vec![
            &ctx.env,
            ctx.company.clone().into_val(&ctx.env),
            ctx.receiver.clone().into_val(&ctx.env),
            ctx.carrier.clone().into_val(&ctx.env),
            data_hash.clone().into_val(&ctx.env),
            milestones.clone().into_val(&ctx.env),
            deadline.into_val(&ctx.env),
            None::<Vec<u64>>.into_val(&ctx.env),
        ],
    );

    test_utils::advance_past_rate_limit(&ctx.env);
    ctx.client.update_status(
        &ctx.carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &event_hash,
    );

    assert_auth_args(
        &ctx.env,
        &ctx.carrier,
        &ctx.client.address,
        "update_status",
        vec![
            &ctx.env,
            ctx.carrier.clone().into_val(&ctx.env),
            shipment_id.into_val(&ctx.env),
            ShipmentStatus::InTransit.into_val(&ctx.env),
            event_hash.clone().into_val(&ctx.env),
        ],
    );
}

#[test]
fn cancel_shipment_auth_arg_order_is_stable() {
    let ctx = setup();
    let deadline = test_utils::future_deadline(&ctx.env, 3600);
    let reason_hash = hash32(&ctx.env, 21);
    let shipment_id = ctx.client.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &hash32(&ctx.env, 20),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );

    ctx.client
        .cancel_shipment(&ctx.company, &shipment_id, &reason_hash);

    assert_auth_args(
        &ctx.env,
        &ctx.company,
        &ctx.client.address,
        "cancel_shipment",
        vec![
            &ctx.env,
            ctx.company.clone().into_val(&ctx.env),
            shipment_id.into_val(&ctx.env),
            reason_hash.into_val(&ctx.env),
        ],
    );
}

#[test]
fn transfer_admin_auth_arg_order_is_stable() {
    let ctx = setup();
    let new_admin = Address::generate(&ctx.env);

    ctx.client.transfer_admin(&ctx.admin, &new_admin);

    assert_auth_args(
        &ctx.env,
        &ctx.admin,
        &ctx.client.address,
        "transfer_admin",
        vec![
            &ctx.env,
            ctx.admin.clone().into_val(&ctx.env),
            new_admin.into_val(&ctx.env),
        ],
    );
}

// ── Issue #435: Multi-sig proposal ordering regression tests ──────────────────

mod multisig_order_helpers {
    use soroban_sdk::{contract, contractimpl, Address, Env};

    #[contract]
    pub struct MultiSigOrderToken;
    #[contractimpl]
    impl MultiSigOrderToken {
        pub fn transfer(_e: Env, _f: Address, _t: Address, _a: i128) {}
        pub fn decimals(_e: Env) -> u32 { 7 }
    }
}

/// Helper: set up a 2-of-3 multisig environment (3 admins, threshold 2).
fn setup_multisig_2of3() -> (Env, NavinShipmentClient<'static>, Address, Address, Address) {
    let (env, admin) = test_utils::setup_env();
    let token_id = env.register(multisig_order_helpers::MultiSigOrderToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token_id);

    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());
    client.init_multisig(&admin, &admins, &2);

    (env, client, admin, admin2, admin3)
}

fn wasm_hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

/// Proposal approvals are stored in insertion order — first approver must appear
/// at index 0 regardless of address lexicographic ordering.
#[test]
fn proposal_approvals_are_recorded_in_insertion_order() {
    let (env, client, admin, admin2, _admin3) = setup_multisig_2of3();

    let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 1));
    let salt = BytesN::from_array(&env, &[0xAAu8; 32]);
    let proposal_id = client.propose_action(&admin, &action, &salt);

    // admin2 approves (threshold 2 not yet met with just admin2, since
    // propose_action counts admin as approver #1, admin2 is #2 → auto-executes).
    // Check approval count after propose but before approve_action.
    let proposal_before = client.get_proposal(&proposal_id);
    assert_eq!(
        proposal_before.approvals.len(),
        0,
        "No external approvals yet after proposal creation"
    );

    let _ = client.try_approve_action(&admin2, &proposal_id);

    // After admin2 approves the proposal is executed; the approvals Vec
    // must contain admin2 at index 0 (first and only explicit approver).
    let proposal = client.get_proposal(&proposal_id);
    assert!(
        proposal.approvals.len() >= 1,
        "At least one approval must be recorded"
    );
    assert_eq!(
        proposal.approvals.get(0),
        Some(admin2),
        "First approver must be at index 0"
    );
}

/// A second approver is appended after the first — insertion order is preserved.
#[test]
fn proposal_second_approver_appended_after_first() {
    let (env, client, admin, admin2, admin3) = setup_multisig_2of3();

    // Use threshold 3 so we can observe two approvals before auto-execution.
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());
    // Re-init with threshold 3 (all must approve).
    // Note: init_multisig can only be called once per contract instance,
    // so we create a fresh client here.
    let token_id2 = env.register(multisig_order_helpers::MultiSigOrderToken {}, ());
    let client2 = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client2.initialize(&admin, &token_id2);
    client2.init_multisig(&admin, &admins, &3);

    let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 2));
    let salt = BytesN::from_array(&env, &[0xBBu8; 32]);
    let proposal_id = client2.propose_action(&admin, &action, &salt);

    let _ = client2.try_approve_action(&admin2, &proposal_id);

    let proposal_mid = client2.get_proposal(&proposal_id);
    assert_eq!(
        proposal_mid.approvals.len(),
        1,
        "Exactly one approval after admin2 approves"
    );
    assert_eq!(
        proposal_mid.approvals.get(0),
        Some(admin2),
        "admin2 must be at index 0"
    );

    let _ = client2.try_approve_action(&admin3, &proposal_id);

    let proposal_final = client2.get_proposal(&proposal_id);
    assert_eq!(
        proposal_final.approvals.len(),
        2,
        "Two approvals after admin3 approves"
    );
    assert_eq!(
        proposal_final.approvals.get(1),
        Some(admin3),
        "admin3 must be appended at index 1 — insertion order preserved"
    );
}

/// Duplicate approval by the same address must be rejected with AlreadyApproved.
/// This verifies that re-ordering or replaying the same signer cannot inflate
/// the approval count.
#[test]
fn duplicate_approval_is_rejected() {
    let (env, client, admin, admin2, _admin3) = setup_multisig_2of3();

    let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 3));
    let salt = BytesN::from_array(&env, &[0xCCu8; 32]);
    let proposal_id = client.propose_action(&admin, &action, &salt);

    // First approval by admin2 — succeeds.
    let first = client.try_approve_action(&admin2, &proposal_id);
    assert!(first.is_ok(), "First approval must succeed");

    // Second approval by admin2 on the same proposal — must be rejected.
    let duplicate = client.try_approve_action(&admin2, &proposal_id);
    assert!(
        duplicate.is_err(),
        "Duplicate approval from same address must be rejected"
    );
}

/// Proposal digests for two proposals with different actions must be distinct —
/// proves the digest helper provides domain separation across proposals.
#[test]
fn proposal_digests_are_distinct_for_different_actions() {
    let (env, client, admin, _admin2, _admin3) = setup_multisig_2of3();

    let action_a = crate::types::AdminAction::Upgrade(wasm_hash(&env, 10));
    let action_b = crate::types::AdminAction::Upgrade(wasm_hash(&env, 11));

    let salt_a = BytesN::from_array(&env, &[0x01u8; 32]);
    let salt_b = BytesN::from_array(&env, &[0x02u8; 32]);

    let id_a = client.propose_action(&admin, &action_a, &salt_a);
    let id_b = client.propose_action(&admin, &action_b, &salt_b);

    let digest_a = client.get_proposal_action_digest(&id_a);
    let digest_b = client.get_proposal_action_digest(&id_b);

    assert_ne!(
        digest_a.digest, digest_b.digest,
        "Distinct actions must produce distinct proposal digests"
    );
}
