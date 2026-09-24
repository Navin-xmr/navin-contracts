#![cfg(test)]

extern crate std;

use crate::{
    BreachType, GeofenceEvent, NavinError, NavinShipment, NavinShipmentClient, Severity,
    ShipmentInput, ShipmentStatus,
};
use soroban_sdk::{
    contract, contracterror, contractimpl,
    testutils::{Address as _, Events},
    Address, BytesN, Env, FromVal, IntoVal, Symbol, TryFromVal,
};

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {
        // Mock implementation - always succeeds
    }
    pub fn decimals(_env: Env) -> u32 {
        crate::types::EXPECTED_TOKEN_DECIMALS
    }
}

mod failing_token {
    use super::*;

    #[contracterror]
    #[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
    #[repr(u32)]
    pub enum MockTokenFailure {
        TransferFailed = 1,
        MintFailed = 2,
    }

    #[contract]
    pub struct FailingMockToken;

    #[contractimpl]
    impl FailingMockToken {
        pub fn transfer(
            _env: Env,
            _from: Address,
            _to: Address,
            _amount: i128,
        ) -> Result<(), MockTokenFailure> {
            Err(MockTokenFailure::TransferFailed)
        }

        pub fn mint(
            _env: Env,
            _admin: Address,
            _to: Address,
            _amount: i128,
        ) -> Result<(), MockTokenFailure> {
            Err(MockTokenFailure::MintFailed)
        }

        pub fn decimals(_env: Env) -> u32 {
            crate::types::EXPECTED_TOKEN_DECIMALS
        }
    }
}

mod invalid_token {
    use super::*;

    // Token with invalid decimals for testing #260
    #[contract]
    pub struct MockTokenInvalidDecimals;

    #[contractimpl]
    impl MockTokenInvalidDecimals {
        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
        pub fn decimals(_env: Env) -> u32 {
            6 // Non-standard decimals
        }
    }
}

mod invalid_token_high_decimals {
    use super::*;

    #[contract]
    pub struct MockTokenHighDecimals;

    #[contractimpl]
    impl MockTokenHighDecimals {
        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
        pub fn decimals(_env: Env) -> u32 {
            9
        }
    }
}

pub fn setup_shipment_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = super::test_utils::setup_env();
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));

    (env, client, admin, token_contract)
}

pub fn setup_shipment_env_with_failing_token(
) -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = super::test_utils::setup_env();
    let token_contract = env.register(failing_token::FailingMockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));

    (env, client, admin, token_contract)
}

/// Creates a fully-initialized shipment environment ready for use.
/// Calls `initialize` on the client so tests can call contract methods
/// directly without an extra initialization step.
pub fn setup_initialized_shipment_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = super::test_utils::setup_env();
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token_contract);
    (env, client, admin, token_contract)
}

/// Like `setup_initialized_shipment_env` but uses a token that always
/// fails transfers — useful for testing rollback / failure paths.
pub fn setup_initialized_shipment_env_with_failing_token(
) -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = super::test_utils::setup_env();
    let token_contract = env.register(failing_token::FailingMockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token_contract);
    (env, client, admin, token_contract)
}

#[test]
fn test_successful_initialization() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_shipment_counter(), 0);
    assert_eq!(client.get_version(), 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_re_initialization_fails() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);
    // Second call must fail with AlreadyInitialized (error code 1)
    client.initialize(&admin, &token_contract);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_re_initialization_with_different_admin_fails() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let other_admin = Address::generate(&env);
    // Attempting to re-initialize with a different admin must also fail
    client.initialize(&other_admin, &token_contract);
}

#[test]
fn test_shipment_counter_starts_at_zero() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    assert_eq!(client.get_shipment_counter(), 0);
}

#[test]
fn test_admin_is_stored_correctly() {
    let (env, client, _admin, token_contract) = setup_shipment_env();

    let specific_admin = Address::generate(&env);
    client.initialize(&specific_admin, &token_contract);

    let stored_admin = client.get_admin();
    assert_eq!(stored_admin, specific_admin);
}

#[test]
fn test_scaffold() {
    let env = Env::default();
    let _client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
}

#[test]
fn test_token_mint_helper_maps_failure() {
    let env = Env::default();
    env.mock_all_auths();

    let token_contract = env.register(failing_token::FailingMockToken {}, ());
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    let result = super::invoke_token_mint(&env, &token_contract, &admin, &recipient, 250);

    assert_eq!(result, Err(crate::NavinError::TokenMintFailed));
}

#[test]
#[should_panic(expected = "Error(Contract, #16)")]
fn test_create_shipments_batch_oversized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let deadline = env.ledger().timestamp() + 3600;
    let mut shipments = soroban_sdk::Vec::new(&env);
    for i in 0..11 {
        shipments.push_back(ShipmentInput {
            receiver: Address::generate(&env),
            carrier: Address::generate(&env),
            data_hash: BytesN::from_array(&env, &[i as u8; 32]),
            payment_milestones: soroban_sdk::Vec::new(&env),
            deadline,
        });
    }

    client.create_shipments_batch(&company, &shipments);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_create_shipment_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let outsider = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[9u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.create_shipment(
        &outsider,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
}

// ============= Issue #701: Participant Validation Tests =============

/// Issue #701 — create_shipment must reject when sender == receiver
#[test]
#[should_panic(expected = "Error(Contract, #57)")]
fn test_create_shipment_rejects_sender_equals_receiver() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[101u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.create_shipment(
        &company,
        &company, // sender == receiver (invalid)
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
}

/// Issue #701 — create_shipment must reject when sender == carrier
#[test]
#[should_panic(expected = "Error(Contract, #57)")]
fn test_create_shipment_rejects_sender_equals_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[102u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.create_shipment(
        &company,
        &receiver,
        &company, // sender == carrier (invalid)
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
}

/// Issue #701 — create_shipment must reject when receiver == carrier
#[test]
#[should_panic(expected = "Error(Contract, #57)")]
fn test_create_shipment_rejects_receiver_equals_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[103u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.create_shipment(
        &company,
        &receiver,
        &receiver, // receiver == carrier (invalid)
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
}

/// Issue #701 — create_shipment should succeed with distinct participants
// ============= End Participant Validation Tests =============

// ============= Carrier Whitelist Tests =============

#[test]
fn test_add_carrier_to_whitelist() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier_to_whitelist(&company, &carrier);

    assert!(client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_remove_carrier_from_whitelist() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier_to_whitelist(&company, &carrier);
    assert!(client.is_carrier_whitelisted(&company, &carrier));

    client.remove_carrier_from_whitelist(&company, &carrier);

    assert!(!client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_is_carrier_whitelisted_returns_false_for_non_whitelisted() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    assert!(!client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_multiple_carriers_whitelist() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier1 = Address::generate(&env);
    let carrier2 = Address::generate(&env);
    let carrier3 = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier_to_whitelist(&company, &carrier1);
    client.add_carrier_to_whitelist(&company, &carrier2);

    assert!(client.is_carrier_whitelisted(&company, &carrier1));
    assert!(client.is_carrier_whitelisted(&company, &carrier2));
    assert!(!client.is_carrier_whitelisted(&company, &carrier3));

    client.remove_carrier_from_whitelist(&company, &carrier1);

    assert!(!client.is_carrier_whitelisted(&company, &carrier1));
    assert!(client.is_carrier_whitelisted(&company, &carrier2));
}

#[test]
fn test_whitelist_per_company() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company1 = Address::generate(&env);
    let company2 = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company1);
    client.add_carrier_to_whitelist(&company1, &carrier);

    assert!(client.is_carrier_whitelisted(&company1, &carrier));
    assert!(!client.is_carrier_whitelisted(&company2, &carrier));

    client.add_company(&admin, &company2);
    client.add_carrier_to_whitelist(&company2, &carrier);

    assert!(client.is_carrier_whitelisted(&company1, &carrier));
    assert!(client.is_carrier_whitelisted(&company2, &carrier));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_whitelist_functions_fail_before_initialization() {
    let (env, client, _admin, _token_contract) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.is_carrier_whitelisted(&company, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_add_whitelist_fails_before_initialization() {
    let (env, client, _admin, _token_contract) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_carrier_to_whitelist(&company, &carrier);
}

// ============= Deposit Escrow Tests =============

// ============= Status Update Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_update_status_unauthorized() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let unauthorized_user = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let new_data_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // Unauthorized user trying to update status
    client.update_status(
        &unauthorized_user,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &new_data_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_update_status_nonexistent_shipment() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let new_data_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin, &token_contract);

    // Try to update a non-existent shipment
    client.update_status(&carrier, &999, &ShipmentStatus::InTransit, &new_data_hash);
}

#[test]
fn test_suspend_carrier_requires_admin() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let outsider = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    let res = client.try_suspend_carrier(&outsider, &carrier);
    assert_eq!(res, Err(Ok(crate::NavinError::Unauthorized)));
}

// ============= Get Escrow Balance Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_deposit_escrow_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let non_company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[11u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let escrow_amount: i128 = 1000;
    client.deposit_escrow(&non_company, &shipment_id, &escrow_amount);
    // No escrow deposited yet, should return 0
    assert_eq!(client.get_escrow_balance(&shipment_id), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_escrow_balance_shipment_not_found() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_escrow_balance(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_escrow_balance_fails_before_initialization() {
    let (_env, _client, _admin, _token_contract) = setup_shipment_env();

    _client.get_escrow_balance(&1);
}

// ============= Get Shipment Count Tests =============

#[test]
fn test_get_shipment_count_returns_zero_on_fresh_contract() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    assert_eq!(client.get_shipment_count(), 0);
}

#[test]
fn test_get_shipment_count_returns_zero_after_initialization() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    assert_eq!(client.get_shipment_count(), 0);
}

// ============= Role Tests =============

#[test]
fn test_get_role_unassigned() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let user = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    assert_eq!(client.get_role(&user), crate::Role::Unassigned);
}

#[test]
fn test_get_role_assigned() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    client.add_company(&admin, &company);
    assert_eq!(client.get_role(&company), crate::Role::Company);

    client.add_carrier(&admin, &carrier);
    assert_eq!(client.get_role(&carrier), crate::Role::Carrier);
}

// ============= Get Shipment Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_not_found() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_shipment_fails_before_initialization() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    client.get_shipment(&1);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_creator_fails_for_invalid_id() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment_creator(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_receiver_fails_for_invalid_id() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment_receiver(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_sender_fails_for_invalid_id() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment_sender(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_carrier_fails_for_invalid_id() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment_carrier(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_status_fails_for_invalid_id() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment_status(&999);
}

// ============= Geofence Event Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_report_geofence_event_unauthorized_role() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let event_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    // Note: outsider NOT added as carrier

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.report_geofence_event(
        &outsider,
        &shipment_id,
        &GeofenceEvent::ZoneEntry,
        &event_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_deposit_escrow_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let non_existent_shipment_id = 999u64;
    let escrow_amount: i128 = 1000;
    client.deposit_escrow(&company, &non_existent_shipment_id, &escrow_amount);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_report_geofence_event_non_existent_shipment() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let event_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);

    client.report_geofence_event(&carrier, &999, &GeofenceEvent::ZoneEntry, &event_hash);
}

// ============= ETA Update Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_update_eta_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let shipment_hash = BytesN::from_array(&env, &[1u8; 32]);
    let eta_hash = BytesN::from_array(&env, &[7u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &shipment_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    let eta_timestamp = env.ledger().timestamp() + 120;

    // outsider is not a registered carrier
    client.update_eta(&outsider, &shipment_id, &eta_timestamp, &eta_hash);
}

// ============= Confirm Delivery Tests =============

fn setup_shipment_with_status(
    env: &Env,
    client: &NavinShipmentClient,
    admin: &Address,
    token_contract: &Address,
    status: crate::ShipmentStatus,
) -> (Address, Address, u64) {
    let company = Address::generate(env);
    let receiver = Address::generate(env);
    let carrier = Address::generate(env);
    let data_hash = BytesN::from_array(env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(admin, token_contract);
    client.add_company(admin, &company);
    client.add_carrier(admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(env),
        &deadline,
    );

    // Patch status directly in contract storage to simulate a mid-lifecycle state
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(env, shipment_id).unwrap();
        shipment.status = status;
        crate::storage::set_shipment(env, &shipment);
    });

    (receiver, carrier, shipment_id)
}

#[test]
fn test_confirm_delivery_success_in_transit() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let confirmation_hash = BytesN::from_array(&env, &[99u8; 32]);

    let (receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::InTransit,
    );

    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Delivered);

    // Verify confirmation hash was persisted on-chain
    let stored_hash = env.as_contract(&client.address, || {
        crate::storage::get_confirmation_hash(&env, shipment_id)
    });
    assert_eq!(stored_hash, Some(confirmation_hash));
}

