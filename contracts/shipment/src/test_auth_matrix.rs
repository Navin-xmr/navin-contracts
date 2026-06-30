//! Tests for issue #508 — operators are blocked from escrow release and refund.
//!
//! Verifies that an address registered only as an Operator cannot call
//! `release_escrow` or `refund_escrow`, and that both calls fail with
//! `NavinError::Unauthorized`.

#[cfg(test)]
mod tests {
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
    }

    fn setup() -> (Env, NavinShipmentClient<'static>, Address) {
        let (env, admin) = test_utils::setup_env();
        let token = env.register(MockToken, ());
        let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
        client.initialize(&admin, &token);
        (env, client, admin)
    }

    fn create_shipment(
        env: &Env,
        client: &NavinShipmentClient,
        admin: &Address,
    ) -> (u64, Address, Address, Address) {
        let company = Address::generate(env);
        let receiver = Address::generate(env);
        let carrier = Address::generate(env);
        client.add_company(admin, &company);
        client.add_carrier(admin, &carrier);
        client.add_carrier_to_whitelist(&company, &carrier);
        let deadline = test_utils::future_deadline(env, 7_200);
        let id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &BytesN::from_array(env, &[1u8; 32]),
            &Vec::new(env),
            &deadline,
        );
        (id, company, receiver, carrier)
    }

    #[test]
    fn operator_cannot_release_escrow() {
        let (env, client, admin) = setup();
        let (shipment_id, _company, _receiver, _carrier) = create_shipment(&env, &client, &admin);

        let operator = Address::generate(&env);
        client.add_operator(&admin, &operator);

        let result = client.try_release_escrow(&operator, &shipment_id);
        assert_eq!(
            result,
            Err(Ok(NavinError::Unauthorized)),
            "operator must be rejected from release_escrow"
        );
    }

    #[test]
    fn operator_cannot_refund_escrow() {
        let (env, client, admin) = setup();
        let (shipment_id, _company, _receiver, _carrier) = create_shipment(&env, &client, &admin);

        let operator = Address::generate(&env);
        client.add_operator(&admin, &operator);

        let result = client.try_refund_escrow(&operator, &shipment_id);
        assert_eq!(
            result,
            Err(Ok(NavinError::Unauthorized)),
            "operator must be rejected from refund_escrow"
        );
    }

    #[test]
    fn operator_blocked_regardless_of_shipment_state() {
        let (env, client, admin) = setup();
        let (shipment_id, company, _receiver, _carrier) = create_shipment(&env, &client, &admin);

        let operator = Address::generate(&env);
        client.add_operator(&admin, &operator);

        // Cancel the shipment (valid state for refund) and still verify operator is blocked.
        let cancel_hash = BytesN::from_array(&env, &[9u8; 32]);
        client.cancel_shipment(&company, &shipment_id, &cancel_hash);

        let result = client.try_refund_escrow(&operator, &shipment_id);
        assert_eq!(
            result,
            Err(Ok(NavinError::Unauthorized)),
            "operator must be rejected from refund_escrow even on a cancelled shipment"
        );
    }
}
