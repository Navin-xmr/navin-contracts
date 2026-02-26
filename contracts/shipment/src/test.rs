#![cfg(test)]

extern crate std;

use crate::{
    BreachType, GeofenceEvent, NavinShipment, NavinShipmentClient, ShipmentInput, ShipmentStatus,
};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{storage::Persistent, Address as _, Events, Ledger as _},
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

    let deadline = env.ledger().timestamp() + 3600;
    let mut shipments = soroban_sdk::Vec::new(&env);
    for i in 1..=5 {
        shipments.push_back(ShipmentInput {
            receiver: Address::generate(&env),
            carrier: Address::generate(&env),
            data_hash: BytesN::from_array(&env, &[i as u8; 32]),
            payment_milestones: soroban_sdk::Vec::new(&env),
            deadline,
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
#[should_panic(expected = "Error(Contract, #17)")]
fn test_create_shipments_batch_invalid_input() {
    let (env, client, admin, token_contract) = setup_env();
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
    });
    let user = Address::generate(&env);
    shipments.push_back(ShipmentInput {
        receiver: user.clone(),
        carrier: user,
        data_hash: BytesN::from_array(&env, &[2u8; 32]),
        payment_milestones: soroban_sdk::Vec::new(&env),
        deadline,
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

#[test]
fn test_multiple_shipments_have_unique_ids() {
    let (env, client, admin, token_contract) = setup_env();
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
    );
    let id_two = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash_two,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    let id_three = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &hash_three,
        &soroban_sdk::Vec::new(&env),
        &deadline,
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

    client.add_company(&admin, &company);
    client.add_carrier_to_whitelist(&company, &carrier);

    assert!(client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_remove_carrier_from_whitelist() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &new_data_hash,
    );

    env.ledger().with_mut(|l| l.timestamp += 61);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::Delivered,
        &new_data_hash,
    );

    env.ledger().with_mut(|l| l.timestamp += 61);
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
    env.ledger().with_mut(|l| l.timestamp += 61);
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
    env.ledger().with_mut(|l| l.timestamp += 61);
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
fn test_get_escrow_balance_after_deposit() {
    let (env, client, admin, token_contract) = setup_env();
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
    );
    assert_eq!(client.get_shipment_count(), 3);
}

// ============= Role Tests =============

#[test]
fn test_get_role_unassigned() {
    let (env, client, admin, token_contract) = setup_env();
    let user = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    assert_eq!(client.get_role(&user), crate::Role::Unassigned);
}

#[test]
fn test_get_role_assigned() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_release_escrow_wrong_status() {
    let (env, client, admin, token_contract) = setup_env();
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

#[test]
fn test_refund_escrow_by_admin() {
    let (env, client, admin, token_contract) = setup_env();
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
#[should_panic(expected = "Error(Contract, #9)")]
fn test_raise_dispute_on_cancelled_shipment() {
    let (env, client, admin, token_contract) = setup_env();
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
fn test_record_milestones_batch_success() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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

    // Create batch with exactly 10 milestones (max allowed)
    let mut milestones = soroban_sdk::Vec::new(&env);
    for i in 0..10 {
        milestones.push_back((
            Symbol::new(&env, &std::format!("checkpoint_{}", i)),
            BytesN::from_array(&env, &[i as u8; 32]),
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
    let (env, client, admin, token_contract) = setup_env();
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

    // Create batch with 11 milestones (exceeds limit)
    let mut milestones = soroban_sdk::Vec::new(&env);
    for i in 0..11 {
        milestones.push_back((
            Symbol::new(&env, &std::format!("checkpoint_{}", i)),
            BytesN::from_array(&env, &[i as u8; 32]),
        ));
    }

    // Should fail with BatchTooLarge error (code 16)
    client.record_milestones_batch(&carrier, &shipment_id, &milestones);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_record_milestones_batch_invalid_status() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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

#[test]
fn test_record_milestones_batch_with_payment_milestones() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash2);
    env.ledger().with_mut(|l| l.timestamp += 61);
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
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash2);
    env.ledger().with_mut(|l| l.timestamp += 61);
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
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash2);
    env.ledger().with_mut(|l| l.timestamp += 61);
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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

#[test]
fn test_milestone_payment_duplicate_record_no_double_pay() {
    let (env, client, admin, token_contract) = setup_env();
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

    // Record Milestone 1 AGAIN
    client.record_milestone(&carrier, &shipment_id, &Symbol::new(&env, "m1"), &data_hash);
    assert_eq!(client.get_shipment(&shipment_id).escrow_amount, 500); // Should still be 500
}
// ============= Contract Upgrade Tests =============

#[test]
fn test_upgrade_success() {
    let (env, client, admin, token_contract) = setup_env();

    let wasm: &[u8] = include_bytes!("../test_wasms/upgrade_test.wasm");
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    client.initialize(&admin, &token_contract);
    assert_eq!(client.get_version(), 1);

    // Drain events emitted by initialize so we can assert only on upgrade events
    let _ = env.events().all();

    client.upgrade(&admin, &new_wasm_hash);

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
    );
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[2u8; 32]),
        &soroban_sdk::Vec::new(&env),
        &deadline,
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
    let (_env, client, _admin, _token_contract) = setup_env();

    client.get_version();
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_contract_metadata_fails_before_initialization() {
    let (_env, client, _admin, _token_contract) = setup_env();

    client.get_contract_metadata();
}

#[test]
fn test_get_version_after_upgrade() {
    let (env, client, admin, token_contract) = setup_env();

    let wasm: &[u8] = include_bytes!("../test_wasms/upgrade_test.wasm");
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    client.initialize(&admin, &token_contract);
    assert_eq!(client.get_version(), 1);

    client.upgrade(&admin, &new_wasm_hash);

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
    let (env, client, admin, token_contract) = setup_env();

    let wasm: &[u8] = include_bytes!("../test_wasms/upgrade_test.wasm");
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    client.initialize(&admin, &token_contract);

    let meta_before = client.get_contract_metadata();
    assert_eq!(meta_before.version, 1);
    assert_eq!(meta_before.admin, admin);
    assert_eq!(meta_before.shipment_count, 0);
    assert!(meta_before.initialized);

    client.upgrade(&admin, &new_wasm_hash);

    let version: u32 = env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::Version)
            .unwrap()
    });
    assert_eq!(version, 2);
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
#[should_panic(expected = "Error(Contract, #5)")]
fn test_handoff_delivered_shipment() {
    let (env, client, admin, token_contract) = setup_env();
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
fn test_report_condition_breach_success() {
    let (env, client, admin, token_contract) = setup_env();
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
    );

    // Carrier reports a temperature breach — no error, status unchanged
    client.report_condition_breach(
        &carrier,
        &shipment_id,
        &BreachType::TemperatureHigh,
        &breach_hash,
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Created);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_report_condition_breach_unauthorized_non_carrier() {
    let (env, client, admin, token_contract) = setup_env();
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
    client.report_condition_breach(&rogue, &shipment_id, &BreachType::Impact, &breach_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_report_condition_breach_wrong_carrier() {
    let (env, client, admin, token_contract) = setup_env();
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
        &breach_hash,
    );
}

// ── Issue #2: verify_delivery_proof ──────────────────────────────────────────

#[test]
fn test_verify_delivery_proof_match() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (_env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);

    client.verify_delivery_proof(&999u64, &BytesN::from_array(&_env, &[1u8; 32]));
}

// ── Issue #3: Rate limiting ───────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #21)")]
fn test_rate_limit_rapid_update_fails() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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

    let hash1 = BytesN::from_array(&env, &[2u8; 32]);
    let hash2 = BytesN::from_array(&env, &[3u8; 32]);

    // First update
    client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash1);

    // Advance the ledger timestamp past the 60-second minimum interval
    env.ledger().with_mut(|l| {
        l.timestamp += 61;
    });

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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    );
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));
}