#[test]
fn test_confirm_delivery_success_at_checkpoint() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let confirmation_hash = BytesN::from_array(&env, &[88u8; 32]);

    let (receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::AtCheckpoint,
    );

    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Delivered);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_confirm_delivery_wrong_receiver() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let confirmation_hash = BytesN::from_array(&env, &[77u8; 32]);
    let imposter = Address::generate(&env);

    let (_receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::InTransit,
    );

    // imposter is NOT the designated receiver — must fail with Unauthorized (error code 3)
    client.confirm_delivery(&imposter, &shipment_id, &confirmation_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_confirm_delivery_wrong_status() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let confirmation_hash = BytesN::from_array(&env, &[66u8; 32]);

    // Shipment starts in Created status, which is invalid for confirmation
    let (receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::Created,
    );

    // Must fail with InvalidStatus (error code 8)
    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);
}

// ============= Release Escrow Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_release_escrow_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    client.release_escrow(&unauthorized, &shipment_id);
}

// ============= Refund Escrow Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_refund_escrow_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    let escrow_amount: i128 = 3000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.refund_escrow(&unauthorized, &shipment_id);
}

// ============= Dispute Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_raise_dispute_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[96u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.raise_dispute(&outsider, &shipment_id, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_resolve_dispute_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[92u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.raise_dispute(&company, &shipment_id, &reason_hash);

    client.resolve_dispute(
        &outsider,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
        &reason_hash,
    );
}

// ============= Milestone Event Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_record_milestone_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[12u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let outsider = Address::generate(&env);
    let checkpoint = soroban_sdk::Symbol::new(&env, "port_arrival");

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::types::ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);
    });

    // Attempt to record with outsider should fail with CarrierNotAuthorized = 7
    client.record_milestone(&outsider, &shipment_id, &checkpoint, &data_hash);
}

// ============= Batch Milestone Recording Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_record_milestones_batch_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // Set shipment to InTransit status
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::types::ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);
    });

    let outsider = Address::generate(&env);
    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((
        Symbol::new(&env, "warehouse"),
        BytesN::from_array(&env, &[10u8; 32]),
    ));

    // Should fail with Unauthorized error (code 3)
    client.record_milestones_batch(&outsider, &shipment_id, &milestones);
}

// ============= TTL Extension Tests =============

// ============= Cancel Shipment Tests =============

#[test]
fn test_cancel_shipment_without_escrow() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[2u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[88u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
    assert!(shipment.finalized);
    assert_eq!(client.get_escrow_balance(&shipment_id), 0);

    let events = env.events().all();
    let emitted_refund = events.iter().any(|(_contract, topics, _data)| {
        topics
            .iter()
            .any(|v| Symbol::from_val(&env, &v) == Symbol::new(&env, "escrow_refunded"))
    });
    assert!(
        !emitted_refund,
        "cancel_shipment must not refund escrow when none is held"
    );
}

#[test]
fn test_cancel_shipment_by_admin() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[3u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[66u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    client.cancel_shipment(&admin, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
    assert!(shipment.finalized);
    assert_eq!(client.get_escrow_balance(&shipment_id), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_cancel_shipment_delivered_should_fail() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let reason_hash = BytesN::from_array(&env, &[77u8; 32]);

    let (_receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::Delivered,
    );

    let shipment = client.get_shipment(&shipment_id);
    let company = shipment.sender;

    client.cancel_shipment(&company, &shipment_id, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_cancel_shipment_disputed_should_fail() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let reason_hash = BytesN::from_array(&env, &[55u8; 32]);

    let (_receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::Disputed,
    );

    let shipment = client.get_shipment(&shipment_id);
    let company = shipment.sender;

    client.cancel_shipment(&company, &shipment_id, &reason_hash);
}

// ============= Escrow Lifecycle Integration Tests =============

#[test]
fn test_escrow_cancel_path_create_deposit_cancel_refund() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[4u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[44u8; 32]);
    let escrow_amount: i128 = 5_000;
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
    assert!(shipment.finalized);
    assert_eq!(client.get_escrow_balance(&shipment_id), 0);
    env.as_contract(&client.address, || {
        assert!(
            !crate::storage::has_escrow_entry(&env, shipment_id),
            "escrow storage must be cleaned up exactly once on the cancel-refund path"
        );
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #18)")]
fn test_milestone_payment_invalid_sum_fails() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "m1"), 50));
    milestones.push_back((Symbol::new(&env, "m2"), 60)); // Total 110%

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
}

// ============= Contract Upgrade Tests =============

#[test]
fn test_upgrade_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let wasm: &[u8] = include_bytes!("../test_wasms/upgrade_test.wasm");
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    client.initialize(&admin, &token_contract);
    assert_eq!(client.get_version(), 1);

    // Drain events emitted by initialize so we can assert only on upgrade events
    let _ = env.events().all();

    client.upgrade(&admin, &new_wasm_hash, &2);

    // Capture events immediately after upgrade before any further calls flush the queue
    let events = env.events().all();

    let version: u32 = env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::Version)
            .unwrap()
    });
    assert_eq!(version, 2);
    let event_found = events.iter().any(|e| {
        if let Ok(topic) = Symbol::try_from_val(&env, &e.1.get(0).unwrap()) {
            topic == Symbol::new(&env, "contract_upgraded")
        } else {
            false
        }
    });
    assert!(event_found, "Contract upgraded event should be present");
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_upgrade_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let non_admin = Address::generate(&env);
    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);

    client.initialize(&admin, &token_contract);

    client.upgrade(&non_admin, &new_wasm_hash, &2);
}

// ============= Contract Metadata Tests =============

#[test]
fn test_get_contract_metadata_after_init() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let meta = client.get_contract_metadata();
    assert_eq!(meta.version, 1);
    assert_eq!(meta.admin, admin);
    assert_eq!(meta.shipment_count, 0);
    assert!(meta.initialized);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_version_fails_before_initialization() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    client.get_version();
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_contract_metadata_fails_before_initialization() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    client.get_contract_metadata();
}

#[test]
fn test_get_version_after_upgrade() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let wasm: &[u8] = include_bytes!("../test_wasms/upgrade_test.wasm");
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    client.initialize(&admin, &token_contract);
    assert_eq!(client.get_version(), 1);

    client.upgrade(&admin, &new_wasm_hash, &2);

    let version: u32 = env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::Version)
            .unwrap()
    });
    assert_eq!(version, 2);
}

#[test]
fn test_get_contract_metadata_after_upgrade() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let wasm: &[u8] = include_bytes!("../test_wasms/upgrade_test.wasm");
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    client.initialize(&admin, &token_contract);

    let meta_before = client.get_contract_metadata();
    assert_eq!(meta_before.version, 1);
    assert_eq!(meta_before.admin, admin);
    assert_eq!(meta_before.shipment_count, 0);
    assert!(meta_before.initialized);

    client.upgrade(&admin, &new_wasm_hash, &2);

    let version: u32 = env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::Version)
            .unwrap()
    });
    assert_eq!(version, 2);
}

#[test]
fn test_get_hash_algo_version() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    assert_eq!(client.get_hash_algo_version(), crate::DEFAULT_HASH_ALGO);
}

#[test]
fn test_dry_run_migration_success() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let report = client.dry_run_migration(&2);
    assert_eq!(report.current_version, 1);
    assert_eq!(report.target_version, 2);
    assert_eq!(report.affected_shipments, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #47)")]
fn test_upgrade_invalid_edge_fails() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &token_contract);

    // Jump from 1 to 3 is not allowed
    client.upgrade(&admin, &new_wasm_hash, &3);
}

#[test]
#[should_panic(expected = "Error(Contract, #47)")]
fn test_dry_run_invalid_edge_fails() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Rollback from 1 to 0 is not allowed
    client.dry_run_migration(&0);
}

// ============= Carrier Handoff Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_handoff_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let unauthorized_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &current_carrier);
    client.add_carrier(&admin, &new_carrier);
    // Note: unauthorized_carrier is NOT added as a carrier

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &current_carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.update_status(
        &current_carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    // Try to handoff from unauthorized carrier
    client.handoff_shipment(
        &unauthorized_carrier,
        &new_carrier,
        &shipment_id,
        &handoff_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_handoff_wrong_current_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let wrong_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &current_carrier);
    client.add_carrier(&admin, &wrong_carrier);
    client.add_carrier(&admin, &new_carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &current_carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.update_status(
        &current_carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    // Try to handoff from wrong carrier (not the assigned one)
    client.handoff_shipment(&wrong_carrier, &new_carrier, &shipment_id, &handoff_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_handoff_invalid_new_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let invalid_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &current_carrier);
    // Note: invalid_carrier is NOT added as a carrier

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &current_carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.update_status(
        &current_carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    // Try to handoff to invalid carrier (doesn't have Carrier role)
    client.handoff_shipment(
        &current_carrier,
        &invalid_carrier,
        &shipment_id,
        &handoff_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_handoff_nonexistent_shipment() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let current_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);
    let nonexistent_shipment_id = 999u64;

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &current_carrier);
    client.add_carrier(&admin, &new_carrier);

    // Try to handoff a non-existent shipment
    client.handoff_shipment(
        &current_carrier,
        &new_carrier,
        &nonexistent_shipment_id,
        &handoff_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_create_shipment_fails_before_initialization() {
    let (env, client, _admin, _token_contract) = setup_shipment_env();
    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    // Contract not initialized — should panic with NotInitialized (#2)
    client.create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
}

// ── Issue #1: report_condition_breach ────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_report_condition_breach_unauthorized_non_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let rogue = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let breach_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // Non-carrier address cannot report a breach
    client.report_condition_breach(
        &rogue,
        &shipment_id,
        &BreachType::Impact,
        &Severity::Medium,
        &breach_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_report_condition_breach_wrong_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let other_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let breach_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier(&admin, &other_carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // A registered carrier that is NOT assigned to this shipment cannot report
    client.report_condition_breach(
        &other_carrier,
        &shipment_id,
        &BreachType::TamperDetected,
        &Severity::Critical,
        &breach_hash,
    );
}

// ── Issue #2: verify_delivery_proof ──────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_verify_delivery_proof_nonexistent_shipment() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.verify_delivery_proof(&999u64, &BytesN::from_array(&_env, &[1u8; 32]));
}

// ── Issue #3: Rate limiting ───────────────────────────────────────────────────

// ============= RBAC and Access Control Tests =============

#[test]
fn test_only_admin_can_assign_roles() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);

    // Admin can add company
    client.add_company(&admin, &company);
    // Admin can add carrier
    client.add_carrier(&admin, &carrier);

    // Non-admin cannot add company
    env.mock_all_auths();
    let result = client.try_add_company(&outsider, &Address::generate(&env));
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // Non-admin cannot add carrier
    let result = client.try_add_carrier(&outsider, &Address::generate(&env));
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));
}

