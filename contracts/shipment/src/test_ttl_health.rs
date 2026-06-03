extern crate std;

use crate::{NavinShipment, NavinShipmentClient};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{storage::Persistent, Address as _, Ledger},
    Address, BytesN, Env,
};

#[contract]
struct TtlMockToken;

#[contractimpl]
impl TtlMockToken {
    pub fn decimals(_env: soroban_sdk::Env) -> u32 {
        7
    }
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

fn setup_shipment_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = crate::test_utils::setup_env();
    let token = env.register(TtlMockToken {}, ());
    let cid = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &cid);
    client.initialize(&admin, &token);
    (env, client, admin, token)
}

fn create_test_shipment(
    client: &NavinShipmentClient,
    env: &Env,
    company: &Address,
    carrier: &Address,
    hash_bytes: u8,
) -> u64 {
    let receiver = Address::generate(env);
    let data_hash = BytesN::from_array(env, &[hash_bytes; 32]);
    let deadline = env.ledger().timestamp() + 86400;

    client.create_shipment(
        company,
        &receiver,
        carrier,
        &data_hash,
        &soroban_sdk::Vec::new(env),
        &deadline,
    )
}

#[test]
fn test_ttl_health_summary_no_shipments() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    let health = client.get_status_summary();

    assert_eq!(health.created, 0);
    assert_eq!(health.in_transit, 0);
    assert_eq!(health.at_checkpoint, 0);
    assert_eq!(health.partially_delivered, 0);
    assert_eq!(health.delivered, 0);
    assert_eq!(health.disputed, 0);
    assert_eq!(health.cancelled, 0);
}

#[test]
fn test_ttl_health_summary_single_shipment() {
    let (env, client, admin, _token_contract) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    create_test_shipment(&client, &env, &company, &carrier, 0);

    let health = client.get_status_summary();

    assert_eq!(health.created, 1);
    assert_eq!(health.in_transit, 0);
    assert_eq!(health.delivered, 0);
}

#[test]
fn test_ttl_health_summary_multiple_shipments() {
    let (env, client, admin, _token_contract) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    for i in 0..5u8 {
        create_test_shipment(&client, &env, &company, &carrier, i);
    }

    let health = client.get_status_summary();

    assert_eq!(health.created, 5);
    assert_eq!(health.in_transit, 0);
    assert_eq!(health.delivered, 0);
}

#[test]
fn test_ttl_health_summary_deterministic() {
    let (env, client, admin, _token_contract) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    for i in 0..10u8 {
        create_test_shipment(&client, &env, &company, &carrier, i);
    }

    let health1 = client.get_status_summary();
    let health2 = client.get_status_summary();

    assert_eq!(health1.created, health2.created);
    assert_eq!(health1.in_transit, health2.in_transit);
    assert_eq!(health1.delivered, health2.delivered);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_ttl_health_summary_not_initialized() {
    let (env, _client, _admin, _token_contract) = setup_shipment_env();
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));

    client.get_status_summary();
}

#[test]
fn test_ttl_health_summary_edge_case_exactly_20_shipments() {
    let (env, client, admin, _token_contract) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    for i in 0..20u8 {
        create_test_shipment(&client, &env, &company, &carrier, i);
    }

    let health = client.get_status_summary();

    assert_eq!(health.created, 20);
    assert_eq!(health.in_transit, 0);
}

#[test]
#[ignore = "pre-existing failure from #377: advancing sequence archives the contract instance"]
fn test_ttl_extended_on_active_mutation() {
    let (env, client, admin, _token) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let receiver = Address::generate(&env);
    let create_hash = BytesN::from_array(&env, &[0x01u8; 32]);
    let deadline = env.ledger().timestamp() + 86_400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &create_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let ttl_after_create = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().get_ttl(&key)
    });
    assert!(
        ttl_after_create >= 518_400,
        "TTL must be set to at least shipment_ttl_extension on creation"
    );

    env.ledger().with_mut(|l| {
        l.sequence_number += 1_000;
        l.timestamp += 61;
    });

    let update_hash = BytesN::from_array(&env, &[0x02u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &crate::ShipmentStatus::InTransit,
        &update_hash,
    );

    let ttl_after_update = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().get_ttl(&key)
    });
    assert!(
        ttl_after_update >= 518_400,
        "TTL must be refreshed to at least shipment_ttl_extension after update_status"
    );
}

#[test]
fn test_ttl_not_extended_for_archived_terminal_shipment() {
    let (env, client, admin, _token) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let receiver = Address::generate(&env);
    let create_hash = BytesN::from_array(&env, &[0x03u8; 32]);
    let deadline = env.ledger().timestamp() + 86_400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &create_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let present_before = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().has(&key)
    });
    assert!(
        present_before,
        "Shipment must be in persistent storage after creation"
    );

    let reason_hash = BytesN::from_array(&env, &[0x04u8; 32]);
    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    client.archive_shipment(&admin, &shipment_id);

    let present_after_archive = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().has(&key)
    });
    assert!(
        !present_after_archive,
        "Archived terminal shipment must not remain in persistent storage"
    );
}