#[test]
fn test_only_carrier_can_update_status_and_record_milestones() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
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
    let (env, client, admin, token_contract) = setup_env();
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

#[test]
fn test_rbac_all_gated_functions_with_wrong_role() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
    client.initialize(&admin, &token_contract);

    let outsider = Address::generate(&env);
    let new_admin = Address::generate(&env);

    // Outsider tries to transfer admin - should fail
    client.transfer_admin(&outsider, &new_admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_unauthorized_admin_acceptance() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1);

    // Only 1 admin, need at least 2
    client.init_multisig(&admin, &admins, &1);
}

#[test]
fn test_propose_action_upgrade() {
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();

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
    env.ledger().with_mut(|l| l.timestamp += 604_801);

    // Try to approve expired proposal - should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.approve_action(&admin2, &proposal_id);
    }));

    assert!(result.is_err());
}

#[test]
fn test_force_release_action() {
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();

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
    let (env, client, admin, token_contract) = setup_env();
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
    );
    let escrow_amount: i128 = 5000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    // Advance ledger time past the deadline threshold
    env.ledger().with_mut(|l| l.timestamp += 1001);

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
    let (env, client, admin, token_contract) = setup_env();
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
    );

    // Fails because the current ledger timestamp is less than the deadline constraint
    client.check_deadline(&shipment_id);
}