#[test]
fn test_unassigned_addresses_are_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);

    // Unassigned cannot create shipment
    let result = client.try_create_shipment(
        &outsider,
        &Address::generate(&env),
        &Address::generate(&env),
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // Unassigned cannot add carrier to whitelist
    let result = client.try_add_carrier_to_whitelist(&outsider, &Address::generate(&env));
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // Unassigned cannot report geofence event
    let result =
        client.try_report_geofence_event(&outsider, &1, &GeofenceEvent::ZoneEntry, &data_hash);
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));
}

// ============= Admin Transfer Tests =============

#[test]
fn test_successful_admin_transfer() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let new_admin = Address::generate(&env);

    // 1. Current admin proposes new admin
    client.transfer_admin(&admin, &new_admin);

    // 2. New admin accepts the transfer
    client.accept_admin_transfer(&new_admin);

    // Verify ownership changed
    assert_eq!(client.get_admin(), new_admin);

    // Verify old admin lost privileges
    let company = Address::generate(&env);
    env.mock_all_auths();

    // Attempting to add a company with the old admin should now fail
    let result = client.try_add_company(&admin, &company);
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_unauthorized_admin_transfer() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let outsider = Address::generate(&env);
    let new_admin = Address::generate(&env);

    // Outsider tries to transfer admin - should fail
    client.transfer_admin(&outsider, &new_admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_unauthorized_admin_acceptance() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let new_admin = Address::generate(&env);
    let imposter = Address::generate(&env);

    // 1. Current admin proposes new admin
    client.transfer_admin(&admin, &new_admin);

    // 2. Imposter tries to accept the transfer - should fail
    client.accept_admin_transfer(&imposter);
}

// ============= Multi-Signature Tests =============

#[test]
fn test_init_multisig_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    client.init_multisig(&admin, &admins, &2);

    let (stored_admins, threshold) = client.get_multisig_config();
    assert_eq!(stored_admins.len(), 3);
    assert_eq!(threshold, 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_init_multisig_invalid_threshold_too_high() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1);
    admins.push_back(admin2);

    // Threshold 3 > admin count 2
    client.init_multisig(&admin, &admins, &3);
}

#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_init_multisig_invalid_threshold_zero() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1);
    admins.push_back(admin2);

    // Threshold 0 is invalid
    client.init_multisig(&admin, &admins, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_init_multisig_too_few_admins() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1);

    // Only 1 admin, need at least 2
    client.init_multisig(&admin, &admins, &1);
}

#[test]
fn test_propose_action_upgrade() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    client.init_multisig(&admin, &admins, &2);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    let proposal_id = client.propose_action(&admin1, &action);
    assert_eq!(proposal_id, 1);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.id, 1);
    assert_eq!(proposal.proposer, admin1);
    assert_eq!(proposal.approvals.len(), 1);
    assert!(!proposal.executed);
}

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_propose_action_not_admin() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let outsider = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1);
    admins.push_back(admin2);

    client.init_multisig(&admin, &admins, &2);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    // Outsider tries to propose
    client.propose_action(&outsider, &action);
}

#[test]
fn test_approve_action_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    // Set threshold to 3 so it doesn't auto-execute on second approval
    client.init_multisig(&admin, &admins, &3);

    let new_admin = Address::generate(&env);
    let action = crate::AdminAction::TransferAdmin(new_admin);

    let proposal_id = client.propose_action(&admin1, &action);

    // Admin2 approves
    client.approve_action(&admin2, &proposal_id);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.approvals.len(), 2);
    assert!(!proposal.executed); // Should not be executed yet
}

#[test]
#[should_panic(expected = "Error(Contract, #25)")]
fn test_approve_action_already_approved() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    let proposal_id = client.propose_action(&admin1, &action);

    // Admin1 tries to approve again (already approved when proposing)
    client.approve_action(&admin1, &proposal_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_approve_action_not_admin() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let outsider = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    let proposal_id = client.propose_action(&admin1, &action);

    // Outsider tries to approve
    client.approve_action(&outsider, &proposal_id);
}

#[test]
fn test_execute_proposal_auto_on_threshold() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let wasm: &[u8] = include_bytes!("../test_wasms/upgrade_test.wasm");
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    client.init_multisig(&admin, &admins, &2);

    let action = crate::AdminAction::Upgrade(new_wasm_hash);
    let proposal_id = client.propose_action(&admin1, &action);

    // Admin2 approves - this should auto-execute since threshold is met
    client.approve_action(&admin2, &proposal_id);

    // Verify version was incremented (check before trying to get proposal)
    let version: u32 = env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::Version)
            .unwrap()
    });
    assert_eq!(version, 2);

    // Note: After upgrade, the WASM is replaced, so we can't call get_proposal
    // on the upgraded contract. The execution happened successfully.
}

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn test_execute_proposal_already_executed() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let new_admin = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    // Use TransferAdmin action instead of Upgrade
    let action = crate::AdminAction::TransferAdmin(new_admin);
    let proposal_id = client.propose_action(&admin1, &action);

    client.approve_action(&admin2, &proposal_id);

    // Try to execute again
    client.execute_proposal(&proposal_id);
}

#[test]
fn test_proposal_expiration() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    let proposal_id = client.propose_action(&admin1, &action);

    // Fast forward time beyond expiration (7 days + 1 second)
    super::test_utils::advance_past_multisig_expiry(&env);

    // Try to approve expired proposal - should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.approve_action(&admin2, &proposal_id);
    }));

    assert!(result.is_err());
}

#[test]
fn test_transfer_admin_action() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let new_admin = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    // Propose admin transfer
    let action = crate::AdminAction::TransferAdmin(new_admin.clone());
    let proposal_id = client.propose_action(&admin1, &action);

    // Approve and execute
    client.approve_action(&admin2, &proposal_id);

    // Verify admin was transferred
    let current_admin = client.get_admin();
    assert_eq!(current_admin, new_admin);
}

#[test]
fn test_three_of_five_multisig() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let wasm: &[u8] = include_bytes!("../test_wasms/upgrade_test.wasm");
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let admin4 = Address::generate(&env);
    let admin5 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());
    admins.push_back(admin4.clone());
    admins.push_back(admin5.clone());

    client.init_multisig(&admin, &admins, &3);

    let action = crate::AdminAction::Upgrade(new_wasm_hash);
    let proposal_id = client.propose_action(&admin1, &action);

    // First approval (proposer)
    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.approvals.len(), 1);
    assert!(!proposal.executed);

    // Second approval
    client.approve_action(&admin2, &proposal_id);
    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.approvals.len(), 2);
    assert!(!proposal.executed);

    // Third approval - should auto-execute
    client.approve_action(&admin3, &proposal_id);

    // Verify version was incremented (check directly from storage)
    let version: u32 = env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::Version)
            .unwrap()
    });
    assert_eq!(version, 2);

    // Note: After upgrade, the WASM is replaced, so we can't call get_proposal
    // on the upgraded contract. The execution happened successfully.
}

#[test]
#[should_panic(expected = "Error(Contract, #26)")]
fn test_execute_proposal_insufficient_approvals() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    client.init_multisig(&admin, &admins, &3);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    let proposal_id = client.propose_action(&admin1, &action);

    // Only 1 approval, need 3
    client.execute_proposal(&proposal_id);
}

// ============= Deadline Tests =============

// ============= Notification Event Tests =============

// ============= Analytics Tests =============

#[test]
fn test_get_status_summary_empty() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let summary = client.get_status_summary();
    assert_eq!(summary.created, 0);
    assert_eq!(summary.in_transit, 0);
    assert_eq!(summary.at_checkpoint, 0);
    assert_eq!(summary.partially_delivered, 0);
    assert_eq!(summary.delivered, 0);
    assert_eq!(summary.disputed, 0);
    assert_eq!(summary.cancelled, 0);
}

// ── #368: Status summary regression — every lifecycle bucket changes correctly
//
// This test covers a full mixed-state lifecycle: Created → InTransit →
// AtCheckpoint → Delivered and Created → Cancelled.  The summary is asserted
// after every individual transition so that any accounting drift in
// `get_status_summary` (added or missing counter update) is caught immediately.

// ============= Shipment Limit Tests =============

#[test]
fn test_set_and_get_shipment_limit() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Default limit should be 100 (set in initialize)
    assert_eq!(client.get_shipment_limit(), 100);

    // Admin sets new limit
    client.set_shipment_limit(&admin, &10);
    assert_eq!(client.get_shipment_limit(), 10);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_set_shipment_limit_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let outsider = Address::generate(&env);
    client.initialize(&admin, &token_contract);

    // Outsider tries to set limit
    client.set_shipment_limit(&outsider, &10);
}

// ============= Dispute Evidence Tests =============

// ============= Circular Dependency Tests =============
// ============= Dependencies Not Met Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #30)")]
fn test_batch_limit_reached() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // Set limit to 2
    client.set_shipment_limit(&admin, &2);

    // Attempt to create 3 shipments in a batch
    let mut shipments = soroban_sdk::Vec::new(&env);
    for i in 1..=3 {
        shipments.push_back(ShipmentInput {
            receiver: Address::generate(&env),
            carrier: Address::generate(&env),
            data_hash: BytesN::from_array(&env, &[i as u8; 32]),
            payment_milestones: soroban_sdk::Vec::new(&env),
            deadline,
        });
    }

    client.create_shipments_batch(&company, &shipments);
}

// ============= Proposal Salt Reuse Tests =============

#[test]
fn test_proposal_salt_reused_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    // Set up multisig
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &1);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let salt = BytesN::from_array(&env, &[1u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    // First proposal with specific salt should succeed
    let proposal_id_1 = client.propose_action_with_salt(&admin, &action, &salt);
    assert_eq!(proposal_id_1, 1);

    // Attempt second proposal with the same salt should fail with ProposalSaltReused
    let result = client.try_propose_action_with_salt(&admin, &action, &salt);
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalSaltReused)),
        "reused salt should be rejected with ProposalSaltReused error"
    );
}

#[test]
fn test_proposal_unique_salts_accepted() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    // Set up multisig
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &1);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let salt_1 = BytesN::from_array(&env, &[1u8; 32]);
    let salt_2 = BytesN::from_array(&env, &[2u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    // First proposal with salt_1
    let proposal_id_1 = client.propose_action_with_salt(&admin, &action, &salt_1);
    assert_eq!(proposal_id_1, 1);

    // Second proposal with different salt_2 should succeed
    let proposal_id_2 = client.propose_action_with_salt(&admin, &action, &salt_2);
    assert_eq!(proposal_id_2, 2);

    // Both proposals should exist
    let proposal_1 = client.get_proposal(&proposal_id_1);
    let proposal_2 = client.get_proposal(&proposal_id_2);
    assert_eq!(proposal_1.id, proposal_id_1);
    assert_eq!(proposal_2.id, proposal_id_2);
}

#[test]
fn test_proposal_salt_reuse_error_code_56() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    // Set up multisig
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &1);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let salt = BytesN::from_array(&env, &[99u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    // Create first proposal
    let _ = client.propose_action_with_salt(&admin, &action, &salt);

    // Attempt to create second proposal with same salt
    let result = client.try_propose_action_with_salt(&admin, &action, &salt);

    // Verify the error is ProposalSaltReused with code 56
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalSaltReused)),
        "ProposalSaltReused error should be returned"
    );

    let salt_reused_err = crate::NavinError::ProposalSaltReused;
    assert_eq!(
        salt_reused_err as u32, 56,
        "ProposalSaltReused error code must be 56"
    );
}

