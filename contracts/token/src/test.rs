#![cfg(test)]

extern crate std;

use crate::{test_utils::setup_env, NavinToken, NavinTokenClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, String, Symbol,
};

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

    client.approve(&admin, &spender, &300, &crate::MAX_EXPIRATION_LEDGER);
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

// ============================================================================
// Allowance Expiration Tests (issue #659)
// ============================================================================

fn set_ledger_sequence(env: &Env, seq: u32) {
    env.ledger().with_mut(|li| {
        li.sequence_number = seq;
    });
}

#[test]
fn test_approve_with_expiration_ledger_readable_before_expiry() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let spender = Address::generate(&env);

    let current = env.ledger().sequence();
    client.approve(&admin, &spender, &500, &(current + 10));

    assert_eq!(client.allowance(&admin, &spender), 500);
}

#[test]
fn test_allowance_valid_at_exact_expiration_ledger() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let spender = Address::generate(&env);

    let expiry = env.ledger().sequence() + 5;
    client.approve(&admin, &spender, &500, &expiry);

    set_ledger_sequence(&env, expiry);
    assert_eq!(client.allowance(&admin, &spender), 500);
}

#[test]
fn test_allowance_reads_as_zero_after_expiration_ledger() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let spender = Address::generate(&env);

    let expiry = env.ledger().sequence() + 5;
    client.approve(&admin, &spender, &500, &expiry);

    set_ledger_sequence(&env, expiry + 1);
    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_transfer_from_fails_once_allowance_expired() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);

    let expiry = env.ledger().sequence() + 5;
    client.approve(&admin, &spender, &500, &expiry);

    set_ledger_sequence(&env, expiry + 1);
    // Allowance reads back as 0, so this must fail InsufficientAllowance.
    client.transfer_from(&spender, &admin, &recipient, &100);
}

#[test]
fn test_approve_max_expiration_ledger_never_expires() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let spender = Address::generate(&env);

    client.approve(&admin, &spender, &500, &crate::MAX_EXPIRATION_LEDGER);

    // Extend the contract instance's own TTL before jumping the ledger
    // sequence far into the future — otherwise the test sandbox treats the
    // instance itself as archived, independent of the allowance logic
    // under test (mirrors contracts/shipment/src/test_ttl_health.rs).
    env.as_contract(&client.address, || {
        env.storage().instance().extend_ttl(200_000, 200_000);
        // Allowances live in persistent storage, so the allowance entry's
        // TTL must be extended as well — a fresh persistent entry only starts
        // with `min_persistent_entry_ttl` (4096) ledgers of life.
        env.storage().persistent().extend_ttl(
            &crate::storage::DataKey::Allowance(admin.clone(), spender.clone()),
            200_000,
            200_000,
        );
    });

    // Advance far beyond any realistic expiration window to demonstrate
    // MAX_EXPIRATION_LEDGER behaves as "effectively never expires".
    set_ledger_sequence(&env, 100_000);
    assert_eq!(client.allowance(&admin, &spender), 500);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_approve_rejects_expiration_ledger_already_passed() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let spender = Address::generate(&env);

    set_ledger_sequence(&env, 100);
    // A positive amount with an expiration_ledger before "now" is invalid.
    client.approve(&admin, &spender, &500, &50);
}

#[test]
fn test_approve_zero_amount_allowed_even_with_past_expiration_ledger() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let spender = Address::generate(&env);

    set_ledger_sequence(&env, 100);
    // Zero-amount approve clears the allowance regardless of expiration_ledger.
    client.approve(&admin, &spender, &0, &50);

    assert_eq!(client.allowance(&admin, &spender), 0);
}

#[test]
fn test_transfer_from_preserves_expiration_ledger_after_partial_spend() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);

    let expiry = env.ledger().sequence() + 5;
    client.approve(&admin, &spender, &500, &expiry);
    client.transfer_from(&spender, &admin, &recipient, &100);

    // Still valid at the original expiry boundary.
    set_ledger_sequence(&env, expiry);
    assert_eq!(client.allowance(&admin, &spender), 400);

    // And still expires at the originally-set ledger, not extended.
    set_ledger_sequence(&env, expiry + 1);
    assert_eq!(client.allowance(&admin, &spender), 0);
}

// ============================================================================
// Burn Tests (issue #658)
// ============================================================================

