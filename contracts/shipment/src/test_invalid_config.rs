//! Tests for the InvalidConfig error variant (#31).
//!
//! Validates that `update_config` rejects configurations with invalid or
//! out-of-range parameters.

#[cfg(test)]
mod invalid_config_tests {
    extern crate std;

    use crate::{NavinShipment, NavinShipmentClient};
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env};

    #[contract]
    struct MockToken;

    #[contractimpl]
    impl MockToken {
        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
        pub fn decimals(_env: Env) -> u32 {
            crate::types::EXPECTED_TOKEN_DECIMALS
        }
    }

    fn setup() -> (Env, NavinShipmentClient<'static>, Address) {
        let (env, admin) = crate::test_utils::setup_env();
        let token_contract = env.register(MockToken, ());
        let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
        client.initialize(&admin, &token_contract);
        (env, client, admin)
    }

    /// batch_operation_limit = 0 is below the minimum (1).
    #[test]
    #[should_panic(expected = "Error(Contract, #31)")]
    fn test_update_config_rejects_zero_batch_operation_limit() {
        let (_env, client, admin) = setup();
        let mut config = client.get_contract_config();
        config.batch_operation_limit = 0;
        client.update_config(&admin, &config);
    }

    /// batch_operation_limit = 101 exceeds the maximum (100).
    #[test]
    #[should_panic(expected = "Error(Contract, #31)")]
    fn test_update_config_rejects_batch_operation_limit_over_max() {
        let (_env, client, admin) = setup();
        let mut config = client.get_contract_config();
        config.batch_operation_limit = 101;
        client.update_config(&admin, &config);
    }

    /// shipment_ttl_threshold = 0 is below the minimum (1).
    #[test]
    #[should_panic(expected = "Error(Contract, #31)")]
    fn test_update_config_rejects_zero_shipment_ttl_threshold() {
        let (_env, client, admin) = setup();
        let mut config = client.get_contract_config();
        config.shipment_ttl_threshold = 0;
        client.update_config(&admin, &config);
    }

    /// min_status_update_interval = 9 is below the minimum (10).
    #[test]
    #[should_panic(expected = "Error(Contract, #31)")]
    fn test_update_config_rejects_low_status_update_interval() {
        let (_env, client, admin) = setup();
        let mut config = client.get_contract_config();
        config.min_status_update_interval = 9;
        client.update_config(&admin, &config);
    }

    /// min_status_update_interval = 86401 exceeds the maximum (86400).
    #[test]
    #[should_panic(expected = "Error(Contract, #31)")]
    fn test_update_config_rejects_high_status_update_interval() {
        let (_env, client, admin) = setup();
        let mut config = client.get_contract_config();
        config.min_status_update_interval = 86_401;
        client.update_config(&admin, &config);
    }

    /// max_metadata_entries = 0 is below the minimum (1).
    #[test]
    #[should_panic(expected = "Error(Contract, #31)")]
    fn test_update_config_rejects_zero_max_metadata_entries() {
        let (_env, client, admin) = setup();
        let mut config = client.get_contract_config();
        config.max_metadata_entries = 0;
        client.update_config(&admin, &config);
    }

    /// default_shipment_limit = 0 is below the minimum (1).
    #[test]
    #[should_panic(expected = "Error(Contract, #31)")]
    fn test_update_config_rejects_zero_default_shipment_limit() {
        let (_env, client, admin) = setup();
        let mut config = client.get_contract_config();
        config.default_shipment_limit = 0;
        client.update_config(&admin, &config);
    }

    /// multisig_min_admins = 1 is below the minimum (2).
    #[test]
    #[should_panic(expected = "Error(Contract, #31)")]
    fn test_update_config_rejects_low_multisig_min_admins() {
        let (_env, client, admin) = setup();
        let mut config = client.get_contract_config();
        config.multisig_min_admins = 1;
        client.update_config(&admin, &config);
    }

    /// proposal_expiry_seconds = 3599 is below the minimum (3600).
    #[test]
    #[should_panic(expected = "Error(Contract, #31)")]
    fn test_update_config_rejects_low_proposal_expiry() {
        let (_env, client, admin) = setup();
        let mut config = client.get_contract_config();
        config.proposal_expiry_seconds = 3_599;
        client.update_config(&admin, &config);
    }

    /// proposal_expiry_seconds = 2592001 exceeds the maximum (2592000).
    #[test]
    #[should_panic(expected = "Error(Contract, #31)")]
    fn test_update_config_rejects_high_proposal_expiry() {
        let (_env, client, admin) = setup();
        let mut config = client.get_contract_config();
        config.proposal_expiry_seconds = 2_592_001;
        client.update_config(&admin, &config);
    }

    /// Valid configuration values should succeed.
    #[test]
    fn test_update_config_valid_values_succeed() {
        let (_env, client, admin) = setup();
        let mut config = client.get_contract_config();
        config.batch_operation_limit = 50;
        config.min_status_update_interval = 120;
        config.max_metadata_entries = 10;
        config.default_shipment_limit = 500;
        config.multisig_min_admins = 3;
        config.proposal_expiry_seconds = 86_400;
        client.update_config(&admin, &config);

        let stored = client.get_contract_config();
        assert_eq!(stored.batch_operation_limit, 50);
        assert_eq!(stored.min_status_update_interval, 120);
    }

    /// InvalidConfig is error code 31.
    #[test]
    fn test_invalid_config_error_code_is_31() {
        assert_eq!(crate::NavinError::InvalidConfig as u32, 31);
    }
}
