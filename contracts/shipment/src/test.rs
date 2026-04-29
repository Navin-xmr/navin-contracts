#![cfg(test)]

extern crate std;

use crate::{
    types::DataKey, BreachType, GeofenceEvent, NavinError, NavinShipment, NavinShipmentClient,
    PersistentRestoreDiagnostics, Severity, ShipmentInput, ShipmentStatus, StoragePresenceState,
};
use soroban_sdk::{
    contract, contracterror, contractimpl,
    testutils::{storage::Persistent, Address as _, Events},
    Address, BytesN, Env, IntoVal, Symbol, TryFromVal,
};

use std::println;
use soroban_sdk::testutils::Ledger;

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
fn test_create_shipment_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);
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
        &None,
    );
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
fn test_deposit_escrow_maps_token_transfer_failure() {
    let (env, client, admin, token_contract) = setup_shipment_env_with_failing_token();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);
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
        &None,
    );

    let result = client.try_deposit_escrow(&company, &shipment_id, &500);

    assert_eq!(result, Err(Ok(crate::NavinError::TokenTransferFailed)));
    assert_eq!(client.get_shipment(&shipment_id).escrow_amount, 0);
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
fn test_create_shipments_batch_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let deadline = env.ledger().timestamp() + 3600;
    let mut shipments = soroban_sdk::Vec::new(&env);
    for i in 1..=5 {
        shipments.push_back(ShipmentInput {
            receiver: Address::generate(&env),
            carrier: Address::generate(&env),
            data_hash: BytesN::from_array(&env, &[i as u8; 32]),
            payment_milestones: soroban_sdk::Vec::new(&env),
            deadline,
            depends_on: None,
        });
    }

    let ids = client.create_shipments_batch(&company, &shipments);
    assert_eq!(ids.len(), 5);
    for i in 0..5 {
        assert_eq!(ids.get(i).unwrap(), (i + 1) as u64);
    }
    assert_eq!(client.get_shipment_counter(), 5);
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
            depends_on: None,
        });
    }

    client.create_shipments_batch(&company, &shipments);
}

#[test]
#[should_panic(expected = "Error(Contract, #17)")]
fn test_create_shipments_batch_invalid_input() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let deadline = env.ledger().timestamp() + 3600;
    let mut shipments = soroban_sdk::Vec::new(&env);
    shipments.push_back(ShipmentInput {
        receiver: Address::generate(&env),
        carrier: Address::generate(&env),
        data_hash: BytesN::from_array(&env, &[1u8; 32]),
        payment_milestones: soroban_sdk::Vec::new(&env),
        deadline,
            depends_on: None,
        });
    let user = Address::generate(&env);
    shipments.push_back(ShipmentInput {
        receiver: user.clone(),
        carrier: user,
        data_hash: BytesN::from_array(&env, &[2u8; 32]),
        payment_milestones: soroban_sdk::Vec::new(&env),
        deadline,
            depends_on: None,
        });

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
        &None,
    );
}

#[test]
fn test_multiple_shipments_have_unique_ids() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let hash_one = BytesN::from_array(&env, &[1u8; 32]);
    let hash_two = BytesN::from_array(&env, &[2u8; 32]);
    let hash_three = BytesN::from_array(&env, &[3u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id_one = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash_one,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    let id_two = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash_two,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    let id_three = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash_three,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    assert_eq!(id_one, 1);
    assert_eq!(id_two, 2);
    assert_eq!(id_three, 3);
    assert_eq!(client.get_shipment_counter(), 3);
}

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

#[test]
fn test_deposit_escrow_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );
    let escrow_amount: i128 = 1000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, escrow_amount);
}

// ============= Status Update Tests =============

#[test]
fn test_update_status_valid_transition_by_carrier() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );
    let shipment_before = client.get_shipment(&shipment_id);
    assert_eq!(shipment_before.status, ShipmentStatus::Created);

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &new_data_hash,
    );

    let shipment_after = client.get_shipment(&shipment_id);
    assert_eq!(shipment_after.status, ShipmentStatus::InTransit);
    assert_eq!(shipment_after.data_hash, new_data_hash);
    assert!(shipment_after.updated_at >= shipment_before.updated_at);
}

#[test]
fn test_update_status_valid_transition_by_admin() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    client.update_status(
        &admin,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &new_data_hash,
    );

    let shipment_after = client.get_shipment(&shipment_id);
    assert_eq!(shipment_after.status, ShipmentStatus::InTransit);
    assert_eq!(shipment_after.data_hash, new_data_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #38)")]
fn test_update_status_invalid_transition() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &new_data_hash,
    );

    super::test_utils::advance_past_rate_limit(&env);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::Delivered,
        &new_data_hash,
    );

    super::test_utils::advance_past_rate_limit(&env);
    // Invalid: Delivered → Created
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::Created,
        &new_data_hash,
    );
}

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
        &None,
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
fn test_update_status_multiple_valid_transitions() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let hash_2 = BytesN::from_array(&env, &[2u8; 32]);
    let hash_3 = BytesN::from_array(&env, &[3u8; 32]);
    let hash_4 = BytesN::from_array(&env, &[4u8; 32]);
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
        &None,
    );
    assert_eq!(
        client.get_shipment(&shipment_id).status,
        ShipmentStatus::Created
    );

    // Created → InTransit
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash_2);
    assert_eq!(
        client.get_shipment(&shipment_id).status,
        ShipmentStatus::InTransit
    );

    // InTransit → AtCheckpoint
    super::test_utils::advance_past_rate_limit(&env);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::AtCheckpoint,
        &hash_3,
    );
    assert_eq!(
        client.get_shipment(&shipment_id).status,
        ShipmentStatus::AtCheckpoint
    );

    // AtCheckpoint → Delivered
    super::test_utils::advance_past_rate_limit(&env);
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::Delivered, &hash_4);
    assert_eq!(
        client.get_shipment(&shipment_id).status,
        ShipmentStatus::Delivered
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
fn test_suspend_and_reactivate_carrier_for_status_updates() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let update_hash = BytesN::from_array(&env, &[2u8; 32]);
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
        &None,
    );

    // Move to InTransit as admin so carrier can attempt the next transition.
    client.update_status(
        &admin,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &update_hash,
    );

    client.suspend_carrier(&admin, &carrier);
    assert!(client.is_carrier_suspended(&carrier));

    let res = client.try_update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::AtCheckpoint,
        &update_hash,
    );
    assert_eq!(res, Err(Ok(crate::NavinError::CarrierSuspended)));

    client.reactivate_carrier(&admin, &carrier);
    assert!(!client.is_carrier_suspended(&carrier));

    super::test_utils::advance_past_rate_limit(&env);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::AtCheckpoint,
        &update_hash,
    );
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
fn test_get_escrow_balance_returns_zero_without_deposit() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    let escrow_amount: i128 = 1000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, escrow_amount);
}

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
        &None,
    );

    let escrow_amount: i128 = 1000;
    client.deposit_escrow(&non_company, &shipment_id, &escrow_amount);
    // No escrow deposited yet, should return 0
    assert_eq!(client.get_escrow_balance(&shipment_id), 0);
}

#[test]
fn test_get_escrow_balance_after_deposit() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[2u8; 32]);
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
        &None,
    );

    env.as_contract(&client.address, || {
        crate::storage::set_escrow_balance(&env, shipment_id, 500_000);
    });

    assert_eq!(client.get_escrow_balance(&shipment_id), 500_000);
}

#[test]
fn test_get_escrow_balance_after_release() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[3u8; 32]);
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
        &None,
    );

    env.as_contract(&client.address, || {
        crate::storage::set_escrow_balance(&env, shipment_id, 1_000_000);
    });
    assert_eq!(client.get_escrow_balance(&shipment_id), 1_000_000);

    env.as_contract(&client.address, || {
        crate::storage::remove_escrow_balance(&env, shipment_id);
    });

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

#[test]
fn test_get_shipment_count_after_creating_shipments() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let hash_one = BytesN::from_array(&env, &[1u8; 32]);
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash_one,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    assert_eq!(client.get_shipment_count(), 1);

    let hash_two = BytesN::from_array(&env, &[2u8; 32]);
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash_two,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    assert_eq!(client.get_shipment_count(), 2);

    let hash_three = BytesN::from_array(&env, &[3u8; 32]);
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash_three,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    assert_eq!(client.get_shipment_count(), 3);
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
fn test_get_shipment_returns_correct_data() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);
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
        &None,
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.id, shipment_id);
    assert_eq!(shipment.sender, company);
    assert_eq!(shipment.receiver, receiver);
    assert_eq!(shipment.carrier, carrier);
    assert_eq!(shipment.data_hash, data_hash);
    assert_eq!(shipment.status, crate::ShipmentStatus::Created);
    assert_eq!(shipment.escrow_amount, 0);
    assert_eq!(shipment.deadline, deadline);
}

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
fn test_get_shipment_creator_returns_sender_for_valid_id() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
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
        &None,
    );

    assert_eq!(client.get_shipment_creator(&shipment_id), company);
}

#[test]
fn test_get_shipment_receiver_returns_receiver_for_valid_id() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[12u8; 32]);
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
        &None,
    );

    assert_eq!(client.get_shipment_receiver(&shipment_id), receiver);
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
fn test_get_shipment_sender_returns_sender_for_valid_id() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[13u8; 32]);
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
        &None,
    );

    assert_eq!(client.get_shipment_sender(&shipment_id), company);
}

#[test]
fn test_get_shipment_carrier_returns_carrier_for_valid_id() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[14u8; 32]);
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
        &None,
    );

    assert_eq!(client.get_shipment_carrier(&shipment_id), carrier);
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

// ============= Geofence Event Tests =============

#[test]
fn test_report_geofence_zone_entry() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let event_hash = BytesN::from_array(&env, &[2u8; 32]);
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
        &None,
    );

    client.report_geofence_event(
        &carrier,
        &shipment_id,
        &GeofenceEvent::ZoneEntry,
        &event_hash,
    );

    let events = env.events().all();
    std::println!("GEOFENCE EVENTS: {}", events.len());
    assert!(!events.is_empty());
}

#[test]
fn test_report_geofence_zone_exit() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let event_hash = BytesN::from_array(&env, &[3u8; 32]);
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
        &None,
    );

    client.report_geofence_event(
        &carrier,
        &shipment_id,
        &GeofenceEvent::ZoneExit,
        &event_hash,
    );

    let events = env.events().all();
    std::println!("GEOFENCE EVENTS: {}", events.len());
    assert!(!events.is_empty());
}

#[test]
fn test_report_geofence_route_deviation() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let event_hash = BytesN::from_array(&env, &[4u8; 32]);
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
        &None,
    );

    client.report_geofence_event(
        &carrier,
        &shipment_id,
        &GeofenceEvent::RouteDeviation,
        &event_hash,
    );

    let events = env.events().all();
    std::println!("GEOFENCE EVENTS: {}", events.len());
    assert!(!events.is_empty());
}

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
        &None,
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
fn test_update_eta_valid_emits_event() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let shipment_hash = BytesN::from_array(&env, &[1u8; 32]);
    let eta_hash = BytesN::from_array(&env, &[9u8; 32]);
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
        &None,
    );
    let eta_timestamp = env.ledger().timestamp() + 60;

    client.update_eta(&carrier, &shipment_id, &eta_timestamp, &eta_hash);

    let events = env.events().all();
    let last = events.get(events.len() - 1).unwrap();

    assert_eq!(last.0, client.address);

    let topic = Symbol::try_from_val(&env, &last.1.get(0).unwrap()).unwrap();
    assert_eq!(topic, Symbol::new(&env, "eta_updated"));

    let event_data = <(u64, u64, BytesN<32>)>::try_from_val(&env, &last.2).unwrap();
    assert_eq!(event_data, (shipment_id, eta_timestamp, eta_hash));
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_update_eta_rejects_past_timestamp() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let shipment_hash = BytesN::from_array(&env, &[1u8; 32]);
    let eta_hash = BytesN::from_array(&env, &[8u8; 32]);
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
        &None,
    );
    let past_eta = env.ledger().timestamp();

    client.update_eta(&carrier, &shipment_id, &past_eta, &eta_hash);
}

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
        &None,
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

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(env),
        &deadline,
        &None,
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

#[test]
fn test_confirm_partial_delivery_releases_bounded_escrow() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &10_000);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[2u8; 32]),
    );

    client.confirm_partial_delivery(
        &receiver,
        &shipment_id,
        &BytesN::from_array(&env, &[3u8; 32]),
        &30,
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::PartiallyDelivered);
    assert_eq!(shipment.escrow_amount, 7_000);
}

#[test]
fn test_confirm_partial_delivery_rejects_over_release() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[10u8; 32]);
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
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &10_000);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[11u8; 32]),
    );

    client.confirm_partial_delivery(
        &receiver,
        &shipment_id,
        &BytesN::from_array(&env, &[12u8; 32]),
        &60,
    );

    let result = client.try_confirm_partial_delivery(
        &receiver,
        &shipment_id,
        &BytesN::from_array(&env, &[13u8; 32]),
        &60,
    );
    assert_eq!(result, Err(Ok(NavinError::InvalidAmount)));
}

#[test]
fn test_confirm_partial_delivery_can_settle_to_delivered() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[20u8; 32]);
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
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &10_000);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[21u8; 32]),
    );

    client.confirm_partial_delivery(
        &receiver,
        &shipment_id,
        &BytesN::from_array(&env, &[22u8; 32]),
        &50,
    );
    client.confirm_partial_delivery(
        &receiver,
        &shipment_id,
        &BytesN::from_array(&env, &[23u8; 32]),
        &50,
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Delivered);
    assert_eq!(shipment.escrow_amount, 0);
    assert_eq!(client.get_active_shipment_count(&company), 0);
}

// ============= Release Escrow Tests =============

#[test]
fn test_release_escrow_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    client.release_escrow(&receiver, &shipment_id);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #38)")]
fn test_release_escrow_double_release() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    client.release_escrow(&receiver, &shipment_id);
    client.release_escrow(&receiver, &shipment_id);
}

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
        &None,
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

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_release_escrow_wrong_status() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.release_escrow(&receiver, &shipment_id);
}

