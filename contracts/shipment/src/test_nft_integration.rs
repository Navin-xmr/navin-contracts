#![cfg(test)]
//! # NFT Integration Tests
//!
//! Cross-contract test suite that exercises the NavinShipmentNft contract
//! alongside the shipment contract to verify:
//!
//! - NFT contract can be deployed and initialized independently
//! - Shipment contract can register and use NFT contract
//! - Automatic NFT minting works when shipments are created
//! - NFT contract coexists with token and shipment contracts
//! - Cross-contract calls succeed in integrated environment

use crate::{test_utils, types::ShipmentStatus, NavinShipment, NavinShipmentClient};
use navin_nft::{NavinShipmentNft, NavinShipmentNftClient};
use navin_token::{NavinToken, NavinTokenClient};
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, BytesN, Env, Map, String, Symbol, Vec,
};

struct TestContext {
    env: Env,
    admin: Address,
    company: Address,
    carrier: Address,
    receiver: Address,
    shipment_client: NavinShipmentClient<'static>,
    nft_client: NavinShipmentNftClient<'static>,
    token_client: NavinTokenClient<'static>,
}

fn setup_integrated_environment() -> TestContext {
    let (env, admin) = test_utils::setup_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    // Deploy and initialize token contract
    let token_addr = env.register(NavinToken, ());
    let token_client = NavinTokenClient::new(&env, &token_addr);
    token_client
        .initialize(
            &admin,
            &String::from_str(&env, "Test Token"),
            &String::from_str(&env, "TEST"),
            &1_000_000_000,
        )
        .unwrap();

    // Deploy and initialize shipment contract
    let shipment_addr = env.register(NavinShipment, ());
    let shipment_client = NavinShipmentClient::new(&env, &shipment_addr);
    shipment_client.initialize(&admin, &token_addr).unwrap();

    // Deploy and initialize NFT contract
    let nft_addr = env.register(NavinShipmentNft, ());
    let nft_client = NavinShipmentNftClient::new(&env, &nft_addr);
    nft_client
        .initialize(
            &admin,
            &String::from_str(&env, "Navin Shipment NFTs"),
            &String::from_str(&env, "NSN"),
        )
        .unwrap();

    // Set up roles in shipment contract
    shipment_client.add_company(&admin, &company).unwrap();
    shipment_client.add_carrier(&admin, &carrier).unwrap();
    shipment_client
        .add_carrier_to_whitelist(&company, &carrier)
        .unwrap();

    TestContext {
        env,
        admin,
        company,
        carrier,
        receiver,
        shipment_client,
        nft_client,
        token_client,
    }
}

fn dummy_hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

/// Test that all three contracts can coexist and be called independently
#[test]
fn test_contracts_coexist_independently() {
    let ctx = setup_integrated_environment();

    // Test token contract functionality
    assert_eq!(ctx.token_client.decimals().unwrap(), 7);
    assert_eq!(
        ctx.token_client.symbol().unwrap(),
        String::from_str(&ctx.env, "TEST")
    );

    // Test NFT contract functionality
    assert_eq!(
        ctx.nft_client.name().unwrap(),
        String::from_str(&ctx.env, "Navin Shipment NFTs")
    );
    assert_eq!(
        ctx.nft_client.symbol().unwrap(),
        String::from_str(&ctx.env, "NSN")
    );
    assert_eq!(ctx.nft_client.total_supply().unwrap(), 0);

    // Test shipment contract functionality
    assert_eq!(ctx.shipment_client.get_admin().unwrap(), ctx.admin);
    assert_eq!(ctx.shipment_client.get_shipment_counter().unwrap(), 0);

    // Verify contracts have different addresses
    assert_ne!(ctx.shipment_client.address, ctx.nft_client.address);
    assert_ne!(ctx.shipment_client.address, ctx.token_client.address);
    assert_ne!(ctx.nft_client.address, ctx.token_client.address);
}

