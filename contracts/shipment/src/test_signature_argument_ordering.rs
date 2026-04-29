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
            None::<Vec<u64>>.into_val(&ctx.env)
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