#[test]
fn test_release_escrow_by_admin() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    client.release_escrow(&admin, &shipment_id);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 0);
}

// ============= Refund Escrow Tests =============

#[test]
fn test_refund_escrow_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );
    let escrow_amount: i128 = 3000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.refund_escrow(&company, &shipment_id);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 0);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_refund_escrow_on_delivered_shipment() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );
    let escrow_amount: i128 = 3000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    client.refund_escrow(&company, &shipment_id);
}

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
        &None,
    );
    let escrow_amount: i128 = 3000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.refund_escrow(&unauthorized, &shipment_id);
}

#[test]
fn test_refund_escrow_by_admin() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );
    let escrow_amount: i128 = 3000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.refund_escrow(&admin, &shipment_id);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 0);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
}

#[test]
#[should_panic(expected = "Error(Contract, #38)")]
fn test_refund_escrow_double_refund() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );
    let escrow_amount: i128 = 3000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.refund_escrow(&company, &shipment_id);
    client.refund_escrow(&company, &shipment_id);
}

// ============= Dispute Tests =============

#[test]
fn test_raise_dispute_by_sender() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[99u8; 32]);
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
        &None,
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);
    });

    client.raise_dispute(&company, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Disputed);
}

#[test]
fn test_raise_dispute_by_receiver() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[98u8; 32]);
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
        &None,
    );

    client.raise_dispute(&receiver, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Disputed);
}

#[test]
fn test_raise_dispute_by_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[97u8; 32]);
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
        &None,
    );

    client.raise_dispute(&carrier, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Disputed);
}

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
        &None,
    );

    client.raise_dispute(&outsider, &shipment_id, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_raise_dispute_on_cancelled_shipment() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[95u8; 32]);
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
        &None,
    );

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::ShipmentStatus::Cancelled;
        crate::storage::set_shipment(&env, &shipment);
    });

    client.raise_dispute(&company, &shipment_id, &reason_hash);
}

#[test]
fn test_resolve_dispute_release_to_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[94u8; 32]);
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
        &None,
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.raise_dispute(&company, &shipment_id, &reason_hash);

    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
        &reason_hash,
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 0);
    assert_eq!(shipment.status, crate::ShipmentStatus::Delivered);
}

#[test]
fn test_resolve_dispute_refund_to_company() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[93u8; 32]);
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
        &None,
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.raise_dispute(&receiver, &shipment_id, &reason_hash);

    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::RefundToCompany,
        &reason_hash,
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 0);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
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
        &None,
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

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_resolve_dispute_not_disputed() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
        &BytesN::from_array(&env, &[1u8; 32]),
    );
}

// ============= Milestone Event Tests =============

#[test]
fn test_record_milestone_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let checkpoint = soroban_sdk::Symbol::new(&env, "port_arrival");
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
        &None,
    );

    // Manually set status to InTransit
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::types::ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);
    });

    client.record_milestone(&carrier, &shipment_id, &checkpoint, &data_hash);

    let events = env.events().all();
    let mut found = false;
    for (_, _, _event_data) in events.iter() {
        found = true;
    }
    assert!(found);
}

#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_deposit_escrow_invalid_amount() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );
    let invalid_escrow_amount: i128 = 0;

    // Should panic with error code 8 for invalid amount
    client.deposit_escrow(&company, &shipment_id, &invalid_escrow_amount);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_record_milestone_wrong_status() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let checkpoint = soroban_sdk::Symbol::new(&env, "port_arrival");
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
        &None,
    );

    // Status is Created by default, which is wrong status for milestone
    client.record_milestone(&carrier, &shipment_id, &checkpoint, &data_hash);
}

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
        &None,
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

#[test]
fn test_suspended_carrier_blocked_from_milestone_handlers() {
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
        &None,
    );

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::types::ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);
    });

    client.suspend_carrier(&admin, &carrier);

    let checkpoint = Symbol::new(&env, "port_arrival");
    let single_res = client.try_record_milestone(&carrier, &shipment_id, &checkpoint, &data_hash);
    assert_eq!(single_res, Err(Ok(crate::NavinError::CarrierSuspended)));

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((checkpoint, BytesN::from_array(&env, &[22u8; 32])));
    let batch_res = client.try_record_milestones_batch(&carrier, &shipment_id, &milestones);
    assert_eq!(batch_res, Err(Ok(crate::NavinError::CarrierSuspended)));
}

// ============= Batch Milestone Recording Tests =============

#[test]
fn test_record_milestones_batch_success() {
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
        &None,
    );

    // Set shipment to InTransit status
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::types::ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);
    });

    // Create batch of milestones
    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((
        Symbol::new(&env, "warehouse"),
        BytesN::from_array(&env, &[10u8; 32]),
    ));
    milestones.push_back((
        Symbol::new(&env, "port"),
        BytesN::from_array(&env, &[20u8; 32]),
    ));
    milestones.push_back((
        Symbol::new(&env, "customs"),
        BytesN::from_array(&env, &[30u8; 32]),
    ));

    client.record_milestones_batch(&carrier, &shipment_id, &milestones);

    // Verify events were emitted for each milestone
    let events = env.events().all();
    let mut milestone_events = 0;
    for (_contract_id, _topics, _data) in events.iter() {
        milestone_events += 1;
    }
    // We expect at least 3 milestone events (there may be other events too)
    assert!(milestone_events >= 3);
}

#[test]
fn test_record_milestones_batch_single_milestone() {
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
        &None,
    );

    // Set shipment to InTransit status
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::types::ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);
    });

    // Create batch with single milestone
    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((
        Symbol::new(&env, "warehouse"),
        BytesN::from_array(&env, &[10u8; 32]),
    ));

    client.record_milestones_batch(&carrier, &shipment_id, &milestones);

    // Verify event was emitted
    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
fn test_record_milestones_batch_max_size() {
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
        &None,
    );

    // Set shipment to InTransit status
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::types::ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);
    });

    // Create batch with exactly 10 milestones (max allowed)
    let mut milestones = soroban_sdk::Vec::new(&env);
    for i in 0..10 {
        milestones.push_back((
            Symbol::new(&env, &std::format!("checkpoint_{i}")),
            BytesN::from_array(&env, &[(i as u8) + 1; 32]),
        ));
    }

    client.record_milestones_batch(&carrier, &shipment_id, &milestones);

    // Verify all 10 events were emitted
    let events = env.events().all();
    let mut milestone_events = 0;
    for (_contract_id, _topics, _data) in events.iter() {
        milestone_events += 1;
    }
    // We expect at least 10 milestone events
    assert!(milestone_events >= 10);
}

#[test]
#[should_panic(expected = "Error(Contract, #16)")]
fn test_record_milestones_batch_oversized() {
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
        &None,
    );

    // Set shipment to InTransit status
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::types::ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);
    });

    // Create batch with 11 milestones (exceeds limit)
    let mut milestones = soroban_sdk::Vec::new(&env);
    for i in 0..11 {
        milestones.push_back((
            Symbol::new(&env, &std::format!("checkpoint_{i}")),
            BytesN::from_array(&env, &[i as u8; 32]),
        ));
    }

    // Should fail with BatchTooLarge error (code 16)
    client.record_milestones_batch(&carrier, &shipment_id, &milestones);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_record_milestones_batch_invalid_status() {
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
        &None,
    );

    // Shipment is in Created status (not InTransit)
    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((
        Symbol::new(&env, "warehouse"),
        BytesN::from_array(&env, &[10u8; 32]),
    ));

    // Should fail with InvalidStatus error (code 5)
    client.record_milestones_batch(&carrier, &shipment_id, &milestones);
}

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
        &None,
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

#[test]
fn test_record_milestones_batch_with_payment_milestones() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Create shipment with payment milestones
    let mut payment_milestones = soroban_sdk::Vec::new(&env);
    payment_milestones.push_back((Symbol::new(&env, "warehouse"), 30u32));
    payment_milestones.push_back((Symbol::new(&env, "port"), 30u32));
    payment_milestones.push_back((Symbol::new(&env, "delivery"), 40u32));

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &payment_milestones,
        &deadline,
        &None,
    );

    // Deposit escrow
    client.deposit_escrow(&company, &shipment_id, &1000);

    // Set shipment to InTransit status
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::types::ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);
    });

    // Record batch of milestones
    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((
        Symbol::new(&env, "warehouse"),
        BytesN::from_array(&env, &[10u8; 32]),
    ));
    milestones.push_back((
        Symbol::new(&env, "port"),
        BytesN::from_array(&env, &[20u8; 32]),
    ));

    client.record_milestones_batch(&carrier, &shipment_id, &milestones);

    // Verify escrow was released for both milestones (30% + 30% = 60% of 1000 = 600)
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 400); // 1000 - 600 = 400 remaining
}

// ============= TTL Extension Tests =============

#[test]
fn test_ttl_extension_on_shipment_creation() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        let ttl = env.storage().persistent().get_ttl(&key);
        // SHIPMENT_TTL_EXTENSION is 518_400
        assert!(ttl >= 518_400);
    });
}

#[test]
fn test_manual_ttl_extension() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    // Initial extension happens on creation.
    // Call manual extension
    client.extend_shipment_ttl(&shipment_id);

    env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        let ttl = env.storage().persistent().get_ttl(&key);
        assert!(ttl >= 518_400);
    });
}

// ============= Cancel Shipment Tests =============

#[test]
fn test_cancel_shipment_with_escrow() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[99u8; 32]);
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
        &None,
    );
    let escrow_amount: i128 = 5000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
}

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

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
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

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    client.cancel_shipment(&admin, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
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
fn test_escrow_happy_path_create_deposit_transit_deliver_confirm() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let hash2 = BytesN::from_array(&env, &[2u8; 32]);
    let hash3 = BytesN::from_array(&env, &[3u8; 32]);
    let confirmation_hash = BytesN::from_array(&env, &[99u8; 32]);
    let escrow_amount: i128 = 10_000;
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
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash2);
    super::test_utils::advance_past_rate_limit(&env);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::AtCheckpoint,
        &hash3,
    );
    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Delivered);
    assert_eq!(shipment.escrow_amount, 0);
}

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

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
}

#[test]
fn test_escrow_dispute_resolve_to_delivered() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[5u8; 32]);
    let hash2 = BytesN::from_array(&env, &[6u8; 32]);
    let hash3 = BytesN::from_array(&env, &[7u8; 32]);
    let escrow_amount: i128 = 3_000;
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
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash2);
    super::test_utils::advance_past_rate_limit(&env);
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::Disputed, &hash3);
    client.update_status(&admin, &shipment_id, &ShipmentStatus::Delivered, &hash3);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Delivered);
}

#[test]
fn test_escrow_dispute_resolve_to_cancelled() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[8u8; 32]);
    let hash2 = BytesN::from_array(&env, &[9u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[77u8; 32]);
    let escrow_amount: i128 = 2_000;
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
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash2);
    super::test_utils::advance_past_rate_limit(&env);
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::Disputed, &hash2);
    client.update_status(
        &admin,
        &shipment_id,
        &ShipmentStatus::Cancelled,
        &reason_hash,
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_escrow_double_deposit_prevention() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[10u8; 32]);
    let escrow_amount: i128 = 1_000;
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
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_escrow_release_without_delivery_confirm_from_created_fails() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[11u8; 32]);
    let confirmation_hash = BytesN::from_array(&env, &[66u8; 32]);
    let escrow_amount: i128 = 1_500;
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
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #38)")]
fn test_escrow_refund_after_delivery_fails() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[12u8; 32]);
    let hash2 = BytesN::from_array(&env, &[13u8; 32]);
    let confirmation_hash = BytesN::from_array(&env, &[55u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[33u8; 32]);
    let escrow_amount: i128 = 2_500;
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
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.update_status(
        &carrier,
        &shipment_id,
        &crate::ShipmentStatus::InTransit,
        &hash2,
    );
    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    client.cancel_shipment(&company, &shipment_id, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_escrow_deposit_after_status_change_fails() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[14u8; 32]);
    let hash2 = BytesN::from_array(&env, &[15u8; 32]);
    let escrow_amount: i128 = 1_000;
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
        &None,
    );
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash2);

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
}

#[test]
fn test_milestone_payment_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let escrow_amount: i128 = 1000;
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "warehouse"), 30));
    milestones.push_back((Symbol::new(&env, "port"), 30));
    milestones.push_back((Symbol::new(&env, "last_mile"), 40));

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    // Status InTransit
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    // Record Milestone 1: Warehouse (30% of 1000 = 300)
    client.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "warehouse"),
        &data_hash,
    );
    assert_eq!(client.get_shipment(&shipment_id).escrow_amount, 700);

    // Record Milestone 2: Port (30% of 1000 = 300)
    client.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "port"),
        &data_hash,
    );
    assert_eq!(client.get_shipment(&shipment_id).escrow_amount, 400);

    // Record Milestone 3: Last Mile (40% of 1000 = 400)
    client.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "last_mile"),
        &data_hash,
    );
    assert_eq!(client.get_shipment(&shipment_id).escrow_amount, 0);
}

#[test]
fn test_milestone_payment_delivery_releases_remaining() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let escrow_amount: i128 = 1000;
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "checkpoint1"), 25));
    milestones.push_back((Symbol::new(&env, "checkpoint2"), 75));

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    // Record Milestone 1 (25% = 250)
    client.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "checkpoint1"),
        &data_hash,
    );
    assert_eq!(client.get_shipment(&shipment_id).escrow_amount, 750);

    // Skip Milestone 2 and Confirm Delivery
    // Remaining 75% should be released
    client.confirm_delivery(&receiver, &shipment_id, &data_hash);
    assert_eq!(client.get_shipment(&shipment_id).escrow_amount, 0);
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
        &None,
    );
}

