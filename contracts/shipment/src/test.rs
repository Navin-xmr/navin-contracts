#![cfg(test)]

extern crate std;

use crate::{GeofenceEvent, NavinShipment, NavinShipmentClient, ShipmentInput, ShipmentStatus};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{storage::Persistent, Address as _, Events},
    Address, BytesN, Env, Symbol, TryFromVal,
};

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {
        // Mock implementation - always succeeds
    }
}

fn setup_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    env.mock_all_auths();

    (env, client, admin, token_contract)
}

#[test]
fn test_successful_initialization() {
    let (_env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_shipment_counter(), 0);
    assert_eq!(client.get_version(), 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_re_initialization_fails() {
    let (_env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);
    // Second call must fail with AlreadyInitialized (error code 1)
    client.initialize(&admin, &token_contract);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_re_initialization_with_different_admin_fails() {
    let (env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);

    let other_admin = Address::generate(&env);
    // Attempting to re-initialize with a different admin must also fail
    client.initialize(&other_admin, &token_contract);
}

#[test]
fn test_shipment_counter_starts_at_zero() {
    let (_env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);

    assert_eq!(client.get_shipment_counter(), 0);
}

#[test]
fn test_admin_is_stored_correctly() {
    let (env, client, _admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
fn test_create_shipments_batch_success() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let mut shipments = soroban_sdk::Vec::new(&env);
    for i in 0..5 {
        shipments.push_back(ShipmentInput {
            receiver: Address::generate(&env),
            carrier: Address::generate(&env),
            data_hash: BytesN::from_array(&env, &[i as u8; 32]),
            payment_milestones: soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let mut shipments = soroban_sdk::Vec::new(&env);
    for i in 0..11 {
        shipments.push_back(ShipmentInput {
            receiver: Address::generate(&env),
            carrier: Address::generate(&env),
            data_hash: BytesN::from_array(&env, &[i as u8; 32]),
            payment_milestones: soroban_sdk::Vec::new(&env),
        });
    }

    client.create_shipments_batch(&company, &shipments);
}

#[test]
#[should_panic(expected = "Error(Contract, #17)")]
fn test_create_shipments_batch_invalid_input() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let mut shipments = soroban_sdk::Vec::new(&env);
    shipments.push_back(ShipmentInput {
        receiver: Address::generate(&env),
        carrier: Address::generate(&env),
        data_hash: BytesN::from_array(&env, &[1u8; 32]),
        payment_milestones: soroban_sdk::Vec::new(&env),
    });
    let user = Address::generate(&env);
    shipments.push_back(ShipmentInput {
        receiver: user.clone(),
        carrier: user,
        data_hash: BytesN::from_array(&env, &[2u8; 32]),
        payment_milestones: soroban_sdk::Vec::new(&env),
    });

    client.create_shipments_batch(&company, &shipments);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_create_shipment_unauthorized() {
    let (env, client, admin, token_contract) = setup_env();
    let outsider = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[9u8; 32]);

    client.initialize(&admin, &token_contract);
    client.create_shipment(
        &outsider,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
}

#[test]
fn test_multiple_shipments_have_unique_ids() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let hash_one = BytesN::from_array(&env, &[1u8; 32]);
    let hash_two = BytesN::from_array(&env, &[2u8; 32]);
    let hash_three = BytesN::from_array(&env, &[3u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id_one = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash_one,
        &soroban_sdk::Vec::new(&env),
    );
    let id_two = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash_two,
        &soroban_sdk::Vec::new(&env),
    );
    let id_three = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash_three,
        &soroban_sdk::Vec::new(&env),
    );

    assert_eq!(id_one, 1);
    assert_eq!(id_two, 2);
    assert_eq!(id_three, 3);
    assert_eq!(client.get_shipment_counter(), 3);
}

// ============= Carrier Whitelist Tests =============

#[test]
fn test_add_carrier_to_whitelist() {
    let (env, client, admin, token_contract) = setup_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_carrier_to_whitelist(&company, &carrier);

    assert!(client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_remove_carrier_from_whitelist() {
    let (env, client, admin, token_contract) = setup_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_carrier_to_whitelist(&company, &carrier);
    assert!(client.is_carrier_whitelisted(&company, &carrier));

    client.remove_carrier_from_whitelist(&company, &carrier);

    assert!(!client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_is_carrier_whitelisted_returns_false_for_non_whitelisted() {
    let (env, client, admin, token_contract) = setup_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    assert!(!client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_multiple_carriers_whitelist() {
    let (env, client, admin, token_contract) = setup_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier1 = Address::generate(&env);
    let carrier2 = Address::generate(&env);
    let carrier3 = Address::generate(&env);

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
    let (env, client, admin, token_contract) = setup_env();
    client.initialize(&admin, &token_contract);

    let company1 = Address::generate(&env);
    let company2 = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_carrier_to_whitelist(&company1, &carrier);

    assert!(client.is_carrier_whitelisted(&company1, &carrier));
    assert!(!client.is_carrier_whitelisted(&company2, &carrier));

    client.add_carrier_to_whitelist(&company2, &carrier);

    assert!(client.is_carrier_whitelisted(&company1, &carrier));
    assert!(client.is_carrier_whitelisted(&company2, &carrier));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_whitelist_functions_fail_before_initialization() {
    let (env, client, _admin, _token_contract) = setup_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.is_carrier_whitelisted(&company, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_add_whitelist_fails_before_initialization() {
    let (env, client, _admin, _token_contract) = setup_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_carrier_to_whitelist(&company, &carrier);
}

// ============= Deposit Escrow Tests =============

#[test]
fn test_deposit_escrow_success() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let new_data_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let new_data_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
#[should_panic(expected = "Error(Contract, #5)")]
fn test_update_status_invalid_transition() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let new_data_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &new_data_hash,
    );

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::Delivered,
        &new_data_hash,
    );

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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let unauthorized_user = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let new_data_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let hash_2 = BytesN::from_array(&env, &[2u8; 32]);
    let hash_3 = BytesN::from_array(&env, &[3u8; 32]);
    let hash_4 = BytesN::from_array(&env, &[4u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let carrier = Address::generate(&env);
    let new_data_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin, &token_contract);

    // Try to update a non-existent shipment
    client.update_status(&carrier, &999, &ShipmentStatus::InTransit, &new_data_hash);
}

// ============= Get Escrow Balance Tests =============

#[test]
fn test_get_escrow_balance_returns_zero_without_deposit() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    let escrow_amount: i128 = 1000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, escrow_amount);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_deposit_escrow_unauthorized() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let non_company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[11u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    let escrow_amount: i128 = 1000;
    client.deposit_escrow(&non_company, &shipment_id, &escrow_amount);
    // No escrow deposited yet, should return 0
    assert_eq!(client.get_escrow_balance(&shipment_id), 0);
}

#[test]
fn test_get_escrow_balance_after_deposit() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    env.as_contract(&client.address, || {
        crate::storage::set_escrow_balance(&env, shipment_id, 500_000);
    });

    assert_eq!(client.get_escrow_balance(&shipment_id), 500_000);
}

#[test]
fn test_get_escrow_balance_after_release() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[3u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (_env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);

    client.get_escrow_balance(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_escrow_balance_fails_before_initialization() {
    let (_env, _client, _admin, _token_contract) = setup_env();

    _client.get_escrow_balance(&1);
}

// ============= Get Shipment Count Tests =============

#[test]
fn test_get_shipment_count_returns_zero_on_fresh_contract() {
    let (_env, client, _admin, _token_contract) = setup_env();

    assert_eq!(client.get_shipment_count(), 0);
}

#[test]
fn test_get_shipment_count_returns_zero_after_initialization() {
    let (_env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);

    assert_eq!(client.get_shipment_count(), 0);
}

#[test]
fn test_get_shipment_count_after_creating_shipments() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let hash_one = BytesN::from_array(&env, &[1u8; 32]);
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash_one,
        &soroban_sdk::Vec::new(&env),
    );
    assert_eq!(client.get_shipment_count(), 1);

    let hash_two = BytesN::from_array(&env, &[2u8; 32]);
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash_two,
        &soroban_sdk::Vec::new(&env),
    );
    assert_eq!(client.get_shipment_count(), 2);

    let hash_three = BytesN::from_array(&env, &[3u8; 32]);
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash_three,
        &soroban_sdk::Vec::new(&env),
    );
    assert_eq!(client.get_shipment_count(), 3);
}

// ============= Get Shipment Tests =============

#[test]
fn test_get_shipment_returns_correct_data() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

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
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_shipment_not_found() {
    let (_env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);

    client.get_shipment(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_shipment_fails_before_initialization() {
    let (_env, client, _admin, _token_contract) = setup_env();

    client.get_shipment(&1);
}

// ============= Geofence Event Tests =============

#[test]
fn test_report_geofence_zone_entry() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let event_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    client.report_geofence_event(
        &carrier,
        &shipment_id,
        &GeofenceEvent::ZoneEntry,
        &event_hash,
    );

    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
fn test_report_geofence_zone_exit() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let event_hash = BytesN::from_array(&env, &[3u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    client.report_geofence_event(
        &carrier,
        &shipment_id,
        &GeofenceEvent::ZoneExit,
        &event_hash,
    );

    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
fn test_report_geofence_route_deviation() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let event_hash = BytesN::from_array(&env, &[4u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    client.report_geofence_event(
        &carrier,
        &shipment_id,
        &GeofenceEvent::RouteDeviation,
        &event_hash,
    );

    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_report_geofence_event_unauthorized_role() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let event_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    // Note: outsider NOT added as carrier

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
    let carrier = Address::generate(&env);
    let event_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);

    client.report_geofence_event(&carrier, &999, &GeofenceEvent::ZoneEntry, &event_hash);
}

// ============= ETA Update Tests =============

#[test]
fn test_update_eta_valid_emits_event() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let shipment_hash = BytesN::from_array(&env, &[1u8; 32]);
    let eta_hash = BytesN::from_array(&env, &[9u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &shipment_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let shipment_hash = BytesN::from_array(&env, &[1u8; 32]);
    let eta_hash = BytesN::from_array(&env, &[8u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &shipment_hash,
        &soroban_sdk::Vec::new(&env),
    );
    let past_eta = env.ledger().timestamp();

    client.update_eta(&carrier, &shipment_id, &past_eta, &eta_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_update_eta_unauthorized() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let shipment_hash = BytesN::from_array(&env, &[1u8; 32]);
    let eta_hash = BytesN::from_array(&env, &[7u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &shipment_hash,
        &soroban_sdk::Vec::new(&env),
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

    client.initialize(admin, token_contract);
    client.add_company(admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(env),
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
fn test_release_escrow_success() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
#[should_panic(expected = "Error(Contract, #8)")]
fn test_release_escrow_double_release() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.release_escrow(&receiver, &shipment_id);
}

#[test]
fn test_release_escrow_by_admin() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    let escrow_amount: i128 = 3000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.refund_escrow(&unauthorized, &shipment_id);
}

#[test]
fn test_refund_escrow_by_admin() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    let escrow_amount: i128 = 3000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.refund_escrow(&admin, &shipment_id);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 0);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_refund_escrow_double_refund() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    let escrow_amount: i128 = 3000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.refund_escrow(&company, &shipment_id);
    client.refund_escrow(&company, &shipment_id);
}

// ============= Dispute Tests =============

#[test]
fn test_raise_dispute_by_sender() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[99u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[98u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    client.raise_dispute(&receiver, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Disputed);
}

#[test]
fn test_raise_dispute_by_carrier() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[97u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    client.raise_dispute(&carrier, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Disputed);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_raise_dispute_unauthorized() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[96u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    client.raise_dispute(&outsider, &shipment_id, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_raise_dispute_on_cancelled_shipment() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[95u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[94u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.raise_dispute(&company, &shipment_id, &reason_hash);

    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 0);
    assert_eq!(shipment.status, crate::ShipmentStatus::Delivered);
}

#[test]
fn test_resolve_dispute_refund_to_company() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[93u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.raise_dispute(&receiver, &shipment_id, &reason_hash);

    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::RefundToCompany,
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 0);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_resolve_dispute_unauthorized() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[92u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.raise_dispute(&company, &shipment_id, &reason_hash);

    client.resolve_dispute(
        &outsider,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_resolve_dispute_not_disputed() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
    );
}

// ============= Milestone Event Tests =============

#[test]
fn test_record_milestone_success() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let checkpoint = soroban_sdk::Symbol::new(&env, "port_arrival");

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
#[should_panic(expected = "Error(Contract, #8)")]
fn test_deposit_escrow_invalid_amount() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    let invalid_escrow_amount: i128 = 0;

    // Should panic with error code 8 for invalid amount
    client.deposit_escrow(&company, &shipment_id, &invalid_escrow_amount);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_record_milestone_wrong_status() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let checkpoint = soroban_sdk::Symbol::new(&env, "port_arrival");

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    // Status is Created by default, which is wrong status for milestone
    client.record_milestone(&carrier, &shipment_id, &checkpoint, &data_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_record_milestone_unauthorized() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[12u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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

// ============= TTL Extension Tests =============

#[test]
fn test_ttl_extension_on_shipment_creation() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[99u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[2u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[88u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
}

#[test]
fn test_cancel_shipment_by_admin() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[3u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[66u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    client.cancel_shipment(&admin, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_cancel_shipment_delivered_should_fail() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let hash2 = BytesN::from_array(&env, &[2u8; 32]);
    let hash3 = BytesN::from_array(&env, &[3u8; 32]);
    let confirmation_hash = BytesN::from_array(&env, &[99u8; 32]);
    let escrow_amount: i128 = 10_000;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash2);
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[4u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[44u8; 32]);
    let escrow_amount: i128 = 5_000;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[5u8; 32]);
    let hash2 = BytesN::from_array(&env, &[6u8; 32]);
    let hash3 = BytesN::from_array(&env, &[7u8; 32]);
    let escrow_amount: i128 = 3_000;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash2);
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::Disputed, &hash3);
    client.update_status(&admin, &shipment_id, &ShipmentStatus::Delivered, &hash3);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Delivered);
}

#[test]
fn test_escrow_dispute_resolve_to_cancelled() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[8u8; 32]);
    let hash2 = BytesN::from_array(&env, &[9u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[77u8; 32]);
    let escrow_amount: i128 = 2_000;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash2);
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[10u8; 32]);
    let escrow_amount: i128 = 1_000;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_escrow_release_without_delivery_confirm_from_created_fails() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[11u8; 32]);
    let confirmation_hash = BytesN::from_array(&env, &[66u8; 32]);
    let escrow_amount: i128 = 1_500;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_escrow_refund_after_delivery_fails() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[12u8; 32]);
    let hash2 = BytesN::from_array(&env, &[13u8; 32]);
    let confirmation_hash = BytesN::from_array(&env, &[55u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[33u8; 32]);
    let escrow_amount: i128 = 2_500;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[14u8; 32]);
    let hash2 = BytesN::from_array(&env, &[15u8; 32]);
    let escrow_amount: i128 = 1_000;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash2);

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
}

#[test]
fn test_milestone_payment_success() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let escrow_amount: i128 = 1000;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "warehouse"), 30));
    milestones.push_back((Symbol::new(&env, "port"), 30));
    milestones.push_back((Symbol::new(&env, "last_mile"), 40));

    let shipment_id =
        client.create_shipment(&company, &receiver, &carrier, &data_hash, &milestones);
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let escrow_amount: i128 = 1000;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "checkpoint1"), 25));
    milestones.push_back((Symbol::new(&env, "checkpoint2"), 75));

    let shipment_id =
        client.create_shipment(&company, &receiver, &carrier, &data_hash, &milestones);
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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "m1"), 50));
    milestones.push_back((Symbol::new(&env, "m2"), 60)); // Total 110%

    client.create_shipment(&company, &receiver, &carrier, &data_hash, &milestones);
}

#[test]
fn test_milestone_payment_duplicate_record_no_double_pay() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let escrow_amount: i128 = 1000;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "m1"), 50));
    milestones.push_back((Symbol::new(&env, "m2"), 50));

    let shipment_id =
        client.create_shipment(&company, &receiver, &carrier, &data_hash, &milestones);
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

    // Record Milestone 1 AGAIN
    client.record_milestone(&carrier, &shipment_id, &Symbol::new(&env, "m1"), &data_hash);
    assert_eq!(client.get_shipment(&shipment_id).escrow_amount, 500); // Should still be 500
}
// ============= Contract Upgrade Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_upgrade_unauthorized() {
    let (env, client, admin, token_contract) = setup_env();
    let non_admin = Address::generate(&env);
    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);

    client.initialize(&admin, &token_contract);

    client.upgrade(&non_admin, &new_wasm_hash);
}

