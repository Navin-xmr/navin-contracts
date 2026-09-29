#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, Map, String, Symbol};
use super::*;

pub fn setup_env() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    (env, admin)
}

pub fn create_nft_client(env: &Env) -> NavinShipmentNftClient {
    let contract_address = env.register(NavinShipmentNft, ());
    NavinShipmentNftClient::new(env, &contract_address)
}

pub fn initialize_nft_contract(
    env: &Env,
    client: &NavinShipmentNftClient,
    admin: &Address,
) -> (String, String) {
    let name = String::from_str(env, "Navin Shipment NFTs");
    let symbol = String::from_str(env, "NSN");
    
    client.initialize(admin, &name, &symbol).unwrap();
    
    (name, symbol)
}

pub fn mint_test_nft(
    env: &Env,
    client: &NavinShipmentNftClient,
    to: &Address,
    shipment_id: u64,
) -> (u64, soroban_sdk::BytesN<32>) {
    let data_hash = soroban_sdk::BytesN::from_array(env, &[1u8; 32]);
    let metadata = create_test_metadata(env);
    
    let token_id = client
        .mint_shipment_nft(to, &shipment_id, &data_hash, &metadata)
        .unwrap();
    
    (token_id, data_hash)
}

pub fn create_test_metadata(env: &Env) -> Map<Symbol, String> {
    let mut metadata = Map::new(env);
    metadata.set(
        Symbol::new(env, "description"),
        String::from_str(env, "Test shipment NFT"),
    );
    metadata.set(
        Symbol::new(env, "origin"),
        String::from_str(env, "New York"),
    );
    metadata.set(
        Symbol::new(env, "destination"),
        String::from_str(env, "Los Angeles"),
    );
    metadata
}

pub fn dummy_hash(env: &Env, seed: u8) -> soroban_sdk::BytesN<32> {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    soroban_sdk::BytesN::from_array(env, &bytes)
}