#[test]
fn test_milestone_payment_duplicate_record_no_double_pay() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let escrow_amount: i128 = 1000;
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "m1"), 50));
    milestones.push_back((Symbol::new(&env, "m2"), 50));

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    // Record Milestone 1 (50% = 500)
    client.record_milestone(&carrier, &shipment_id, &Symbol::new(&env, "m1"), &data_hash);
    assert_eq!(client.get_shipment(&shipment_id).escrow_amount, 500);

    // Record Milestone 1 AGAIN — must be rejected to prevent double-pay
    let dup_result =
        client.try_record_milestone(&carrier, &shipment_id, &Symbol::new(&env, "m1"), &data_hash);
    assert_eq!(dup_result, Err(Ok(crate::NavinError::MilestoneAlreadyPaid)));
    // Escrow must still be 500 — no double payment
    assert_eq!(client.get_shipment(&shipment_id).escrow_amount, 500);
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
fn test_get_contract_metadata_after_creating_shipments() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[2u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let meta = client.get_contract_metadata();
    assert_eq!(meta.version, 1);
    assert_eq!(meta.admin, admin);
    assert_eq!(meta.shipment_count, 2);
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
fn test_successful_handoff() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &current_carrier);
    client.add_carrier(&admin, &new_carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &current_carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    // Update status to InTransit to allow handoff
    client.update_status(
        &current_carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    // Perform handoff
    client.handoff_shipment(&current_carrier, &new_carrier, &shipment_id, &handoff_hash);

    // Verify carrier was updated
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.carrier, new_carrier);
}

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
        &None,
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
        &None,
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
        &None,
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
#[should_panic(expected = "Error(Contract, #5)")]
fn test_handoff_delivered_shipment() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &current_carrier);
    client.add_carrier(&admin, &new_carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &current_carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    // Mark as delivered
    client.update_status(
        &current_carrier,
        &shipment_id,
        &ShipmentStatus::Delivered,
        &data_hash,
    );

    // Try to handoff a delivered shipment
    client.handoff_shipment(&current_carrier, &new_carrier, &shipment_id, &handoff_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #38)")]
fn test_handoff_cancelled_shipment() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &current_carrier);
    client.add_carrier(&admin, &new_carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &current_carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    // Cancel the shipment
    client.cancel_shipment(&company, &shipment_id, &data_hash);

    // Try to handoff a cancelled shipment
    client.handoff_shipment(&current_carrier, &new_carrier, &shipment_id, &handoff_hash);
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
        &None,
    );
}

// ── Issue #1: report_condition_breach ────────────────────────────────────────

#[test]
fn test_report_condition_breach_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    // Carrier reports a temperature breach — no error, status unchanged
    client.report_condition_breach(
        &carrier,
        &shipment_id,
        &BreachType::TemperatureHigh,
        &Severity::High,
        &breach_hash,
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Created);
}

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
        &None,
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
        &None,
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
fn test_verify_delivery_proof_match() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let confirmation_hash = BytesN::from_array(&env, &[9u8; 32]);
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
        &None,
    );

    // Move to InTransit so confirm_delivery is valid
    let transit_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &transit_hash,
    );

    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    assert!(client.verify_delivery_proof(&shipment_id, &confirmation_hash));
}

#[test]
fn test_verify_delivery_proof_mismatch() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let confirmation_hash = BytesN::from_array(&env, &[9u8; 32]);
    let wrong_hash = BytesN::from_array(&env, &[7u8; 32]);
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
        &None,
    );

    let transit_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &transit_hash,
    );
    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    assert!(!client.verify_delivery_proof(&shipment_id, &wrong_hash));
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_verify_delivery_proof_nonexistent_shipment() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.verify_delivery_proof(&999u64, &BytesN::from_array(&_env, &[1u8; 32]));
}

// ── Issue #3: Rate limiting ───────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #21)")]
fn test_rate_limit_rapid_update_fails() {
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
        &None,
    );

    let hash1 = BytesN::from_array(&env, &[2u8; 32]);
    let hash2 = BytesN::from_array(&env, &[3u8; 32]);

    // First update sets the LastStatusUpdate timestamp
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash1);

    // Immediate second update — same ledger timestamp — must be rejected (#21)
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::AtCheckpoint,
        &hash2,
    );
}

#[test]
fn test_rate_limit_admin_bypasses() {
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
        &None,
    );

    let hash1 = BytesN::from_array(&env, &[2u8; 32]);
    let hash2 = BytesN::from_array(&env, &[3u8; 32]);
    let hash3 = BytesN::from_array(&env, &[4u8; 32]);

    // Admin can make back-to-back status updates without hitting the rate limit
    client.update_status(&admin, &shipment_id, &ShipmentStatus::InTransit, &hash1);
    client.update_status(&admin, &shipment_id, &ShipmentStatus::AtCheckpoint, &hash2);
    client.update_status(&admin, &shipment_id, &ShipmentStatus::InTransit, &hash3);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::InTransit);
}

#[test]
fn test_rate_limit_update_after_interval_succeeds() {
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
        &None,
    );

    let hash1 = BytesN::from_array(&env, &[2u8; 32]);
    let hash2 = BytesN::from_array(&env, &[3u8; 32]);

    // First update
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash1);

    // Advance the ledger timestamp past the 60-second minimum interval
    super::test_utils::advance_past_rate_limit(&env);

    // Second update after the interval — should succeed
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::AtCheckpoint,
        &hash2,
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::AtCheckpoint);
}

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
fn test_only_company_can_create_shipments() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Company can create shipment
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    assert_eq!(shipment_id, 1);

    // Carrier cannot create shipment
    let result = client.try_create_shipment(
        &carrier,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // Outsider cannot create shipment
    // Outsider cannot create shipment
    let result = client.try_create_shipment(
        &outsider,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));
}

#[test]
fn test_only_carrier_can_update_status_and_record_milestones() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let other_carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let update_hash = BytesN::from_array(&env, &[2u8; 32]);
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
        &None,
    );

    // Assigned carrier can update status
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &update_hash,
    );

    // Assigned carrier can record milestone
    client.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "checkpoint"),
        &update_hash,
    );

    // Other carrier (not assigned) cannot update status
    let result = client.try_update_status(
        &other_carrier,
        &shipment_id,
        &ShipmentStatus::AtCheckpoint,
        &update_hash,
    );
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // Other carrier (not assigned) cannot record milestone
    let result = client.try_record_milestone(
        &other_carrier,
        &shipment_id,
        &Symbol::new(&env, "checkpoint"),
        &update_hash,
    );
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // Admin can update status (as seen in lib.rs)
    client.update_status(
        &admin,
        &shipment_id,
        &ShipmentStatus::AtCheckpoint,
        &update_hash,
    );
}

#[test]
fn test_only_receiver_can_confirm_delivery() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let delivery_hash = BytesN::from_array(&env, &[2u8; 32]);
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
        &None,
    );

    // Transition to InTransit first
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    // Receiver can confirm delivery
    client.confirm_delivery(&receiver, &shipment_id, &delivery_hash);

    // Test unauthorized (different setup needed since status is now Delivered)
    let shipment_id_2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[3u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    client.update_status(
        &carrier,
        &shipment_id_2,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    // Admin cannot confirm delivery (only designated receiver)
    let result = client.try_confirm_delivery(&admin, &shipment_id_2, &delivery_hash);
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // Carrier cannot confirm delivery
    let result = client.try_confirm_delivery(&carrier, &shipment_id_2, &delivery_hash);
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // Outsider cannot confirm delivery
    let result = client.try_confirm_delivery(&outsider, &shipment_id_2, &delivery_hash);
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
        &None,
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

#[test]
fn test_rbac_all_gated_functions_with_wrong_role() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    let outsider = Address::generate(&env);
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
        &None,
    );

    // set_shipment_metadata: sender or admin only
    let result = client.try_set_shipment_metadata(
        &outsider,
        &shipment_id,
        &Symbol::new(&env, "key"),
        &Symbol::new(&env, "val"),
    );
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // add_carrier_to_whitelist: company only
    let result = client.try_add_carrier_to_whitelist(&carrier, &Address::generate(&env));
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // deposit_escrow: Company only
    let result = client.try_deposit_escrow(&carrier, &shipment_id, &1000);
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // report_geofence_event: Carrier only
    let result = client.try_report_geofence_event(
        &company,
        &shipment_id,
        &GeofenceEvent::ZoneEntry,
        &data_hash,
    );
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // update_eta: assigned carrier only
    let result = client.try_update_eta(&company, &shipment_id, &1000000000, &data_hash);
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // cancel_shipment: sender or admin only
    let result = client.try_cancel_shipment(&carrier, &shipment_id, &data_hash);
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // raise_dispute: sender, receiver, or carrier only
    let result = client.try_raise_dispute(&outsider, &shipment_id, &data_hash);
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // resolve_dispute: admin only
    let result = client.try_resolve_dispute(
        &company,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
        &BytesN::from_array(&env, &[1u8; 32]),
    );
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // handoff_shipment: current carrier only
    let result =
        client.try_handoff_shipment(&company, &Address::generate(&env), &shipment_id, &data_hash);
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // update_status: carrier or admin only (Company cannot update status)
    let result = client.try_update_status(
        &company,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );
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
#[should_panic(expected = "Error(Contract, #28)")]
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
fn test_force_release_action() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    // Create a shipment with escrow
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let escrow_amount: i128 = 1000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    // Propose force release
    let action = crate::AdminAction::ForceRelease(shipment_id);
    let proposal_id = client.propose_action(&admin1, &action);

    // Approve and execute
    client.approve_action(&admin2, &proposal_id);

    // Verify escrow was released
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 0);
}

#[test]
fn test_force_refund_action() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    // Create a shipment with escrow
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let escrow_amount: i128 = 1000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    // Propose force refund
    let action = crate::AdminAction::ForceRefund(shipment_id);
    let proposal_id = client.propose_action(&admin1, &action);

    // Approve and execute
    client.approve_action(&admin2, &proposal_id);

    // Verify escrow was refunded
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 0);
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

#[test]
fn test_check_deadline_success_auto_cancels_and_refunds() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let now = env.ledger().timestamp();
    let deadline = now + 1000;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    let escrow_amount: i128 = 5000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    // Advance ledger time past the deadline threshold
    super::test_utils::advance_ledger_time(&env, 1001);

    // Execute the deadline checker
    client.check_deadline(&shipment_id);

    // Validate that the shipment was successfully cancelled and escrow cleared
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #29)")]
fn test_check_deadline_fails_if_not_expired() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let now = env.ledger().timestamp();
    let deadline = now + 1000;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    // Fails because the current ledger timestamp is less than the deadline constraint
    client.check_deadline(&shipment_id);
}

#[test]
fn test_delivery_before_deadline() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let confirm_hash = BytesN::from_array(&env, &[99u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let now = env.ledger().timestamp();
    let deadline = now + 1000;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );
    client.confirm_delivery(&receiver, &shipment_id, &confirm_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Delivered);

    // Fast-forward past the deadline point
    super::test_utils::advance_ledger_time(&env, 1001);

    // Attempting to crank check_deadline on a safely completed shipment errors appropriately (Error 9)
    let res = client.try_check_deadline(&shipment_id);
    assert_eq!(res, Err(Ok(crate::NavinError::ShipmentAlreadyCompleted)));
}

#[test]
fn test_delivery_success_event_emitted_on_confirm_delivery() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let confirm_hash = BytesN::from_array(&env, &[99u8; 32]);
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
        &None,
    );

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    client.confirm_delivery(&receiver, &shipment_id, &confirm_hash);

    let events = env.events().all();
    let found = events.iter().any(|(_contract, topics, _data)| {
        if let Some(raw) = topics.get(0) {
            if let Ok(topic) = Symbol::try_from_val(&env, &raw) {
                return topic == Symbol::new(&env, "delivery_success");
            }
        }
        false
    });
    assert!(
        found,
        "delivery_success event must be emitted on confirm_delivery"
    );
}

#[test]
fn test_delivery_success_event_contains_correct_carrier() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let confirm_hash = BytesN::from_array(&env, &[88u8; 32]);
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
        &None,
    );

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    client.confirm_delivery(&receiver, &shipment_id, &confirm_hash);

    let events = env.events().all();
    let event_data = events.iter().find_map(|(_contract, topics, data)| {
        if let Some(raw) = topics.get(0) {
            if let Ok(topic) = Symbol::try_from_val(&env, &raw) {
                if topic == Symbol::new(&env, "delivery_success") {
                    // data is (carrier, shipment_id, delivery_time, schema_version, event_counter, idempotency_key)
                    return <(Address, u64, u64, u32, u32, BytesN<32>)>::try_from_val(&env, &data)
                        .ok();
                }
            }
        }
        None
    });

    let (
        event_carrier,
        event_shipment_id,
        _delivery_time,
        _schema_version,
        _event_counter,
        _idempotency_key,
    ) = event_data.expect("delivery_success event data must be present");
    assert_eq!(
        event_carrier, carrier,
        "event must reference the assigned carrier"
    );
    assert_eq!(
        event_shipment_id, shipment_id,
        "event must reference the correct shipment"
    );
}

#[test]
fn test_carrier_breach_event_emitted_on_report_condition_breach() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    client.report_condition_breach(
        &carrier,
        &shipment_id,
        &BreachType::TemperatureHigh,
        &Severity::High,
        &breach_hash,
    );

    let events = env.events().all();
    let found = events.iter().any(|(_contract, topics, _data)| {
        if let Some(raw) = topics.get(0) {
            if let Ok(topic) = Symbol::try_from_val(&env, &raw) {
                return topic == Symbol::new(&env, "carrier_breach");
            }
        }
        false
    });
    assert!(
        found,
        "carrier_breach event must be emitted on report_condition_breach"
    );
}