// ── TTL pre-flight boundary tests (issue #17) ────────────────────────────────────

/// Test TTL extension behavior at the exact threshold boundary.
/// When TTL equals the threshold, extension should be triggered.
#[test]
fn test_ttl_extension_at_threshold_boundary() {
    let (env, client, admin, _token) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let receiver = Address::generate(&env);
    let create_hash = BytesN::from_array(&env, &[0x10u8; 32]);
    let deadline = env.ledger().timestamp() + 86_400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &create_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // Get initial TTL
    let initial_ttl = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().get_ttl(&key)
    });

    // Advance time to bring TTL close to threshold
    // The threshold used in extend_shipment_ttl is typically 100 ledgers
    env.ledger().with_mut(|l| {
        l.sequence_number += initial_ttl - 100;
        l.timestamp += 60;
    });

    // Check TTL before extension
    let ttl_before = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().get_ttl(&key)
    });

    // Perform an action that triggers TTL extension
    let update_hash = BytesN::from_array(&env, &[0x11u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &crate::ShipmentStatus::InTransit,
        &update_hash,
    );

    // Check TTL after extension - should be refreshed
    let ttl_after = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().get_ttl(&key)
    });

    assert!(
        ttl_after > ttl_before,
        "TTL must be extended when at or below threshold"
    );
    assert!(
        ttl_after >= 518_400,
        "TTL after extension must meet minimum requirement"
    );
}

/// Test TTL extension behavior just below the threshold.
/// When TTL is just below threshold, extension should definitely trigger.
#[test]
fn test_ttl_extension_just_below_threshold() {
    let (env, client, admin, _token) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let receiver = Address::generate(&env);
    let create_hash = BytesN::from_array(&env, &[0x20u8; 32]);
    let deadline = env.ledger().timestamp() + 86_400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &create_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // Get initial TTL
    let initial_ttl = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().get_ttl(&key)
    });

    // Advance time to bring TTL just below threshold (99 ledgers remaining)
    env.ledger().with_mut(|l| {
        l.sequence_number += initial_ttl - 99;
        l.timestamp += 60;
    });

    // Check TTL before extension
    let ttl_before = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().get_ttl(&key)
    });

    // Perform an action that triggers TTL extension
    let update_hash = BytesN::from_array(&env, &[0x21u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &crate::ShipmentStatus::InTransit,
        &update_hash,
    );

    // Check TTL after extension - should be refreshed
    let ttl_after = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().get_ttl(&key)
    });

    assert!(
        ttl_after > ttl_before,
        "TTL must be extended when just below threshold"
    );
    assert!(
        ttl_after >= 518_400,
        "TTL after extension must meet minimum requirement"
    );
}

/// Test TTL extension behavior just above the threshold.
/// When TTL is just above threshold, extension should not trigger.
#[test]
fn test_ttl_extension_just_above_threshold() {
    let (env, client, admin, _token) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let receiver = Address::generate(&env);
    let create_hash = BytesN::from_array(&env, &[0x30u8; 32]);
    let deadline = env.ledger().timestamp() + 86_400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &create_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // Get initial TTL
    let initial_ttl = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().get_ttl(&key)
    });

    // Advance time to bring TTL just above threshold (101 ledgers remaining)
    env.ledger().with_mut(|l| {
        l.sequence_number += initial_ttl - 101;
        l.timestamp += 60;
    });

    // Check TTL before extension attempt
    let ttl_before = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().get_ttl(&key)
    });

    // Perform an action - TTL should NOT be extended since we're above threshold
    let update_hash = BytesN::from_array(&env, &[0x31u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &crate::ShipmentStatus::InTransit,
        &update_hash,
    );

    // Check TTL after - should be unchanged or only slightly decreased
    let ttl_after = env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        env.storage().persistent().get_ttl(&key)
    });

    // When above threshold, TTL should not be extended
    // It may decrease slightly due to ledger advancement, but not jump up
    assert!(
        ttl_after <= ttl_before + 10,
        "TTL should not be extended when just above threshold"
    );
}

/// Verify that health output matches actual storage state at TTL boundaries.
#[test]
fn test_ttl_health_output_matches_storage_state() {
    let (env, client, admin, _token) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Create shipments and verify health matches storage
    for i in 0..3u8 {
        create_test_shipment(&client, &env, &company, &carrier, i);
    }

    let health = client.get_status_summary();

    // Verify health output matches actual storage state
    assert_eq!(health.created, 3);

    // Verify by checking storage directly
    env.as_contract(&client.address, || {
        let created_count = crate::storage::get_status_count(&env, &crate::ShipmentStatus::Created);
        assert_eq!(
            created_count, 3,
            "Health output must match actual storage state"
        );
    });
}