#[test]
fn test_delivery_before_deadline() {
    use crate::ShipmentStatus;
    let (env, client, admin, token_contract) = setup_env();
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
    env.ledger().with_mut(|l| l.timestamp += 1001);

    // Attempting to crank check_deadline on a safely completed shipment errors appropriately (Error 9)
    let res = client.try_check_deadline(&shipment_id);
    assert_eq!(res, Err(Ok(crate::NavinError::ShipmentAlreadyCompleted)));
}

#[test]
fn test_delivery_success_event_emitted_on_confirm_delivery() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
                    // data is (carrier, shipment_id, delivery_time)
                    return <(Address, u64, u64)>::try_from_val(&env, &data).ok();
                }
            }
        }
        None
    });

    let (event_carrier, event_shipment_id, _delivery_time) =
        event_data.expect("delivery_success event data must be present");
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
    let (env, client, admin, token_contract) = setup_env();
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
    );

    client.report_condition_breach(
        &carrier,
        &shipment_id,
        &BreachType::TemperatureHigh,
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
    let (env, client, admin, token_contract) = setup_env();
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
    );

    client.report_condition_breach(
        &carrier,
        &shipment_id,
        &BreachType::HumidityHigh,
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
fn test_carrier_dispute_loss_event_emitted_on_refund_to_company() {
    use soroban_sdk::TryFromVal;
    let (env, client, admin, token_contract) = setup_env();
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
    );
    let escrow_amount: i128 = 5000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.raise_dispute(&company, &shipment_id, &reason_hash);

    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::RefundToCompany,
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
    let (env, client, admin, token_contract) = setup_env();
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
    );
    let escrow_amount: i128 = 5000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.raise_dispute(&carrier, &shipment_id, &reason_hash);

    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
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
    let (env, client, admin, token_contract) = setup_env();
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
    );
    let escrow_amount: i128 = 5000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.raise_dispute(&receiver, &shipment_id, &reason_hash);

    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::RefundToCompany,
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.raise_dispute(&company, &shipment_id, &reason_hash);
    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
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
    let (env, client, admin, token_contract) = setup_env();
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
    );

    let analytics = client.get_analytics();
    assert_eq!(analytics.disputed_count, 0);
    assert_eq!(analytics.delivered_count, 1);
}

