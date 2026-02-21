#![cfg(test)]

extern crate std;

use crate::{GeofenceEvent, NavinShipment, NavinShipmentClient};
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, BytesN, Env,
};

fn setup_env() -> (Env, NavinShipmentClient<'static>, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    env.mock_all_auths();
    (env, client, admin)
}

#[test]
fn test_successful_initialization() {
    let (_env, client, admin) = setup_env();

    client.initialize(&admin);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_shipment_counter(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_re_initialization_fails() {
    let (_env, client, admin) = setup_env();

    client.initialize(&admin);
    // Second call must fail with AlreadyInitialized (error code 1)
    client.initialize(&admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_re_initialization_with_different_admin_fails() {
    let (env, client, admin) = setup_env();

    client.initialize(&admin);

    let other_admin = Address::generate(&env);
    // Attempting to re-initialize with a different admin must also fail
    client.initialize(&other_admin);
}

#[test]
fn test_shipment_counter_starts_at_zero() {
    let (_env, client, admin) = setup_env();

    client.initialize(&admin);

    assert_eq!(client.get_shipment_counter(), 0);
}

#[test]
fn test_admin_is_stored_correctly() {
    let (env, client, _admin) = setup_env();

    let specific_admin = Address::generate(&env);
    client.initialize(&specific_admin);

    let stored_admin = client.get_admin();
    assert_eq!(stored_admin, specific_admin);
}

#[test]
fn test_scaffold() {
    let env = Env::default();
    let _client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
}

#[test]
fn test_create_shipment_success() {
    let (env, client, admin) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);

    client.initialize(&admin);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(&company, &receiver, &carrier, &data_hash);
    assert_eq!(shipment_id, 1);
    assert_eq!(client.get_shipment_counter(), 1);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.id, shipment_id);
    assert_eq!(shipment.sender, company);
    assert_eq!(shipment.receiver, receiver);
    assert_eq!(shipment.carrier, carrier);
    assert_eq!(shipment.data_hash, data_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_create_shipment_unauthorized() {
    let (env, client, admin) = setup_env();
    let outsider = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[9u8; 32]);

    client.initialize(&admin);
    client.create_shipment(&outsider, &receiver, &carrier, &data_hash);
}

#[test]
fn test_multiple_shipments_have_unique_ids() {
    let (env, client, admin) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let hash_one = BytesN::from_array(&env, &[1u8; 32]);
    let hash_two = BytesN::from_array(&env, &[2u8; 32]);
    let hash_three = BytesN::from_array(&env, &[3u8; 32]);

    client.initialize(&admin);
    client.add_company(&admin, &company);

    let id_one = client.create_shipment(&company, &receiver, &carrier, &hash_one);
    let id_two = client.create_shipment(&company, &receiver, &carrier, &hash_two);
    let id_three = client.create_shipment(&company, &receiver, &carrier, &hash_three);

    assert_eq!(id_one, 1);
    assert_eq!(id_two, 2);
    assert_eq!(id_three, 3);
    assert_eq!(client.get_shipment_counter(), 3);
}

// ============= Carrier Whitelist Tests =============

#[test]
fn test_add_carrier_to_whitelist() {
    let (_env, client, admin) = setup_env();
    client.initialize(&admin);

    let company = Address::generate(&_env);
    let carrier = Address::generate(&_env);

    // Add carrier to whitelist
    client.add_carrier_to_whitelist(&company, &carrier);

    // Verify carrier is whitelisted
    assert!(client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_remove_carrier_from_whitelist() {
    let (_env, client, admin) = setup_env();
    client.initialize(&admin);

    let company = Address::generate(&_env);
    let carrier = Address::generate(&_env);

    // Add carrier to whitelist
    client.add_carrier_to_whitelist(&company, &carrier);
    assert!(client.is_carrier_whitelisted(&company, &carrier));

    // Remove carrier from whitelist
    client.remove_carrier_from_whitelist(&company, &carrier);

    // Verify carrier is no longer whitelisted
    assert!(!client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_is_carrier_whitelisted_returns_false_for_non_whitelisted() {
    let (_env, client, admin) = setup_env();
    client.initialize(&admin);

    let company = Address::generate(&_env);
    let carrier = Address::generate(&_env);

    // Verify carrier is not whitelisted by default
    assert!(!client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_multiple_carriers_whitelist() {
    let (_env, client, admin) = setup_env();
    client.initialize(&admin);

    let company = Address::generate(&_env);
    let carrier1 = Address::generate(&_env);
    let carrier2 = Address::generate(&_env);
    let carrier3 = Address::generate(&_env);

    // Add multiple carriers
    client.add_carrier_to_whitelist(&company, &carrier1);
    client.add_carrier_to_whitelist(&company, &carrier2);

    // Verify added carriers are whitelisted
    assert!(client.is_carrier_whitelisted(&company, &carrier1));
    assert!(client.is_carrier_whitelisted(&company, &carrier2));

    // Verify carrier3 is not whitelisted
    assert!(!client.is_carrier_whitelisted(&company, &carrier3));

    // Remove one carrier
    client.remove_carrier_from_whitelist(&company, &carrier1);

    // Verify carrier1 is removed but carrier2 is still whitelisted
    assert!(!client.is_carrier_whitelisted(&company, &carrier1));
    assert!(client.is_carrier_whitelisted(&company, &carrier2));
}

#[test]
fn test_whitelist_per_company() {
    let (_env, client, admin) = setup_env();
    client.initialize(&admin);

    let company1 = Address::generate(&_env);
    let company2 = Address::generate(&_env);
    let carrier = Address::generate(&_env);

    // Add carrier to company1's whitelist only
    client.add_carrier_to_whitelist(&company1, &carrier);

    // Verify carrier is whitelisted for company1 but not for company2
    assert!(client.is_carrier_whitelisted(&company1, &carrier));
    assert!(!client.is_carrier_whitelisted(&company2, &carrier));

    // Add same carrier to company2's whitelist
    client.add_carrier_to_whitelist(&company2, &carrier);

    // Verify carrier is now whitelisted for both companies
    assert!(client.is_carrier_whitelisted(&company1, &carrier));
    assert!(client.is_carrier_whitelisted(&company2, &carrier));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_whitelist_functions_fail_before_initialization() {
    let (_env, client, _admin) = setup_env();

    let company = Address::generate(&_env);
    let carrier = Address::generate(&_env);

    // Must fail with NotInitialized (error code 2)
    client.is_carrier_whitelisted(&company, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_add_whitelist_fails_before_initialization() {
    let (_env, client, _admin) = setup_env();

    let company = Address::generate(&_env);
    let carrier = Address::generate(&_env);

    // Must fail with NotInitialized (error code 2)
    client.add_carrier_to_whitelist(&company, &carrier);
}

// ============= Get Escrow Balance Tests =============

#[test]
fn test_get_escrow_balance_returns_zero_without_deposit() {
    let (env, client, admin) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(&company, &receiver, &carrier, &data_hash);

    // No escrow deposited yet, should return 0
    assert_eq!(client.get_escrow_balance(&shipment_id), 0);
}

#[test]
fn test_get_escrow_balance_after_deposit() {
    let (env, client, admin) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(&company, &receiver, &carrier, &data_hash);

    // Simulate a deposit by setting escrow balance directly
    env.as_contract(&client.address, || {
        crate::storage::set_escrow_balance(&env, shipment_id, 500_000);
    });

    assert_eq!(client.get_escrow_balance(&shipment_id), 500_000);
}

#[test]
fn test_get_escrow_balance_after_release() {
    let (env, client, admin) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[3u8; 32]);

    client.initialize(&admin);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(&company, &receiver, &carrier, &data_hash);

    // Simulate deposit then release
    env.as_contract(&client.address, || {
        crate::storage::set_escrow_balance(&env, shipment_id, 1_000_000);
    });
    assert_eq!(client.get_escrow_balance(&shipment_id), 1_000_000);

    env.as_contract(&client.address, || {
        crate::storage::remove_escrow_balance(&env, shipment_id);
    });

    // After release, should return 0
    assert_eq!(client.get_escrow_balance(&shipment_id), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_get_escrow_balance_shipment_not_found() {
    let (_env, client, admin) = setup_env();

    client.initialize(&admin);

    // Must fail with ShipmentNotFound (error code 6)
    client.get_escrow_balance(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_escrow_balance_fails_before_initialization() {
    let (_env, client, _admin) = setup_env();

    // Must fail with NotInitialized (error code 2)
    client.get_escrow_balance(&1);
}

// ============= Get Shipment Count Tests =============

#[test]
fn test_get_shipment_count_returns_zero_on_fresh_contract() {
    let (_env, client, _admin) = setup_env();

    // Returns 0 even without initialization
    assert_eq!(client.get_shipment_count(), 0);
}

#[test]
fn test_get_shipment_count_returns_zero_after_initialization() {
    let (_env, client, admin) = setup_env();

    client.initialize(&admin);

    assert_eq!(client.get_shipment_count(), 0);
}

#[test]
fn test_get_shipment_count_after_creating_shipments() {
    let (env, client, admin) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin);
    client.add_company(&admin, &company);

    // Count after 1 shipment
    let hash_one = BytesN::from_array(&env, &[1u8; 32]);
    client.create_shipment(&company, &receiver, &carrier, &hash_one);
    assert_eq!(client.get_shipment_count(), 1);

    // Count after 2 shipments
    let hash_two = BytesN::from_array(&env, &[2u8; 32]);
    client.create_shipment(&company, &receiver, &carrier, &hash_two);
    assert_eq!(client.get_shipment_count(), 2);

    // Count after 3 shipments
    let hash_three = BytesN::from_array(&env, &[3u8; 32]);
    client.create_shipment(&company, &receiver, &carrier, &hash_three);
    assert_eq!(client.get_shipment_count(), 3);
}

// ============= Get Shipment Tests =============

#[test]
fn test_get_shipment_returns_correct_data() {
    let (env, client, admin) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);

    client.initialize(&admin);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(&company, &receiver, &carrier, &data_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.id, shipment_id);
    assert_eq!(shipment.sender, company);
    assert_eq!(shipment.receiver, receiver);
    assert_eq!(shipment.carrier, carrier);
    assert_eq!(shipment.data_hash, data_hash);
    assert_eq!(shipment.status, crate::ShipmentStatus::Created);
    assert_eq!(shipment.escrow_amount, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_get_shipment_not_found() {
    let (_env, client, admin) = setup_env();

    client.initialize(&admin);

    // Must fail with ShipmentNotFound (error code 6)
    client.get_shipment(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_shipment_fails_before_initialization() {
    let (_env, client, _admin) = setup_env();

    // Must fail with NotInitialized (error code 2)
    client.get_shipment(&1);
}

// ============= Geofence Event Tests =============

#[test]
fn test_report_geofence_zone_entry() {
    let (env, client, admin) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let event_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(&company, &receiver, &carrier, &data_hash);

    // Report ZoneEntry
    client.report_geofence_event(
        &carrier,
        &shipment_id,
        &GeofenceEvent::ZoneEntry,
        &event_hash,
    );

    // Verify event was emitted (at least 1 geofence event)
    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
fn test_report_geofence_zone_exit() {
    let (env, client, admin) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let event_hash = BytesN::from_array(&env, &[3u8; 32]);

    client.initialize(&admin);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(&company, &receiver, &carrier, &data_hash);

    // Report ZoneExit
    client.report_geofence_event(
        &carrier,
        &shipment_id,
        &GeofenceEvent::ZoneExit,
        &event_hash,
    );

    // Verify event was emitted
    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
fn test_report_geofence_route_deviation() {
    let (env, client, admin) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let event_hash = BytesN::from_array(&env, &[4u8; 32]);

    client.initialize(&admin);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(&company, &receiver, &carrier, &data_hash);

    // Report RouteDeviation
    client.report_geofence_event(
        &carrier,
        &shipment_id,
        &GeofenceEvent::RouteDeviation,
        &event_hash,
    );

    // Verify event was emitted
    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_report_geofence_event_unauthorized_role() {
    let (env, client, admin) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let event_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin);
    client.add_company(&admin, &company);
    // Note: outsider NOT added as carrier

    let shipment_id = client.create_shipment(&company, &receiver, &carrier, &data_hash);

    // Attempt to report geofence event should fail with CarrierNotAuthorized (error code 7)
    client.report_geofence_event(
        &outsider,
        &shipment_id,
        &GeofenceEvent::ZoneEntry,
        &event_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_report_geofence_event_non_existent_shipment() {
    let (env, client, admin) = setup_env();
    let carrier = Address::generate(&env);
    let event_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin);
    client.add_carrier(&admin, &carrier);

    // Attempt to report for non-existent shipment should fail with ShipmentNotFound (error code 6)
    client.report_geofence_event(&carrier, &999, &GeofenceEvent::ZoneEntry, &event_hash);
}