/// Test setting NFT contract in shipment contract
#[test]
fn test_shipment_nft_configuration() {
    let ctx = setup_integrated_environment();

    // Initially no NFT contract is set
    assert!(ctx.shipment_client.get_nft_contract().unwrap().is_none());

    // Set NFT contract
    ctx.shipment_client
        .set_nft_contract(&ctx.admin, &ctx.nft_client.address)
        .unwrap();

    // Verify NFT contract is set
    let nft_address = ctx.shipment_client.get_nft_contract().unwrap();
    assert!(nft_address.is_some());
    assert_eq!(nft_address.unwrap(), ctx.nft_client.address);

    // Clear NFT contract
    ctx.shipment_client.clear_nft_contract(&ctx.admin).unwrap();

    // Verify NFT contract is cleared
    assert!(ctx.shipment_client.get_nft_contract().unwrap().is_none());
}

/// Test NFT auto-minting configuration
#[test]
fn test_nft_auto_mint_configuration() {
    let ctx = setup_integrated_environment();

    // Set NFT contract and enable auto-minting
    ctx.shipment_client
        .set_nft_contract(&ctx.admin, &ctx.nft_client.address)
        .unwrap();

    ctx.shipment_client
        .set_auto_mint_nft(&ctx.admin, &true)
        .unwrap();

    // Create a shipment to test auto-minting
    let deadline = ctx.env.ledger().timestamp() + 3600;
    let data_hash = dummy_hash(&ctx.env, 1);

    let shipment_id = ctx
        .shipment_client
        .create_shipment(
            &ctx.company,
            &ctx.receiver,
            &ctx.carrier,
            &data_hash,
            &Vec::new(&ctx.env),
            &deadline,
        )
        .unwrap();

    // Verify shipment was created
    assert_eq!(shipment_id, 1);
    let shipment = ctx.shipment_client.get_shipment(&shipment_id).unwrap();
    assert_eq!(shipment.status, ShipmentStatus::Created);

    // Verify NFT was minted automatically
    assert_eq!(ctx.nft_client.total_supply().unwrap(), 1);

    // Verify NFT properties
    let nft_owner = ctx.nft_client.owner_of(&1).unwrap();
    assert_eq!(nft_owner, ctx.company);

    let nft_shipment_id = ctx.nft_client.get_shipment_id(&1).unwrap();
    assert_eq!(nft_shipment_id, shipment_id);

    let nft_data_hash = ctx.nft_client.get_data_hash(&1).unwrap();
    assert_eq!(nft_data_hash, data_hash);
}

/// Test batch shipment creation with NFT minting
#[test]
fn test_batch_shipment_creation_with_nft_minting() {
    let ctx = setup_integrated_environment();

    // Configure NFT auto-minting
    ctx.shipment_client
        .set_nft_contract(&ctx.admin, &ctx.nft_client.address)
        .unwrap();

    ctx.shipment_client
        .set_auto_mint_nft(&ctx.admin, &true)
        .unwrap();

    // Create batch of shipments
    let deadline = ctx.env.ledger().timestamp() + 3600;
    let mut shipments = Vec::new(&ctx.env);

    for i in 1u8..=3 {
        let shipment_input = crate::types::ShipmentInput {
            receiver: Address::generate(&ctx.env),
            carrier: ctx.carrier.clone(),
            data_hash: dummy_hash(&ctx.env, i),
            payment_milestones: Vec::new(&ctx.env),
            deadline,
        };
        shipments.push_back(shipment_input);
    }

    let shipment_ids = ctx
        .shipment_client
        .create_shipments_batch(&ctx.company, &shipments)
        .unwrap();

    // Verify all shipments were created
    assert_eq!(shipment_ids.len(), 3);

    // Verify all NFTs were minted
    assert_eq!(ctx.nft_client.total_supply().unwrap(), 3);

    // Verify each NFT corresponds to its shipment
    for (i, shipment_id) in shipment_ids.iter().enumerate() {
        let token_id = (i + 1) as u64;
        
        let nft_owner = ctx.nft_client.owner_of(&token_id).unwrap();
        assert_eq!(nft_owner, ctx.company);

        let nft_shipment_id = ctx.nft_client.get_shipment_id(&token_id).unwrap();
        assert_eq!(nft_shipment_id, shipment_id);
    }
}