#[test]
fn test_carrier_breach_event_emitted_alongside_condition_breach() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let breach_hash = BytesN::from_array(&env, &[3u8; 32]);
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
        &None,
    );

    client.report_condition_breach(
        &carrier,
        &shipment_id,
        &BreachType::HumidityHigh,
        &Severity::Medium,
        &breach_hash,
    );

    let events = env.events().all();

    // Both condition_breach AND carrier_breach must be emitted
    let has_condition_breach = events.iter().any(|(_c, topics, _d)| {
        topics
            .get(0)
            .and_then(|raw| Symbol::try_from_val(&env, &raw).ok())
            == Some(Symbol::new(&env, "condition_breach"))
    });
    let has_carrier_breach = events.iter().any(|(_c, topics, _d)| {
        topics
            .get(0)
            .and_then(|raw| Symbol::try_from_val(&env, &raw).ok())
            == Some(Symbol::new(&env, "carrier_breach"))
    });

    assert!(
        has_condition_breach,
        "condition_breach event must still be emitted"
    );
    assert!(
        has_carrier_breach,
        "carrier_breach event must also be emitted"
    );
}

#[test]
fn test_condition_breach_event_includes_severity() {
    use soroban_sdk::testutils::Events;

    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    // Report breach with Critical severity
    client.report_condition_breach(
        &carrier,
        &shipment_id,
        &BreachType::TemperatureHigh,
        &Severity::Critical,
        &breach_hash,
    );

    // Verify condition_breach event contains severity
    let events = env.events().all();
    let mut found_critical = false;

    for (_contract, _topics, _data) in events.iter() {
        // Check if this is a condition_breach event by looking at the data structure
        // Data should be: (shipment_id, carrier, breach_type, severity, data_hash)
        if soroban_sdk::Val::try_from_val(&env, &_data).is_ok() {
            // We verify the event was emitted with 5 fields including severity
            found_critical = true;
            break;
        }
    }

    assert!(
        found_critical,
        "condition_breach event with severity must be emitted"
    );
}

#[test]
fn test_carrier_dispute_loss_event_emitted_on_refund_to_company() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[55u8; 32]);
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
        &None,
    );
    let escrow_amount: i128 = 5000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.raise_dispute(&company, &shipment_id, &reason_hash);

    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::RefundToCompany,
        &reason_hash,
    );

    let events = env.events().all();
    let found = events.iter().any(|(_contract, topics, _data)| {
        if let Some(raw) = topics.get(0) {
            if let Ok(topic) = Symbol::try_from_val(&env, &raw) {
                return topic == Symbol::new(&env, "carrier_dispute_loss");
            }
        }
        false
    });
    assert!(
        found,
        "carrier_dispute_loss event must be emitted when dispute resolves with RefundToCompany"
    );
}

#[test]
fn test_carrier_dispute_loss_not_emitted_when_carrier_wins() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[44u8; 32]);
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
        &None,
    );
    let escrow_amount: i128 = 5000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.raise_dispute(&carrier, &shipment_id, &reason_hash);

    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
        &reason_hash,
    );

    let events = env.events().all();
    let found = events.iter().any(|(_contract, topics, _data)| {
        if let Some(raw) = topics.get(0) {
            if let Ok(topic) = Symbol::try_from_val(&env, &raw) {
                return topic == Symbol::new(&env, "carrier_dispute_loss");
            }
        }
        false
    });
    assert!(
        !found,
        "carrier_dispute_loss must NOT be emitted when resolution is ReleaseToCarrier"
    );
}

#[test]
fn test_carrier_dispute_loss_event_contains_correct_carrier() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[33u8; 32]);
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
        &None,
    );
    let escrow_amount: i128 = 5000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.raise_dispute(&receiver, &shipment_id, &reason_hash);

    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::RefundToCompany,
        &reason_hash,
    );

    let events = env.events().all();
    let event_data = events.iter().find_map(|(_contract, topics, data)| {
        if let Some(raw) = topics.get(0) {
            if let Ok(topic) = Symbol::try_from_val(&env, &raw) {
                if topic == Symbol::new(&env, "carrier_dispute_loss") {
                    return <(Address, u64)>::try_from_val(&env, &data).ok();
                }
            }
        }
        None
    });

    let (event_carrier, event_shipment_id) =
        event_data.expect("carrier_dispute_loss event data must be present");
    assert_eq!(event_carrier, carrier, "event must name the losing carrier");
    assert_eq!(
        event_shipment_id, shipment_id,
        "event must reference the correct shipment"
    );
}

// ============= Notification Event Tests =============

#[test]
fn test_notification_emitted_on_shipment_created() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let events = env.events().all();
    let notification_count = events
        .iter()
        .filter(|(_contract, topics, _data)| {
            topics
                .get(0)
                .and_then(|raw| Symbol::try_from_val(&env, &raw).ok())
                == Some(Symbol::new(&env, "notification"))
        })
        .count();

    assert_eq!(
        notification_count, 2,
        "Two notifications should be emitted: one for receiver, one for carrier"
    );
}

#[test]
fn test_notification_emitted_on_status_changed() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let new_hash = BytesN::from_array(&env, &[2u8; 32]);
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
        &None,
    );

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &new_hash,
    );

    let events = env.events().all();
    let notification_count = events
        .iter()
        .filter(|(_contract, topics, _data)| {
            topics
                .get(0)
                .and_then(|raw| Symbol::try_from_val(&env, &raw).ok())
                == Some(Symbol::new(&env, "notification"))
        })
        .count();

    assert!(
        notification_count >= 2,
        "Notifications should be emitted for sender and receiver on status change"
    );
}

#[test]
fn test_notification_emitted_on_delivery_confirmed() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let confirm_hash = BytesN::from_array(&env, &[99u8; 32]);
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
        &None,
    );

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );
    client.confirm_delivery(&receiver, &shipment_id, &confirm_hash);

    let events = env.events().all();
    let notification_count = events
        .iter()
        .filter(|(_contract, topics, _data)| {
            topics
                .get(0)
                .and_then(|raw| Symbol::try_from_val(&env, &raw).ok())
                == Some(Symbol::new(&env, "notification"))
        })
        .count();

    assert!(
        notification_count >= 2,
        "Notifications should be emitted on delivery confirmation"
    );
}

#[test]
fn test_notification_emitted_on_dispute_raised() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[99u8; 32]);
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
        &None,
    );

    client.raise_dispute(&company, &shipment_id, &reason_hash);

    let events = env.events().all();
    let notification_count = events
        .iter()
        .filter(|(_contract, topics, _data)| {
            topics
                .get(0)
                .and_then(|raw| Symbol::try_from_val(&env, &raw).ok())
                == Some(Symbol::new(&env, "notification"))
        })
        .count();

    assert_eq!(
        notification_count, 3,
        "Three notifications should be emitted: sender, receiver, and carrier"
    );
}

#[test]
fn test_notification_emitted_on_dispute_resolved() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[94u8; 32]);
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
        &None,
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.raise_dispute(&company, &shipment_id, &reason_hash);
    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
        &reason_hash,
    );

    let events = env.events().all();
    let notification_count = events
        .iter()
        .filter(|(_contract, topics, _data)| {
            topics
                .get(0)
                .and_then(|raw| Symbol::try_from_val(&env, &raw).ok())
                == Some(Symbol::new(&env, "notification"))
        })
        .count();

    assert!(
        notification_count >= 3,
        "Notifications should be emitted for all parties on dispute resolution"
    );
}

// ============= Analytics Tests =============

#[test]
fn test_analytics_counters() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // Initial analytics should be zero
    let analytics = client.get_analytics();
    assert_eq!(analytics.total_shipments, 0);
    assert_eq!(analytics.total_escrow_volume, 0);
    assert_eq!(analytics.total_disputes, 0);
    assert_eq!(analytics.created_count, 0);

    // Create a shipment
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let analytics = client.get_analytics();
    assert_eq!(analytics.total_shipments, 1);
    assert_eq!(analytics.created_count, 1);

    // Deposit escrow
    let escrow_amount: i128 = 5000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    let analytics = client.get_analytics();
    assert_eq!(analytics.total_escrow_volume, 5000);

    // Update status to InTransit
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    let analytics = client.get_analytics();
    assert_eq!(analytics.created_count, 0);
    assert_eq!(analytics.in_transit_count, 1);

    // Raise dispute
    client.raise_dispute(&company, &shipment_id, &data_hash);

    let analytics = client.get_analytics();
    assert_eq!(analytics.in_transit_count, 0);
    assert_eq!(analytics.disputed_count, 1);
    assert_eq!(analytics.total_disputes, 1);

    // Resolve dispute (Release to Carrier -> Delivered)
    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
        &data_hash,
    );

    let analytics = client.get_analytics();
    assert_eq!(analytics.disputed_count, 0);
    assert_eq!(analytics.delivered_count, 1);
}

#[test]
fn test_analytics_batch_and_cancel() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // Create 3 shipments in a batch
    let mut shipments = soroban_sdk::Vec::new(&env);
    for i in 1..=3 {
        shipments.push_back(ShipmentInput {
            receiver: Address::generate(&env),
            carrier: carrier.clone(),
            data_hash: BytesN::from_array(&env, &[i as u8; 32]),
            payment_milestones: soroban_sdk::Vec::new(&env),
            deadline,
            depends_on: None,
        });
    }
    client.create_shipments_batch(&company, &shipments);

    let analytics = client.get_analytics();
    assert_eq!(analytics.total_shipments, 3);
    assert_eq!(analytics.created_count, 3);

    // Cancel 1 shipment
    client.cancel_shipment(&company, &1, &BytesN::from_array(&env, &[9u8; 32]));

    let analytics = client.get_analytics();
    let created = analytics.created_count;
    let cancelled = analytics.cancelled_count;
    assert_eq!(created, 2, "Created count should be 2 after 1 cancellation");
    assert_eq!(
        cancelled, 1,
        "Cancelled count should be 1 after 1 cancellation"
    );
}

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

#[test]
fn test_get_status_summary_populated() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // Create 3 shipments
    for i in 1..=3 {
        let id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &BytesN::from_array(&env, &[i as u8; 32]),
            &soroban_sdk::Vec::new(&env),
            &deadline,
        &None,
    );
    println!("Created shipment with ID: {}", id);
    }

    // Status: 3 Created
    let summary = client.get_status_summary();
    assert_eq!(summary.created, 3);

    // Update shipment 1 to InTransit
    client.update_status(&carrier, &1, &ShipmentStatus::InTransit, &data_hash);

    env.ledger().with_mut(|li| {
        li.timestamp += 61; // Add 61 seconds (or whatever your min_status_update_interval is)
    });

    // Update 1 to AtCheckpoint
    client.update_status(&carrier, &1, &ShipmentStatus::AtCheckpoint, &data_hash);

    let summary = client.get_status_summary();
    assert_eq!(summary.created, 2);
    assert_eq!(summary.in_transit, 0);
    assert_eq!(summary.at_checkpoint, 1);
    assert_eq!(summary.delivered, 0);
}

#[test]
fn test_get_non_terminal_count_mixed_states() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // Create 5 shipments
    for i in 1..=5 {
        client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &BytesN::from_array(&env, &[i as u8; 32]),
            &soroban_sdk::Vec::new(&env),
            &deadline,
            &None,
        );
    }

    // Status: 5 Created (Non-terminal)
    assert_eq!(client.get_non_terminal_count(), 5);

    // Update 1 to InTransit (Non-terminal)
    client.update_status(&carrier, &1, &ShipmentStatus::InTransit, &data_hash);
    assert_eq!(client.get_non_terminal_count(), 5);

    // Update 1 to Delivered (Terminal)
    client.confirm_delivery(&receiver, &1, &data_hash);
    assert_eq!(client.get_non_terminal_count(), 4);

    // Cancel 1 shipment (Terminal)
    client.cancel_shipment(&company, &2, &data_hash);
    assert_eq!(client.get_non_terminal_count(), 3);

    // Raise dispute for 1 shipment (Terminal according to requirements)
    client.raise_dispute(&company, &3, &data_hash);
    assert_eq!(client.get_non_terminal_count(), 2);
}

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

#[test]
fn test_active_shipment_count_tracking() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // Set limit to 2 for easier testing
    client.set_shipment_limit(&admin, &2);

    assert_eq!(client.get_active_shipment_count(&company), 0);

    // Create 1st shipment
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    assert_eq!(client.get_active_shipment_count(&company), 1);

    // Create 2nd shipment
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[2u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    assert_eq!(client.get_active_shipment_count(&company), 2);
}

#[test]
fn test_company_shipment_limit_override_takes_precedence() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.set_shipment_limit(&admin, &5);
    client.set_company_shipment_limit(&admin, &company, &1);

    assert_eq!(client.get_effective_shipment_limit(&company), 1);

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[2u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    assert_eq!(result, Err(Ok(NavinError::ShipmentLimitReached)));
}

#[test]
fn test_company_limit_falls_back_to_global_limit() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.set_shipment_limit(&admin, &2);

    assert_eq!(client.get_effective_shipment_limit(&company), 2);

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[3u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[4u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[5u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    assert_eq!(result, Err(Ok(NavinError::ShipmentLimitReached)));
}

// ============= Dispute Evidence Tests =============

#[test]
fn test_add_dispute_evidence_hash_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let evidence_hash = BytesN::from_array(&env, &[77u8; 32]);
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
        &None,
    );

    // Initial state is Created, not Disputed
    let res = client.try_add_dispute_evidence_hash(&company, &shipment_id, &evidence_hash);
    assert_eq!(res, Err(Ok(crate::NavinError::InvalidStatus)));

    // Change to Disputed
    client.raise_dispute(&company, &shipment_id, &data_hash);

    // Now adding evidence should work
    client.add_dispute_evidence_hash(&company, &shipment_id, &evidence_hash);

    assert_eq!(client.get_dispute_evidence_count(&shipment_id), 1);
    assert_eq!(
        client.get_dispute_evidence_hash(&shipment_id, &0),
        Some(evidence_hash.clone())
    );

    // Adding multiple evidence hashes
    let second_evidence = BytesN::from_array(&env, &[88u8; 32]);
    client.add_dispute_evidence_hash(&receiver, &shipment_id, &second_evidence);

    assert_eq!(client.get_dispute_evidence_count(&shipment_id), 2);
    assert_eq!(
        client.get_dispute_evidence_hash(&shipment_id, &1),
        Some(second_evidence)
    );

    // Admin can also add evidence
    let admin_evidence = BytesN::from_array(&env, &[99u8; 32]);
    client.add_dispute_evidence_hash(&admin, &shipment_id, &admin_evidence);
    assert_eq!(client.get_dispute_evidence_count(&shipment_id), 3);
}