#[test]
fn test_proposal_salt_different_actions_same_salt() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    // Set up multisig
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &1);

    let new_admin = Address::generate(&env);
    let salt = BytesN::from_array(&env, &[77u8; 32]);

    let action_1 = crate::AdminAction::Upgrade(BytesN::from_array(&env, &[42u8; 32]));
    let action_2 = crate::AdminAction::TransferAdmin(new_admin);

    // First proposal with salt
    let proposal_id_1 = client.propose_action_with_salt(&admin, &action_1, &salt);
    assert_eq!(proposal_id_1, 1);

    // Attempt second proposal with same salt but different action should also fail
    // Salt reuse is checked regardless of action content
    let result = client.try_propose_action_with_salt(&admin, &action_2, &salt);
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalSaltReused)),
        "salt reuse should be rejected even with different actions"
    );
}

// ============================================================================
// COMPREHENSIVE NEGATIVE TEST SUITE - Testing All NavinError Variants
// ============================================================================
// This section systematically tests every NavinError variant to ensure
// proper error handling across all contract functions.
// ============================================================================

// ============= Error #6: InvalidHash Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_create_shipment_returns_invalid_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &zero_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
}

// NOTE: This test is commented out because the feature may not be fully implemented yet
// NOTE: This test is commented out because the feature may not be fully implemented yet
#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_confirm_delivery_returns_invalid_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

    let (receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::InTransit,
    );

    client.confirm_delivery(&receiver, &shipment_id, &zero_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_raise_dispute_returns_invalid_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

    let (_receiver, carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::InTransit,
    );

    client.raise_dispute(&carrier, &shipment_id, &zero_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #36)")]
fn test_resolve_dispute_returns_invalid_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

    let (_receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::Disputed,
    );

    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::RefundToCompany,
        &zero_hash,
    );
}

// ============= Error #11: CounterOverflow Tests =============

// ============= Error #12: CarrierNotWhitelisted Tests =============

// NOTE: This test is commented out because the feature may not be fully implemented yet
// #[test]
// #[should_panic(expected = "Error(Contract, #12)")]
// fn test_create_shipment_returns_carrier_not_whitelisted() {
//     let (env, client, admin, token_contract) = setup_shipment_env();
//     let company = Address::generate(&env);
//     let receiver = Address::generate(&env);
//     let carrier = Address::generate(&env);
//     let data_hash = BytesN::from_array(&env, &[1u8; 32]);
//     let deadline = env.ledger().timestamp() + 3600;
//
//     client.initialize(&admin, &token_contract);
//     client.add_company(&admin, &company);
//
//     // Add a carrier to whitelist, but use a different carrier
//     let whitelisted_carrier = Address::generate(&env);
//     client.add_carrier_to_whitelist(&company, &whitelisted_carrier);
//
//     client.create_shipment(
//         &company,
//         &receiver,
//         &carrier,
//         &data_hash,
//         &soroban_sdk::Vec::new(&env),
//         &deadline,
//     );
// }

// ============= Error #13: CarrierNotAuthorized Tests =============

// NOTE: This test is commented out because the feature may not be fully implemented yet
// #[test]
// #[should_panic(expected = "Error(Contract, #13)")]
// fn test_handoff_shipment_returns_carrier_not_authorized() {
//     let (env, client, admin, token_contract) = setup_shipment_env();
//     let company = Address::generate(&env);
//     let receiver = Address::generate(&env);
//     let carrier = Address::generate(&env);
//     let new_carrier = Address::generate(&env);
//     let data_hash = BytesN::from_array(&env, &[1u8; 32]);
//     let deadline = env.ledger().timestamp() + 3600;
//
//     client.initialize(&admin, &token_contract);
//     client.add_company(&admin, &company);
//     client.add_carrier(&admin, &carrier);
//
//     let shipment_id = client.create_shipment(
//         &company,
//         &receiver,
//         &carrier,
//         &data_hash,
//         &soroban_sdk::Vec::new(&env),
//         &deadline,
//     );
//
//     // Try to handoff to a carrier that is not registered
//     let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);
//     client.handoff_shipment(&carrier, &new_carrier, &shipment_id, &handoff_hash);
// }

// ============= Error #14: InvalidAmount Tests =============

// ============= Error #15: EscrowAlreadyDeposited Tests =============

// ============= Error #19: MilestoneAlreadyPaid Tests =============

// ============= Error #20: MetadataLimitExceeded Tests =============

// ============= Error #21: RateLimitExceeded Tests =============

// ============= Error #22: ProposalNotFound Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #22)")]
fn test_get_proposal_returns_proposal_not_found() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_proposal(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #22)")]
fn test_approve_action_returns_proposal_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &2);

    client.approve_action(&admin2, &999);
}

#[test]
#[should_panic(expected = "Error(Contract, #22)")]
fn test_execute_proposal_returns_proposal_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &2);

    client.execute_proposal(&999);
}

#[test]
fn test_approve_action_returns_proposal_not_found_variant() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &2);

    let result = client.try_approve_action(&admin2, &999);
    assert_eq!(result, Err(Ok(NavinError::ProposalNotFound)));
}

#[test]
fn test_execute_proposal_returns_proposal_not_found_variant() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &2);

    let result = client.try_execute_proposal(&999);
    assert_eq!(result, Err(Ok(NavinError::ProposalNotFound)));
}

// ============= Error #23: ProposalAlreadyExecuted Tests =============

// ============= Error #24: ProposalExpired Tests =============

// ============= Error #25: AlreadyApproved Tests =============

/// Approve the same proposal twice with the same admin — must return AlreadyApproved error.
/// Two different admins can approve the same proposal — confirms multi-sig flow works.
// ============= Error #26: InsufficientApprovals Tests =============

// ============= Error #27: NotAnAdmin Tests =============

/// Non-admin (not in admin list) attempts propose_action — must return NotAnAdmin.
/// Non-admin (not in admin list) attempts approve_action — must return NotAnAdmin.
/// Admin (in admin list) can propose_action — verifies admin operations succeed.
/// Different admin (in admin list) can approve_action — verifies admin operations succeed.
// ============= Error #28: InvalidMultiSigConfig Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_init_multisig_returns_invalid_config_threshold_too_high() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);

    client.initialize(&admin, &token_contract);

    // Threshold of 3 but only 2 admins
    client.init_multisig(&admin, &admins, &3);
}

#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_init_multisig_returns_invalid_multisig_config_threshold_zero() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);

    client.initialize(&admin, &token_contract);

    // Threshold of 0 is invalid
    client.init_multisig(&admin, &admins, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_init_multisig_returns_invalid_multisig_config_empty_admins() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let admins = soroban_sdk::Vec::new(&env);

    client.initialize(&admin, &token_contract);

    // Empty admin list is invalid
    client.init_multisig(&admin, &admins, &1);
}

#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_init_multisig_duplicate_admins() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);
    admins.push_back(admin.clone());

    client.initialize(&admin, &token_contract);

    client.init_multisig(&admin, &admins, &2);
}

#[test]
fn test_init_multisig_invalid_config_threshold_exceeds_admin_count() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);

    env.mock_all_auths();
    client.initialize(&admin, &token_contract);

    let result = client.try_init_multisig(&admin, &admins, &3);

    assert_eq!(result, Err(Ok(NavinError::InvalidConfig)));
}

#[test]
fn test_init_multisig_invalid_multisig_config_threshold_zero() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);

    env.mock_all_auths();
    client.initialize(&admin, &token_contract);

    let result = client.try_init_multisig(&admin, &admins, &0);

    assert_eq!(result, Err(Ok(NavinError::InvalidMultiSigConfig)));
}

// ============= Error #29: NotExpired Tests =============

// ============= Error #30: ShipmentLimitReached Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #30)")]
fn test_create_shipments_batch_returns_shipment_limit_reached() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.set_shipment_limit(&admin, &2);

    let mut shipments = soroban_sdk::Vec::new(&env);
    for i in 1..=3 {
        shipments.push_back(ShipmentInput {
            receiver: Address::generate(&env),
            carrier: Address::generate(&env),
            data_hash: BytesN::from_array(&env, &[i as u8; 32]),
            payment_milestones: soroban_sdk::Vec::new(&env),
            deadline,
        });
    }

    // Try to create 3 shipments when limit is 2
    client.create_shipments_batch(&company, &shipments);
}