/// Test NFT functionality without auto-minting (manual NFT operations)
#[test]
fn test_manual_nft_operations_with_shipment() {
    let ctx = setup_integrated_environment();

    // Create shipment without NFT auto-minting (disabled by default)
    let deadline = ctx.env.ledger().timestamp() + 3600;
    let data_hash = dummy_hash(&ctx.env, 1);

    let shipment_id = ctx
        .shipment_client
        .create_shipment(
            &ctx.company,
            &ctx.receiver,
            &ctx.carrier,
            &data_hash,
            &Vec::new(&ctx.env),
            &deadline,
        )
        .unwrap();

    // Verify no NFT was minted automatically
    assert_eq!(ctx.nft_client.total_supply().unwrap(), 0);

    // Manually mint an NFT for the shipment
    let metadata = Map::new(&ctx.env);
    let token_id = ctx
        .nft_client
        .mint_shipment_nft(&ctx.company, &shipment_id, &data_hash, &metadata)
        .unwrap();

    // Verify manual NFT minting worked
    assert_eq!(token_id, 1);
    assert_eq!(ctx.nft_client.total_supply().unwrap(), 1);

    let nft_owner = ctx.nft_client.owner_of(&token_id).unwrap();
    assert_eq!(nft_owner, ctx.company);

    // Transfer NFT to receiver
    ctx.nft_client
        .transfer(&ctx.company, &ctx.receiver, &token_id)
        .unwrap();

    // Verify transfer worked
    let new_owner = ctx.nft_client.owner_of(&token_id).unwrap();
    assert_eq!(new_owner, ctx.receiver);
}

/// Test error handling when NFT contract is not properly configured
#[test]
fn test_nft_error_handling() {
    let ctx = setup_integrated_environment();

    // Enable auto-minting without setting NFT contract address
    ctx.shipment_client
        .set_auto_mint_nft(&ctx.admin, &true)
        .unwrap();

    // Create shipment - should succeed even though NFT minting will fail
    let deadline = ctx.env.ledger().timestamp() + 3600;
    let data_hash = dummy_hash(&ctx.env, 1);

    let shipment_id = ctx
        .shipment_client
        .create_shipment(
            &ctx.company,
            &ctx.receiver,
            &ctx.carrier,
            &data_hash,
            &Vec::new(&ctx.env),
            &deadline,
        )
        .unwrap();

    // Verify shipment was created despite NFT minting failure
    assert_eq!(shipment_id, 1);
    let shipment = ctx.shipment_client.get_shipment(&shipment_id).unwrap();
    assert_eq!(shipment.status, ShipmentStatus::Created);

    // Verify no NFT was minted
    assert_eq!(ctx.nft_client.total_supply().unwrap(), 0);
}

/// Test NFT events are emitted correctly during integration
#[test]
fn test_nft_integration_events() {
    let ctx = setup_integrated_environment();

    // Configure NFT integration
    ctx.shipment_client
        .set_nft_contract(&ctx.admin, &ctx.nft_client.address)
        .unwrap();

    ctx.shipment_client
        .set_auto_mint_nft(&ctx.admin, &true)
        .unwrap();

    // Get initial event count
    let events_before = ctx.env.events().all().len();

    // Create shipment
    let deadline = ctx.env.ledger().timestamp() + 3600;
    let data_hash = dummy_hash(&ctx.env, 1);

    ctx.shipment_client
        .create_shipment(
            &ctx.company,
            &ctx.receiver,
            &ctx.carrier,
            &data_hash,
            &Vec::new(&ctx.env),
            &deadline,
        )
        .unwrap();

    // Verify events were emitted
    let events_after = ctx.env.events().all();
    assert!(events_after.len() > events_before);

    // Look for NFT minting success event
    let nft_mint_events: Vec<_> = events_after
        .iter()
        .filter(|e| {
            if let Ok(topic) = Symbol::try_from_val(&ctx.env, &e.1.get(0).unwrap_or_default()) {
                topic == Symbol::new(&ctx.env, "nft_mint")
            } else {
                false
            }
        })
        .collect();

    assert!(!nft_mint_events.is_empty(), "NFT mint event should be emitted");
}

