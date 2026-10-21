#![cfg(test)]

extern crate std;

use crate::{NavinNft, NavinNftClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup() -> (Env, NavinNftClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(NavinNft, ());
    let client = NavinNftClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, client, admin)
}

#[test]
fn base_uri_is_used_for_minted_token() {
    let (env, client, admin) = setup();
    let base_uri = String::from_str(&env, "https://example.com/nft/");
    let owner = Address::generate(&env);

    client.set_base_uri(&admin, &base_uri);
    client.mint(&admin, &owner, &42, &None);

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
    let owner = Address::generate(&env);

    client.set_base_uri(&admin, &base_uri);
    client.mint(&admin, &owner, &42, &Some(metadata_uri.clone()));

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
#[should_panic(expected = "Error(Contract, #6)")]
fn mint_without_base_or_metadata_uri_fails() {
    let (env, client, admin) = setup();
    let owner = Address::generate(&env);
    client.mint(&admin, &owner, &42, &None);
}
