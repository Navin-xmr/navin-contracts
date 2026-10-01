#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, Map, String};

fn setup() -> (Env, Address, NavinShipmentNftClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_address = env.register(NavinShipmentNft, ());
    let client = NavinShipmentNftClient::new(&env, &contract_address);

    (env, admin, client)
}

#[test]
fn test_initialize() {
    let (env, admin, client) = setup();
    
    let name = String::from_str(&env, "Navin Shipment NFTs");
    let symbol = String::from_str(&env, "NSN");

    let result = client.initialize(&admin, &name, &symbol);
    assert!(result.is_ok());

    assert_eq!(client.name().unwrap(), name);
    assert_eq!(client.symbol().unwrap(), symbol);
    assert_eq!(client.get_admin().unwrap(), admin);
    assert!(client.is_initialized());
}

#[test]
fn test_initialize_twice_fails() {
    let (env, admin, client) = setup();
    
    let name = String::from_str(&env, "Navin Shipment NFTs");
    let symbol = String::from_str(&env, "NSN");

    client.initialize(&admin, &name, &symbol).unwrap();
    
    let result = client.initialize(&admin, &name, &symbol);
    assert!(result.is_err());
}

#[test]
fn test_mint_shipment_nft() {
    let (env, admin, client) = setup();
    
    let name = String::from_str(&env, "Navin Shipment NFTs");
    let symbol = String::from_str(&env, "NSN");
    client.initialize(&admin, &name, &symbol).unwrap();

    let recipient = Address::generate(&env);
    let shipment_id = 12345u64;
    let data_hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
    let metadata = Map::new(&env);

    let token_id = client.mint_shipment_nft(&recipient, &shipment_id, &data_hash, &metadata).unwrap();
    
    assert_eq!(token_id, 1);
    assert_eq!(client.owner_of(&token_id).unwrap(), recipient);
    assert_eq!(client.get_shipment_id(&token_id).unwrap(), shipment_id);
    assert_eq!(client.get_data_hash(&token_id).unwrap(), data_hash);
    assert_eq!(client.total_supply().unwrap(), 1);
}

#[test]
fn test_transfer() {
    let (env, admin, client) = setup();
    
    let name = String::from_str(&env, "Navin Shipment NFTs");
    let symbol = String::from_str(&env, "NSN");
    client.initialize(&admin, &name, &symbol).unwrap();

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let shipment_id = 12345u64;
    let data_hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
    let metadata = Map::new(&env);

    let token_id = client.mint_shipment_nft(&from, &shipment_id, &data_hash, &metadata).unwrap();
    
    client.transfer(&from, &to, &token_id).unwrap();
    
    assert_eq!(client.owner_of(&token_id).unwrap(), to);
}

#[test]
fn test_transfer_unauthorized() {
    let (env, admin, client) = setup();
    
    let name = String::from_str(&env, "Navin Shipment NFTs");
    let symbol = String::from_str(&env, "NSN");
    client.initialize(&admin, &name, &symbol).unwrap();

    let owner = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let to = Address::generate(&env);
    let shipment_id = 12345u64;
    let data_hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
    let metadata = Map::new(&env);

    let token_id = client.mint_shipment_nft(&owner, &shipment_id, &data_hash, &metadata).unwrap();
    
    let result = client.try_transfer(&unauthorized, &to, &token_id);
    assert!(result.is_err());
}

#[test]
fn test_queries_uninitialized_contract() {
    let (env, _admin, client) = setup();
    
    assert!(!client.is_initialized());
    assert!(client.try_name().is_err());
    assert!(client.try_symbol().is_err());
    assert!(client.try_get_admin().is_err());
    assert!(client.try_total_supply().is_err());
}