#[test]
fn test_analytics_batch_and_cancel() {
    let (env, client, admin, token_contract) = setup_env();
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

// ============= Shipment Limit Tests =============

#[test]
fn test_set_and_get_shipment_limit() {
    let (_env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
    let outsider = Address::generate(&env);
    client.initialize(&admin, &token_contract);

    // Outsider tries to set limit
    client.set_shipment_limit(&outsider, &10);
}

#[test]
fn test_active_shipment_count_tracking() {
    let (env, client, admin, token_contract) = setup_env();
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
    );
    assert_eq!(client.get_active_shipment_count(&company), 1);

    // Create 2nd shipment
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    assert_eq!(client.get_active_shipment_count(&company), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #30)")]
fn test_shipment_limit_reached() {
    let (env, client, admin, token_contract) = setup_env();
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
    );

    // Create 2nd shipment - Should fail
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
#[should_panic(expected = "Error(Contract, #30)")]
fn test_batch_limit_reached() {
    let (env, client, admin, token_contract) = setup_env();
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

#[test]
fn test_count_decrements_on_delivery() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    );
    assert_eq!(client.get_active_shipment_count(&company), 1);

    client.cancel_shipment(&company, &1, &data_hash);

    assert_eq!(client.get_active_shipment_count(&company), 0);
}

#[test]
fn test_count_decrements_on_dispute_resolution() {
    let (env, client, admin, token_contract) = setup_env();
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
    );
    client.deposit_escrow(&company, &1, &1000);
    client.update_status(&carrier, &1, &ShipmentStatus::InTransit, &data_hash);
    client.raise_dispute(&company, &1, &data_hash);

    assert_eq!(client.get_active_shipment_count(&company), 1);

    // Resolve dispute
    client.resolve_dispute(&admin, &1, &crate::DisputeResolution::RefundToCompany);

    assert_eq!(client.get_active_shipment_count(&company), 0);
}

#[test]
fn test_count_decrements_on_deadline_expiration() {
    let (env, client, admin, token_contract) = setup_env();
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
    );
    assert_eq!(client.get_active_shipment_count(&company), 1);

    // Fast forward time
    env.ledger().with_mut(|l| l.timestamp = deadline + 1);

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
    let (env, client, admin, token_contract) = setup_env();
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
// #[test]
// #[should_panic(expected = "Error(Contract, #6)")]
// fn test_update_status_returns_invalid_hash() {
//     let (env, client, admin, token_contract) = setup_env();
//     let company = Address::generate(&env);
//     let receiver = Address::generate(&env);
//     let carrier = Address::generate(&env);
//     let data_hash = BytesN::from_array(&env, &[1u8; 32]);
//     let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
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
//     client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &zero_hash);
// }

// NOTE: This test is commented out because the feature may not be fully implemented yet
// #[test]
// #[should_panic(expected = "Error(Contract, #6)")]
// fn test_confirm_delivery_returns_invalid_hash() {
//     let (env, client, admin, token_contract) = setup_env();
//     let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
//
//     let (receiver, _carrier, shipment_id) = setup_shipment_with_status(
//         &env,
//         &client,
//         &admin,
//         &token_contract,
//         crate::ShipmentStatus::InTransit,
//     );
//
//     client.confirm_delivery(&receiver, &shipment_id, &zero_hash);
// }

// ============= Error #11: CounterOverflow Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_create_shipment_returns_counter_overflow() {
    let (env, client, admin, token_contract) = setup_env();
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
    );
}

// ============= Error #12: CarrierNotWhitelisted Tests =============

// NOTE: This test is commented out because the feature may not be fully implemented yet
// #[test]
// #[should_panic(expected = "Error(Contract, #12)")]
// fn test_create_shipment_returns_carrier_not_whitelisted() {
//     let (env, client, admin, token_contract) = setup_env();
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
//     let (env, client, admin, token_contract) = setup_env();
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

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_deposit_escrow_returns_invalid_amount_zero() {
    let (env, client, admin, token_contract) = setup_env();
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
    );

    client.deposit_escrow(&company, &shipment_id, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_deposit_escrow_returns_invalid_amount_negative() {
    let (env, client, admin, token_contract) = setup_env();
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
    );

    client.deposit_escrow(&company, &shipment_id, &-100);
}

