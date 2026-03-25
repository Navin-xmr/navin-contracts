#![cfg(test)]

extern crate std;

use crate::{test_utils::setup_env, NavinToken, NavinTokenClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Symbol};

fn setup_token_env() -> (Env, NavinTokenClient<'static>, Address) {
    let (env, admin) = setup_env();
    let contract_id = env.register(NavinToken, ());
    let client = NavinTokenClient::new(&env, &contract_id);

    (env, client, admin)
}

fn initialize_token(client: &NavinTokenClient, env: &Env, admin: &Address, total_supply: i128) {
    let name = String::from_str(env, "NavinToken");
    let symbol = String::from_str(env, "NVN");
    client.initialize(admin, &name, &symbol, &total_supply);
}

// ============================================================================
// Basic Token Tests
// ============================================================================

#[test]
fn test_initialize() {
    let (env, client, admin) = setup_token_env();
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
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    // Second initialization must fail with AlreadyInitialized
    initialize_token(&client, &env, &admin, 1_000_000);
}

#[test]
fn test_mint() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let recipient = Address::generate(&env);
    client.mint(&admin, &recipient, &500);

    assert_eq!(client.balance(&recipient), 500);
    assert_eq!(client.total_supply(), 1_000_500);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_mint_unauthorized() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let non_admin = Address::generate(&env);
    client.mint(&non_admin, &non_admin, &500);
}

#[test]
fn test_transfer() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let recipient = Address::generate(&env);
    client.transfer(&admin, &recipient, &200);

    assert_eq!(client.balance(&admin), 999_800);
    assert_eq!(client.balance(&recipient), 200);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_transfer_insufficient_balance() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    // sender has 0 balance
    client.transfer(&sender, &recipient, &100);
}

#[test]
fn test_balance_default_zero() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let unknown = Address::generate(&env);
    assert_eq!(client.balance(&unknown), 0);
}

#[test]
fn test_approve_and_transfer_from() {
    let (env, client, admin) = setup_token_env();
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

// ============================================================================
// Metadata Allowlist Tests
// ============================================================================

#[test]
fn test_add_allowed_metadata_key_success() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let key = Symbol::new(&env, "website");
    client.add_allowed_metadata_key(&admin, &key);

    assert!(client.is_metadata_key_allowed(&key));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_add_allowed_metadata_key_unauthorized() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let non_admin = Address::generate(&env);
    let key = Symbol::new(&env, "website");
    client.add_allowed_metadata_key(&non_admin, &key);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_add_allowed_metadata_key_already_exists() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let key = Symbol::new(&env, "website");
    client.add_allowed_metadata_key(&admin, &key);
    // Adding the same key again should fail
    client.add_allowed_metadata_key(&admin, &key);
}

#[test]
fn test_remove_allowed_metadata_key_success() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let key = Symbol::new(&env, "website");
    client.add_allowed_metadata_key(&admin, &key);
    assert!(client.is_metadata_key_allowed(&key));

    client.remove_allowed_metadata_key(&admin, &key);
    assert!(!client.is_metadata_key_allowed(&key));
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_remove_allowed_metadata_key_not_found() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let key = Symbol::new(&env, "nonexistent");
    client.remove_allowed_metadata_key(&admin, &key);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_remove_allowed_metadata_key_unauthorized() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let key = Symbol::new(&env, "website");
    client.add_allowed_metadata_key(&admin, &key);

    let non_admin = Address::generate(&env);
    client.remove_allowed_metadata_key(&non_admin, &key);
}

// ============================================================================
// Metadata Set/Get Tests
// ============================================================================

