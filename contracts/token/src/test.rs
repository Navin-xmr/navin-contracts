#![cfg(test)]

extern crate std;

use crate::{NavinToken, NavinTokenClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup_env() -> (Env, NavinTokenClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(NavinToken, ());
    let client = NavinTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    (env, client, admin)
}

fn initialize_token(client: &NavinTokenClient, env: &Env, admin: &Address, total_supply: i128) {
    let name = String::from_str(env, "NavinToken");
    let symbol = String::from_str(env, "NVN");
    client.initialize(admin, &name, &symbol, &total_supply);
}

#[test]
fn test_initialize() {
    let (env, client, admin) = setup_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.name(), String::from_str(&env, "NavinToken"));
    assert_eq!(client.symbol(), String::from_str(&env, "NVN"));
    assert_eq!(client.total_supply(), 1_000_000);
    assert_eq!(client.balance(&admin), 1_000_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_re_initialization_fails() {
    let (env, client, admin) = setup_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    // Second initialization must fail with AlreadyInitialized
    initialize_token(&client, &env, &admin, 1_000_000);
}

#[test]
fn test_mint() {
    let (env, client, admin) = setup_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let recipient = Address::generate(&env);
    client.mint(&admin, &recipient, &500);

    assert_eq!(client.balance(&recipient), 500);
    assert_eq!(client.total_supply(), 1_000_500);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_mint_unauthorized() {
    let (env, client, admin) = setup_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let non_admin = Address::generate(&env);
    client.mint(&non_admin, &non_admin, &500);
}

#[test]
fn test_transfer() {
    let (env, client, admin) = setup_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let recipient = Address::generate(&env);
    client.transfer(&admin, &recipient, &200);

    assert_eq!(client.balance(&admin), 999_800);
    assert_eq!(client.balance(&recipient), 200);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_transfer_insufficient_balance() {
    let (env, client, admin) = setup_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    // sender has 0 balance
    client.transfer(&sender, &recipient, &100);
}

#[test]
fn test_balance_default_zero() {
    let (env, client, admin) = setup_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let unknown = Address::generate(&env);
    assert_eq!(client.balance(&unknown), 0);
}

#[test]
fn test_approve_and_transfer_from() {
    let (env, client, admin) = setup_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.approve(&admin, &spender, &300);
    assert_eq!(client.allowance(&admin, &spender), 300);

    client.transfer_from(&spender, &admin, &recipient, &200);
    assert_eq!(client.balance(&admin), 999_800);
    assert_eq!(client.balance(&recipient), 200);
    assert_eq!(client.allowance(&admin, &spender), 100);
}