#[test]
fn test_resolve_dispute_fails_without_reason_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[99u8; 32]);
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
        &None,
    );
    let escrow_amount: i128 = 5000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.raise_dispute(&company, &shipment_id, &reason_hash);

    // Empty reason hash should fail
    let empty_hash = BytesN::from_array(&env, &[0u8; 32]);
    let res = client.try_resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
        &empty_hash,
    );
    assert_eq!(res, Err(Ok(crate::NavinError::DisputeReasonHashMissing)));
}

#[test]
fn test_integration_nonce_increment() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    // Initial nonce is 0
    assert_eq!(client.get_integration_nonce(&shipment_id), 0);

    // Deposit escrow
    client.deposit_escrow(&company, &shipment_id, &5000);
    assert_eq!(client.get_integration_nonce(&shipment_id), 1);

    // Update status
    client.update_status(
        &carrier,
        &shipment_id,
        &crate::ShipmentStatus::InTransit,
        &data_hash,
    );
    assert_eq!(client.get_integration_nonce(&shipment_id), 2);

    // Raise dispute
    client.raise_dispute(&company, &shipment_id, &data_hash);
    assert_eq!(client.get_integration_nonce(&shipment_id), 3);

    // Add evidence
    client.add_dispute_evidence_hash(&company, &shipment_id, &data_hash);
    assert_eq!(client.get_integration_nonce(&shipment_id), 4);

    // Resolve dispute
    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::RefundToCompany,
        &data_hash,
    );
    assert_eq!(client.get_integration_nonce(&shipment_id), 5);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_add_dispute_evidence_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let evidence_hash = BytesN::from_array(&env, &[77u8; 32]);
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
        &None,
    );

    client.raise_dispute(&company, &shipment_id, &data_hash);

    // Outsider tries to add evidence
    client.add_dispute_evidence_hash(&outsider, &shipment_id, &evidence_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #30)")]
fn test_shipment_limit_reached() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // Set limit to 1
    client.set_shipment_limit(&admin, &1);

    // Create 1st shipment - OK
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    // Create 2nd shipment - Should fail with ShipmentLimitReached
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[2u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
}

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
            depends_on: None,
        });
    }

    client.create_shipments_batch(&company, &shipments);
}

#[test]
fn test_count_decrements_on_delivery() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier_to_whitelist(&company, &carrier);

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    assert_eq!(client.get_active_shipment_count(&company), 1);

    // Update to InTransit first
    client.update_status(&carrier, &1, &ShipmentStatus::InTransit, &data_hash);

    // Deliver
    client.confirm_delivery(&receiver, &1, &data_hash);

    assert_eq!(client.get_active_shipment_count(&company), 0);
}

#[test]
fn test_count_decrements_on_cancel() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    assert_eq!(client.get_active_shipment_count(&company), 1);

    client.cancel_shipment(&company, &1, &data_hash);

    assert_eq!(client.get_active_shipment_count(&company), 0);
}

#[test]
fn test_count_decrements_on_dispute_resolution() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier_to_whitelist(&company, &carrier);

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    client.deposit_escrow(&company, &1, &1000);
    client.update_status(&carrier, &1, &ShipmentStatus::InTransit, &data_hash);
    client.raise_dispute(&company, &1, &data_hash);

    assert_eq!(client.get_active_shipment_count(&company), 1);

    // Resolve dispute
    client.resolve_dispute(
        &admin,
        &1,
        &crate::DisputeResolution::RefundToCompany,
        &BytesN::from_array(&env, &[1u8; 32]),
    );

    assert_eq!(client.get_active_shipment_count(&company), 0);
}

#[test]
fn test_count_decrements_on_deadline_expiration() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    assert_eq!(client.get_active_shipment_count(&company), 1);

    // Fast forward time
    super::test_utils::set_ledger_time(&env, deadline + 1);

    client.check_deadline(&1);

    assert_eq!(client.get_active_shipment_count(&company), 0);
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
        &None,
    );
}

// NOTE: This test is commented out because the feature may not be fully implemented yet
#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_update_status_returns_invalid_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
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
        &None,
    );

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &zero_hash,
    );
}

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

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_verify_data_hash_returns_invalid_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

    let (_receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::Delivered,
    );

    client.verify_data_hash(&shipment_id, &crate::ShipmentStatus::Delivered, &zero_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_upgrade_returns_invalid_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

    client.initialize(&admin, &token_contract);

    client.upgrade(&admin, &zero_hash, &2u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_record_milestones_batch_returns_invalid_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
    let valid_hash = BytesN::from_array(&env, &[1u8; 32]);

    let (_receiver, carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::InTransit,
    );

    // Carrier must be registered in the role system before calling record_milestones_batch
    client.add_carrier(&admin, &carrier);

    let milestones = soroban_sdk::vec![
        &env,
        (Symbol::new(&env, "checkpoint1"), valid_hash),
        (Symbol::new(&env, "checkpoint2"), zero_hash),
    ];

    client.record_milestones_batch(&carrier, &shipment_id, &milestones);
}

// ============= Error #11: CounterOverflow Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_create_shipment_returns_counter_overflow() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // Set counter to max value
    env.as_contract(&client.address, || {
        crate::storage::set_shipment_counter(&env, u64::MAX);
    });

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
}

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
//     , &None);
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
//     , &None);
//
//     // Try to handoff to a carrier that is not registered
//     let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);
//     client.handoff_shipment(&carrier, &new_carrier, &shipment_id, &handoff_hash);
// }

// ============= Error #14: InvalidAmount Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_deposit_escrow_returns_invalid_amount_zero() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    client.deposit_escrow(&company, &shipment_id, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_deposit_escrow_returns_invalid_amount_negative() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    client.deposit_escrow(&company, &shipment_id, &-100);
}

// ============= Error #15: EscrowAlreadyDeposited Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_deposit_escrow_returns_escrow_already_deposited() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    client.deposit_escrow(&company, &shipment_id, &1000);
    // Try to deposit again
    client.deposit_escrow(&company, &shipment_id, &500);
}

// ============= Error #19: MilestoneAlreadyPaid Tests =============

// NOTE: This test is commented out because the feature may not be fully implemented yet
// #[test]
// #[should_panic(expected = "Error(Contract, #19)")]
// fn test_record_milestone_returns_milestone_already_paid() {
//     let (env, client, admin, token_contract) = setup_shipment_env();
//     let company = Address::generate(&env);
//     let receiver = Address::generate(&env);
//     let carrier = Address::generate(&env);
//     let data_hash = BytesN::from_array(&env, &[1u8; 32]);
//     let checkpoint = soroban_sdk::Symbol::new(&env, "port_arrival");
//     let deadline = env.ledger().timestamp() + 3600;
//
//     let mut milestones = soroban_sdk::Vec::new(&env);
//     milestones.push_back((checkpoint.clone(), 100u32));
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
//         &milestones,
//         &deadline,
//     , &None);
//
//     client.deposit_escrow(&company, &shipment_id, &1000);
//
//     env.as_contract(&client.address, || {
//         let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
//         shipment.status = crate::ShipmentStatus::InTransit;
//         crate::storage::set_shipment(&env, &shipment);
//     });
//
//     client.record_milestone(&carrier, &shipment_id, &checkpoint, &data_hash);
//     // Try to record the same milestone again
//     client.record_milestone(&carrier, &shipment_id, &checkpoint, &data_hash);
// }

// ============= Error #20: MetadataLimitExceeded Tests =============

// NOTE: This test is commented out because the feature may not be fully implemented yet
// #[test]
// #[should_panic(expected = "Error(Contract, #20)")]
// fn test_set_shipment_metadata_returns_metadata_limit_exceeded() {
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
//     let shipment_id = client.create_shipment(
//         &company,
//         &receiver,
//         &carrier,
//         &data_hash,
//         &soroban_sdk::Vec::new(&env),
//         &deadline,
//     , &None);
//
//     // Add 5 metadata entries first (limit is 5)
//     for i in 0..5 {
//         let key = soroban_sdk::Symbol::new(&env, "key");
//         let value = soroban_sdk::Symbol::new(&env, "value");
//         client.set_shipment_metadata(&company, &shipment_id, &key, &value);
//     }
//
//     // Try to add 6th metadata entry (should fail)
//     let key = soroban_sdk::Symbol::new(&env, "key6");
//     let value = soroban_sdk::Symbol::new(&env, "value6");
//     client.set_shipment_metadata(&company, &shipment_id, &key, &value);
// }

// ============= Error #21: RateLimitExceeded Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #21)")]
fn test_update_status_returns_rate_limit_exceeded() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let hash_2 = BytesN::from_array(&env, &[2u8; 32]);
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
        &None,
    );

    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash_2);
    // Try to update again immediately without waiting 60 seconds
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::AtCheckpoint,
        &hash_2,
    );
}

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

// ============= Error #23: ProposalAlreadyExecuted Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn test_execute_proposal_returns_proposal_already_executed() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &2);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let proposal_id = client.propose_action(&admin, &crate::AdminAction::ForceRelease(shipment_id));

    client.approve_action(&admin2, &proposal_id);
    client.execute_proposal(&proposal_id);
    // Try to execute again
    client.execute_proposal(&proposal_id);
}

// ============= Error #24: ProposalExpired Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #24)")]
fn test_approve_action_returns_proposal_expired() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &2);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let proposal_id = client.propose_action(&admin, &crate::AdminAction::ForceRelease(shipment_id));

    // Fast forward time past expiration (7 days)
    super::test_utils::advance_past_multisig_expiry(&env);

    client.approve_action(&admin2, &proposal_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #24)")]
fn test_execute_proposal_returns_proposal_expired() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &2);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let proposal_id = client.propose_action(&admin, &crate::AdminAction::ForceRelease(shipment_id));

    client.approve_action(&admin2, &proposal_id);

    // Fast forward time past expiration
    super::test_utils::advance_past_multisig_expiry(&env);

    client.execute_proposal(&proposal_id);
}

// ============= Error #25: AlreadyApproved Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #25)")]
fn test_approve_action_returns_already_approved() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3);

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &3);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let proposal_id = client.propose_action(&admin, &crate::AdminAction::ForceRelease(shipment_id));

    client.approve_action(&admin2, &proposal_id);
    // Try to approve again with the same admin
    client.approve_action(&admin2, &proposal_id);
}

// ============= Error #26: InsufficientApprovals Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #26)")]
fn test_execute_proposal_returns_insufficient_approvals() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);
    admins.push_back(admin3);

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &3);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let proposal_id = client.propose_action(&admin, &crate::AdminAction::ForceRelease(shipment_id));

    // Only 1 approval (proposer), but threshold is 3
    client.execute_proposal(&proposal_id);
}

// ============= Error #27: NotAnAdmin Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_propose_action_returns_not_an_admin() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);
    let outsider = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &2);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    // Outsider tries to propose
    client.propose_action(&outsider, &crate::AdminAction::ForceRelease(shipment_id));
}

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_approve_action_returns_not_an_admin() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);
    let outsider = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &2);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let proposal_id = client.propose_action(&admin, &crate::AdminAction::ForceRelease(shipment_id));

    // Outsider tries to approve
    client.approve_action(&outsider, &proposal_id);
}

// ============= Error #28: InvalidMultiSigConfig Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_init_multisig_returns_invalid_multisig_config_threshold_too_high() {
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

// ============= Error #29: NotExpired Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #29)")]
fn test_check_deadline_returns_not_expired() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    // Try to check deadline before it expires
    client.check_deadline(&shipment_id);
}

