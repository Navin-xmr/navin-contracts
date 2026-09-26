//! Tests for issue #7 — archived restore consistency regression tests.
//!
//! Verifies preflight_check_shipment_available distinguishes shipment states.

#[cfg(test)]
mod tests {
    extern crate std;
    use crate::{test_utils, NavinError, NavinShipment, NavinShipmentClient};
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Vec};

    #[contract]
    struct MockToken;

    #[contractimpl]
    impl MockToken {
        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
        pub fn decimals(_env: Env) -> u32 {
            7
        }
        pub fn transfer_from(
            _env: Env,
            _spender: Address,
            _from: Address,
            _to: Address,
            _amount: i128,
        ) {
        }
    }

    fn setup() -> (Env, NavinShipmentClient<'static>, Address) {
        let (env, admin) = test_utils::setup_env();
        let contract_id = env.register(NavinShipment, ());
        let client = NavinShipmentClient::new(&env, &contract_id);
        let token_id = env.register(MockToken, ());
        client.initialize(&admin, &token_id);
        (env, client, admin)
    }

    /// A shipment that was never created must return ShipmentNotFound.
    #[test]
    fn preflight_nonexistent_shipment_returns_shipment_not_found() {
        let (env, client, _admin) = setup();

        let result = env.as_contract(&client.address, || {
            crate::validation::preflight_check_shipment_available(&env, 9999u64)
        });

        assert_eq!(
            result,
            Err(NavinError::ShipmentNotFound),
            "non-existent shipment must return ShipmentNotFound"
        );
    }

    /// An active (non-finalized) shipment must be returned successfully by
    /// preflight_check_shipment_available.
    #[test]
    fn preflight_active_shipment_returns_ok() {
        let (env, client, admin) = setup();
        let company = Address::generate(&env);
        let receiver = Address::generate(&env);
        let carrier = Address::generate(&env);
        let data_hash = BytesN::from_array(&env, &[0x21u8; 32]);
        let deadline = test_utils::future_deadline(&env, 7200);

        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);
        crate::test_utils::allow_carrier(&client, &company, &carrier);

        let shipment_id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &Vec::new(&env),
            &deadline,
        );

        let result = env.as_contract(&client.address, || {
            crate::validation::preflight_check_shipment_available(&env, shipment_id)
        });

        assert!(
            result.is_ok(),
            "active shipment must be returned successfully by preflight_check_shipment_available"
        );
        let shipment = result.unwrap();
        assert_eq!(shipment.id, shipment_id);
        assert!(!shipment.finalized);
    }

    /// ShipmentUnavailable error code must be 42.
    /// This pins the discriminant so it cannot drift across refactors.
    #[test]
    fn shipment_unavailable_error_code_is_42() {
        assert_eq!(
            NavinError::ShipmentUnavailable as u32,
            42,
            "ShipmentUnavailable discriminant must be 42 per the error contract"
        );
    }
}