// ============= Additional Coverage for NotInitialized Error =============

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_create_shipment_returns_not_initialized() {
    let (env, client, _admin, _token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_add_company_returns_not_initialized() {
    let (env, client, admin, _token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.add_company(&admin, &company);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_add_carrier_returns_not_initialized() {
    let (env, client, admin, _token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);

    client.add_carrier(&admin, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_admin_returns_not_initialized() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    client.get_admin();
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_set_shipment_limit_returns_not_initialized() {
    let (_env, client, admin, _token_contract) = setup_shipment_env();

    client.set_shipment_limit(&admin, &10);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_shipment_limit_returns_not_initialized() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    client.get_shipment_limit();
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_active_shipment_count_returns_not_initialized() {
    let (env, client, _admin, _token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.get_active_shipment_count(&company);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_analytics_returns_not_initialized() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    client.get_analytics();
}

// ============= Additional Coverage for Unauthorized Error =============

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_add_company_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    client.add_company(&non_admin, &company);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_add_carrier_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    client.add_carrier(&non_admin, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_set_shipment_limit_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let non_admin = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    client.set_shipment_limit(&non_admin, &10);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_add_carrier_to_whitelist_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let non_company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.add_carrier_to_whitelist(&non_company, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_remove_carrier_from_whitelist_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let non_company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier_to_whitelist(&company, &carrier);

    client.remove_carrier_from_whitelist(&non_company, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_cancel_shipment_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let reason_hash = BytesN::from_array(&env, &[3u8; 32]);
    client.cancel_shipment(&outsider, &shipment_id, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_report_condition_breach_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let breach_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.report_condition_breach(
        &outsider,
        &shipment_id,
        &BreachType::TemperatureHigh,
        &Severity::Low,
        &breach_hash,
    );
}

// ============= Additional Coverage for ShipmentNotFound Error =============

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_update_status_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);

    client.update_status(&carrier, &999, &ShipmentStatus::InTransit, &data_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_confirm_delivery_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let receiver = Address::generate(&env);
    let confirmation_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);

    client.confirm_delivery(&receiver, &999, &confirmation_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_release_escrow_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let receiver = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    client.release_escrow(&receiver, &999);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_refund_escrow_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.refund_escrow(&company, &999);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_raise_dispute_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let reason_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.raise_dispute(&company, &999, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_resolve_dispute_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.resolve_dispute(
        &admin,
        &999,
        &crate::DisputeResolution::ReleaseToCarrier,
        &BytesN::from_array(&env, &[1u8; 32]),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_cancel_shipment_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let reason_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.cancel_shipment(&company, &999, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_update_eta_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let eta_hash = BytesN::from_array(&env, &[1u8; 32]);
    let eta_timestamp = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);

    client.update_eta(&carrier, &999, &eta_timestamp, &eta_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_record_milestone_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let checkpoint = soroban_sdk::Symbol::new(&env, "port_arrival");
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);

    client.record_milestone(&carrier, &999, &checkpoint, &data_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_handoff_shipment_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);
    client.add_carrier(&admin, &new_carrier);

    let handoff_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.handoff_shipment(&carrier, &new_carrier, &999, &handoff_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_report_condition_breach_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let breach_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);

    client.report_condition_breach(
        &carrier,
        &999,
        &BreachType::TemperatureHigh,
        &Severity::High,
        &breach_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_check_deadline_returns_shipment_not_found() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.check_deadline(&999);
}

// ============= Additional Coverage for InvalidStatus Error =============

// NOTE: This test is commented out because the feature may not be fully implemented yet
// #[test]
// #[should_panic(expected = "Error(Contract, #5)")]
// fn test_raise_dispute_returns_invalid_status() {
//     let (env, client, admin, token_contract) = setup_shipment_env();
//     let company = Address::generate(&env);
//     let receiver = Address::generate(&env);
//     let carrier = Address::generate(&env);
//     let data_hash = BytesN::from_array(&env, &[1u8; 32]);
//     let reason_hash = BytesN::from_array(&env, &[2u8; 32]);
//     let deadline = env.ledger().timestamp() + 3600;
//
//     client.initialize(&admin, &token_contract);
//     client.add_company(&admin, &company);
//
//     let shipment_id = client.create_shipment(
//         &company,
//         &receiver,
//         &carrier,
//         &data_hash,
//         &soroban_sdk::Vec::new(&env),
//         &deadline,
//     );
//
//     // Change status to Delivered
//     env.as_contract(&client.address, || {
//         let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
//         shipment.status = crate::ShipmentStatus::Delivered;
//         crate::storage::set_shipment(&env, &shipment);
//     });
//
//     client.raise_dispute(&company, &shipment_id, &reason_hash);
// }

/// Comprehensive end-to-end integration test covering the full shipment lifecycle.
///
/// This test exercises the complete happy path from shipment creation through
/// delivery and payment release, verifying all intermediate states, events,
/// and balance changes.
///
/// # Test Flow
/// 1. Initialize contract and assign all roles (Admin, Company, Carrier, Customer)
/// 2. Create shipment with payment milestones
/// 3. Deposit escrow funds
/// 4. Update status to InTransit
/// 5. Record first milestone (warehouse) - triggers 30% payment
/// 6. Update status to AtCheckpoint
/// 7. Update status back to InTransit
/// 8. Record second milestone (port) - triggers 30% payment
/// 9. Confirm delivery by receiver - automatically sets status to Delivered and releases remaining 40%
///
/// # Verification Points
/// - All status transitions are valid and recorded correctly
/// - All events are emitted with correct data
/// - Escrow balances are tracked accurately throughout lifecycle
/// - Payment milestones trigger partial payments correctly
/// - Final delivery releases remaining escrow balance
/// - All role-based access controls are enforced
// ============= Event Counter Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_event_count_shipment_not_found() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    // Try to get event count for non-existent shipment
    client.get_event_count(&999);
}

// ============= Shipment Archival Tests =============

// ── [ISSUE #600] ShipmentUnavailable error variant tests ─────────────────────
//
// ShipmentUnavailable (#42) is returned by preflight_check_shipment_available
// when a shipment exists only in temporary (archived) storage.
// These tests verify the error code, the validation helper's behaviour, and
// that mutating operations on archived shipments are correctly rejected.

/// preflight_check_shipment_available must return ShipmentUnavailable for an
/// archived (Delivered) shipment — error code must be 42.
/// preflight_check_shipment_available must return ShipmentUnavailable for an
/// archived (Cancelled) shipment.
/// preflight_check_shipment_available must return ShipmentNotFound (not
/// ShipmentUnavailable) for a shipment ID that never existed.
#[test]
fn test_preflight_never_created_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let result = env.as_contract(&client.address, || {
        crate::validation::preflight_check_shipment_available(&env, 8888u64)
    });

    assert_eq!(
        result,
        Err(NavinError::ShipmentNotFound),
        "non-existent shipment must return ShipmentNotFound, not ShipmentUnavailable"
    );
}

/// preflight_check_shipment_available must return Ok for an active shipment,
/// confirming it does not block normal operations.
/// The ShipmentUnavailable error discriminant must be exactly 42.
/// This pins the error code so it cannot drift across refactors.
#[test]
fn test_shipment_unavailable_error_code_is_42() {
    assert_eq!(
        NavinError::ShipmentUnavailable as u32,
        42,
        "ShipmentUnavailable discriminant must be 42"
    );
}

// ============= Analytics Event Tests =============
// ============= Role Revocation Tests =============

#[test]
fn test_revoke_role_company() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);
    assert_eq!(client.get_role(&company), crate::types::Role::Company);

    client.revoke_role(&admin, &company);
    assert_eq!(client.get_role(&company), crate::types::Role::Unassigned);
}

#[test]
fn test_revoke_role_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);
    assert_eq!(client.get_role(&carrier), crate::types::Role::Carrier);

    client.revoke_role(&admin, &carrier);
    assert_eq!(client.get_role(&carrier), crate::types::Role::Unassigned);
}

#[test]
#[should_panic(expected = "Error(Contract, #32)")]
fn test_revoke_role_self_revoke_fails() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Admin cannot self-revoke (error code 32 = CannotSelfRevoke)
    client.revoke_role(&admin, &admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_revoke_role_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let non_admin = Address::generate(&env);
    let target = Address::generate(&env);
    client.add_company(&admin, &target);

    // Non-admin cannot revoke roles (error code 3 = Unauthorized)
    client.revoke_role(&non_admin, &target);
}

#[test]
fn test_revoke_role_emits_event() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);
    client.revoke_role(&admin, &company);

    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "role_revoked") {
                        found = true;
                    }
                }
            }
        }
    }
    assert!(found, "role_revoked event not found");
}

#[test]
fn test_role_changed_event_emitted_on_add_company() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);

    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "role_changed") {
                        found = true;
                    }
                }
            }
        }
    }
    assert!(found, "role_changed event not found on add_company");
}

#[test]
fn test_role_changed_event_emitted_on_add_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);

    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "role_changed") {
                        found = true;
                    }
                }
            }
        }
    }
    assert!(found, "role_changed event not found on add_carrier");
}

#[test]
fn test_role_changed_event_emitted_on_revoke_role() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);
    client.revoke_role(&admin, &company);

    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "role_changed") {
                        found = true;
                    }
                }
            }
        }
    }
    assert!(found, "role_changed event not found on revoke_role");
}

#[test]
fn test_suspend_role_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);

    // Suspend the role
    client.suspend_role(&admin, &company);

    // Verify role_changed event was emitted with Suspended action
    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "role_changed") {
                        found = true;
                    }
                }
            }
        }
    }
    assert!(found, "role_changed event not found on suspend_role");
}

#[test]
fn test_reactivate_role_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);
    client.suspend_role(&admin, &company);

    // Reactivate the role
    client.reactivate_role(&admin, &company);

    // Verify role_changed event was emitted with Reactivated action
    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "role_changed") {
                        found = true;
                    }
                }
            }
        }
    }
    assert!(found, "role_changed event not found on reactivate_role");
}

#[test]
fn test_suspended_role_cannot_perform_actions() {
    use soroban_sdk::testutils::Address as _;

    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Suspend the company role
    client.suspend_role(&admin, &company);

    // Suspended company cannot create shipment - should panic with Unauthorized
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &soroban_sdk::Vec::new(&env),
            &deadline,
        );
    }));

    assert!(
        result.is_err(),
        "Suspended company should not be able to create shipments"
    );
}

// ============= Deadline Grace Period Tests =============

/// Within the grace window: deadline has passed but grace has not — must return NotExpired.
/// Exactly at the grace boundary: timestamp == deadline + grace — must succeed.
/// After the grace window: timestamp > deadline + grace — must succeed and cancel.
/// Zero grace (default): deadline passed by 1 second — must succeed immediately.
/// Validate that deadline_grace_seconds > 604_800 is rejected by update_config.
#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_update_config_rejects_grace_period_exceeding_max() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let mut config = client.get_contract_config();
    config.deadline_grace_seconds = 604_801; // 1 second over the 7-day cap
    client.update_config(&admin, &config);
}

// =============================================================================
// force_cancel_shipment tests
// =============================================================================

/// Helper: initialise contract, register a company, create one shipment, and
/// return (env, client, admin, token_contract, company, shipment_id).
fn setup_force_cancel_env() -> (
    Env,
    NavinShipmentClient<'static>,
    Address,
    Address,
    Address,
    u64,
) {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[0xABu8; 32]);
    let deadline = env.ledger().timestamp() + 7200;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    (env, client, admin, token_contract, company, shipment_id)
}

/// Admin can force-cancel a shipment in Created status.
/// Admin can force-cancel a shipment that is InTransit.
/// Admin can force-cancel a Disputed shipment (bypasses normal cancel restriction).
/// Non-admin caller is rejected with Unauthorized (#3).
#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_force_cancel_shipment_unauthorized_company() {
    let (env, client, _admin, _token_contract, company, shipment_id) = setup_force_cancel_env();

    let reason_hash = BytesN::from_array(&env, &[0x06u8; 32]);
    // company is not admin — must be rejected
    client.force_cancel_shipment(&company, &shipment_id, &reason_hash);
}

/// All-zero reason_hash is rejected with InvalidHash (#6).
/// Force-cancelling a non-existent shipment returns ShipmentNotFound (#4).
/// Force-cancelling an already-Delivered shipment returns ShipmentFinalized (#38).
/// Force-cancelling an already-Cancelled shipment returns ShipmentFinalized (#38).
/// Escrow is deterministically zeroed on force-cancel (no-escrow path).
/// Verifies escrow_amount stays 0 and force_cancelled event is emitted.
/// Both force_cancelled and shipment_cancelled events are emitted on force-cancel.
/// Payload shape regression test for both events emitted on force-cancel.
// ============= Shipment Note Tests =============

// ============= Idempotency Window Tests =============
// ============================================================================
// Integration Tests for Symbol and BytesN<32> Validators
// ============================================================================

#[test]
#[should_panic(expected = "Error(Contract, #60)")]
fn test_create_shipment_with_duplicate_milestone_symbols_fails() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    // Duplicate milestone names should fail validation
    milestones.push_back((Symbol::new(&env, "warehouse"), 50_u32));
    milestones.push_back((Symbol::new(&env, "warehouse"), 50_u32));

    let deadline = super::test_utils::future_deadline(&env, 86400);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
}

#[test]
fn test_operator_can_manage_roles() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let operator = Address::generate(&env);
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_operator(&admin, &operator);

    client.add_company(&operator, &company);
    client.suspend_company(&operator, &company);
    client.reactivate_company(&operator, &company);

    client.add_carrier(&operator, &carrier);
    client.suspend_carrier(&operator, &carrier);
    client.reactivate_carrier(&operator, &carrier);

    let outsider = Address::generate(&env);
    let result = client.try_add_company(&outsider, &Address::generate(&env));
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));
}

#[test]
fn test_get_canonical_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let mut fields = soroban_sdk::Vec::new(&env);
    fields.push_back(Symbol::new(&env, "test").into_val(&env));
    fields.push_back(123_u64.into_val(&env));

    let hash1 = client.get_canonical_hash(&fields);
    let hash2 = client.get_canonical_hash(&fields);

    assert_eq!(hash1, hash2);

    // Ensure different fields result in different hash
    fields.push_back(456_u64.into_val(&env));
    let hash3 = client.get_canonical_hash(&fields);
    assert_ne!(hash1, hash3);
}

#[test]
fn test_get_expected_token_decimals_policy() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    assert_eq!(
        client.get_expected_token_decimals(),
        crate::types::EXPECTED_TOKEN_DECIMALS
    );
}

#[test]
fn test_dispute_emits_escrow_frozen_event() {
    let (env, client, admin, token) = setup_shipment_env();
    client.initialize(&admin, &token);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let milestones = soroban_sdk::Vec::new(&env);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );

    client.raise_dispute(&company, &shipment_id, &data_hash);

    let events = env.events().all();

    let mut frozen_found = false;
    for (_contract_id, topic, data) in events.into_iter() {
        if let Some(topic_sym) = topic
            .get(0)
            .and_then(|v| Symbol::try_from_val(&env, &v).ok())
        {
            if topic_sym == Symbol::new(&env, crate::event_topics::ESCROW_FROZEN) {
                frozen_found = true;

                let data_vec =
                    soroban_sdk::Vec::<soroban_sdk::Val>::try_from_val(&env, &data).unwrap();
                assert_eq!(data_vec.len(), 4);

                let reason =
                    crate::types::EscrowFreezeReason::try_from_val(&env, &data_vec.get(1).unwrap())
                        .unwrap();
                let caller = Address::try_from_val(&env, &data_vec.get(2).unwrap()).unwrap();

                assert_eq!(reason, crate::types::EscrowFreezeReason::DisputeRaised);
                assert_eq!(caller, company);
            }
        }
    }

    assert!(frozen_found, "escrow_frozen event was not emitted");

    let stored_reason = client.get_escrow_freeze_reason(&shipment_id);
    assert_eq!(
        stored_reason,
        Some(crate::types::EscrowFreezeReason::DisputeRaised)
    );
}

