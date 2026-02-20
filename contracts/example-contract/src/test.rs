#![cfg(test)]

extern crate std;

use crate::types::ShipmentInput;
use crate::{DeliveryStatus, SecureAssetVault, SecureAssetVaultClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

#[test]
fn test_create_shipments_batch_success() {
    let env = Env::default();
    let company = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    let mut shipments = Vec::new(&env);
    for i in 0..5 {
        shipments.push_back(ShipmentInput {
            receiver: Address::generate(&env),
            carrier: Address::generate(&env),
            data_hash: BytesN::from_array(&env, &[i as u8; 32]),
        });
    }

    let ids = contract_client.create_shipments_batch(&company, &shipments);
    assert_eq!(ids.len(), 5);
    for i in 0..5 {
        assert_eq!(ids.get(i).unwrap(), (i + 1) as u64);
    }
}

#[test]
fn test_create_shipments_batch_oversized() {
    let env = Env::default();
    let company = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    let mut shipments = Vec::new(&env);
    for i in 0..11 {
        shipments.push_back(ShipmentInput {
            receiver: Address::generate(&env),
            carrier: Address::generate(&env),
            data_hash: BytesN::from_array(&env, &[i as u8; 32]),
        });
    }

    let result = contract_client.try_create_shipments_batch(&company, &shipments);
    assert!(result.is_err());
}

#[test]
fn test_create_shipments_batch_invalid_input() {
    let env = Env::default();
    let company = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    let mut shipments = Vec::new(&env);
    shipments.push_back(ShipmentInput {
        receiver: Address::generate(&env),
        carrier: Address::generate(&env),
        data_hash: BytesN::from_array(&env, &[1; 32]),
    });
    let user = Address::generate(&env);
    shipments.push_back(ShipmentInput {
        receiver: user.clone(),
        carrier: user.clone(),
        data_hash: BytesN::from_array(&env, &[2; 32]),
    });

    let result = contract_client.try_create_shipments_batch(&company, &shipments);
    assert!(result.is_err());

    let mut valid_shipments = Vec::new(&env);
    valid_shipments.push_back(ShipmentInput {
        receiver: Address::generate(&env),
        carrier: Address::generate(&env),
        data_hash: BytesN::from_array(&env, &[3; 32]),
    });

    let ids = contract_client.create_shipments_batch(&company, &valid_shipments);
    assert_eq!(ids.get(0).unwrap(), 1u64);
}

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

fn setup_delivery_escrow(
    env: &Env,
    amount: i128,
    auto_release_after: u64,
) -> (
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
    assert!(contract_client.check_auto_release(&shipment_id));
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
    assert!(!contract_client.check_auto_release(&shipment_id));
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
    assert!(!contract_client.check_auto_release(&shipment_id));
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
    assert!(!contract_client.check_auto_release(&shipment_id));
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
    assert!(contract_client.check_auto_release(&shipment_id));
    assert!(!contract_client.check_auto_release(&shipment_id));
    assert_eq!(contract_client.get_balance(&carrier), 500);
}

#[test]
fn test_deposit_insurance() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    contract_client.initialize(&admin);

    let shipment_id = contract_client.create_shipment(&company, &receiver, &10000);
    contract_client.deposit_insurance(&company, &shipment_id, &2000);

    let shipment = contract_client.get_shipment(&shipment_id);
    assert_eq!(shipment.insurance_amount, 2000);
    assert_eq!(shipment.escrow_amount, 10000);
}

#[test]
fn test_claim_insurance_after_dispute() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    contract_client.initialize(&admin);

    let shipment_id = contract_client.create_shipment(&company, &receiver, &10000);
    contract_client.deposit_insurance(&company, &shipment_id, &2000);
    contract_client.mark_disputed(&admin, &shipment_id);
    contract_client.claim_insurance(&admin, &shipment_id, &receiver);

    let shipment = contract_client.get_shipment(&shipment_id);
    assert_eq!(shipment.insurance_amount, 2000);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_claim_insurance_twice_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    contract_client.initialize(&admin);

    let shipment_id = contract_client.create_shipment(&company, &receiver, &10000);
    contract_client.deposit_insurance(&company, &shipment_id, &2000);
    contract_client.mark_disputed(&admin, &shipment_id);
    contract_client.claim_insurance(&admin, &shipment_id, &receiver);
    contract_client.claim_insurance(&admin, &shipment_id, &receiver);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_unauthorized_claim_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let unauthorized = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    contract_client.initialize(&admin);

    let shipment_id = contract_client.create_shipment(&company, &receiver, &10000);
    contract_client.deposit_insurance(&company, &shipment_id, &2000);
    contract_client.mark_disputed(&admin, &shipment_id);
    contract_client.claim_insurance(&unauthorized, &shipment_id, &receiver);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_claim_insurance_without_dispute_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    contract_client.initialize(&admin);

    let shipment_id = contract_client.create_shipment(&company, &receiver, &10000);
    contract_client.deposit_insurance(&company, &shipment_id, &2000);
    contract_client.claim_insurance(&admin, &shipment_id, &receiver);
}

#[test]
fn test_update_status_valid_transition() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    contract_client.initialize(&admin);
    contract_client.add_carrier(&admin, &carrier);

    let shipment_id = contract_client.create_shipment(&company, &receiver, &10000);

    use crate::ShipmentStatus;
    contract_client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &String::from_str(&env, "hash123"),
    );

    let shipment = contract_client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::InTransit);
    assert_eq!(shipment.data_hash, String::from_str(&env, "hash123"));
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_update_status_invalid_transition() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    contract_client.initialize(&admin);
    contract_client.add_carrier(&admin, &carrier);

    let shipment_id = contract_client.create_shipment(&company, &receiver, &10000);

    use crate::ShipmentStatus;
    contract_client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &String::from_str(&env, "hash123"),
    );

    contract_client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::Created,
        &String::from_str(&env, "hash456"),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_update_status_unauthorized() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let unauthorized = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    contract_client.initialize(&admin);

    let shipment_id = contract_client.create_shipment(&company, &receiver, &10000);

    use crate::ShipmentStatus;
    contract_client.update_status(
        &unauthorized,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &String::from_str(&env, "hash123"),
    );
}

#[test]
fn test_update_status_admin_can_update() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    contract_client.initialize(&admin);

    let shipment_id = contract_client.create_shipment(&company, &receiver, &10000);

    use crate::ShipmentStatus;
    contract_client.update_status(
        &admin,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &String::from_str(&env, "hash_admin"),
    );

    let shipment = contract_client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::InTransit);
    assert_eq!(shipment.data_hash, String::from_str(&env, "hash_admin"));
}

#[test]
fn test_update_status_full_workflow() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    let contract_client = SecureAssetVaultClient::new(&env, &env.register(SecureAssetVault {}, ()));

    env.mock_all_auths();

    contract_client.initialize(&admin);
    contract_client.add_carrier(&admin, &carrier);

    let shipment_id = contract_client.create_shipment(&company, &receiver, &10000);

    use crate::ShipmentStatus;

    contract_client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &String::from_str(&env, "gps_data_1"),
    );

    let shipment = contract_client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::InTransit);

    contract_client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::Delivered,
        &String::from_str(&env, "final_location"),
    );

    let shipment = contract_client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Delivered);
    assert_eq!(shipment.data_hash, String::from_str(&env, "final_location"));
}