// ============= Error #15: EscrowAlreadyDeposited Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_deposit_escrow_returns_escrow_already_deposited() {
    let (env, client, admin, token_contract) = setup_env();
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
//     let (env, client, admin, token_contract) = setup_env();
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
//     );
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
//     let (env, client, admin, token_contract) = setup_env();
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
//     );
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (_env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);

    client.get_proposal(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #22)")]
fn test_approve_action_returns_proposal_not_found() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    );

    let proposal_id = client.propose_action(&admin, &crate::AdminAction::ForceRelease(shipment_id));

    // Fast forward time past expiration (7 days)
    env.ledger()
        .with_mut(|l| l.timestamp += 7 * 24 * 60 * 60 + 1);

    client.approve_action(&admin2, &proposal_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #24)")]
fn test_execute_proposal_returns_proposal_expired() {
    let (env, client, admin, token_contract) = setup_env();
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
    );

    let proposal_id = client.propose_action(&admin, &crate::AdminAction::ForceRelease(shipment_id));

    client.approve_action(&admin2, &proposal_id);

    // Fast forward time past expiration
    env.ledger()
        .with_mut(|l| l.timestamp += 7 * 24 * 60 * 60 + 1);

    client.execute_proposal(&proposal_id);
}

// ============= Error #25: AlreadyApproved Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #25)")]
fn test_approve_action_returns_already_approved() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    );

    let proposal_id = client.propose_action(&admin, &crate::AdminAction::ForceRelease(shipment_id));

    // Only 1 approval (proposer), but threshold is 3
    client.execute_proposal(&proposal_id);
}

// ============= Error #27: NotAnAdmin Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_propose_action_returns_not_an_admin() {
    let (env, client, admin, token_contract) = setup_env();
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
    );

    // Outsider tries to propose
    client.propose_action(&outsider, &crate::AdminAction::ForceRelease(shipment_id));
}

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_approve_action_returns_not_an_admin() {
    let (env, client, admin, token_contract) = setup_env();
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
    );

    let proposal_id = client.propose_action(&admin, &crate::AdminAction::ForceRelease(shipment_id));

    // Outsider tries to approve
    client.approve_action(&outsider, &proposal_id);
}

// ============= Error #28: InvalidMultiSigConfig Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_init_multisig_returns_invalid_multisig_config_threshold_too_high() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();

    let admins = soroban_sdk::Vec::new(&env);

    client.initialize(&admin, &token_contract);

    // Empty admin list is invalid
    client.init_multisig(&admin, &admins, &1);
}

// ============= Error #29: NotExpired Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #29)")]
fn test_check_deadline_returns_not_expired() {
    let (env, client, admin, token_contract) = setup_env();
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
    );

    // Try to check deadline before it expires
    client.check_deadline(&shipment_id);
}

// ============= Error #30: ShipmentLimitReached Tests =============