/// Test that NFT contract can be upgraded independently
#[test]
fn test_nft_contract_independence() {
    let ctx = setup_integrated_environment();

    // Set up initial NFT integration
    ctx.shipment_client
        .set_nft_contract(&ctx.admin, &ctx.nft_client.address)
        .unwrap();

    // Deploy a second NFT contract instance (simulating upgrade)
    let nft_addr_v2 = ctx.env.register(NavinShipmentNft, ());
    let nft_client_v2 = NavinShipmentNftClient::new(&ctx.env, &nft_addr_v2);
    nft_client_v2
        .initialize(
            &ctx.admin,
            &String::from_str(&ctx.env, "Navin Shipment NFTs v2"),
            &String::from_str(&ctx.env, "NSNv2"),
        )
        .unwrap();

    // Switch to new NFT contract
    ctx.shipment_client
        .set_nft_contract(&ctx.admin, &nft_addr_v2)
        .unwrap();

    // Verify new NFT contract is used
    let nft_address = ctx.shipment_client.get_nft_contract().unwrap();
    assert_eq!(nft_address.unwrap(), nft_addr_v2);

    // Verify old and new NFT contracts are independent
    assert_eq!(ctx.nft_client.total_supply().unwrap(), 0);
    assert_eq!(nft_client_v2.total_supply().unwrap(), 0);
    assert_ne!(
        ctx.nft_client.name().unwrap(),
        nft_client_v2.name().unwrap()
    );
}

/// Test unauthorized access to NFT configuration functions
#[test]
fn test_nft_configuration_authorization() {
    let ctx = setup_integrated_environment();
    let unauthorized = Address::generate(&ctx.env);

    // Test unauthorized set_nft_contract
    let result = ctx
        .shipment_client
        .try_set_nft_contract(&unauthorized, &ctx.nft_client.address);
    assert!(result.is_err());

    // Test unauthorized clear_nft_contract
    let result = ctx.shipment_client.try_clear_nft_contract(&unauthorized);
    assert!(result.is_err());

    // Test unauthorized set_auto_mint_nft
    let result = ctx.shipment_client.try_set_auto_mint_nft(&unauthorized, &true);
    assert!(result.is_err());
}

/// Integration test demonstrating the complete workflow
#[test]
fn test_complete_shipment_nft_workflow() {
    let ctx = setup_integrated_environment();

    // 1. Configure NFT integration
    ctx.shipment_client
        .set_nft_contract(&ctx.admin, &ctx.nft_client.address)
        .unwrap();

    ctx.shipment_client
        .set_auto_mint_nft(&ctx.admin, &true)
        .unwrap();

    // 2. Create shipment (with automatic NFT minting)
    let deadline = ctx.env.ledger().timestamp() + 3600;
    let data_hash = dummy_hash(&ctx.env, 42);

    let shipment_id = ctx
        .shipment_client
        .create_shipment(
            &ctx.company,
            &ctx.receiver,
            &ctx.carrier,
            &data_hash,
            &Vec::new(&ctx.env),
            &deadline,
        )
        .unwrap();

    // 3. Verify shipment and NFT were created
    assert_eq!(shipment_id, 1);
    assert_eq!(ctx.nft_client.total_supply().unwrap(), 1);

    // 4. Verify NFT represents the shipment correctly
    let token_id = 1u64;
    assert_eq!(ctx.nft_client.owner_of(&token_id).unwrap(), ctx.company);
    assert_eq!(
        ctx.nft_client.get_shipment_id(&token_id).unwrap(),
        shipment_id
    );
    assert_eq!(ctx.nft_client.get_data_hash(&token_id).unwrap(), data_hash);

    // 5. Transfer NFT ownership
    ctx.nft_client
        .transfer(&ctx.company, &ctx.receiver, &token_id)
        .unwrap();

    // 6. Verify NFT ownership changed but shipment remains unchanged
    assert_eq!(ctx.nft_client.owner_of(&token_id).unwrap(), ctx.receiver);
    let shipment = ctx.shipment_client.get_shipment(&shipment_id).unwrap();
    assert_eq!(shipment.sender, ctx.company); // Shipment sender unchanged
    assert_eq!(shipment.receiver, ctx.receiver); // Shipment receiver unchanged

    // 7. Update shipment status
    test_utils::advance_past_rate_limit(&ctx.env);
    ctx.shipment_client
        .update_status(
            &ctx.carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &dummy_hash(&ctx.env, 43),
        )
        .unwrap();

    // 8. Verify shipment status changed but NFT remains valid
    let updated_shipment = ctx.shipment_client.get_shipment(&shipment_id).unwrap();
    assert_eq!(updated_shipment.status, ShipmentStatus::InTransit);

    // NFT still points to the same shipment
    assert_eq!(
        ctx.nft_client.get_shipment_id(&token_id).unwrap(),
        shipment_id
    );
}