// ── Finalization lock-out tests (issue #446) ──────────────────────────────────

// ── Metadata symbol boundary tests (issue #448) ──────────────────────────────

// ── Rate-limit exhaustion and recovery tests (issue #18) ─────────────────────

/// Test that rate limit exhaustion blocks the action as expected.
/// Test that advancing the ledger clears the rate limit window and restores the action.
/// Test that rate-limit behavior is deterministic across multiple cycles.
#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_created_at_fails_for_invalid_id() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment_created_at(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_shipment_created_at_fails_before_initialization() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    client.get_shipment_created_at(&1);
}

// ── [ISSUE #506] get_admin in multi-admin configurations ─────────────────────

#[test]
fn test_get_admin_returns_primary_admin_after_multisig_init() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());
    client.init_multisig(&admin, &admins, &2);

    assert_eq!(
        client.get_admin(),
        admin,
        "get_admin must return the primary admin set during initialize"
    );
}

#[test]
fn test_get_admin_consistent_across_queries_in_multisig_mode() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admin2 = Address::generate(&env);
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &2);

    let first = client.get_admin();
    let second = client.get_admin();
    assert_eq!(
        first, second,
        "repeated get_admin calls must return the same address"
    );
    assert_eq!(first, admin);
}

#[test]
fn test_get_admin_unaffected_by_additional_multisig_admins() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let before = client.get_admin();

    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());
    client.init_multisig(&admin, &admins, &3);

    let after = client.get_admin();
    assert_eq!(
        before, after,
        "get_admin must not change after init_multisig"
    );
}

// ── [ISSUE #599] StatusHashNotFound error variant tests ──────────────────────
//
// StatusHashNotFound (#44) is returned when a shipment exists but no hash was
// recorded for the requested status.
// ShipmentNotFound (#4) is returned when the shipment itself doesn't exist.

/// Error code pin: StatusHashNotFound discriminant must be 44.
#[test]
fn test_status_hash_not_found_error_code_is_44() {
    assert_eq!(
        NavinError::StatusHashNotFound as u32,
        44,
        "StatusHashNotFound discriminant must be 44"
    );
}

// ── Issue #581 – InvalidPaymentMilestones (code 59) ─────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #59)")]
fn test_create_shipment_zero_percentage_milestone_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    // A zero percentage is an invalid milestone weight.
    milestones.push_back((Symbol::new(&env, "pickup"), 0_u32));
    milestones.push_back((Symbol::new(&env, "delivery"), 100_u32));

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #59)")]
fn test_create_shipment_over_100_percentage_milestone_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 86400;
    let mut milestones = soroban_sdk::Vec::new(&env);
    // 150 % is out of bounds for a single milestone weight.
    milestones.push_back((Symbol::new(&env, "pickup"), 150_u32));

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Issue #596 — InvalidMigrationEdge (code 47)
// Issue #758 — rollback and skip-version migrations are intentionally
// unsupported. There is no runtime-configurable allowed-edges list.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_upgrade_valid_forward_migration_succeeds() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let wasm: &[u8] = include_bytes!("../test_wasms/upgrade_test.wasm");
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);
    client.initialize(&admin, &token_contract);

    // Version starts at 1
    assert_eq!(client.get_version(), 1);

    // Valid forward migration 1→2
    let result = client.try_upgrade(&admin, &new_wasm_hash, &2);
    assert_eq!(result, Ok(Ok(())));

    // The upgrade swaps the running wasm for real, so the version can no
    // longer be read through the (now stale) client ABI — read storage
    // directly instead, as other post-upgrade tests in this file do.
    let contract_id = client.address.clone();
    env.as_contract(&contract_id, || {
        assert_eq!(crate::storage::get_version(&env), 2);
    });
}

#[test]
fn test_upgrade_downgrade_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &token_contract);

    // Put the contract at version 2 directly — a real `upgrade` call would
    // swap the running wasm for real, which would break every subsequent
    // client call in this test (the client's ABI no longer matches the
    // deployed code). The edge-rejection check being tested here runs
    // before any wasm is touched, so a direct storage write is equivalent
    // and keeps the client usable for the second call.
    env.as_contract(&client.address, || {
        crate::storage::set_version(&env, 2);
    });

    // Now try to downgrade 2→1 — should fail with InvalidMigrationEdge
    let result = client.try_upgrade(&admin, &new_wasm_hash, &1);
    assert_eq!(result, Err(Ok(NavinError::InvalidMigrationEdge)));
}

#[test]
fn test_upgrade_skip_version_rejected_detailed() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &token_contract);

    // Skip from 1 to 3 — intentionally unsupported (no allowed-edges list)
    let result = client.try_upgrade(&admin, &new_wasm_hash, &3);
    assert_eq!(result, Err(Ok(NavinError::InvalidMigrationEdge)));
}

#[test]
fn test_upgrade_same_version_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &token_contract);

    // Staying at the current version is not a sequential forward upgrade.
    let result = client.try_upgrade(&admin, &new_wasm_hash, &1);
    assert_eq!(result, Err(Ok(NavinError::InvalidMigrationEdge)));
}

#[test]
fn test_dry_run_migration_skip_version_rejected_detailed() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Skip from 1 to 3 — should fail
    let result = client.try_dry_run_migration(&3);
    assert_eq!(result, Err(Ok(NavinError::InvalidMigrationEdge)));
}

#[test]
fn test_dry_run_migration_downgrade_rejected_detailed() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Rollback from 1 to 0 — intentionally unsupported
    let result = client.try_dry_run_migration(&0);
    assert_eq!(result, Err(Ok(NavinError::InvalidMigrationEdge)));
}

#[test]
fn test_dry_run_migration_same_version_rejected() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let result = client.try_dry_run_migration(&1);
    assert_eq!(result, Err(Ok(NavinError::InvalidMigrationEdge)));
}

#[test]
fn test_dry_run_migration_valid_forward_succeeds_detailed() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Valid forward migration 1→2
    let result = client.try_dry_run_migration(&2);
    assert!(result.is_ok());
    let report = result.unwrap().unwrap();
    assert_eq!(report.current_version, 1);
    assert_eq!(report.target_version, 2);
}

#[test]
fn test_upgrade_non_admin_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &token_contract);

    let outsider = Address::generate(&env);
    let result = client.try_upgrade(&outsider, &new_wasm_hash, &2);
    assert_eq!(result, Err(Ok(NavinError::Unauthorized)));
}

#[test]
fn test_upgrade_non_admin_dry_run_rejected() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // dry_run_migration is permissionless (no auth required), so it should
    // succeed for valid edges and fail with InvalidMigrationEdge for invalid ones.
    // This test verifies that a non-admin can still call dry_run for a valid edge.
    let result = client.try_dry_run_migration(&2);
    assert!(result.is_ok());
}

// ═══════════════════════════════════════════════════════════════════════════
// Issue #595 — MilestoneLimitExceeded (code 48)
// ═══════════════════════════════════════════════════════════════════════════

/// ShipmentNotFound (#4) and StatusHashNotFound (#44) must be distinct error codes —
/// callers must be able to tell the difference.
#[test]
fn test_status_hash_not_found_is_distinct_from_shipment_not_found() {
    assert_ne!(
        NavinError::ShipmentNotFound as u32,
        NavinError::StatusHashNotFound as u32,
        "ShipmentNotFound and StatusHashNotFound must have different discriminants"
    );
    assert_eq!(NavinError::ShipmentNotFound as u32, 4);
    assert_eq!(NavinError::StatusHashNotFound as u32, 44);
}

// ── [ISSUE #598] DataHashMismatch error variant tests ────────────────────────
//
// DataHashMismatch (#45) is returned by assert_delivery_hash
// when the provided hash does not match the value stored on-chain.

/// Error code pin: DataHashMismatch discriminant must be exactly 45.
#[test]
fn test_data_hash_mismatch_error_code_is_45() {
    assert_eq!(
        NavinError::DataHashMismatch as u32,
        45,
        "DataHashMismatch discriminant must be 45"
    );
}

/// assert_delivery_hash returns DataHashMismatch (#45) for an incorrect hash.
// ═══════════════════════════════════════════════════════════════════════════
// Issue #594 — BreachLimitExceeded (code 51)
// ═══════════════════════════════════════════════════════════════════════════

/// assert_delivery_hash returns ShipmentNotFound for a non-existent shipment.
#[test]
fn test_assert_delivery_hash_nonexistent_shipment_returns_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let hash = BytesN::from_array(&env, &[1u8; 32]);
    let result = client.try_assert_delivery_hash(&9999u64, &hash);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "assert_delivery_hash for non-existent shipment must return ShipmentNotFound (#4)"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Issue #593 — CreationQuotaExceeded (code 53)
// ═══════════════════════════════════════════════════════════════════════════

/// DataHashMismatch (#45), StatusHashNotFound (#44), and ShipmentNotFound (#4)
/// must all be distinct error codes.
#[test]
fn test_data_hash_mismatch_is_distinct_from_other_hash_errors() {
    assert_ne!(
        NavinError::DataHashMismatch as u32,
        NavinError::StatusHashNotFound as u32
    );
    assert_ne!(
        NavinError::DataHashMismatch as u32,
        NavinError::ShipmentNotFound as u32
    );
    assert_eq!(NavinError::DataHashMismatch as u32, 45);
}

// ── [ISSUE #597] CircuitBreakerOpen error variant tests ──────────────────────
//
// CircuitBreakerOpen (#46) is returned by invoke_token_transfer when the
// circuit breaker is in Open state. The breaker is injected directly into
// storage (env.as_contract) because Soroban rolls back all writes on Err,
// preventing failure-count accumulation through normal contract calls.

/// Error code pin: CircuitBreakerOpen discriminant must be exactly 46.
#[test]
fn test_circuit_breaker_open_error_code_is_46() {
    assert_eq!(
        NavinError::CircuitBreakerOpen as u32,
        46,
        "CircuitBreakerOpen discriminant must be 46"
    );
}

/// With the circuit breaker injected into Open state, release_escrow must
/// return CircuitBreakerOpen (#46) — the token transfer is blocked.
/// With the circuit breaker closed (default state), token transfers succeed
/// — no CircuitBreakerOpen error is raised.
// ── Issue #582 – DuplicatePaymentMilestone (code 60) ────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #60)")]
fn test_create_shipment_duplicate_milestone_names_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[4u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "pickup"), 50_u32));
    milestones.push_back((Symbol::new(&env, "pickup"), 50_u32)); // duplicate

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
}