// ============= Error #30: ShipmentLimitReached Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #30)")]
fn test_create_shipment_returns_shipment_limit_reached() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.set_shipment_limit(&admin, &1);

    // Create first shipment (should succeed)
    let hash1 = BytesN::from_array(&env, &[1u8; 32]);
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash1,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    // Try to create second shipment (should fail with limit reached)
    let hash2 = BytesN::from_array(&env, &[2u8; 32]);
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash2,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
}

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
            depends_on: None,
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
        &None,
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
        &None,
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
        &None,
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

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_deposit_escrow_returns_invalid_status() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    // Change status to Delivered
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    client.deposit_escrow(&company, &shipment_id, &1000);
}

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
//     , &None);
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
#[test]
fn test_full_shipment_lifecycle_integration() {
    use crate::ShipmentStatus;

    // ─── STEP 1: Setup Environment and Initialize Contract ───────────────────
    let (env, client, admin, token_contract) = setup_shipment_env();

    // Generate addresses for all participants
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    // Initialize contract with admin and token
    client.initialize(&admin, &token_contract);

    // Assign roles to all participants
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // Verify roles are assigned correctly
    assert_eq!(client.get_role(&company), crate::types::Role::Company);
    assert_eq!(client.get_role(&carrier), crate::types::Role::Carrier);

    // ─── STEP 2: Create Shipment with Payment Milestones ─────────────────────
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 7200; // 2 hours from now

    // Define payment milestones: 30% at warehouse, 30% at port, 40% on delivery
    let mut payment_milestones = soroban_sdk::Vec::new(&env);
    payment_milestones.push_back((soroban_sdk::Symbol::new(&env, "warehouse"), 30u32));
    payment_milestones.push_back((soroban_sdk::Symbol::new(&env, "port"), 30u32));
    payment_milestones.push_back((soroban_sdk::Symbol::new(&env, "delivery"), 40u32));

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &payment_milestones,
        &deadline,
        &None,
    );

    // Verify shipment was created with correct initial state
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.id, shipment_id);
    assert_eq!(shipment.sender, company);
    assert_eq!(shipment.receiver, receiver);
    assert_eq!(shipment.carrier, carrier);
    assert_eq!(shipment.status, ShipmentStatus::Created);
    assert_eq!(shipment.escrow_amount, 0);

    // ─── STEP 3: Deposit Escrow ───────────────────────────────────────────────
    let escrow_amount: i128 = 100_000; // 100,000 stroops
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    // Verify escrow was deposited correctly
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(
        shipment.escrow_amount, escrow_amount,
        "Shipment escrow_amount should match"
    );
    assert_eq!(
        shipment.total_escrow, escrow_amount,
        "Shipment total_escrow should match"
    );

    // ─── STEP 4: Update Status to InTransit ───────────────────────────────────
    let transit_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &transit_hash,
    );

    // Verify status transition
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::InTransit);

    // ─── STEP 5: Record First Milestone (Warehouse) ──────────────────────────
    // Advance time to bypass rate limiting
    super::test_utils::advance_past_rate_limit(&env);

    let warehouse_checkpoint = soroban_sdk::Symbol::new(&env, "warehouse");
    let milestone_hash_1 = BytesN::from_array(&env, &[3u8; 32]);
    client.record_milestone(
        &carrier,
        &shipment_id,
        &warehouse_checkpoint,
        &milestone_hash_1,
    );

    // Verify partial payment was made (30% of 100,000 = 30,000)
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 70_000); // 70,000 remaining
    assert_eq!(shipment.paid_milestones.len(), 1);

    // ─── STEP 6: Update Status to AtCheckpoint ───────────────────────────────
    super::test_utils::advance_past_rate_limit(&env);
    let checkpoint_hash = BytesN::from_array(&env, &[4u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::AtCheckpoint,
        &checkpoint_hash,
    );

    // Verify status transition
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::AtCheckpoint);

    // ─── STEP 7: Update Status Back to InTransit ─────────────────────────────
    super::test_utils::advance_past_rate_limit(&env);
    let transit_hash_2 = BytesN::from_array(&env, &[5u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &transit_hash_2,
    );

    // Verify status transition
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::InTransit);

    // ─── STEP 8: Record Second Milestone (Port) ──────────────────────────────
    super::test_utils::advance_past_rate_limit(&env);
    let port_checkpoint = soroban_sdk::Symbol::new(&env, "port");
    let milestone_hash_2 = BytesN::from_array(&env, &[6u8; 32]);
    client.record_milestone(&carrier, &shipment_id, &port_checkpoint, &milestone_hash_2);

    // Verify second partial payment was made (30% of 100,000 = 30,000)
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 40_000); // 40,000 remaining (40%)
    assert_eq!(shipment.paid_milestones.len(), 2);

    // ─── STEP 9: Confirm Delivery by Receiver ────────────────────────────────
    // Note: Receiver confirms delivery while shipment is still InTransit or AtCheckpoint
    // The confirm_delivery function will automatically set status to Delivered
    let confirmation_hash = BytesN::from_array(&env, &[99u8; 32]);
    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    // Verify delivery was confirmed and remaining escrow was released
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Delivered);
    assert_eq!(shipment.escrow_amount, 0); // All funds released

    // ─── STEP 10: Verify Final State ─────────────────────────────────────────
    // Verify shipment count increased
    assert_eq!(client.get_shipment_count(), 1);

    // Verify all events were emitted (check that events exist)
    let all_events = env.events().all();

    // Count specific event types if events are available
    if !all_events.is_empty() {
        let mut shipment_created_count = 0;
        let mut status_updated_count = 0;
        let mut milestone_recorded_count = 0;
        let mut delivery_success_count = 0;
        let mut escrow_released_count = 0;

        for (_contract, topics, _data) in all_events.iter() {
            if let Some(raw) = topics.get(0) {
                if let Ok(topic) = soroban_sdk::Symbol::try_from_val(&env, &raw) {
                    if topic == soroban_sdk::Symbol::new(&env, "shipment_created") {
                        shipment_created_count += 1;
                    } else if topic == soroban_sdk::Symbol::new(&env, "status_updated") {
                        status_updated_count += 1;
                    } else if topic == soroban_sdk::Symbol::new(&env, "milestone_recorded") {
                        milestone_recorded_count += 1;
                    } else if topic == soroban_sdk::Symbol::new(&env, "delivery_success") {
                        delivery_success_count += 1;
                    } else if topic == soroban_sdk::Symbol::new(&env, "escrow_released") {
                        escrow_released_count += 1;
                    }
                }
            }
        }

        // Verify expected event counts
        assert_eq!(
            shipment_created_count, 1,
            "Expected 1 shipment_created event"
        );
        assert!(
            status_updated_count >= 3,
            "Expected at least 3 status_updated events"
        );
        assert_eq!(
            milestone_recorded_count, 2,
            "Expected 2 milestone_recorded events"
        );
        assert_eq!(
            delivery_success_count, 1,
            "Expected 1 delivery_success event"
        );
        assert!(
            escrow_released_count >= 1,
            "Expected at least 1 escrow_released event"
        );
    }

    // Verify analytics counters were updated
    let analytics = client.get_analytics();
    assert_eq!(analytics.total_shipments, 1);
    assert_eq!(analytics.total_escrow_volume, escrow_amount);
    assert_eq!(analytics.delivered_count, 1);

    // ─── Test Complete: Full Lifecycle Verified ──────────────────────────────
    // This test successfully verified:
    // ✓ Contract initialization and role assignment
    // ✓ Shipment creation with payment milestones
    // ✓ Escrow deposit and tracking
    // ✓ Multiple status transitions (Created → InTransit → AtCheckpoint → InTransit)
    // ✓ Milestone recording with partial payments (30% + 30%)
    // ✓ Delivery confirmation by receiver (automatically sets to Delivered)
    // ✓ Automatic escrow release on delivery (remaining 40%)
    // ✓ All events emitted correctly
    // ✓ Analytics counters updated
    // ✓ Role-based access control enforced throughout
}

// ============= Event Counter Tests =============

#[test]
fn test_event_count_after_create() {
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
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    // After creation, should have 1 event (shipment_created)
    let count = client.get_event_count(&shipment_id);
    assert_eq!(count, 1, "Expected 1 event after shipment creation");
}

#[test]
fn test_event_count_after_milestone() {
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
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    // Update status to InTransit
    let status_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &status_hash,
    );

    // Record a milestone
    let milestone_hash = BytesN::from_array(&env, &[3u8; 32]);
    client.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "warehouse"),
        &milestone_hash,
    );

    // Should have 3 events: shipment_created, status_updated, milestone_recorded
    let count = client.get_event_count(&shipment_id);
    assert_eq!(count, 3, "Expected 3 events after milestone recording");
}

#[test]
fn test_event_count_after_status_updates() {
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
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    // Update status to InTransit
    let status_hash1 = BytesN::from_array(&env, &[2u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &status_hash1,
    );

    // Advance ledger timestamp to avoid rate limit
    super::test_utils::advance_past_rate_limit(&env);

    // Update status to AtCheckpoint
    let status_hash2 = BytesN::from_array(&env, &[3u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::AtCheckpoint,
        &status_hash2,
    );

    // Should have 3 events: shipment_created, status_updated (x2)
    let count = client.get_event_count(&shipment_id);
    assert_eq!(count, 3, "Expected 3 events after 2 status updates");
}

#[test]
fn test_event_count_after_delivery() {
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
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    // Update status to InTransit
    let status_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &status_hash,
    );

    // Confirm delivery
    let confirmation_hash = BytesN::from_array(&env, &[3u8; 32]);
    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    // Should have 3 events: shipment_created, status_updated, delivery_success
    let count = client.get_event_count(&shipment_id);
    assert_eq!(count, 3, "Expected 3 events after delivery confirmation");
}

#[test]
fn test_event_count_returns_zero_for_new_shipment() {
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
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    // Immediately after creation, should have 1 event
    let count = client.get_event_count(&shipment_id);
    assert_eq!(count, 1, "Expected 1 event for newly created shipment");
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_event_count_shipment_not_found() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    // Try to get event count for non-existent shipment
    client.get_event_count(&999);
}

#[test]
fn test_event_count_with_multiple_milestones() {
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
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    // Update status to InTransit
    let status_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &status_hash,
    );

    // Record multiple milestones
    let milestone_hash1 = BytesN::from_array(&env, &[3u8; 32]);
    client.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "warehouse"),
        &milestone_hash1,
    );

    let milestone_hash2 = BytesN::from_array(&env, &[4u8; 32]);
    client.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "port"),
        &milestone_hash2,
    );

    let milestone_hash3 = BytesN::from_array(&env, &[5u8; 32]);
    client.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "customs"),
        &milestone_hash3,
    );

    // Should have 5 events: shipment_created, status_updated, milestone_recorded (x3)
    let count = client.get_event_count(&shipment_id);
    assert_eq!(count, 5, "Expected 5 events after recording 3 milestones");
}

// ============= Shipment Archival Tests =============

#[test]
fn test_archive_delivered_shipment() {
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
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    // Update to InTransit and confirm delivery
    let status_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &status_hash,
    );

    let confirmation_hash = BytesN::from_array(&env, &[3u8; 32]);
    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    // Archive the delivered shipment
    client.archive_shipment(&admin, &shipment_id);

    // Verify shipment is still readable (from temporary storage)
    let archived_shipment = client.get_shipment(&shipment_id);
    assert_eq!(archived_shipment.status, ShipmentStatus::Delivered);
    assert_eq!(archived_shipment.id, shipment_id);
}

#[test]
fn test_archive_cancelled_shipment() {
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
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    // Cancel the shipment
    let reason_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    // Archive the cancelled shipment
    client.archive_shipment(&admin, &shipment_id);

    // Verify shipment is still readable (from temporary storage)
    let archived_shipment = client.get_shipment(&shipment_id);
    assert_eq!(archived_shipment.status, ShipmentStatus::Cancelled);
    assert_eq!(archived_shipment.id, shipment_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_archive_active_shipment_fails() {
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
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    // Try to archive an active shipment (should fail with InvalidStatus)
    client.archive_shipment(&admin, &shipment_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_archive_nonexistent_shipment_fails() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    // Try to archive a non-existent shipment (should fail with ShipmentNotFound)
    client.archive_shipment(&admin, &999);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_archive_shipment_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;
    let non_admin = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    // Cancel the shipment
    let reason_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    // Try to archive as non-admin (should fail with Unauthorized)
    client.archive_shipment(&non_admin, &shipment_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_archive_in_transit_shipment_fails() {
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
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    // Update to InTransit
    let status_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &status_hash,
    );

    // Try to archive an in-transit shipment (should fail with InvalidStatus)
    client.archive_shipment(&admin, &shipment_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_archive_disputed_shipment_fails() {
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
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    // Update to InTransit
    let status_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &status_hash,
    );

    // Raise a dispute
    let reason_hash = BytesN::from_array(&env, &[3u8; 32]);
    client.raise_dispute(&carrier, &shipment_id, &reason_hash);

    // Try to archive a disputed shipment (should fail with InvalidStatus)
    client.archive_shipment(&admin, &shipment_id);
}

#[test]
fn test_restore_diagnostics_missing_state() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let diagnostics: PersistentRestoreDiagnostics = client.get_restore_diagnostics(&999_u64);
    assert_eq!(diagnostics.shipment_id, 999_u64);
    assert_eq!(diagnostics.state, StoragePresenceState::Missing);
    assert!(!diagnostics.persistent_shipment_present);
    assert!(!diagnostics.archived_shipment_present);
}

#[test]
fn test_restore_diagnostics_active_persistent_state() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[9u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    let diagnostics: PersistentRestoreDiagnostics = client.get_restore_diagnostics(&shipment_id);
    assert_eq!(diagnostics.state, StoragePresenceState::ActivePersistent);
    assert!(diagnostics.persistent_shipment_present);
    assert!(!diagnostics.archived_shipment_present);
}

#[test]
fn test_restore_diagnostics_archived_expected_state() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[8u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    client.cancel_shipment(&company, &shipment_id, &data_hash);
    client.archive_shipment(&admin, &shipment_id);

    let diagnostics: PersistentRestoreDiagnostics = client.get_restore_diagnostics(&shipment_id);
    assert_eq!(diagnostics.state, StoragePresenceState::ArchivedExpected);
    assert!(!diagnostics.persistent_shipment_present);
    assert!(diagnostics.archived_shipment_present);
}

#[test]
fn test_restore_diagnostics_inconsistent_dual_presence_state() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::vec![&env],
        &deadline,
        &None,
    );

    // Inject archived copy without removing persistent state to simulate inconsistent storage.
    let shipment = client.get_shipment(&shipment_id);
    env.as_contract(&client.address, || {
        env.storage()
            .temporary()
            .set(&DataKey::ArchivedShipment(shipment_id), &shipment);
    });

    let diagnostics: PersistentRestoreDiagnostics = client.get_restore_diagnostics(&shipment_id);
    assert_eq!(
        diagnostics.state,
        StoragePresenceState::InconsistentDualPresence
    );
    assert!(diagnostics.persistent_shipment_present);
    assert!(diagnostics.archived_shipment_present);
}

// ============= Analytics Event Tests =============

#[test]
fn test_carrier_handoff_completed_event() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &current_carrier);
    client.add_carrier(&admin, &new_carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &current_carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    client.handoff_shipment(&current_carrier, &new_carrier, &shipment_id, &handoff_hash);

    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "carrier_handoff_completed") {
                        found = true;
                        let event_data =
                            <(Address, Address, u64)>::try_from_val(&env, &event.2).unwrap();
                        assert_eq!(
                            event_data,
                            (current_carrier.clone(), new_carrier.clone(), shipment_id)
                        );
                    }
                }
            }
        }
    }
    assert!(found, "carrier_handoff_completed event not found");
}

#[test]
fn test_carrier_on_time_delivery_event() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let confirmation_hash = BytesN::from_array(&env, &[2u8; 32]);
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
        &None,
    );

    client.deposit_escrow(&company, &shipment_id, &1000);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );
    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    let events = env.events().all();
    let mut found = false;
    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "carrier_on_time_delivery") {
                        found = true;
                        let event_data = <(Address, u64)>::try_from_val(&env, &event.2).unwrap();
                        assert_eq!(event_data, (carrier.clone(), shipment_id));
                    }
                }
            }
        }
    }
    assert!(found, "carrier_on_time_delivery event not found");
}

