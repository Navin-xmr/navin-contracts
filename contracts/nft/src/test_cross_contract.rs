#![cfg(test)]
//! # Cross-Contract Compatibility Tests
//!
//! Tests to verify NavinShipmentNft can coexist and interact with other contracts
//! in the same environment, providing groundwork for future integrations.

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, Map, String, Symbol};

/// Mock token contract for testing coexistence
mod mock_token {
    use soroban_sdk::{contract, contractimpl, Address, Env, String};

    #[contract]
    pub struct MockToken;

    #[contractimpl]
    impl MockToken {
        pub fn decimals(_env: Env) -> u32 {
            7
        }

        pub fn symbol(env: Env) -> String {
            String::from_str(&env, "MOCK")
        }

        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    }
}

/// Mock shipment contract for testing coexistence
mod mock_shipment {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec, Symbol, BytesN};

    #[contract]
    pub struct MockShipment;

    #[contractimpl]
    impl MockShipment {
        pub fn initialize(_env: Env, _admin: Address, _token: Address) {}

        pub fn create_shipment(
            _env: Env,
            _sender: Address,
            _receiver: Address,
            _carrier: Address,
            _data_hash: BytesN<32>,
            _milestones: Vec<(Symbol, u32)>,
            _deadline: u64,
        ) -> u64 {
            1 // Mock shipment ID
        }

        pub fn get_shipment_counter(_env: Env) -> u64 {
            1
        }
    }
}

struct MultiContractContext {
    env: Env,
    admin: Address,
    user: Address,
    nft_client: NavinShipmentNftClient<'static>,
    token_address: Address,
    shipment_address: Address,
}

fn setup_multi_contract_env() -> MultiContractContext {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Deploy token contract
    let token_address = env.register(mock_token::MockToken, ());

    // Deploy shipment contract
    let shipment_address = env.register(mock_shipment::MockShipment, ());

    // Deploy and initialize NFT contract
    let nft_address = env.register(NavinShipmentNft, ());
    let nft_client = NavinShipmentNftClient::new(&env, &nft_address);

    let name = String::from_str(&env, "Navin Shipment NFTs");
    let symbol = String::from_str(&env, "NSN");
    nft_client.initialize(&admin, &name, &symbol).unwrap();

    MultiContractContext {
        env,
        admin,
        user,
        nft_client,
        token_address,
        shipment_address,
    }
}

/// Test that NFT contract can coexist with token and shipment contracts
#[test]
fn test_multi_contract_coexistence() {
    let ctx = setup_multi_contract_env();

    // Verify NFT contract is initialized and functional
    assert!(ctx.nft_client.is_initialized());
    assert_eq!(
        ctx.nft_client.name().unwrap(),
        String::from_str(&ctx.env, "Navin Shipment NFTs")
    );
    assert_eq!(ctx.nft_client.get_admin().unwrap(), ctx.admin);
    assert_eq!(ctx.nft_client.total_supply().unwrap(), 0);

    // Verify other contracts exist and have different addresses
    assert_ne!(ctx.nft_client.address, ctx.token_address);
    assert_ne!(ctx.nft_client.address, ctx.shipment_address);
    assert_ne!(ctx.token_address, ctx.shipment_address);

    // Test calling token contract
    let token_symbol: String = ctx.env.invoke_contract(
        &ctx.token_address,
        &Symbol::new(&ctx.env, "symbol"),
        soroban_sdk::vec![&ctx.env],
    );
    assert_eq!(token_symbol, String::from_str(&ctx.env, "MOCK"));

    // Test calling shipment contract
    let shipment_counter: u64 = ctx.env.invoke_contract(
        &ctx.shipment_address,
        &Symbol::new(&ctx.env, "get_shipment_counter"),
        soroban_sdk::vec![&ctx.env],
    );
    assert_eq!(shipment_counter, 1);
}

/// Test NFT operations in multi-contract environment
#[test]
fn test_nft_operations_multi_contract() {
    let ctx = setup_multi_contract_env();

    // Mint an NFT
    let shipment_id = 42u64;
    let data_hash = soroban_sdk::BytesN::from_array(&ctx.env, &[1u8; 32]);
    let metadata = Map::new(&ctx.env);

    let token_id = ctx
        .nft_client
        .mint_shipment_nft(&ctx.user, &shipment_id, &data_hash, &metadata)
        .unwrap();

    // Verify NFT was minted correctly
    assert_eq!(token_id, 1);
    assert_eq!(ctx.nft_client.owner_of(&token_id).unwrap(), ctx.user);
    assert_eq!(
        ctx.nft_client.get_shipment_id(&token_id).unwrap(),
        shipment_id
    );
    assert_eq!(ctx.nft_client.get_data_hash(&token_id).unwrap(), data_hash);

    // Test NFT transfer
    let new_owner = Address::generate(&ctx.env);
    ctx.nft_client
        .transfer(&ctx.user, &new_owner, &token_id)
        .unwrap();

    assert_eq!(ctx.nft_client.owner_of(&token_id).unwrap(), new_owner);

    // Verify other contracts still work after NFT operations
    let token_symbol: String = ctx.env.invoke_contract(
        &ctx.token_address,
        &Symbol::new(&ctx.env, "symbol"),
        soroban_sdk::vec![&ctx.env],
    );
    assert_eq!(token_symbol, String::from_str(&ctx.env, "MOCK"));
}

