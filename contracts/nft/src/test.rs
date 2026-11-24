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
    assert!(!client.is_paused(), "contract must be unpaused after unpause()");
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