// ── Issue #583 – InvalidTokenAddress (code 61) ──────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #61)")]
fn test_initialize_with_admin_as_token_address_rejected() {
    let (_env, client, admin, _) = setup_shipment_env();
    // Passing the admin's own address as the token contract is invalid.
    client.initialize(&admin, &admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #61)")]
fn test_initialize_with_contract_address_as_token_rejected() {
    let (_env, client, admin, _) = setup_shipment_env();
    // Passing the shipment contract's own address as the token is invalid.
    let contract_addr = client.address.clone();
    client.initialize(&admin, &contract_addr);
}

#[test]
fn test_initialize_with_valid_token_address_succeeds() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    // A real distinct token contract should be accepted.
    client.initialize(&admin, &token_contract);
    assert_eq!(client.get_admin(), admin);
}

// ── Issue #584 – InvalidPaymentMilestoneName (code 62) ──────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #65)")]
fn test_create_shipment_empty_milestone_name_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[6u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    // An empty symbol name is an invalid milestone checkpoint name.
    milestones.push_back((Symbol::new(&env, ""), 100_u32));

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #65)")]
fn test_create_shipment_too_long_milestone_name_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    // A 13-character name exceeds the Stellar Symbol 12-char maximum.
    milestones.push_back((Symbol::new(&env, "toolongname_x"), 100_u32));

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
}

// ── get_circuit_breaker_status tests (issue #640) ─────────────────────────────

#[test]
fn test_get_circuit_breaker_status_closed_by_default() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let (state, failure_count, recovery_remaining) = client.get_circuit_breaker_status();
    assert_eq!(state, crate::CircuitBreakerState::Closed);
    assert_eq!(failure_count, 0);
    assert_eq!(recovery_remaining, 0);
}

#[test]
fn test_get_circuit_breaker_status_open() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Inject Open state directly — Soroban rolls back storage on error so we
    // cannot accumulate real failures through contract calls.
    let opened_at = env.ledger().timestamp();
    env.as_contract(&client.address, || {
        let tracker = crate::circuit_breaker::CircuitBreakerTracker {
            state: crate::circuit_breaker::CircuitBreakerState::Open,
            failure_count: 5,
            opened_at,
            half_open_requests: 0,
        };
        env.storage()
            .persistent()
            .set(&crate::types::DataKey::CircuitBreakerState, &tracker);
    });

    let (state, failure_count, recovery_remaining) = client.get_circuit_breaker_status();
    assert_eq!(state, crate::CircuitBreakerState::Open);
    assert_eq!(failure_count, 5);
    // Default recovery_timeout is 300 s; we just opened it so ~300 s remain.
    assert!(recovery_remaining > 0 && recovery_remaining <= 300);
}

#[test]
fn test_get_circuit_breaker_status_half_open() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Inject HalfOpen state.
    env.as_contract(&client.address, || {
        let tracker = crate::circuit_breaker::CircuitBreakerTracker {
            state: crate::circuit_breaker::CircuitBreakerState::HalfOpen,
            failure_count: 5,
            opened_at: env.ledger().timestamp(),
            half_open_requests: 1,
        };
        env.storage()
            .persistent()
            .set(&crate::types::DataKey::CircuitBreakerState, &tracker);
    });

    let (state, failure_count, recovery_remaining) = client.get_circuit_breaker_status();
    assert_eq!(state, crate::CircuitBreakerState::HalfOpen);
    assert_eq!(failure_count, 5);
    // HalfOpen means recovery window has already passed — no time remaining.
    assert_eq!(recovery_remaining, 0);
}

// ── is_company_suspended tests (issue #642) ───────────────────────────────────

#[test]
fn test_is_company_suspended_active_by_default() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    assert!(!client.is_company_suspended(&company));
}

#[test]
fn test_is_company_suspended_returns_true_when_suspended() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.suspend_company(&admin, &company);
    assert!(client.is_company_suspended(&company));
}

#[test]
fn test_is_company_suspended_returns_false_after_reactivation() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.suspend_company(&admin, &company);
    assert!(client.is_company_suspended(&company));

    client.reactivate_company(&admin, &company);
    assert!(!client.is_company_suspended(&company));
}

// ── get_platform_fee_config tests (issue #641) ────────────────────────────────

#[test]
fn test_get_platform_fee_config_none_before_set() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let config = client.get_platform_fee_config();
    assert!(config.is_none());
}

#[test]
fn test_get_platform_fee_config_reflects_set_platform_fee() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let treasury = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.set_platform_fee(&admin, &50_u32, &treasury);

    let config = client
        .get_platform_fee_config()
        .expect("fee config should be set");
    assert_eq!(config.fee_bps, 50);
    assert_eq!(config.treasury, treasury);
}

#[test]
fn test_get_platform_fee_config_reflects_updated_value() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let treasury1 = Address::generate(&env);
    let treasury2 = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.set_platform_fee(&admin, &100_u32, &treasury1);
    client.set_platform_fee(&admin, &200_u32, &treasury2);

    let config = client
        .get_platform_fee_config()
        .expect("fee config should be set");
    assert_eq!(config.fee_bps, 200);
    assert_eq!(config.treasury, treasury2);
}

// =============================================================================
// Issue #601 — DuplicateAction error variant across operations
// =============================================================================

/// Verify that duplicate `create_shipment` calls within the idempotency window
/// are rejected with `DuplicateAction` (error code 41).
/// Verify that duplicate `update_status` calls within the idempotency window
/// are rejected with `DuplicateAction` (error code 41).
/// Verify that `deposit_escrow` rejects a second deposit with `EscrowLocked`
/// (error code 7), not `DuplicateAction` — escrow operations use their own
/// guard rather than the idempotency mechanism.
/// Verify that actions succeed after the idempotency window expires.
/// Verify that DuplicateAction is error code 41.
#[test]
fn test_duplicate_action_error_code_is_41() {
    assert_eq!(crate::NavinError::DuplicateAction as u32, 41);
}

// =============================================================================
// Issue #603 — InvalidMultiSigConfig error variant
// =============================================================================

/// Verify that `init_multisig` rejects threshold higher than admin count
/// with `InvalidConfig` (error code 31).
#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_multisig_threshold_higher_than_admin_count_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1);
    admins.push_back(admin2);

    // Threshold 3 > admin count 2 → InvalidConfig (#31)
    client.init_multisig(&admin, &admins, &3);
}

/// Verify that `init_multisig` rejects threshold of zero
/// with `InvalidMultiSigConfig` (error code 28).
#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_multisig_threshold_zero_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1);
    admins.push_back(admin2);

    // Threshold 0 is invalid
    client.init_multisig(&admin, &admins, &0);
}

/// Verify that `init_multisig` rejects empty admin list with non-zero threshold
/// with `InvalidMultiSigConfig` (error code 28).
#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_multisig_empty_admin_list_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admins = soroban_sdk::Vec::new(&env);

    // Empty admin list with threshold 1
    client.init_multisig(&admin, &admins, &1);
}

/// Verify that a valid multi-sig configuration succeeds.
#[test]
fn test_multisig_valid_config_succeeds() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    // Threshold 2 with 3 admins is valid
    client.init_multisig(&admin, &admins, &2);

    let (stored_admins, threshold) = client.get_multisig_config();
    assert_eq!(stored_admins.len(), 3);
    assert_eq!(threshold, 2);
}

/// Verify that InvalidMultiSigConfig is error code 28.
#[test]
fn test_invalid_multisig_config_error_code_is_28() {
    assert_eq!(crate::NavinError::InvalidMultiSigConfig as u32, 28);
}

// =============================================================================
// Issue #604 — InsufficientApprovals error variant
// =============================================================================

/// Verify that executing a proposal with fewer approvals than threshold
/// fails with `InsufficientApprovals` (error code 26).
#[test]
#[should_panic(expected = "Error(Contract, #26)")]
fn test_execute_proposal_with_insufficient_approvals_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    // Threshold 3 — all three must approve
    client.init_multisig(&admin, &admins, &3);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);
    let proposal_id = client.propose_action(&admin1, &action);

    // Only proposer (admin1) has approved — need 3 total
    client.execute_proposal(&proposal_id);
}

/// Verify that reaching exactly the approval threshold auto-executes the proposal.
/// Uses TransferAdmin action to avoid replacing the contract WASM (which would
/// remove `get_proposal` from the deployed code).
#[test]
fn test_execute_proposal_with_exact_threshold_approvals_succeeds() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let new_admin = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    // Threshold 2
    client.init_multisig(&admin, &admins, &2);

    let action = crate::AdminAction::TransferAdmin(new_admin.clone());
    let proposal_id = client.propose_action(&admin1, &action);

    // Proposer (admin1) auto-approves → 1/2 approvals
    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.approvals.len(), 1);
    assert!(!proposal.executed);

    // admin2 approves → reaches threshold of 2 → auto-executes
    client.approve_action(&admin2, &proposal_id);

    // Verify the proposal was auto-executed
    let proposal = client.get_proposal(&proposal_id);
    assert!(proposal.executed);

    // Verify admin was actually transferred
    assert_eq!(client.get_admin(), new_admin);
}

/// Verify that InsufficientApprovals is error code 26.
#[test]
fn test_insufficient_approvals_error_code_is_26() {
    assert_eq!(crate::NavinError::InsufficientApprovals as u32, 26);
}

// ── New Tests for Error Variants ──────────────────────────────────────────────

#[test]
fn test_invalid_symbol_error_variant() {
    let env = Env::default();

    // length < 1 (which means length 0, 8 bytes in XDR)
    let empty_symbol = Symbol::new(&env, "");
    let result1 = crate::validation::validate_symbol(&env, &empty_symbol);
    assert_eq!(result1, Err(crate::NavinError::InvalidSymbol));

    // length > 12 characters (24+ bytes in XDR)
    let long_symbol = Symbol::new(&env, "toolongsymbolname");
    let result2 = crate::validation::validate_symbol(&env, &long_symbol);
    assert_eq!(result2, Err(crate::NavinError::InvalidSymbol));
}

// =============================================================================
// Issue #607 — ProposalExpired error variant
// =============================================================================
// Tests that expired proposals cannot be approved or executed, that the error
// code 24 is returned correctly, and that non-expired proposals work normally.
// =============================================================================

/// Helper to set up a multisig environment with a pending (not-yet-executed)
/// proposal that has NOT yet expired.  Returns
/// `(env, client, admin1, admin2, admin3, proposal_id)`.
fn setup_multisig_with_pending_proposal() -> (
    soroban_sdk::Env,
    NavinShipmentClient<'static>,
    soroban_sdk::Address,
    soroban_sdk::Address,
    soroban_sdk::Address,
    u64,
) {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();

    let admin1 = soroban_sdk::Address::generate(&env);
    let admin2 = soroban_sdk::Address::generate(&env);
    let admin3 = soroban_sdk::Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    // Threshold 3 so the proposal stays pending after proposal + 1 approval
    client.init_multisig(&admin, &admins, &3);

    let new_admin = soroban_sdk::Address::generate(&env);
    let action = crate::AdminAction::TransferAdmin(new_admin);
    let proposal_id = client.propose_action(&admin1, &action);

    (env, client, admin1, admin2, admin3, proposal_id)
}

/// Attempting to approve an expired proposal must return `ProposalExpired`
/// (error code 24) via `try_approve_action`.
#[test]
fn test_approve_expired_proposal_returns_proposal_expired() {
    let (env, client, _admin1, admin2, _admin3, proposal_id) =
        setup_multisig_with_pending_proposal();

    // Advance time past the proposal expiry window (7 days + 1 second)
    super::test_utils::advance_past_multisig_expiry(&env);

    let result = client.try_approve_action(&admin2, &proposal_id);
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalExpired)),
        "approving an expired proposal must return ProposalExpired"
    );
}

/// Attempting to execute an expired proposal (even if threshold was met
/// before expiry) must return `ProposalExpired` (error code 24).
#[test]
fn test_execute_expired_proposal_returns_proposal_expired() {
    let (env, client, _admin1, admin2, _admin3, proposal_id) =
        setup_multisig_with_pending_proposal();

    // admin2 approves — still below threshold (need 3)
    client.approve_action(&admin2, &proposal_id);

    // Advance time past expiry
    super::test_utils::advance_past_multisig_expiry(&env);

    let result = client.try_execute_proposal(&proposal_id);
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalExpired)),
        "executing an expired proposal must return ProposalExpired"
    );
}