/// Test NFT contract state isolation from other contracts
#[test]
fn test_nft_state_isolation() {
    let ctx = setup_multi_contract_env();

    // Mint multiple NFTs
    for i in 1..=3 {
        let shipment_id = i as u64;
        let data_hash = soroban_sdk::BytesN::from_array(&ctx.env, &[i; 32]);
        let metadata = Map::new(&ctx.env);

        ctx.nft_client
            .mint_shipment_nft(&ctx.user, &shipment_id, &data_hash, &metadata)
            .unwrap();
    }

    // Verify NFT contract state
    assert_eq!(ctx.nft_client.total_supply().unwrap(), 3);

    // Verify each NFT has correct data
    for i in 1..=3 {
        let token_id = i as u64;
        assert_eq!(ctx.nft_client.owner_of(&token_id).unwrap(), ctx.user);
        assert_eq!(
            ctx.nft_client.get_shipment_id(&token_id).unwrap(),
            token_id
        );

        let expected_hash = soroban_sdk::BytesN::from_array(&ctx.env, &[i; 32]);
        assert_eq!(
            ctx.nft_client.get_data_hash(&token_id).unwrap(),
            expected_hash
        );
    }

    // Verify other contracts' state is unaffected
    let shipment_counter: u64 = ctx.env.invoke_contract(
        &ctx.shipment_address,
        &Symbol::new(&ctx.env, "get_shipment_counter"),
        soroban_sdk::vec![&ctx.env],
    );
    assert_eq!(shipment_counter, 1); // Mock value, unchanged
}

/// Test error handling in multi-contract environment
#[test]
fn test_error_handling_multi_contract() {
    let ctx = setup_multi_contract_env();

    // Test NFT error doesn't affect other contracts
    let result = ctx.nft_client.try_owner_of(&999); // Non-existent token
    assert!(result.is_err());

    // Verify other contracts still work after NFT error
    let token_symbol: String = ctx.env.invoke_contract(
        &ctx.token_address,
        &Symbol::new(&ctx.env, "symbol"),
        soroban_sdk::vec![&ctx.env],
    );
    assert_eq!(token_symbol, String::from_str(&ctx.env, "MOCK"));
}

/// Smoke test for cross-contract calls from NFT contract perspective
#[test]
fn test_nft_calling_other_contracts() {
    let ctx = setup_multi_contract_env();

    // NFT contract making calls to other contracts
    // (This simulates future functionality where NFT might query shipment status)

    // Call token contract from test context (simulating cross-contract call)
    let token_decimals: u32 = ctx.env.invoke_contract(
        &ctx.token_address,
        &Symbol::new(&ctx.env, "decimals"),
        soroban_sdk::vec![&ctx.env],
    );
    assert_eq!(token_decimals, 7);

    // Call shipment contract from test context
    let shipment_counter: u64 = ctx.env.invoke_contract(
        &ctx.shipment_address,
        &Symbol::new(&ctx.env, "get_shipment_counter"),
        soroban_sdk::vec![&ctx.env],
    );
    assert_eq!(shipment_counter, 1);

    // Verify NFT contract still works after making external calls
    assert!(ctx.nft_client.is_initialized());
    assert_eq!(ctx.nft_client.total_supply().unwrap(), 0);
}

/// Test that NFT contract can be deployed multiple times independently
#[test]
fn test_multiple_nft_instances() {
    let env = Env::default();
    env.mock_all_auths();

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    // Deploy first NFT contract
    let nft1_addr = env.register(NavinShipmentNft, ());
    let nft1_client = NavinShipmentNftClient::new(&env, &nft1_addr);
    nft1_client
        .initialize(
            &admin1,
            &String::from_str(&env, "NFT Collection 1"),
            &String::from_str(&env, "NFT1"),
        )
        .unwrap();

    // Deploy second NFT contract
    let nft2_addr = env.register(NavinShipmentNft, ());
    let nft2_client = NavinShipmentNftClient::new(&env, &nft2_addr);
    nft2_client
        .initialize(
            &admin2,
            &String::from_str(&env, "NFT Collection 2"),
            &String::from_str(&env, "NFT2"),
        )
        .unwrap();

    // Verify contracts are independent
    assert_ne!(nft1_addr, nft2_addr);
    assert_ne!(
        nft1_client.get_admin().unwrap(),
        nft2_client.get_admin().unwrap()
    );
    assert_ne!(nft1_client.name().unwrap(), nft2_client.name().unwrap());

    // Operations on one contract don't affect the other
    let data_hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
    let metadata = Map::new(&env);

    nft1_client
        .mint_shipment_nft(&admin1, &1, &data_hash, &metadata)
        .unwrap();

    assert_eq!(nft1_client.total_supply().unwrap(), 1);
    assert_eq!(nft2_client.total_supply().unwrap(), 0);
}