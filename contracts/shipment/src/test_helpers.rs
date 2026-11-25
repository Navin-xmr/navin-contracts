//! Shared test setup and mock tokens (extracted from test.rs, issue #1103).
//!
//! Re-exported by the themed test modules so they can call
//! `setup_shipment_env`, `setup_initialized_shipment_env`, etc.

extern crate std;

pub use crate::{
    BreachType, GeofenceEvent, NavinError, NavinShipment, NavinShipmentClient, Severity,
    ShipmentInput, ShipmentStatus,
};
pub use soroban_sdk::{
    contract, contracterror, contractimpl,
    testutils::{Address as _, Events},
    Address, BytesN, Env, FromVal, IntoVal, Symbol, TryFromVal, Vec,
};

#[contract]
pub struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {
        // Mock implementation - always succeeds
    }
    pub fn decimals(_env: Env) -> u32 {
        crate::types::EXPECTED_TOKEN_DECIMALS
    }
}

pub mod failing_token {
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

    // Token reporting non-standard decimals; exercises the decimals guard.
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

/// Create a shipment and patch its status directly in contract storage to
/// simulate a mid-lifecycle state. Returns (receiver, carrier, shipment_id).
pub fn setup_shipment_with_status(
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

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(env, shipment_id).unwrap();
        shipment.status = status;
        crate::storage::set_shipment(env, &shipment);
    });

    (receiver, carrier, shipment_id)
}