// ============= Contract Metadata Tests =============

#[test]
fn test_get_contract_metadata_after_init() {
    let (_env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);

    let meta = client.get_contract_metadata();
    assert_eq!(meta.version, 1);
    assert_eq!(meta.admin, admin);
    assert_eq!(meta.shipment_count, 0);
    assert!(meta.initialized);
}

#[test]
fn test_get_contract_metadata_after_creating_shipments() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &soroban_sdk::Vec::new(&env),
    );
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[2u8; 32]),
        &soroban_sdk::Vec::new(&env),
    );

    let meta = client.get_contract_metadata();
    assert_eq!(meta.version, 1);
    assert_eq!(meta.admin, admin);
    assert_eq!(meta.shipment_count, 2);
    assert!(meta.initialized);
}

// ============= Carrier Handoff Tests =============

#[test]
fn test_successful_handoff() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);

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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let unauthorized_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);

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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let wrong_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);

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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let invalid_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);

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
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);

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
#[should_panic(expected = "Error(Contract, #9)")]
fn test_handoff_cancelled_shipment() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);

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
    );

    // Cancel the shipment
    client.cancel_shipment(&company, &shipment_id, &data_hash);

    // Try to handoff a cancelled shipment
    client.handoff_shipment(&current_carrier, &new_carrier, &shipment_id, &handoff_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_handoff_nonexistent_shipment() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, _admin, _token_contract) = setup_env();
    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    // Contract not initialized — should panic with NotInitialized (#2)
    client.create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
}