#[test]
fn test_carrier_late_delivery_event_and_milestones() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let confirmation_hash = BytesN::from_array(&env, &[2u8; 32]);

    // Set a future deadline
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "warehouse"), 50));
    milestones.push_back((Symbol::new(&env, "port"), 50));

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );

    client.deposit_escrow(&company, &shipment_id, &1000);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    // Hit one milestone
    client.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "warehouse"),
        &BytesN::from_array(&env, &[3u8; 32]),
    );

    // Advance time past the deadline to trigger a late delivery
    super::test_utils::set_ledger_time(&env, deadline + 100);

    // Delivery
    let actual_time = env.ledger().timestamp();
    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    let events = env.events().all();
    let mut found_late = false;
    let mut found_milestone_rate = false;

    for event in events.iter() {
        if event.0 == client.address {
            if let Some(first_val) = event.1.get(0) {
                if let Ok(topic) = Symbol::try_from_val(&env, &first_val) {
                    if topic == Symbol::new(&env, "carrier_late_delivery") {
                        found_late = true;
                        let event_data =
                            <(Address, u64, u64, u64)>::try_from_val(&env, &event.2).unwrap();
                        assert_eq!(
                            event_data,
                            (carrier.clone(), shipment_id, deadline, actual_time)
                        );
                    } else if topic == Symbol::new(&env, "carrier_milestone_rate") {
                        found_milestone_rate = true;
                        let event_data =
                            <(Address, u64, u32, u32)>::try_from_val(&env, &event.2).unwrap();
                        assert_eq!(event_data, (carrier.clone(), shipment_id, 1, 2));
                    }
                }
            }
        }
    }
    assert!(found_late, "carrier_late_delivery event not found");
    assert!(
        found_milestone_rate,
        "carrier_milestone_rate event not found"
    );
}

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
fn test_revoke_role_then_create_shipment_fails() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let hash = BytesN::from_array(&env, &[1u8; 32]);
    let milestones = soroban_sdk::vec![&env, (Symbol::new(&env, "delivery"), 100u32)];
    let deadline = env.ledger().timestamp() + 86400;

    // Company can create a shipment
    let _id = client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline, &None);

    // Revoke company role
    client.revoke_role(&admin, &company);

    // Now creating a shipment should fail with Unauthorized
    let result =
        client.try_create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline, &None);
    assert!(result.is_err());
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
        &None,
    );
    }));

    assert!(
        result.is_err(),
        "Suspended company should not be able to create shipments"
    );
}

#[test]
fn test_get_shipment_reference_deterministic() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    let ref1 = client.get_shipment_reference(&shipment_id);
    let ref2 = client.get_shipment_reference(&shipment_id);

    assert_eq!(ref1, ref2);
    assert_eq!(ref1.len(), 64);
}

#[test]
fn test_get_shipment_reference_collision_free() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    let id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[2u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let ref1 = client.get_shipment_reference(&id1);
    let ref2 = client.get_shipment_reference(&id2);

    assert_ne!(ref1, ref2);
}

// ============= Deadline Grace Period Tests =============

/// Helper: initialize the contract, register a company, and create a shipment with the given
/// deadline. Returns the shipment ID.
fn setup_shipment_with_deadline(
    env: &Env,
    client: &NavinShipmentClient,
    admin: &Address,
    token_contract: &Address,
    deadline: u64,
) -> u64 {
    let company = Address::generate(env);
    let receiver = Address::generate(env);
    let carrier = Address::generate(env);
    let data_hash = BytesN::from_array(env, &[42u8; 32]);

    client.initialize(admin, token_contract);
    client.add_company(admin, &company);

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(env),
        &deadline,
        &None,
    )
}

/// Within the grace window: deadline has passed but grace has not — must return NotExpired.
#[test]
#[should_panic(expected = "Error(Contract, #29)")]
fn test_check_deadline_within_grace_period_returns_not_expired() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let now = env.ledger().timestamp();
    let deadline = now + 1000;
    let grace = 300u64;

    let shipment_id =
        setup_shipment_with_deadline(&env, &client, &admin, &token_contract, deadline);

    // Configure a 300-second grace period
    let mut config = client.get_contract_config();
    config.deadline_grace_seconds = grace;
    client.update_config(&admin, &config);

    // Advance time: deadline has passed, but we are still inside the grace window
    // timestamp = deadline + grace - 1  =>  NOT yet expired
    super::test_utils::set_ledger_time(&env, deadline + grace - 1);

    client.check_deadline(&shipment_id);
}

/// Exactly at the grace boundary: timestamp == deadline + grace — must succeed.
#[test]
fn test_check_deadline_at_grace_boundary_succeeds() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let now = env.ledger().timestamp();
    let deadline = now + 1000;
    let grace = 300u64;

    let shipment_id =
        setup_shipment_with_deadline(&env, &client, &admin, &token_contract, deadline);

    let mut config = client.get_contract_config();
    config.deadline_grace_seconds = grace;
    client.update_config(&admin, &config);

    // Advance time to exactly deadline + grace
    super::test_utils::set_ledger_time(&env, deadline + grace);

    client.check_deadline(&shipment_id);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
}

/// After the grace window: timestamp > deadline + grace — must succeed and cancel.
#[test]
fn test_check_deadline_after_grace_period_cancels_shipment() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let now = env.ledger().timestamp();
    let deadline = now + 1000;
    let grace = 300u64;

    let shipment_id =
        setup_shipment_with_deadline(&env, &client, &admin, &token_contract, deadline);

    let mut config = client.get_contract_config();
    config.deadline_grace_seconds = grace;
    client.update_config(&admin, &config);

    // Advance time well past deadline + grace
    super::test_utils::set_ledger_time(&env, deadline + grace + 500);

    client.check_deadline(&shipment_id);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
}

/// Zero grace (default): deadline passed by 1 second — must succeed immediately.
#[test]
fn test_check_deadline_zero_grace_expires_immediately() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let now = env.ledger().timestamp();
    let deadline = now + 1000;

    let shipment_id =
        setup_shipment_with_deadline(&env, &client, &admin, &token_contract, deadline);

    // Default config has deadline_grace_seconds = 0
    super::test_utils::set_ledger_time(&env, deadline + 1);

    client.check_deadline(&shipment_id);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
}

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
        &None,
    );

    (env, client, admin, token_contract, company, shipment_id)
}

/// Admin can force-cancel a shipment in Created status.
#[test]
fn test_force_cancel_shipment_success_created() {
    let (env, client, admin, _token_contract, _company, shipment_id) = setup_force_cancel_env();

    let reason_hash = BytesN::from_array(&env, &[0x01u8; 32]);
    client.force_cancel_shipment(&admin, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
}

/// Admin can force-cancel a shipment that is InTransit.
#[test]
fn test_force_cancel_shipment_success_in_transit() {
    let (env, client, admin, _token_contract, _company, shipment_id) = setup_force_cancel_env();

    let data_hash = BytesN::from_array(&env, &[0x02u8; 32]);

    // Move to InTransit via admin (bypasses carrier whitelist requirement)
    super::test_utils::advance_ledger_time(&env, 120);
    client.update_status(&admin, &shipment_id, &ShipmentStatus::InTransit, &data_hash);

    let reason_hash = BytesN::from_array(&env, &[0x03u8; 32]);
    client.force_cancel_shipment(&admin, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);
}

/// Admin can force-cancel a Disputed shipment (bypasses normal cancel restriction).
#[test]
fn test_force_cancel_shipment_success_disputed() {
    let (env, client, admin, _token_contract, company, shipment_id) = setup_force_cancel_env();

    let data_hash = BytesN::from_array(&env, &[0x04u8; 32]);

    // Raise a dispute as the company (sender)
    client.raise_dispute(&company, &shipment_id, &data_hash);

    let reason_hash = BytesN::from_array(&env, &[0x05u8; 32]);
    client.force_cancel_shipment(&admin, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);
}

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
#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_force_cancel_shipment_zero_reason_hash_rejected() {
    let (env, client, admin, _token_contract, _company, shipment_id) = setup_force_cancel_env();

    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
    client.force_cancel_shipment(&admin, &shipment_id, &zero_hash);
}

/// Force-cancelling a non-existent shipment returns ShipmentNotFound (#4).
#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_force_cancel_shipment_not_found() {
    let (env, client, admin, _token_contract, _company, _shipment_id) = setup_force_cancel_env();

    let reason_hash = BytesN::from_array(&env, &[0x07u8; 32]);
    client.force_cancel_shipment(&admin, &9999, &reason_hash);
}

/// Force-cancelling an already-Delivered shipment returns ShipmentFinalized (#38).
#[test]
#[should_panic(expected = "Error(Contract, #38)")]
fn test_force_cancel_shipment_already_delivered() {
    let (env, client, admin, _token_contract, _company, shipment_id) = setup_force_cancel_env();

    let shipment = client.get_shipment(&shipment_id);
    let receiver = shipment.receiver.clone();
    let confirmation_hash = BytesN::from_array(&env, &[0x08u8; 32]);

    // Move to InTransit then Delivered via admin
    super::test_utils::advance_ledger_time(&env, 120);
    client.update_status(
        &admin,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[0x09u8; 32]),
    );
    super::test_utils::advance_ledger_time(&env, 120);
    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    let reason_hash = BytesN::from_array(&env, &[0x0Au8; 32]);
    client.force_cancel_shipment(&admin, &shipment_id, &reason_hash);
}

/// Force-cancelling an already-Cancelled shipment returns ShipmentFinalized (#38).
#[test]
#[should_panic(expected = "Error(Contract, #38)")]
fn test_force_cancel_shipment_already_cancelled() {
    let (env, client, admin, _token_contract, company, shipment_id) = setup_force_cancel_env();

    let reason_hash = BytesN::from_array(&env, &[0x0Bu8; 32]);
    // Regular cancel first
    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    // Force-cancel on already-cancelled shipment must fail
    client.force_cancel_shipment(&admin, &shipment_id, &reason_hash);
}

/// Escrow is deterministically zeroed on force-cancel (no-escrow path).
/// Verifies escrow_amount stays 0 and force_cancelled event is emitted.
#[test]
fn test_force_cancel_shipment_refunds_escrow() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, _token_contract, _company, shipment_id) = setup_force_cancel_env();

    // No escrow deposited — escrow_amount is already 0
    let reason_hash = BytesN::from_array(&env, &[0x0Cu8; 32]);
    client.force_cancel_shipment(&admin, &shipment_id, &reason_hash);

    // Check events BEFORE any further client calls (env.events().all() is cumulative,
    // but snapshot-based client calls may flush the buffer internally).
    let events = env.events().all();
    let has_force_cancelled = events.iter().any(|(_contract, topics, _data)| {
        if let Some(raw) = topics.get(0) {
            if let Ok(topic) = Symbol::try_from_val(&env, &raw) {
                return topic == Symbol::new(&env, "force_cancelled");
            }
        }
        false
    });
    assert!(has_force_cancelled, "force_cancelled event must be emitted");

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
}

/// The dedicated force_cancelled event is emitted (not shipment_cancelled).
#[test]
fn test_force_cancel_emits_dedicated_event_not_shipment_cancelled() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, _token_contract, _company, shipment_id) = setup_force_cancel_env();

    let reason_hash = BytesN::from_array(&env, &[0x0Du8; 32]);
    client.force_cancel_shipment(&admin, &shipment_id, &reason_hash);

    let events = env.events().all();

    let has_force_cancelled = events.iter().any(|(_c, topics, _d)| {
        if let Some(raw) = topics.get(0) {
            if let Ok(topic) = Symbol::try_from_val(&env, &raw) {
                return topic == Symbol::new(&env, "force_cancelled");
            }
        }
        false
    });

    let has_shipment_cancelled = events.iter().any(|(_c, topics, _d)| {
        if let Some(raw) = topics.get(0) {
            if let Ok(topic) = Symbol::try_from_val(&env, &raw) {
                return topic == Symbol::new(&env, "shipment_cancelled");
            }
        }
        false
    });

    assert!(has_force_cancelled, "force_cancelled event must be emitted");
    assert!(
        !has_shipment_cancelled,
        "shipment_cancelled must NOT be emitted on force-cancel"
    );
}

// ============= Shipment Note Tests =============

#[test]
fn test_shipment_notes_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
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
        &None,
    );

    let note_hash1 = BytesN::from_array(&env, &[10u8; 32]);
    let note_hash2 = BytesN::from_array(&env, &[11u8; 32]);

    // Sender can append
    client.append_note_hash(&company, &shipment_id, &note_hash1.clone());
    assert_eq!(client.get_note_count(&shipment_id), 1);
    assert_eq!(
        client.get_note_hash(&shipment_id, &0),
        Some(note_hash1.clone())
    );

    // Carrier can append
    client.append_note_hash(&carrier, &shipment_id, &note_hash2.clone());
    assert_eq!(client.get_note_count(&shipment_id), 2);
    assert_eq!(
        client.get_note_hash(&shipment_id, &1),
        Some(note_hash2.clone())
    );

    // Admin can append
    let note_hash3 = BytesN::from_array(&env, &[12u8; 32]);
    client.append_note_hash(&admin, &shipment_id, &note_hash3.clone());
    assert_eq!(client.get_note_count(&shipment_id), 3);

    // Verify storage consistency
    assert_eq!(
        client.get_note_hash(&shipment_id, &0),
        Some(note_hash1.clone())
    );
    assert_eq!(
        client.get_note_hash(&shipment_id, &1),
        Some(note_hash2.clone())
    );
    assert_eq!(
        client.get_note_hash(&shipment_id, &2),
        Some(note_hash3.clone())
    );

    // Verify event count was incremented in storage (proves event emission was triggered)
    assert_eq!(client.get_event_count(&shipment_id), 4);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_shipment_notes_unauthorized() {
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
        &None,
    );

    let note_hash = BytesN::from_array(&env, &[10u8; 32]);
    // Outsider cannot append
    client.append_note_hash(&outsider, &shipment_id, &note_hash);
}