#[test]
fn test_admin_burn_success() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let holder = Address::generate(&env);
    client.mint(&admin, &holder, &500);

    client.admin_burn(&admin, &holder, &200);

    assert_eq!(client.balance(&holder), 300);
    assert_eq!(client.total_supply(), 1_000_300);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_admin_burn_unauthorized() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let holder = Address::generate(&env);
    client.mint(&admin, &holder, &500);

    let non_admin = Address::generate(&env);
    client.admin_burn(&non_admin, &holder, &200);
}

#[test]
fn test_holder_burn_success() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let holder = Address::generate(&env);
    client.mint(&admin, &holder, &500);

    // No admin involved — the holder authorizes their own burn.
    client.burn(&holder, &200);

    assert_eq!(client.balance(&holder), 300);
    assert_eq!(client.total_supply(), 1_000_300);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_holder_burn_insufficient_balance() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let holder = Address::generate(&env);

    client.burn(&holder, &100);
}

#[test]
fn test_burn_from_success() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let spender = Address::generate(&env);

    client.approve(&admin, &spender, &300, &crate::MAX_EXPIRATION_LEDGER);
    client.burn_from(&spender, &admin, &200);

    assert_eq!(client.balance(&admin), 999_800);
    assert_eq!(client.total_supply(), 999_800);
    assert_eq!(client.allowance(&admin, &spender), 100);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_burn_from_insufficient_allowance() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let spender = Address::generate(&env);

    client.approve(&admin, &spender, &100, &crate::MAX_EXPIRATION_LEDGER);
    client.burn_from(&spender, &admin, &200);
}

// ============================================================================
// Pause/Unpause Tests (issue #657)
// ============================================================================

#[test]
fn test_pause_and_unpause_toggle_is_paused() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    assert!(!client.is_paused());
    client.pause(&admin);
    assert!(client.is_paused());
    client.unpause(&admin);
    assert!(!client.is_paused());
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_pause_unauthorized() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let non_admin = Address::generate(&env);
    client.pause(&non_admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_paused_blocks_transfer() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let recipient = Address::generate(&env);

    client.pause(&admin);
    client.transfer(&admin, &recipient, &100);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_paused_blocks_transfer_from() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.approve(&admin, &spender, &300, &crate::MAX_EXPIRATION_LEDGER);

    client.pause(&admin);
    client.transfer_from(&spender, &admin, &recipient, &100);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_paused_blocks_mint() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let recipient = Address::generate(&env);

    client.pause(&admin);
    client.mint(&admin, &recipient, &100);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_paused_blocks_holder_burn() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    client.pause(&admin);
    client.burn(&admin, &100);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_paused_blocks_admin_burn() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    client.pause(&admin);
    client.admin_burn(&admin, &admin, &100);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_paused_blocks_burn_from() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let spender = Address::generate(&env);
    client.approve(&admin, &spender, &300, &crate::MAX_EXPIRATION_LEDGER);

    client.pause(&admin);
    client.burn_from(&spender, &admin, &100);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_paused_blocks_batch_transfer() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let recipient = Address::generate(&env);

    let mut recipients = soroban_sdk::Vec::new(&env);
    recipients.push_back((recipient, 100));

    client.pause(&admin);
    client.batch_transfer(&admin, &recipients);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_paused_blocks_approve() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let spender = Address::generate(&env);

    client.pause(&admin);
    client.approve(&admin, &spender, &100, &crate::MAX_EXPIRATION_LEDGER);
}

#[test]
fn test_unpause_restores_transfer() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let recipient = Address::generate(&env);

    client.pause(&admin);
    client.unpause(&admin);
    client.transfer(&admin, &recipient, &100);

    assert_eq!(client.balance(&recipient), 100);
}

// ============================================================================
// Batch Transfer Tests (issue #656)
// ============================================================================

#[test]
fn test_batch_transfer_success() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    let r3 = Address::generate(&env);

    let mut recipients = soroban_sdk::Vec::new(&env);
    recipients.push_back((r1.clone(), 100));
    recipients.push_back((r2.clone(), 200));
    recipients.push_back((r3.clone(), 300));

    client.batch_transfer(&admin, &recipients);

    assert_eq!(client.balance(&r1), 100);
    assert_eq!(client.balance(&r2), 200);
    assert_eq!(client.balance(&r3), 300);
    assert_eq!(client.balance(&admin), 1_000_000 - 600);
}

