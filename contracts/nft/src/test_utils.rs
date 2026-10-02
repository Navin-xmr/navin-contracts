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
//! Shared test utilities for deterministic Soroban SDK testing.
//!
//! This module provides helper functions to set up test environments
//! with explicit protocol version, timestamp, and sequence number
//! to ensure deterministic behavior across all tests.

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

/// Default protocol version for tests
pub const DEFAULT_PROTOCOL_VERSION: u32 = 22;

/// Default timestamp for tests (Unix epoch + 1 day)
pub const DEFAULT_TIMESTAMP: u64 = 86400;

/// Default sequence number for tests
pub const DEFAULT_SEQUENCE_NUMBER: u32 = 1;

/// Sets up a deterministic test environment with explicit protocol version,
/// timestamp, and sequence number.
///
/// # Returns
/// A tuple containing:
/// - `Env` - The configured Soroban environment
/// - `Address` - A generated admin address
///
/// # Example
/// ```rust
/// let (env, admin) = test_utils::setup_env();
/// ```
pub fn setup_env() -> (Env, Address) {
    let env = Env::default();

    // Set protocol version explicitly for deterministic behavior
    env.ledger().with_mut(|li| {
        li.protocol_version = DEFAULT_PROTOCOL_VERSION;
    });

    // Set explicit timestamp
    env.ledger().set_timestamp(DEFAULT_TIMESTAMP);

    // Set explicit sequence number
    env.ledger().with_mut(|li| {
        li.sequence_number = DEFAULT_SEQUENCE_NUMBER;
    });

    let admin = Address::generate(&env);
    env.mock_all_auths();

    (env, admin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_env_sets_protocol_version() {
        let (env, _admin) = setup_env();
        env.ledger().with_mut(|li| {
            assert_eq!(li.protocol_version, DEFAULT_PROTOCOL_VERSION);
        });
    }

    #[test]
    fn test_setup_env_sets_timestamp() {
        let (env, _admin) = setup_env();
        assert_eq!(env.ledger().timestamp(), DEFAULT_TIMESTAMP);
    }

    #[test]
    fn test_setup_env_sets_sequence_number() {
        let (env, _admin) = setup_env();
        env.ledger().with_mut(|li| {
            assert_eq!(li.sequence_number, DEFAULT_SEQUENCE_NUMBER);
        });
    }
}
