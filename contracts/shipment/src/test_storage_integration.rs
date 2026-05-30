//! Contract tests for Storage Service integration
//!
//! These tests verify that the contract correctly interacts with the storage layer,
//! including data persistence, retrieval, and state management across different
//! storage scopes (instance, persistent, temporary).

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

    /// Test: Storage persistence across multiple shipment creations
    /// Verifies that shipment data is correctly stored and retrieved from persistent storage
    #[test]
    fn test_storage_persistence_multiple_shipments() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        // Create multiple shipments with unique hashes
        let mut shipment_ids = Vec::new(&env);
        for i in 0..5 {
            let mut hash_bytes = [0u8; 32];
            hash_bytes[0] = i as u8;
            hash_bytes[1] = (i + 1) as u8;
            hash_bytes[2] = (i + 2) as u8;
            let hash = BytesN::from_array(&env, &hash_bytes);
            let mut milestones = Vec::new(&env);
            milestones.push_back((checkpoint_symbol(&env, "checkpoint_1"), 100u32));
            let deadline = future_deadline(&env, 86400);

            let shipment_id = client.create_shipment(
                &company,
                &receiver,
                &carrier,
                &hash,
                &milestones,
                &deadline,
                &None,
            );
            shipment_ids.push_back(shipment_id);
        }

        // Verify all shipments are retrievable from storage
        for shipment_id in shipment_ids.iter() {
            let shipment = client.get_shipment(&shipment_id);
            assert_eq!(shipment.id, shipment_id);
            assert_eq!(shipment.sender, company);
            assert_eq!(shipment.receiver, receiver);
            assert_eq!(shipment.carrier, carrier);
        }

        // Verify shipment counter is correct
        let counter = client.get_shipment_counter();
        assert_eq!(counter, 5);
    }

    /// Test: Storage isolation between different shipments
    /// Verifies that modifying one shipment doesn't affect others
    #[test]
    fn test_storage_isolation_between_shipments() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        // Create two shipments with different hashes
        let mut hash1_bytes = [0u8; 32];
        hash1_bytes[0] = 1;
        let hash1 = BytesN::from_array(&env, &hash1_bytes);
        
        let mut hash2_bytes = [0u8; 32];
        hash2_bytes[0] = 2;
        let hash2 = BytesN::from_array(&env, &hash2_bytes);
        
        let mut milestones = Vec::new(&env);
        milestones.push_back((checkpoint_symbol(&env, "checkpoint_1"), 100u32));
        let deadline = future_deadline(&env, 86400);

        let shipment_id1 = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &hash1,
            &milestones,
            &deadline,
            &None,
        );

        let shipment_id2 = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &hash2,
            &milestones,
            &deadline,
            &None,
        );

        // Update status of first shipment
        let status_hash = BytesN::from_array(&env, &[3u8; 32]);
        client.update_status(&carrier, &shipment_id1, &ShipmentStatus::InTransit, &status_hash);

        // Verify first shipment status changed
        let shipment1 = client.get_shipment(&shipment_id1);
        assert_eq!(shipment1.status, ShipmentStatus::InTransit);

        // Verify second shipment status unchanged
        let shipment2 = client.get_shipment(&shipment_id2);
        assert_eq!(shipment2.status, ShipmentStatus::Created);
    }

    /// Test: Escrow storage and retrieval
    /// Verifies that escrow amounts are correctly stored and retrieved
    #[test]
    fn test_escrow_storage_and_retrieval() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        // Create shipment
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let mut milestones = Vec::new(&env);
        milestones.push_back((checkpoint_symbol(&env, "checkpoint_1"), 100u32));
        let deadline = future_deadline(&env, 86400);

        let shipment_id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &hash,
            &milestones,
            &deadline,
            &None,
        );

        // Deposit escrow
        let escrow_amount = 1_000_000i128;
        client.deposit_escrow(&company, &shipment_id, &escrow_amount);

        // Retrieve and verify escrow
        let shipment = client.get_shipment(&shipment_id);
        assert_eq!(shipment.escrow_amount, escrow_amount);
    }

    /// Test: Role storage and retrieval
    /// Verifies that role assignments are correctly stored and retrieved
    #[test]
    fn test_role_storage_and_retrieval() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company1 = Address::generate(&env);
        let company2 = Address::generate(&env);
        let carrier1 = Address::generate(&env);
        let carrier2 = Address::generate(&env);

        client.initialize(&admin, &token_contract);

        // Add multiple companies and carriers
        client.add_company(&admin, &company1);
        client.add_company(&admin, &company2);
        client.add_carrier(&admin, &carrier1);
        client.add_carrier(&admin, &carrier2);

        // Verify all roles are stored correctly by attempting operations
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let mut milestones = Vec::new(&env);
        milestones.push_back((checkpoint_symbol(&env, "checkpoint_1"), 100u32));
        let deadline = future_deadline(&env, 86400);
        let receiver = Address::generate(&env);

        let shipment_id = client.create_shipment(
            &company1,
            &receiver,
            &carrier1,
            &hash,
            &milestones,
            &deadline,
            &None,
        );
        assert_eq!(shipment_id, 1);

        // Verify second company can also create shipments
        let mut hash2_bytes = [0u8; 32];
        hash2_bytes[0] = 2;
        let hash2 = BytesN::from_array(&env, &hash2_bytes);
        let shipment_id2 = client.create_shipment(
            &company2,
            &receiver,
            &carrier2,
            &hash2,
            &milestones,
            &deadline,
            &None,
        );
        assert_eq!(shipment_id2, 2);
    }

    /// Test: Carrier whitelist storage
    /// Verifies that carrier whitelist entries are correctly stored and retrieved
    #[test]
    fn test_carrier_whitelist_storage() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier1 = Address::generate(&env);
        let carrier2 = Address::generate(&env);
        let _carrier3 = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier1);
        client.add_carrier(&admin, &carrier2);
        client.add_carrier(&admin, &_carrier3);

        // Create shipments with different carriers to verify they work
        let hash1 = BytesN::from_array(&env, &[1u8; 32]);
        let mut milestones = Vec::new(&env);
        milestones.push_back((checkpoint_symbol(&env, "checkpoint_1"), 100u32));
        let deadline = future_deadline(&env, 86400);
        let receiver = Address::generate(&env);

        // Should succeed with carrier1
        let shipment_id1 = client.create_shipment(
            &company,
            &receiver,
            &carrier1,
            &hash1,
            &milestones,
            &deadline,
            &None,
        );
        assert_eq!(shipment_id1, 1);

        // Should succeed with carrier2
        let mut hash2_bytes = [0u8; 32];
        hash2_bytes[0] = 2;
        let hash2 = BytesN::from_array(&env, &hash2_bytes);
        let shipment_id2 = client.create_shipment(
            &company,
            &receiver,
            &carrier2,
            &hash2,
            &milestones,
            &deadline,
            &None,
        );
        assert_eq!(shipment_id2, 2);
    }

    /// Test: Storage counter increments correctly
    /// Verifies that the shipment counter is correctly incremented and stored
    #[test]
    fn test_storage_counter_increments() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        // Verify initial counter
        let initial_counter = client.get_shipment_counter();
        assert_eq!(initial_counter, 0);

        // Create shipments and verify counter increments
        for i in 1..=10 {
            let mut hash_bytes = [0u8; 32];
            hash_bytes[0] = i as u8;
            let hash = BytesN::from_array(&env, &hash_bytes);
            let mut milestones = Vec::new(&env);
            milestones.push_back((checkpoint_symbol(&env, "checkpoint_1"), 100u32));
            let deadline = future_deadline(&env, 86400);

            client.create_shipment(
                &company,
                &receiver,
                &carrier,
                &hash,
                &milestones,
                &deadline,
                &None,
            );

            let counter = client.get_shipment_counter();
            assert_eq!(counter, i as u64);
        }
    }

    /// Test: Admin storage persistence
    /// Verifies that admin address is correctly stored and retrieved
    #[test]
    fn test_admin_storage_persistence() {
        let (env, client, admin, token_contract) = setup_test_env();

        client.initialize(&admin, &token_contract);

        // Verify admin is stored
        let stored_admin = client.get_admin();
        assert_eq!(stored_admin, admin);

        // Perform other operations
        let company = Address::generate(&env);
        client.add_company(&admin, &company);

        // Verify admin is still stored correctly
        let stored_admin_again = client.get_admin();
        assert_eq!(stored_admin_again, admin);
    }

    /// Test: Storage with large number of shipments
    /// Verifies that storage handles many shipments correctly
    #[test]
    fn test_storage_with_many_shipments() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        // Create many shipments with unique hashes
        let num_shipments = 50;
        for i in 0..num_shipments {
            let mut hash_bytes = [0u8; 32];
            hash_bytes[0] = (i % 256) as u8;
            hash_bytes[1] = ((i / 256) % 256) as u8;
            hash_bytes[2] = ((i / 65536) % 256) as u8;
            let hash = BytesN::from_array(&env, &hash_bytes);
            let mut milestones = Vec::new(&env);
            milestones.push_back((checkpoint_symbol(&env, "checkpoint_1"), 100u32));
            let deadline = future_deadline(&env, 86400);

            client.create_shipment(
                &company,
                &receiver,
                &carrier,
                &hash,
                &milestones,
                &deadline,
                &None,
            );
        }

        // Verify counter
        let counter = client.get_shipment_counter();
        assert_eq!(counter, num_shipments as u64);

        // Verify random shipments are retrievable
        let shipment_1 = client.get_shipment(&1);
        assert_eq!(shipment_1.id, 1);

        let shipment_25 = client.get_shipment(&25);
        assert_eq!(shipment_25.id, 25);

        let last_id = (num_shipments - 1) as u64;
        let shipment_49 = client.get_shipment(&last_id);
        assert_eq!(shipment_49.id, last_id);
    }

    /// Test: Storage state consistency after operations
    /// Verifies that storage remains consistent after various operations
    #[test]
    fn test_storage_consistency_after_operations() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        // Create shipment
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let mut milestones = Vec::new(&env);
        milestones.push_back((checkpoint_symbol(&env, "checkpoint_1"), 100u32));
        let deadline = future_deadline(&env, 86400);

        let shipment_id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &hash,
            &milestones,
            &deadline,
            &None,
        );

        // Perform multiple operations
        let status_hash1 = BytesN::from_array(&env, &[2u8; 32]);
        client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &status_hash1);

        advance_past_rate_limit(&env);

        let status_hash2 = BytesN::from_array(&env, &[3u8; 32]);
        client.update_status(&carrier, &shipment_id, &ShipmentStatus::AtCheckpoint, &status_hash2);

        // Verify storage is consistent
        let shipment = client.get_shipment(&shipment_id);
        assert_eq!(shipment.id, shipment_id);
        assert_eq!(shipment.status, ShipmentStatus::AtCheckpoint);
        assert_eq!(shipment.sender, company);
        assert_eq!(shipment.receiver, receiver);
        assert_eq!(shipment.carrier, carrier);
    }
}
