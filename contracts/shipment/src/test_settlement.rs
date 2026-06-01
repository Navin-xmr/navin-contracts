use crate::types::*;
use crate::{NavinShipment, NavinShipmentClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{contract, contractimpl, contracterror, Address, BytesN, Env};

// ── Mock token that always succeeds ──────────────────────────────────────────
#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {
        // no-op: always succeeds
    }
}

// ── Mock token that always fails ─────────────────────────────────────────────
mod failing_token {
    use super::*;

    #[contracterror]
    #[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
    #[repr(u32)]
    pub enum MockTokenFailure {
        TransferFailed = 1,
    }

    #[contract]
    pub struct FailingMockToken;

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

// ── Mock token stubs ──────────────────────────────────────────────────────────

mod ok_token {
    use soroban_sdk::{contract, contractimpl, Address, Env};
    #[contract]
    pub struct MockToken;
    #[contractimpl]
    impl MockToken {
        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    }
}

mod fail_token {
    use soroban_sdk::{contract, contracterror, contractimpl, Address, Env};
    #[contracterror]
    #[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
    #[repr(u32)]
    pub enum MockTokenError {
        TransferFailed = 1,
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
        ) -> Result<(), MockTokenError> {
            Err(MockTokenError::TransferFailed)
        }
    }
}

use fail_token::FailingMockToken;
use ok_token::MockToken;

// ── Setup helpers ─────────────────────────────────────────────────────────────

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

fn setup_shipment_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = crate::test_utils::setup_env();
    let token_contract = env.register(MockToken {}, ());
    let token_contract = env.register(mock_token::MockToken, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token_contract);

    (env, client, admin, token_contract)
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

fn setup_shipment_env_with_failing_token() -> (Env, NavinShipmentClient<'static>, Address, Address)
{
    let (env, admin) = crate::test_utils::setup_env();
    let token_contract = env.register(failing_token::FailingMockToken {}, ());
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

/// Test that deposit_escrow creates a settlement record in Pending state
/// and transitions to Completed on success.
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
    );

    let escrow_amount: i128 = 1000;

    // Deposit escrow - should create settlement record
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    // Verify settlement was created
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 1);

    // Get the settlement record
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

    // Verify no active settlement remains
    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

/// Test that deposit_escrow returns an error when token transfer fails.
/// Because Soroban reverts state on panic, no settlement record is persisted.
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
    );

    let escrow_amount: i128 = 1000;

    // Attempt to deposit escrow via the failing token - should return error
    let result = client.try_deposit_escrow(&company, &shipment_id, &escrow_amount);
    assert!(result.is_err());

    // On Soroban, the failed call is reverted atomically, so no settlement is persisted.
    // Verify settlement was NOT created (transaction rolled back)
    // Soroban reverts all state when a contract call panics/errors,
    // so no settlement record is persisted.
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 0);

    // Verify no active settlement
    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

/// Test that release_escrow creates a settlement record and transitions correctly.
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
    );

    let escrow_amount: i128 = 5000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    // Transition to Delivered
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    // Release escrow - should create settlement record
    client.release_escrow(&receiver, &shipment_id);

    // Verify two settlements: deposit + release
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 2);

    // Get the release settlement record
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

    // Verify no active settlement remains
    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

/// Test that refund_escrow creates a settlement record and transitions correctly.
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
    );

    let escrow_amount: i128 = 3000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    // Refund escrow - should create settlement record
    client.refund_escrow(&company, &shipment_id);

    // Verify two settlements: deposit + refund
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 2);

    // Get the refund settlement record
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

    // Verify no active settlement remains
    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

/// Test that refund_escrow returns an error when token transfer fails.
/// Because Soroban reverts state on panic, the settlement record is not persisted.
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
    );

    // Manually set escrow to bypass the failing transfer during deposit
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.escrow_amount = 3000;
        crate::storage::set_shipment(&env, &shipment);
        crate::storage::set_escrow(&env, shipment_id, 3000);
    });

    // Attempt to refund escrow - should fail because token.transfer panics
    let result = client.try_refund_escrow(&company, &shipment_id);
    assert!(result.is_err());

    // On Soroban, the failed call is reverted atomically, so no settlement is persisted.
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 0);

    // Verify no active settlement remains
    // Verify settlement was NOT created (transaction rolled back)
    // Soroban reverts all state when a contract call panics,
    // so no settlement record is persisted.
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 0);

    // Verify no active settlement
    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

/// Test settlement state transitions through full lifecycle.
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
    );

    // Step 1: Deposit escrow
    client.deposit_escrow(&company, &shipment_id, &10000);
    let settlement1 = client.get_settlement(&1);
    assert_eq!(settlement1.state, SettlementState::Completed);
    assert_eq!(settlement1.operation, SettlementOperation::Deposit);

    // Step 2: Transition to Delivered
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    // Step 3: Release escrow
    client.release_escrow(&receiver, &shipment_id);
    let settlement2 = client.get_settlement(&2);
    assert_eq!(settlement2.state, SettlementState::Completed);
    assert_eq!(settlement2.operation, SettlementOperation::Release);

    // Verify total settlements
    assert_eq!(client.get_settlement_count(), 2);

    // Verify no active settlements
    assert!(client.get_active_settlement(&shipment_id).is_none());
}

/// Test that settlement records are queryable and contain correct metadata.
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
    );

    let before_timestamp = env.ledger().timestamp();
    client.deposit_escrow(&company, &shipment_id, &5000);
    let after_timestamp = env.ledger().timestamp();

    let settlement = client.get_settlement(&1);

    // Verify all metadata fields
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

/// Test that multiple shipments can have independent settlement records.
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
    let data_hash2 = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 86400;

    // Use distinct data hashes so the idempotency check treats them as separate operations.
    let shipment_id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash1,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let shipment_id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash2,
        &seeded_hash(&env, 2),
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // Deposit escrow for both
    client.deposit_escrow(&company, &shipment_id1, &1000);
    client.deposit_escrow(&company, &shipment_id2, &2000);

    // Verify settlements are independent
    let settlement1 = client.get_settlement(&1);
    let settlement2 = client.get_settlement(&2);

    assert_eq!(settlement1.shipment_id, shipment_id1);
    assert_eq!(settlement1.amount, 1000);

    assert_eq!(settlement2.shipment_id, shipment_id2);
    assert_eq!(settlement2.amount, 2000);

    assert_eq!(client.get_settlement_count(), 2);
}