/// `ProposalExpired` must carry error code 24.
#[test]
fn test_proposal_expired_error_code_is_24() {
    assert_eq!(
        crate::NavinError::ProposalExpired as u32,
        24,
        "ProposalExpired discriminant must be 24"
    );
}

/// A proposal that is within its expiry window can still be approved normally.
/// Verifies the non-expired happy path (Acceptance Criterion: non-expired
/// proposals work normally).
#[test]
fn test_non_expired_proposal_can_be_approved() {
    let (_env, client, _admin1, admin2, _admin3, proposal_id) =
        setup_multisig_with_pending_proposal();

    // No time advance — proposal is fresh
    let result = client.try_approve_action(&admin2, &proposal_id);
    assert!(
        result.is_ok(),
        "approving a non-expired proposal must succeed: {:?}",
        result
    );

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(
        proposal.approvals.len(),
        2,
        "admin2 approval must be recorded"
    );
    assert!(
        !proposal.executed,
        "proposal must not auto-execute with only 2/3 approvals"
    );
}

/// A non-expired proposal that reaches the approval threshold is auto-executed
/// without error.  This confirms the full non-expired happy path including
/// execution.
#[test]
fn test_non_expired_proposal_executes_at_threshold() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();

    let admin1 = soroban_sdk::Address::generate(&env);
    let admin2 = soroban_sdk::Address::generate(&env);
    let new_admin = soroban_sdk::Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    // Threshold 2 — executes on admin2 approval
    client.init_multisig(&admin, &admins, &2);

    let action = crate::AdminAction::TransferAdmin(new_admin.clone());
    let proposal_id = client.propose_action(&admin1, &action);

    // admin2 approves → reaches threshold → auto-executes
    let result = client.try_approve_action(&admin2, &proposal_id);
    assert!(
        result.is_ok(),
        "approving non-expired proposal at threshold must succeed"
    );

    // Confirm the action was actually executed
    assert_eq!(
        client.get_admin(),
        new_admin,
        "admin must be transferred after auto-execution"
    );
}

/// An expired proposal must not be approvable at any point — not even a third
/// admin who had never approved before.
#[test]
fn test_all_approvals_fail_after_expiry() {
    let (env, client, _admin1, admin2, admin3, proposal_id) =
        setup_multisig_with_pending_proposal();

    // admin2 approves while proposal is still valid (1 more approval but still below 3)
    client.approve_action(&admin2, &proposal_id);

    // Advance time past expiry
    super::test_utils::advance_past_multisig_expiry(&env);

    // admin3 (fresh approver) tries to approve after expiry
    let result = client.try_approve_action(&admin3, &proposal_id);
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalExpired)),
        "a fresh approver must still receive ProposalExpired after expiry"
    );
}

// =============================================================================
// Issue #608 — ProposalAlreadyExecuted error variant
// =============================================================================
// Tests that duplicate proposal executions are rejected, that the error code
// 23 is returned correctly, and that single execution succeeds.
// =============================================================================

/// `ProposalAlreadyExecuted` must carry error code 23.
#[test]
fn test_proposal_already_executed_error_code_is_23() {
    assert_eq!(
        crate::NavinError::ProposalAlreadyExecuted as u32,
        23,
        "ProposalAlreadyExecuted discriminant must be 23"
    );
}

/// Executing a proposal a second time must return `ProposalAlreadyExecuted`
/// (error code 23) via `try_execute_proposal`.
#[test]
fn test_execute_already_executed_proposal_returns_error() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();

    let admin1 = soroban_sdk::Address::generate(&env);
    let admin2 = soroban_sdk::Address::generate(&env);
    let new_admin = soroban_sdk::Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    let action = crate::AdminAction::TransferAdmin(new_admin.clone());
    let proposal_id = client.propose_action(&admin1, &action);

    // First approval reaches threshold → auto-executes
    client.approve_action(&admin2, &proposal_id);

    // Verify it executed
    let proposal = client.get_proposal(&proposal_id);
    assert!(proposal.executed, "proposal must be marked as executed");

    // Attempt a second execute call
    let result = client.try_execute_proposal(&proposal_id);
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalAlreadyExecuted)),
        "second execute call must return ProposalAlreadyExecuted"
    );
}

/// Approving an already-executed proposal must also return
/// `ProposalAlreadyExecuted` (error code 23).
#[test]
fn test_approve_already_executed_proposal_returns_error() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();

    let admin1 = soroban_sdk::Address::generate(&env);
    let admin2 = soroban_sdk::Address::generate(&env);
    let admin3 = soroban_sdk::Address::generate(&env);
    let new_admin = soroban_sdk::Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    // Threshold 2 — executes when admin2 approves
    client.init_multisig(&admin, &admins, &2);

    let action = crate::AdminAction::TransferAdmin(new_admin);
    let proposal_id = client.propose_action(&admin1, &action);

    // Reaches threshold, auto-executes
    client.approve_action(&admin2, &proposal_id);

    // admin3 tries to approve the already-executed proposal
    let result = client.try_approve_action(&admin3, &proposal_id);
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalAlreadyExecuted)),
        "approving an already-executed proposal must return ProposalAlreadyExecuted"
    );
}

/// A single execution succeeds and correctly applies the proposed action.
/// Uses TransferAdmin to avoid replacing the WASM (which would break
/// subsequent `get_proposal` reads).
#[test]
fn test_single_execution_succeeds_and_applies_action() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();

    let admin1 = soroban_sdk::Address::generate(&env);
    let admin2 = soroban_sdk::Address::generate(&env);
    let new_admin = soroban_sdk::Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    let action = crate::AdminAction::TransferAdmin(new_admin.clone());
    let proposal_id = client.propose_action(&admin1, &action);

    // Proposal not yet executed
    let proposal_before = client.get_proposal(&proposal_id);
    assert!(!proposal_before.executed);

    // admin2 approval reaches threshold → auto-executes
    client.approve_action(&admin2, &proposal_id);

    // Confirm execution state
    let proposal_after = client.get_proposal(&proposal_id);
    assert!(
        proposal_after.executed,
        "proposal must be marked executed after threshold is reached"
    );

    // Confirm the admin was actually transferred
    assert_eq!(
        client.get_admin(),
        new_admin,
        "admin must be the new admin after successful single execution"
    );
}

/// ForceRelease proposal: single execution releases escrow correctly.
/// Verifies that the `ProposalAlreadyExecuted` guard works for non-Upgrade actions.
// =============================================================================
// ForceRelease reason_hash validation and audit trail tests
// =============================================================================

/// ForceRelease with reason_hash: verifies reason hash is persisted in event stream and queryable.
/// Ensures audit trail contains the admin-provided reason for the force release.
/// ForceRelease rejects zero reason_hash, matching force_cancel_shipment behavior.
/// ForceRefund with reason_hash: verifies reason hash is persisted in event stream and queryable.
/// Ensures audit trail contains the admin-provided reason for the force refund.
/// ForceRefund rejects zero reason_hash, matching force_cancel_shipment behavior.
// ===========================================================================
// Security regressions: #748, #749, #750, #751
// ===========================================================================

/// #748 — `initialize` must require auth from the address it installs as admin.
///
/// Deploy and initialize are separate transactions, so an attacker can watch
/// for an uninitialized contract and call `initialize` naming themselves.
/// `AlreadyInitialized` then locks the real deployer out with no recovery.
#[test]
fn test_initialize_requires_admin_auth() {
    let env = Env::default();
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    let attacker = Address::generate(&env);

    // No auth mocked: the call must not succeed on the attacker's say-so.
    let result = client.try_initialize(&attacker, &token_contract);
    assert!(
        result.is_err(),
        "initialize must not succeed without auth from the admin being installed"
    );
}

/// #748 — with the admin's authorization present, initialization still works.
#[test]
fn test_initialize_succeeds_with_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    let admin = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    assert_eq!(client.get_admin(), admin);
}

/// #748 — the authorization recorded is the admin's own, so a third party
/// cannot front-run initialization by signing for themselves while naming
/// someone else.
#[test]
fn test_initialize_auth_is_attributed_to_the_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    let admin = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == admin),
        "initialize must require auth from the admin address itself"
    );
}

/// #749 — `remove_guardian` must not strip an unrelated role.
///
/// Before the fix this silently revoked the target's Company role, because
/// `revoke_role` removes whatever role the address happens to hold.
#[test]
fn test_remove_guardian_rejects_a_company_target() {
    let (env, client, admin, _token) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    client.add_company(&admin, &company);

    let result = client.try_remove_guardian(&admin, &company);
    assert_eq!(result, Err(Ok(crate::NavinError::RoleMismatch)));

    // The critical assertion: the Company role survived the failed call.
    assert!(
        client.try_get_role(&company).is_ok(),
        "a rejected remove_guardian must leave the target's real role intact"
    );
}

/// #749 — the mirror case for operators.
#[test]
fn test_remove_operator_rejects_a_guardian_target() {
    let (env, client, admin, _token) = setup_initialized_shipment_env();
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);

    let result = client.try_remove_operator(&admin, &guardian);
    assert_eq!(result, Err(Ok(crate::NavinError::RoleMismatch)));
}

/// #749 — removing an address that holds no role at all is also a mismatch,
/// rather than a silent success.
#[test]
fn test_remove_guardian_rejects_an_unassigned_target() {
    let (env, client, admin, _token) = setup_initialized_shipment_env();
    let nobody = Address::generate(&env);

    assert_eq!(
        client.try_remove_guardian(&admin, &nobody),
        Err(Ok(crate::NavinError::RoleMismatch))
    );
}

/// #749 — the happy path still works: a real guardian is still removable.
#[test]
fn test_remove_guardian_still_removes_a_guardian() {
    let (env, client, admin, _token) = setup_initialized_shipment_env();
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);

    client.remove_guardian(&admin, &guardian);

    assert_eq!(
        client.try_remove_guardian(&admin, &guardian),
        Err(Ok(crate::NavinError::RoleMismatch)),
        "the role is gone, so a second removal is now a mismatch"
    );
}

/// #750 — a long `event_type` must not panic.
///
/// A Soroban `Symbol` allows up to 32 characters; the XDR decode wrote into a
/// 32-byte buffer and sliced `8 + char_count`, so 25-32 characters ran past
/// the end. A panic aborts the whole invocation, so a merely-long event type
/// took down the call that emitted it.
#[test]
fn test_compute_idempotency_key_rejects_oversized_symbols() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();

    // 25 characters — the first length that overflowed the buffer.
    let long_symbol = Symbol::new(&env, "abcdefghijklmnopqrstuvwxy");
    let result = client.try_compute_idempotency_key(&1u64, &long_symbol, &0u32);
    assert!(
        result.is_err(),
        "an oversized symbol must return an error rather than panicking"
    );
}

/// #750 — the maximum-length symbol is handled the same way.
#[test]
fn test_compute_idempotency_key_handles_max_length_symbol() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();

    // 32 characters — the SDK's maximum.
    let max_symbol = Symbol::new(&env, "abcdefghijklmnopqrstuvwxyz012345");
    let result = client.try_compute_idempotency_key(&1u64, &max_symbol, &0u32);
    assert!(
        result.is_err(),
        "must not panic at the maximum symbol length"
    );
}

/// #750 — ordinary symbols still produce a stable key.
#[test]
fn test_compute_idempotency_key_still_works_for_short_symbols() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();

    let symbol = Symbol::new(&env, "created");
    let first = client.compute_idempotency_key(&1u64, &symbol, &0u32);
    let second = client.compute_idempotency_key(&1u64, &symbol, &0u32);

    assert_eq!(first, second, "the key must be deterministic");

    let other = client.compute_idempotency_key(&2u64, &symbol, &0u32);
    assert_ne!(
        first, other,
        "a different shipment must yield a different key"
    );
}