#[test]
fn test_batch_transfer_partial_failure_rejects_all() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);
    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);

    let mut recipients = soroban_sdk::Vec::new(&env);
    recipients.push_back((r1.clone(), 100));
    // Second leg is invalid (non-positive amount) — the whole batch must
    // fail, and r1's leg must NOT have been applied either.
    recipients.push_back((r2.clone(), 0));

    let result = client.try_batch_transfer(&admin, &recipients);
    assert!(result.is_err());

    assert_eq!(
        client.balance(&r1),
        0,
        "no leg should apply when any leg fails"
    );
    assert_eq!(
        client.balance(&admin),
        1_000_000,
        "sender balance must be untouched"
    );
}

#[test]
fn test_batch_transfer_insufficient_total_balance_rejects_all() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000);
    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);

    let mut recipients = soroban_sdk::Vec::new(&env);
    recipients.push_back((r1.clone(), 600));
    recipients.push_back((r2.clone(), 600)); // 1200 total > 1000 balance

    let result = client.try_batch_transfer(&admin, &recipients);
    assert!(result.is_err());

    assert_eq!(client.balance(&r1), 0);
    assert_eq!(client.balance(&r2), 0);
    assert_eq!(client.balance(&admin), 1_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_batch_transfer_empty_batch_rejected() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let recipients: soroban_sdk::Vec<(Address, i128)> = soroban_sdk::Vec::new(&env);
    client.batch_transfer(&admin, &recipients);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_batch_transfer_rejects_self_transfer_leg() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let mut recipients = soroban_sdk::Vec::new(&env);
    recipients.push_back((admin.clone(), 100));

    client.batch_transfer(&admin, &recipients);
}

#[test]
fn test_batch_transfer_emits_per_recipient_detail() {
    use soroban_sdk::{testutils::Events as _, TryFromVal, Vec as SdkVec};

    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    let r3 = Address::generate(&env);

    let mut recipients = soroban_sdk::Vec::new(&env);
    recipients.push_back((r1.clone(), 100));
    recipients.push_back((r2.clone(), 250));
    recipients.push_back((r3.clone(), 375));

    client.batch_transfer(&admin, &recipients);

    // Reconstruct per-recipient amounts from the `batch_leg` events alone.
    let leg_topic = Symbol::new(&env, "batch_leg");
    let mut reconstructed: std::vec::Vec<(Address, i128)> = std::vec::Vec::new();
    for (_cid, topics, data) in env.events().all().iter() {
        if topics.get(0).and_then(|t| Symbol::try_from_val(&env, &t).ok()) != Some(leg_topic.clone())
        {
            continue;
        }
        let (from, to, amount): (Address, Address, i128) =
            TryFromVal::try_from_val(&env, &data).unwrap();
        assert_eq!(from, admin, "each leg event records the sender");
        reconstructed.push((to, amount));
    }

    assert_eq!(reconstructed.len(), 3, "one detail event per recipient");
    assert!(reconstructed.contains(&(r1.clone(), 100)));
    assert!(reconstructed.contains(&(r2.clone(), 250)));
    assert!(reconstructed.contains(&(r3.clone(), 375)));

    // The `batch_tr` summary still carries the full recipient/amount list.
    let sum_topic = Symbol::new(&env, "batch_tr");
    let summary = env
        .events()
        .all()
        .iter()
        .find(|(_cid, topics, _data)| {
            topics.get(0).and_then(|t| Symbol::try_from_val(&env, &t).ok()) == Some(sum_topic.clone())
        })
        .map(|(_cid, _topics, data)| data)
        .expect("batch_tr summary event must be emitted");
    let (from, list, count): (Address, SdkVec<(Address, i128)>, u32) =
        TryFromVal::try_from_val(&env, &summary).unwrap();
    assert_eq!(from, admin);
    assert_eq!(count, 3);
    assert_eq!(list, recipients);
}

#[test]
fn test_transfer_admin_success() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let new_admin = Address::generate(&env);
    client.transfer_admin(&admin, &new_admin);

    assert_eq!(client.get_admin(), new_admin);
}

#[test]
fn test_transfer_admin_unauthorized() {
    let (env, client, admin) = setup_token_env();
    initialize_token(&client, &env, &admin, 1_000_000);

    let non_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.transfer_admin(&non_admin, &new_admin);
    }));
    assert!(
        result.is_err(),
        "Non-admin must not be able to transfer admin"
    );
}
