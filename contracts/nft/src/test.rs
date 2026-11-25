#![cfg(test)]

extern crate std;

use crate::{NavinShipmentNft, NavinShipmentNftClient};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, String};

fn setup() -> (Env, NavinShipmentNftClient<'static>, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(
        NavinShipmentNft,
        (
            admin.clone(),
            symbol_short!("NavinNFT"),
            symbol_short!("NNFT"),
        ),
    );
    let client = NavinShipmentNftClient::new(&env, &contract_id);
    env.mock_all_auths();
    (env, client, admin)
}

#[test]
fn base_uri_is_used_for_minted_token() {
    let (env, client, admin) = setup();
    let base_uri = String::from_str(&env, "https://example.com/nft/");
    let owner = crate::test_utils::account_address(&env, 0);

    client.set_base_uri(&admin, &base_uri);
    client.mint(&owner, &42);

    assert_eq!(
        client.get_token_uri(&42),
        String::from_str(&env, "https://example.com/nft/42")
    );
    assert_eq!(client.owner_of(&42), owner);
}

#[test]
fn explicit_metadata_uri_overrides_base_uri() {
    let (env, client, admin) = setup();
    let base_uri = String::from_str(&env, "https://example.com/nft/");
    let metadata_uri = String::from_str(&env, "ipfs://metadata/42.json");
    let owner = crate::test_utils::account_address(&env, 0);

    client.set_base_uri(&admin, &base_uri);
    client.mint(&owner, &42);
    client.set_token_uri(&admin, &42, &metadata_uri);

    assert_eq!(client.get_token_uri(&42), metadata_uri);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn non_admin_cannot_set_base_uri() {
    let (env, client, _) = setup();
    let unauthorized = Address::generate(&env);
    client.set_base_uri(
        &unauthorized,
        &String::from_str(&env, "https://example.com/"),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #16)")]
fn mint_without_base_or_metadata_uri_fails() {
    let (env, client, _) = setup();
    let owner = crate::test_utils::account_address(&env, 0);
    client.mint(&owner, &42);

    // Neither a base URI nor a per-token metadata URI is configured, so the
    // URI of the freshly minted token cannot be resolved.
    let _ = client.get_token_uri(&42);
}

// ── Pause / unpause ──────────────────────────────────────────────────────────

#[test]
fn pause_sets_paused_flag_and_unpause_clears_it() {
    let (_env, client, admin) = setup();

    assert!(!client.is_paused(), "contract must start unpaused");
    client.pause(&admin);
    assert!(client.is_paused(), "contract must be paused after pause()");
    client.unpause(&admin);
    assert!(
        !client.is_paused(),
        "contract must be unpaused after unpause()"
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn mint_fails_when_contract_is_paused() {
    let (env, client, admin) = setup();
    let base_uri = String::from_str(&env, "https://example.com/nft/");
    client.set_base_uri(&admin, &base_uri);
    let owner = Address::generate(&env);

    client.pause(&admin);
    // ContractPaused (error #8) must be returned for any mutating call.
    client.mint(&owner, &1);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn non_admin_cannot_pause() {
    let (env, client, _) = setup();
    let attacker = Address::generate(&env);
    client.pause(&attacker);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn non_admin_cannot_unpause() {
    let (env, client, admin) = setup();
    client.pause(&admin);
    let attacker = Address::generate(&env);
    client.unpause(&attacker);
}

// ── TTL Tests ────────────────────────────────────────────────────────────────

/// Verify that per-token persistent keys (Owner, MetadataUri, BurnApproved) have
/// their TTL extended when the token is accessed (issue #1250).
///
/// Without these extensions, ownership records of long-held proof NFTs would be
/// archived while the owner's list says they still own them.
#[test]
fn test_per_token_ttl_extended_on_access() {
    let (env, client, admin) = setup();
    let base_uri = String::from_str(&env, "https://example.com/nft/");
    client.set_base_uri(&admin, &base_uri);
    let owner = crate::test_utils::account_address(&env, 0);
    let token_id = 42;

    // Mint the token
    client.mint(&owner, &token_id);

    // Set a custom metadata URI
    let metadata_uri = String::from_str(&env, "ipfs://metadata/42.json");
    client.set_token_uri(&admin, &token_id, &metadata_uri);

    // Approve burn
    client.approve_burn(&token_id);

    // Check initial TTL values for all per-token keys
    let owner_key = crate::NftKey::Owner(token_id);
    let metadata_key = crate::NftKey::MetadataUri(token_id);
    let burn_approved_key = crate::NftKey::BurnApproved(token_id);

    let initial_owner_ttl = env.as_contract(&client.address, || {
        env.storage().persistent().get_ttl(&owner_key)
    });
    let initial_metadata_ttl = env.as_contract(&client.address, || {
        env.storage().persistent().get_ttl(&metadata_key)
    });
    let initial_burn_ttl = env.as_contract(&client.address, || {
        env.storage().persistent().get_ttl(&burn_approved_key)
    });

    // All keys should exist and have positive TTL
    assert!(initial_owner_ttl > 0, "Owner key must have positive TTL");
    assert!(initial_metadata_ttl > 0, "MetadataUri key must have positive TTL");
    assert!(initial_burn_ttl > 0, "BurnApproved key must have positive TTL");

    // Advance ledger to reduce TTL below threshold
    env.ledger().with_mut(|ledger| {
        ledger.sequence_number += crate::TTL_THRESHOLD + 1000;
    });

    // Extend instance TTL to prevent contract expiration during the test
    env.as_contract(&client.address, || {
        env.storage().instance().extend_ttl(200_000, 200_000);
    });

    // Access the token via owner_of, which should extend all per-token TTLs
    let _ = client.owner_of(&token_id);

    let owner_ttl_after = env.as_contract(&client.address, || {
        env.storage().persistent().get_ttl(&owner_key)
    });
    let metadata_ttl_after = env.as_contract(&client.address, || {
        env.storage().persistent().get_ttl(&metadata_key)
    });
    let burn_ttl_after = env.as_contract(&client.address, || {
        env.storage().persistent().get_ttl(&burn_approved_key)
    });

    // Verify TTL was extended for all keys
    assert!(
        owner_ttl_after >= crate::TTL_EXTENSION,
        "Owner TTL must be extended to at least TTL_EXTENSION: {} >= {}",
        owner_ttl_after,
        crate::TTL_EXTENSION
    );
    assert!(
        metadata_ttl_after >= crate::TTL_EXTENSION,
        "MetadataUri TTL must be extended to at least TTL_EXTENSION: {} >= {}",
        metadata_ttl_after,
        crate::TTL_EXTENSION
    );
    assert!(
        burn_ttl_after >= crate::TTL_EXTENSION,
        "BurnApproved TTL must be extended to at least TTL_EXTENSION: {} >= {}",
        burn_ttl_after,
        crate::TTL_EXTENSION
    );
}

/// Verify that get_token_uri extends per-token TTL (issue #1250).
#[test]
fn test_get_token_uri_extends_ttl() {
    let (env, client, admin) = setup();
    let base_uri = String::from_str(&env, "https://example.com/nft/");
    client.set_base_uri(&admin, &base_uri);
    let owner = crate::test_utils::account_address(&env, 0);
    let token_id = 99;

    client.mint(&owner, &token_id);

    // Advance ledger
    env.ledger().with_mut(|ledger| {
        ledger.sequence_number += crate::TTL_THRESHOLD + 1000;
    });

    // Extend instance TTL
    env.as_contract(&client.address, || {
        env.storage().instance().extend_ttl(200_000, 200_000);
    });

    // Access via get_token_uri
    let _ = client.get_token_uri(&token_id);

    let owner_key = crate::NftKey::Owner(token_id);
    let owner_ttl = env.as_contract(&client.address, || {
        env.storage().persistent().get_ttl(&owner_key)
    });

    assert!(
        owner_ttl >= crate::TTL_EXTENSION,
        "get_token_uri must extend Owner TTL: {} >= {}",
        owner_ttl,
        crate::TTL_EXTENSION
    );
}

/// Verify that transfer extends per-token TTL (issue #1250).
#[test]
fn test_transfer_extends_per_token_ttl() {
    let (env, client, admin) = setup();
    let base_uri = String::from_str(&env, "https://example.com/nft/");
    client.set_base_uri(&admin, &base_uri);
    let owner = crate::test_utils::account_address(&env, 0);
    let new_owner = crate::test_utils::account_address(&env, 1);
    let token_id = 123;

    client.mint(&owner, &token_id);

    // Advance ledger
    env.ledger().with_mut(|ledger| {
        ledger.sequence_number += crate::TTL_THRESHOLD + 1000;
    });

    // Extend instance TTL
    env.as_contract(&client.address, || {
        env.storage().instance().extend_ttl(200_000, 200_000);
    });

    // Transfer the token
    client.transfer(&owner, &new_owner, &token_id, &soroban_sdk::vec![&env]);

    let owner_key = crate::NftKey::Owner(token_id);
    let owner_ttl = env.as_contract(&client.address, || {
        env.storage().persistent().get_ttl(&owner_key)
    });

    assert!(
        owner_ttl >= crate::TTL_EXTENSION,
        "transfer must extend Owner TTL: {} >= {}",
        owner_ttl,
        crate::TTL_EXTENSION
    );
}