#[test]
fn test_set_metadata_success() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let key = Symbol::new(&env, "website");
    let value = String::from_str(&env, "https://example.com");

    client.add_allowed_metadata_key(&admin, &key);
    client.set_metadata(&admin, &key, &value);

    let result = client.get_metadata(&key);
    assert_eq!(result, Some(value.clone()));
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_set_metadata_key_not_allowed() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let key = Symbol::new(&env, "unauthorized_key");
    let value = String::from_str(&env, "https://example.com");

    // Try to set metadata without adding key to allowlist
    client.set_metadata(&admin, &key, &value);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_set_metadata_unauthorized() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let key = Symbol::new(&env, "website");
    let value = String::from_str(&env, "https://example.com");

    client.add_allowed_metadata_key(&admin, &key);

    let non_admin = Address::generate(&env);
    client.set_metadata(&non_admin, &key, &value);
}

#[test]
fn test_get_metadata_nonexistent() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let key = Symbol::new(&env, "nonexistent");
    let result = client.get_metadata(&key);
    assert_eq!(result, None);
}

#[test]
fn test_remove_metadata_success() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let key = Symbol::new(&env, "website");
    let value = String::from_str(&env, "https://example.com");

    client.add_allowed_metadata_key(&admin, &key);
    client.set_metadata(&admin, &key, &value);
    assert_eq!(client.get_metadata(&key), Some(value.clone()));

    client.remove_metadata(&admin, &key);
    assert_eq!(client.get_metadata(&key), None);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_remove_metadata_not_found() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let key = Symbol::new(&env, "nonexistent");
    client.remove_metadata(&admin, &key);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_remove_metadata_unauthorized() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let key = Symbol::new(&env, "website");
    let value = String::from_str(&env, "https://example.com");

    client.add_allowed_metadata_key(&admin, &key);
    client.set_metadata(&admin, &key, &value);

    let non_admin = Address::generate(&env);
    client.remove_metadata(&non_admin, &key);
}

// ============================================================================
// Allowlist Update Immediacy Tests
// ============================================================================

#[test]
fn test_allowlist_updates_reflected_immediately() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let key = Symbol::new(&env, "twitter");
    let value = String::from_str(&env, "@navin");

    // Add key and set metadata
    client.add_allowed_metadata_key(&admin, &key);
    client.set_metadata(&admin, &key, &value);
    assert_eq!(client.get_metadata(&key), Some(value.clone()));

    // Remove key from allowlist
    client.remove_allowed_metadata_key(&admin, &key);

    // Metadata should still exist (removal doesn't delete data)
    assert_eq!(client.get_metadata(&key), Some(value.clone()));

    // But setting new value should fail
    let new_value = String::from_str(&env, "@newnavin");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.set_metadata(&admin, &key, &new_value);
    }));
    assert!(
        result.is_err(),
        "Should fail after key removed from allowlist"
    );
}

#[test]
fn test_multiple_allowed_keys() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let key1 = Symbol::new(&env, "website");
    let key2 = Symbol::new(&env, "twitter");
    let key3 = Symbol::new(&env, "discord");

    // Add all keys
    client.add_allowed_metadata_key(&admin, &key1);
    client.add_allowed_metadata_key(&admin, &key2);
    client.add_allowed_metadata_key(&admin, &key3);

    assert!(client.is_metadata_key_allowed(&key1));
    assert!(client.is_metadata_key_allowed(&key2));
    assert!(client.is_metadata_key_allowed(&key3));

    // Set metadata for all keys
    let value1 = String::from_str(&env, "value1");
    let value2 = String::from_str(&env, "value2");
    let value3 = String::from_str(&env, "value3");

    client.set_metadata(&admin, &key1, &value1);
    client.set_metadata(&admin, &key2, &value2);
    client.set_metadata(&admin, &key3, &value3);

    assert_eq!(client.get_metadata(&key1), Some(value1));
    assert_eq!(client.get_metadata(&key2), Some(value2));
    assert_eq!(client.get_metadata(&key3), Some(value3));

    // Remove middle key
    client.remove_allowed_metadata_key(&admin, &key2);
    assert!(!client.is_metadata_key_allowed(&key2));
    assert!(client.is_metadata_key_allowed(&key1));
    assert!(client.is_metadata_key_allowed(&key3));
}
