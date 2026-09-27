//! Tests for reset_circuit_breaker authorization on single-admin deployments

#[cfg(test)]
mod tests {
    use crate::test_utils::*;
    use crate::{NavinShipment, NavinShipmentClient};
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env};

    #[contract]
    struct MockToken;

    #[contractimpl]
    impl MockToken {
        pub fn decimals(_env: Env) -> u32 {
            7
        }
    }

    fn setup() -> (Env, NavinShipmentClient<'static>, Address) {
        let (env, admin) = setup_env();
        let token = env.register(MockToken {}, ());
        let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
        client.initialize(&admin, &token);
        (env, client, admin)
    }

    #[test]
    fn sole_admin_without_multisig_can_reset_circuit_breaker() {
        let (_env, client, admin) = setup();

        client.reset_circuit_breaker(&admin);
    }

    #[test]
    fn non_admin_cannot_reset_circuit_breaker() {
        let (env, client, _admin) = setup();
        let stranger = Address::generate(&env);

        assert!(client.try_reset_circuit_breaker(&stranger).is_err());
    }
}
