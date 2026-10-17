//! Tests for reset_circuit_breaker authorization on single-admin deployments

#[cfg(test)]
mod tests {
    use crate::test_utils::setup;
    use soroban_sdk::{testutils::Address as _, Address};

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