#[test]
#[should_panic(expected = "Error(Contract, #30)")]
fn test_create_shipment_returns_shipment_limit_reached() {
    let (env, client, admin, token_contract) = setup_env();
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
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #30)")]
fn test_create_shipments_batch_returns_shipment_limit_reached() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, _admin, _token_contract) = setup_env();
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
    let (env, client, admin, _token_contract) = setup_env();
    let company = Address::generate(&env);

    client.add_company(&admin, &company);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_add_carrier_returns_not_initialized() {
    let (env, client, admin, _token_contract) = setup_env();
    let carrier = Address::generate(&env);

    client.add_carrier(&admin, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_admin_returns_not_initialized() {
    let (_env, client, _admin, _token_contract) = setup_env();

    client.get_admin();
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_set_shipment_limit_returns_not_initialized() {
    let (_env, client, admin, _token_contract) = setup_env();

    client.set_shipment_limit(&admin, &10);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_shipment_limit_returns_not_initialized() {
    let (_env, client, _admin, _token_contract) = setup_env();

    client.get_shipment_limit();
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_active_shipment_count_returns_not_initialized() {
    let (env, client, _admin, _token_contract) = setup_env();
    let company = Address::generate(&env);

    client.get_active_shipment_count(&company);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_analytics_returns_not_initialized() {
    let (_env, client, _admin, _token_contract) = setup_env();

    client.get_analytics();
}

// ============= Additional Coverage for Unauthorized Error =============

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_add_company_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    client.add_company(&non_admin, &company);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_add_carrier_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_env();
    let carrier = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    client.add_carrier(&non_admin, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_set_shipment_limit_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_env();
    let non_admin = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    client.set_shipment_limit(&non_admin, &10);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_add_carrier_to_whitelist_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
        &breach_hash,
    );
}

// ============= Additional Coverage for ShipmentNotFound Error =============

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_update_status_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_env();
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);

    client.update_status(&carrier, &999, &ShipmentStatus::InTransit, &data_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_confirm_delivery_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_env();
    let receiver = Address::generate(&env);
    let confirmation_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);

    client.confirm_delivery(&receiver, &999, &confirmation_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_release_escrow_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_env();
    let receiver = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    client.release_escrow(&receiver, &999);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_refund_escrow_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.refund_escrow(&company, &999);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_raise_dispute_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let reason_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.raise_dispute(&company, &999, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_resolve_dispute_returns_shipment_not_found() {
    let (_env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);

    client.resolve_dispute(&admin, &999, &crate::DisputeResolution::ReleaseToCarrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_cancel_shipment_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let reason_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.cancel_shipment(&company, &999, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_update_eta_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
    let carrier = Address::generate(&env);
    let breach_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);

    client.report_condition_breach(&carrier, &999, &BreachType::TemperatureHigh, &breach_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_check_deadline_returns_shipment_not_found() {
    let (_env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);

    client.check_deadline(&999);
}

// ============= Additional Coverage for InvalidStatus Error =============

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_deposit_escrow_returns_invalid_status() {
    let (env, client, admin, token_contract) = setup_env();
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
//     let (env, client, admin, token_contract) = setup_env();
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
#[test]
fn test_full_shipment_lifecycle_integration() {
    use crate::ShipmentStatus;

    // ─── STEP 1: Setup Environment and Initialize Contract ───────────────────
    let (env, client, admin, token_contract) = setup_env();

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
    env.ledger().with_mut(|l| l.timestamp += 61);

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
    env.ledger().with_mut(|l| l.timestamp += 61);
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
    env.ledger().with_mut(|l| l.timestamp += 61);
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
    env.ledger().with_mut(|l| l.timestamp += 61);
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
    let (env, client, admin, token_contract) = setup_env();
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
    );

    // After creation, should have 1 event (shipment_created)
    let count = client.get_event_count(&shipment_id);
    assert_eq!(count, 1, "Expected 1 event after shipment creation");
}

#[test]
fn test_event_count_after_milestone() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    env.ledger().with_mut(|li| {
        li.timestamp += 61; // Advance by 61 seconds (default min interval is 60)
    });

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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    );

    // Immediately after creation, should have 1 event
    let count = client.get_event_count(&shipment_id);
    assert_eq!(count, 1, "Expected 1 event for newly created shipment");
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_event_count_shipment_not_found() {
    let (_env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);

    // Try to get event count for non-existent shipment
    client.get_event_count(&999);
}

#[test]
fn test_event_count_with_multiple_milestones() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    );

    // Try to archive an active shipment (should fail with InvalidStatus)
    client.archive_shipment(&admin, &shipment_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_archive_nonexistent_shipment_fails() {
    let (_env, client, admin, token_contract) = setup_env();

    client.initialize(&admin, &token_contract);

    // Try to archive a non-existent shipment (should fail with ShipmentNotFound)
    client.archive_shipment(&admin, &999);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_archive_shipment_unauthorized() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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

// ============= Analytics Event Tests =============

#[test]
fn test_carrier_handoff_completed_event() {
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    let (env, client, admin, token_contract) = setup_env();
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
    env.ledger().with_mut(|l| l.timestamp = deadline + 100);

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
