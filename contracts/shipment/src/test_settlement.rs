#![cfg(test)]

use crate::types::*;
use crate::{NavinShipment, NavinShipmentClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env};

// ── Mock token stubs ──────────────────────────────────────────────────────────

mod mock_token {
    use soroban_sdk::{contract, contractimpl, Address, Env};
    #[contract]
    pub struct MockToken;
    #[contractimpl]
    impl MockToken {
        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
        pub fn decimals(_env: Env) -> u32 {
            7
        }
    }
}

mod failing_mock_token {
    use soroban_sdk::{contract, contractimpl, Address, Env};
    #[contract]
    pub struct FailingMockToken;
    #[contractimpl]
    impl FailingMockToken {
        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {
            panic!("transfer failed");
        }
        pub fn decimals(_env: Env) -> u32 {
            7
        }
    }
}

// ── Setup helpers ─────────────────────────────────────────────────────────────

fn setup_shipment_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = crate::test_utils::setup_env();
    let token_contract = env.register(mock_token::MockToken, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token_contract);
    (env, client, admin, token_contract)
}

fn setup_shipment_env_with_failing_token() -> (Env, NavinShipmentClient<'static>, Address, Address)
{
    let (env, admin) = crate::test_utils::setup_env();
    let token_contract = env.register(failing_mock_token::FailingMockToken, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token_contract);
    (env, client, admin, token_contract)
}

fn dummy_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[1u8; 32])
}

fn seeded_hash(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [1u8; 32];
    bytes[31] = seed;
    BytesN::from_array(env, &bytes)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn test_deposit_escrow_settlement_success() {
    let (env, client, admin, _token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

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

    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 1);

    let settlement = client.get_settlement(&1);
    assert_eq!(settlement.settlement_id, 1);
    assert_eq!(settlement.shipment_id, shipment_id);
    assert_eq!(settlement.operation, SettlementOperation::Deposit);
    assert_eq!(settlement.state, SettlementState::Completed);
    assert_eq!(settlement.amount, escrow_amount);
    assert_eq!(settlement.from, company);
    assert_eq!(settlement.to, client.address.clone());
    assert!(settlement.completed_at.is_some());
    assert!(settlement.error_code.is_none());

    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

#[test]
fn test_deposit_escrow_settlement_failure() {
    let (env, client, admin, _token_contract) = setup_shipment_env_with_failing_token();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let result = client.try_deposit_escrow(&company, &shipment_id, &1000);
    assert!(result.is_err());

    // Soroban reverts all state on panic — no settlement is persisted.
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 0);

    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

#[test]
fn test_release_escrow_settlement_success() {
    let (env, client, admin, _token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

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
        shipment.status = ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    client.release_escrow(&receiver, &shipment_id);

    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 2);

    let settlement = client.get_settlement(&2);
    assert_eq!(settlement.settlement_id, 2);
    assert_eq!(settlement.shipment_id, shipment_id);
    assert_eq!(settlement.operation, SettlementOperation::Release);
    assert_eq!(settlement.state, SettlementState::Completed);
    assert_eq!(settlement.amount, escrow_amount);
    assert_eq!(settlement.from, client.address.clone());
    assert_eq!(settlement.to, carrier);
    assert!(settlement.completed_at.is_some());
    assert!(settlement.error_code.is_none());

    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

#[test]
fn test_refund_escrow_settlement_success() {
    let (env, client, admin, _token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

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

    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 2);

    let settlement = client.get_settlement(&2);
    assert_eq!(settlement.settlement_id, 2);
    assert_eq!(settlement.shipment_id, shipment_id);
    assert_eq!(settlement.operation, SettlementOperation::Refund);
    assert_eq!(settlement.state, SettlementState::Completed);
    assert_eq!(settlement.amount, escrow_amount);
    assert_eq!(settlement.from, client.address.clone());
    assert_eq!(settlement.to, company);
    assert!(settlement.completed_at.is_some());
    assert!(settlement.error_code.is_none());

    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

#[test]
fn test_refund_escrow_settlement_failure() {
    let (env, client, admin, _token_contract) = setup_shipment_env_with_failing_token();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    // Manually set escrow to bypass the failing transfer during deposit.
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.escrow_amount = 3000;
        crate::storage::set_shipment(&env, &shipment);
        crate::storage::set_escrow(&env, shipment_id, 3000);
    });

    let result = client.try_refund_escrow(&company, &shipment_id);
    assert!(result.is_err());

    // Soroban reverts all state on panic — no settlement is persisted.
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 0);

    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

#[test]
fn test_settlement_full_lifecycle() {
    let (env, client, admin, _token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    client.deposit_escrow(&company, &shipment_id, &10000);
    let settlement1 = client.get_settlement(&1);
    assert_eq!(settlement1.state, SettlementState::Completed);
    assert_eq!(settlement1.operation, SettlementOperation::Deposit);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    client.release_escrow(&receiver, &shipment_id);
    let settlement2 = client.get_settlement(&2);
    assert_eq!(settlement2.state, SettlementState::Completed);
    assert_eq!(settlement2.operation, SettlementOperation::Release);

    assert_eq!(client.get_settlement_count(), 2);
    assert!(client.get_active_settlement(&shipment_id).is_none());
}

#[test]
fn test_settlement_record_metadata() {
    let (env, client, admin, _token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let before_timestamp = env.ledger().timestamp();
    client.deposit_escrow(&company, &shipment_id, &5000);
    let after_timestamp = env.ledger().timestamp();

    let settlement = client.get_settlement(&1);

    assert_eq!(settlement.settlement_id, 1);
    assert_eq!(settlement.shipment_id, shipment_id);
    assert_eq!(settlement.amount, 5000);
    assert_eq!(settlement.from, company);
    assert_eq!(settlement.to, client.address.clone());
    assert!(settlement.initiated_at >= before_timestamp);
    assert!(settlement.initiated_at <= after_timestamp);
    assert!(settlement.completed_at.is_some());
    assert!(settlement.completed_at.unwrap() >= settlement.initiated_at);
}

#[test]
fn test_multiple_shipments_independent_settlements() {
    let (env, client, admin, _token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash1 = BytesN::from_array(&env, &[1u8; 32]);
    let data_hash2 = seeded_hash(&env, 2);
    let deadline = env.ledger().timestamp() + 86400;

    let shipment_id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash1,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    let shipment_id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash2,
        &soroban_sdk::Vec::new(&env),
        &deadline,
        &None,
    );

    client.deposit_escrow(&company, &shipment_id1, &1000);
    client.deposit_escrow(&company, &shipment_id2, &2000);

    let settlement1 = client.get_settlement(&1);
    let settlement2 = client.get_settlement(&2);

    assert_eq!(settlement1.shipment_id, shipment_id1);
    assert_eq!(settlement1.amount, 1000);

    assert_eq!(settlement2.shipment_id, shipment_id2);
    assert_eq!(settlement2.amount, 2000);

    assert_eq!(client.get_settlement_count(), 2);
}
