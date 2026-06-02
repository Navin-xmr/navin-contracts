//! Tests for IoT sensor data hash verification

#[cfg(test)]
mod tests {
    use crate::test_utils::*;
    use crate::types::*;
    use crate::{NavinShipment, NavinShipmentClient};
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Vec};

    #[contract]
    struct MockToken;

    #[contractimpl]
    impl MockToken {
        pub fn decimals(_env: soroban_sdk::Env) -> u32 {
            7
        }

        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {
            // Mock implementation - always succeeds
        }
    }

    fn setup_test_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
        let (env, admin) = setup_env();
        let token_contract = env.register(MockToken {}, ());
        let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
        (env, client, admin, token_contract)
    }

    #[test]
    fn test_status_hash_stored_on_update() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        // Create shipment
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);

        // Update status to InTransit
        let transit_hash = BytesN::from_array(&env, &[2u8; 32]);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &transit_hash,
        );

        // Verify the hash was stored
        let stored_hash = client.get_status_hash(&shipment_id, &ShipmentStatus::InTransit);
        assert_eq!(stored_hash, transit_hash);
    }

    #[test]
    fn test_verify_data_hash_success() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        // Create shipment
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);

        // Update status to InTransit
        let transit_hash = BytesN::from_array(&env, &[2u8; 32]);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &transit_hash,
        );

        // Verify with correct hash
        let verified =
            client.verify_data_hash(&shipment_id, &ShipmentStatus::InTransit, &transit_hash);
        assert!(verified);
    }

    #[test]
    fn test_verify_data_hash_mismatch() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        // Create shipment
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);

        // Update status to InTransit
        let transit_hash = BytesN::from_array(&env, &[2u8; 32]);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &transit_hash,
        );

        // Verify with wrong hash
        let wrong_hash = BytesN::from_array(&env, &[3u8; 32]);
        let verified =
            client.verify_data_hash(&shipment_id, &ShipmentStatus::InTransit, &wrong_hash);
        assert!(!verified);
    }

    #[test]
    fn test_multiple_status_hashes() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        // Create shipment
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);

        // Update to InTransit
        let transit_hash = BytesN::from_array(&env, &[2u8; 32]);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &transit_hash,
        );

        // Update to AtCheckpoint
        let checkpoint_hash = BytesN::from_array(&env, &[3u8; 32]);
        advance_past_rate_limit(&env);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::AtCheckpoint,
            &checkpoint_hash,
        );

        // Verify both hashes are stored independently
        let transit_stored = client.get_status_hash(&shipment_id, &ShipmentStatus::InTransit);
        assert_eq!(transit_stored, transit_hash);

        let checkpoint_stored = client.get_status_hash(&shipment_id, &ShipmentStatus::AtCheckpoint);
        assert_eq!(checkpoint_stored, checkpoint_hash);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #44)")]
    fn test_get_status_hash_not_found() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        // Create shipment
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);

        // Try to get hash for status that was never set
        client.get_status_hash(&shipment_id, &ShipmentStatus::Delivered);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_verify_data_hash_nonexistent_shipment() {
        let (_env, client, admin, token_contract) = setup_test_env();

        client.initialize(&admin, &token_contract);

        let hash = BytesN::from_array(&_env, &[1u8; 32]);
        client.verify_data_hash(&999, &ShipmentStatus::InTransit, &hash);
    }

    #[test]
    fn test_iot_verification_no_auth_required() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        // Create shipment
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);

        // Update status
        let transit_hash = BytesN::from_array(&env, &[2u8; 32]);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &transit_hash,
        );

        // Anyone can verify (no auth required) - this is a read-only operation
        let verified =
            client.verify_data_hash(&shipment_id, &ShipmentStatus::InTransit, &transit_hash);
        assert!(verified);
    }
}