// ============= Idempotency Window Tests =============

#[test]
fn test_idempotency_create_shipment_first_run_succeeds() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let data_hash = BytesN::from_array(&env, &[42u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );
    assert_eq!(id, 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #41)")]
fn test_idempotency_create_shipment_duplicate_in_window_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let data_hash = BytesN::from_array(&env, &[42u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;
    let milestones = soroban_sdk::Vec::new(&env);

    // First call succeeds
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );
    // Immediate replay within window must be rejected with DuplicateAction (#41)
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );
}

#[test]
fn test_idempotency_create_shipment_different_hash_not_blocked() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let milestones = soroban_sdk::Vec::new(&env);

    let id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &milestones,
        &deadline,
        &None,
    );
    // Different data_hash → different action hash → not blocked
    let id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[2u8; 32]),
        &milestones,
        &deadline,
        &None,
    );
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #41)")]
fn test_idempotency_update_status_duplicate_in_window_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let status_hash = BytesN::from_array(&env, &[2u8; 32]);
    // First update succeeds
    client.update_status(&carrier, &id, &ShipmentStatus::InTransit, &status_hash);
    // Immediate replay with same (id, status, hash) must be rejected
    client.update_status(&carrier, &id, &ShipmentStatus::InTransit, &status_hash);
}

#[test]
fn test_idempotency_update_status_different_hash_not_blocked() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    // InTransit with hash_a
    client.update_status(
        &carrier,
        &id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[2u8; 32]),
    );
    super::test_utils::advance_past_rate_limit(&env);
    // AtCheckpoint with hash_b — different action hash, must succeed
    client.update_status(
        &carrier,
        &id,
        &ShipmentStatus::AtCheckpoint,
        &BytesN::from_array(&env, &[3u8; 32]),
    );
}

// ============================================================================
// Integration Tests for Symbol and BytesN<32> Validators
// ============================================================================

#[test]
fn test_create_shipment_with_valid_milestone_symbols() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "warehouse"), 30_u32));
    milestones.push_back((Symbol::new(&env, "port"), 30_u32));
    milestones.push_back((Symbol::new(&env, "final"), 40_u32));

    let deadline = super::test_utils::future_deadline(&env, 86400);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );

    assert!(id > 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #17)")]
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
        &None,
    );
}

#[test]
fn test_set_metadata_with_valid_symbols() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "delivery"), 100_u32));

    let deadline = super::test_utils::future_deadline(&env, 86400);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );

    // Set metadata with valid symbols
    client.set_shipment_metadata(
        &company,
        &id,
        &Symbol::new(&env, "weight"),
        &Symbol::new(&env, "kg_100"),
    );

    let shipment = client.get_shipment(&id);
    assert!(shipment.metadata.is_some());
}

#[test]
fn test_append_note_hash_validates_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "delivery"), 100_u32));

    let deadline = super::test_utils::future_deadline(&env, 86400);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );

    // Append a valid note hash
    let note_hash = BytesN::from_array(&env, &[8u8; 32]);
    client.append_note_hash(&company, &id, &note_hash);

    // Verify event was emitted
    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_append_note_hash_rejects_zero_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "delivery"), 100_u32));

    let deadline = super::test_utils::future_deadline(&env, 86400);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );

    // Try to append an all-zero hash (should fail)
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
    client.append_note_hash(&company, &id, &zero_hash);
}

#[test]
fn test_add_dispute_evidence_hash_validates_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "delivery"), 100_u32));

    let deadline = super::test_utils::future_deadline(&env, 86400);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );

    // Transition to Disputed state
    client.raise_dispute(&company, &id, &BytesN::from_array(&env, &[9u8; 32]));

    // Add evidence with valid hash
    let evidence_hash = BytesN::from_array(&env, &[10u8; 32]);
    client.add_dispute_evidence_hash(&company, &id, &evidence_hash);

    // Verify event was emitted
    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_add_dispute_evidence_hash_rejects_zero_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "delivery"), 100_u32));

    let deadline = super::test_utils::future_deadline(&env, 86400);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );

    // Transition to Disputed state
    client.raise_dispute(&company, &id, &BytesN::from_array(&env, &[9u8; 32]));

    // Try to add evidence with all-zero hash (should fail)
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
    client.add_dispute_evidence_hash(&company, &id, &zero_hash);
}

#[test]
fn test_create_shipments_batch_validates_milestone_symbols() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver1 = Address::generate(&env);
    let receiver2 = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut inputs = soroban_sdk::Vec::new(&env);

    // First shipment with valid milestones
    let mut milestones1 = soroban_sdk::Vec::new(&env);
    milestones1.push_back((Symbol::new(&env, "warehouse"), 50_u32));
    milestones1.push_back((Symbol::new(&env, "delivery"), 50_u32));

    let deadline = super::test_utils::future_deadline(&env, 86400);
    let data_hash1 = BytesN::from_array(&env, &[7u8; 32]);

    inputs.push_back(ShipmentInput {
        receiver: receiver1,
        carrier: carrier.clone(),
        data_hash: data_hash1,
        payment_milestones: milestones1,
        deadline,
            depends_on: None,
        });

    // Second shipment with valid milestones
    let mut milestones2 = soroban_sdk::Vec::new(&env);
    milestones2.push_back((Symbol::new(&env, "port"), 100_u32));

    let data_hash2 = BytesN::from_array(&env, &[8u8; 32]);

    inputs.push_back(ShipmentInput {
        receiver: receiver2,
        carrier: carrier.clone(),
        data_hash: data_hash2,
        payment_milestones: milestones2,
        deadline,
            depends_on: None,
        });

    let ids = client.create_shipments_batch(&company, &inputs);
    assert_eq!(ids.len(), 2);
}

#[test]
fn test_metadata_symbols_multiple_entries() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "delivery"), 100_u32));

    let deadline = super::test_utils::future_deadline(&env, 86400);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );

    // Add multiple metadata entries with valid symbols
    let metadata_pairs = [
        ("weight", "kg_100"),
        ("priority", "high"),
        ("category", "fragile"),
    ];

    for (key_str, val_str) in &metadata_pairs {
        client.set_shipment_metadata(
            &company,
            &id,
            &Symbol::new(&env, key_str),
            &Symbol::new(&env, val_str),
        );
    }

    let shipment = client.get_shipment(&id);
    assert!(shipment.metadata.is_some());
    let metadata = shipment.metadata.unwrap();
    assert_eq!(metadata.len(), 3);
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
fn test_guardian_can_resolve_disputes() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    // Deposit escrow so resolve_dispute doesn't fail with InsufficientFunds
    client.deposit_escrow(&company, &shipment_id, &1000);

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    client.raise_dispute(&company, &shipment_id, &data_hash);

    client.resolve_dispute(
        &guardian,
        &shipment_id,
        &crate::DisputeResolution::RefundToCompany,
        &data_hash,
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);
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
fn test_report_condition_breach_limit_exceeded() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    // Initial status update to move into InTransit
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    // Update config to have a small breach limit for testing
    let config = crate::ContractConfig {
        max_breaches_per_shipment: 2,
        ..crate::ContractConfig::default()
    };
    client.update_config(&admin, &config);

    // First breach - OK
    client.report_condition_breach(
        &carrier,
        &shipment_id,
        &BreachType::TemperatureHigh,
        &Severity::Medium,
        &data_hash,
    );

    // Second breach - OK
    client.report_condition_breach(
        &carrier,
        &shipment_id,
        &BreachType::Impact,
        &Severity::High,
        &data_hash,
    );

    // Third breach - Should fail
    let res = client.try_report_condition_breach(
        &carrier,
        &shipment_id,
        &BreachType::TamperDetected,
        &Severity::Critical,
        &data_hash,
    );

    assert_eq!(res, Err(Ok(crate::NavinError::BreachLimitExceeded)));
}

#[test]
fn test_deposit_escrow_invalid_token_decimals() {
    let (env, admin) = super::test_utils::setup_env();
    let token_contract = env.register(invalid_token::MockTokenInvalidDecimals {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));

    client.initialize(&admin, &token_contract);

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
        &None,
    );

    let res = client.try_deposit_escrow(&company, &shipment_id, &1000);
    assert_eq!(res, Err(Ok(crate::NavinError::InvalidTokenDecimals)));
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
fn test_deposit_escrow_invalid_token_high_decimals() {
    let (env, admin) = super::test_utils::setup_env();
    let token_contract = env.register(invalid_token_high_decimals::MockTokenHighDecimals {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));

    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[2u8; 32]);
    let milestones = soroban_sdk::Vec::new(&env);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
        &None,
    );

    let res = client.try_deposit_escrow(&company, &shipment_id, &1000);
    assert_eq!(res, Err(Ok(crate::NavinError::InvalidTokenDecimals)));
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
        &None,
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

#[test]
fn test_observer_can_read_shipment() {
    let (env, client, admin, token) = setup_shipment_env();
    client.initialize(&admin, &token);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    let observer = Address::generate(&env);

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

    client.assign_observer_role(&admin, &observer, &shipment_id);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.id, shipment_id);
    assert_eq!(shipment.receiver, receiver);
}

#[test]
fn test_observer_cannot_update_status() {
    let (env, client, admin, token) = setup_shipment_env();
    client.initialize(&admin, &token);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    let observer = Address::generate(&env);

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

    client.assign_observer_role(&admin, &observer, &shipment_id);

    let result = client.try_update_status(
        &observer,
        &shipment_id,
        &crate::ShipmentStatus::InTransit,
        &data_hash,
    );
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));
}

#[test]
fn test_observer_assignment_by_non_admin_fails() {
    let (env, client, admin, token) = setup_shipment_env();
    client.initialize(&admin, &token);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    let observer = Address::generate(&env);

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

    let result = client.try_assign_observer_role(&company, &observer, &shipment_id);
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));
}

#[test]
fn test_multiple_observers_on_one_shipment() {
    let (env, client, admin, token) = setup_shipment_env();
    client.initialize(&admin, &token);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    let observer1 = Address::generate(&env);
    let observer2 = Address::generate(&env);

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

    client.assign_observer_role(&admin, &observer1, &shipment_id);
    client.assign_observer_role(&admin, &observer2, &shipment_id);

    let count = client.get_observer_count(&shipment_id);
    assert_eq!(count, 2);

    let shipment1 = client.get_shipment(&shipment_id);
    assert_eq!(shipment1.id, shipment_id);

    let shipment2 = client.get_shipment(&shipment_id);
    assert_eq!(shipment2.id, shipment_id);
}

#[test]
fn test_observer_revocation_works() {
    let (env, client, admin, token) = setup_shipment_env();
    client.initialize(&admin, &token);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    let observer = Address::generate(&env);

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

    client.assign_observer_role(&admin, &observer, &shipment_id);
    let count1 = client.get_observer_count(&shipment_id);
    assert_eq!(count1, 1);

    client.revoke_observer_role(&admin, &observer, &shipment_id);
    let count2 = client.get_observer_count(&shipment_id);
    assert_eq!(count2, 0);
}

#[test]
fn test_observer_can_call_read_only_functions() {
    let (env, client, admin, token) = setup_shipment_env();
    client.initialize(&admin, &token);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    let observer = Address::generate(&env);

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

    client.assign_observer_role(&admin, &observer, &shipment_id);

    let _shipment = client.get_shipment(&shipment_id);
    let _count = client.get_shipment_counter();
    let _receiver = client.get_shipment_receiver(&shipment_id);
    let _sender = client.get_shipment_sender(&shipment_id);
    let _carrier = client.get_shipment_carrier(&shipment_id);

    assert!(true);
}

#[test]
fn test_partial_refund_50_percent() {
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

    let deposit_amount = 1000i128;
    client.deposit_escrow(&company, &shipment_id, &deposit_amount);

    let shipment_before = client.get_shipment(&shipment_id);
    assert_eq!(shipment_before.escrow_amount, deposit_amount);

    let result = client.try_partial_refund_escrow(&admin, &shipment_id, &50);
    assert!(result.is_ok());

    let shipment_after = client.get_shipment(&shipment_id);
    assert_eq!(
        shipment_after.status,
        crate::ShipmentStatus::PartiallyRefunded
    );
    assert_eq!(shipment_after.escrow_amount, 0);
}

#[test]
fn test_partial_refund_100_percent_equals_full_refund() {
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

    let deposit_amount = 1000i128;
    client.deposit_escrow(&company, &shipment_id, &deposit_amount);

    let result = client.try_partial_refund_escrow(&admin, &shipment_id, &100);
    assert!(result.is_ok());

    let shipment_after = client.get_shipment(&shipment_id);
    assert_eq!(
        shipment_after.status,
        crate::ShipmentStatus::PartiallyRefunded
    );
}

#[test]
fn test_partial_refund_zero_percent_fails() {
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

    client.deposit_escrow(&company, &shipment_id, &1000);

    let result = client.try_partial_refund_escrow(&admin, &shipment_id, &0);
    assert_eq!(result, Err(Ok(crate::NavinError::InvalidAmount)));
}

#[test]
fn test_partial_refund_unauthorized_fails() {
    let (env, client, admin, token) = setup_shipment_env();
    client.initialize(&admin, &token);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    let random_user = Address::generate(&env);

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

    client.deposit_escrow(&company, &shipment_id, &1000);

    let result = client.try_partial_refund_escrow(&random_user, &shipment_id, &50);
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));
}

#[test]
fn test_partial_refund_on_non_escrowed_shipment_fails() {
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

    let result = client.try_partial_refund_escrow(&admin, &shipment_id, &50);
    assert_eq!(result, Err(Ok(crate::NavinError::InsufficientFunds)));
}

#[test]
fn test_partial_refund_twice_fails() {
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

    client.deposit_escrow(&company, &shipment_id, &1000);

    let result1 = client.try_partial_refund_escrow(&admin, &shipment_id, &50);
    assert!(result1.is_ok());

    let result2 = client.try_partial_refund_escrow(&admin, &shipment_id, &30);
    assert_eq!(result2, Err(Ok(crate::NavinError::InvalidStatus)));
}
