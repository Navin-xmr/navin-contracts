#![cfg(test)]

extern crate std;

use crate::{DeliveryStatus, SecureAssetVault, SecureAssetVaultClient};
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, BytesN, Env, String};

#[test]
fn test_initialization() {
    let env = Env::default();
    let admin = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    // Initialize the contract
    contract_client.initialize(&admin);

    // Verify initialization by checking balance is 0
    let test_user = Address::generate(&env);
    assert_eq!(contract_client.get_balance(&test_user), 0);
}

#[test]
fn test_deposit_and_withdraw() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    // Initialize contract
    contract_client.initialize(&admin);

    // Deposit funds
    contract_client.deposit(&user, &1000);
    assert_eq!(contract_client.get_balance(&user), 1000);

    // Withdraw funds
    contract_client.withdraw(&user, &user, &500);
    assert_eq!(contract_client.get_balance(&user), 500);
}

#[test]
fn test_transaction_logging() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    // Initialize contract
    contract_client.initialize(&admin);

    // Deposit funds
    contract_client.deposit(&user, &1000);

    // Withdraw some funds
    contract_client.withdraw(&user, &user, &500);

    // Lock some assets
    let current_time = env.ledger().timestamp();
    contract_client.lock_assets(
        &user,
        &300,
        &(current_time + 3600), // Lock for 1 hour
        &String::from_str(&env, "Temporary lock"),
    );

    // Verify balance after locking (locked assets don't reduce balance)
    assert_eq!(contract_client.get_balance(&user), 500);
}

#[test]
fn test_add_admin() {
    let env = Env::default();
    let initial_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    // Initialize contract with initial admin
    contract_client.initialize(&initial_admin);

    // Add new admin
    contract_client.add_admin(&initial_admin, &new_admin);

    // Attempt to add another admin using the new admin
    contract_client.add_admin(&new_admin, &Address::generate(&env));
}

#[test]
fn test_multiple_deposits() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    // Initialize contract
    contract_client.initialize(&admin);

    // Multiple deposits
    contract_client.deposit(&user, &1000);
    contract_client.deposit(&user, &500);

    // Verify total balance
    assert_eq!(contract_client.get_balance(&user), 1500);
}

#[test]
fn test_multiple_withdrawals() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    // Initialize contract
    contract_client.initialize(&admin);

    // Deposit funds
    contract_client.deposit(&user, &1000);

    // Multiple withdrawals
    contract_client.withdraw(&user, &user, &300);
    contract_client.withdraw(&user, &user, &200);

    // Verify remaining balance
    assert_eq!(contract_client.get_balance(&user), 500);
}

fn setup_delivery_escrow(env: &Env, amount: i128, auto_release_after: u64) -> (
    SecureAssetVaultClient<'_>,
    Address,
    Address,
    Address,
    BytesN<32>,
) {
    let admin = Address::generate(env);
    let sender = Address::generate(env);
    let carrier = Address::generate(env);
    let receiver = Address::generate(env);
    let shipment_id = BytesN::from_array(env, &[7; 32]);

    let contract_client = SecureAssetVaultClient::new(env, &env.register(SecureAssetVault {}, ()));
    env.mock_all_auths();

    contract_client.initialize(&admin);
    contract_client.deposit(&sender, &amount);
    contract_client.create_delivery(
        &shipment_id,
        &sender,
        &carrier,
        &receiver,
        &amount,
        &auto_release_after,
    );

    (contract_client, sender, carrier, receiver, shipment_id)
}

#[test]
fn test_check_auto_release_releases_after_timeout() {
    let env = Env::default();
    env.ledger().set_timestamp(100);

    let (contract_client, _sender, carrier, _receiver, shipment_id) =
        setup_delivery_escrow(&env, 500, 200);

    env.ledger().set_timestamp(201);
    assert_eq!(contract_client.check_auto_release(&shipment_id), true);
    assert_eq!(contract_client.get_balance(&carrier), 500);

    let delivery = contract_client.get_delivery(&shipment_id);
    assert_eq!(delivery.status, DeliveryStatus::AutoReleased);
}

#[test]
fn test_check_auto_release_early_no_release() {
    let env = Env::default();
    env.ledger().set_timestamp(100);

    let (contract_client, _sender, carrier, _receiver, shipment_id) =
        setup_delivery_escrow(&env, 500, 200);

    env.ledger().set_timestamp(199);
    assert_eq!(contract_client.check_auto_release(&shipment_id), false);
    assert_eq!(contract_client.get_balance(&carrier), 0);

    let delivery = contract_client.get_delivery(&shipment_id);
    assert_eq!(delivery.status, DeliveryStatus::Pending);
}

#[test]
fn test_check_auto_release_no_release_if_confirmed() {
    let env = Env::default();
    env.ledger().set_timestamp(100);

    let (contract_client, _sender, carrier, receiver, shipment_id) =
        setup_delivery_escrow(&env, 500, 200);

    contract_client.confirm_delivery(&shipment_id, &receiver);
    env.ledger().set_timestamp(300);
    assert_eq!(contract_client.check_auto_release(&shipment_id), false);
    assert_eq!(contract_client.get_balance(&carrier), 500);

    let delivery = contract_client.get_delivery(&shipment_id);
    assert_eq!(delivery.status, DeliveryStatus::Confirmed);
}

#[test]
fn test_check_auto_release_no_release_if_disputed() {
    let env = Env::default();
    env.ledger().set_timestamp(100);

    let (contract_client, _sender, carrier, receiver, shipment_id) =
        setup_delivery_escrow(&env, 500, 200);

    contract_client.dispute_delivery(&shipment_id, &receiver);
    env.ledger().set_timestamp(300);
    assert_eq!(contract_client.check_auto_release(&shipment_id), false);
    assert_eq!(contract_client.get_balance(&carrier), 0);

    let delivery = contract_client.get_delivery(&shipment_id);
    assert_eq!(delivery.status, DeliveryStatus::Disputed);
}

#[test]
fn test_check_auto_release_idempotent() {
    let env = Env::default();
    env.ledger().set_timestamp(100);

    let (contract_client, _sender, carrier, _receiver, shipment_id) =
        setup_delivery_escrow(&env, 500, 200);

    env.ledger().set_timestamp(201);
    assert_eq!(contract_client.check_auto_release(&shipment_id), true);
    assert_eq!(contract_client.check_auto_release(&shipment_id), false);
    assert_eq!(contract_client.get_balance(&carrier), 500);
}
