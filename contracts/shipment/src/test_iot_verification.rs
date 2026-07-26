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

    // ── [ISSUE #599] StatusHashNotFound error variant tests ──────────────────
    //
    // StatusHashNotFound (#44) is returned by get_status_hash and verify_data_hash
    // when the shipment exists but no hash was ever recorded for the requested status.
    // ShipmentNotFound (#4) is returned when the shipment itself doesn't exist.
    // These tests pin both error codes and cover every path that can surface them.

    /// Error code pin: StatusHashNotFound discriminant must be exactly 44.
    #[test]
    fn test_status_hash_not_found_error_code_is_44() {
        use crate::NavinError;
        assert_eq!(
            NavinError::StatusHashNotFound as u32,
            44,
            "StatusHashNotFound discriminant must be 44"
        );
    }

    /// get_status_hash on a non-existent shipment must return ShipmentNotFound (#4).
    #[test]
    fn test_get_status_hash_nonexistent_shipment_returns_not_found() {
        use crate::NavinError;
        let (_env, client, admin, token_contract) = setup_test_env();
        client.initialize(&admin, &token_contract);

        let result = client.try_get_status_hash(&9999u64, &ShipmentStatus::InTransit);
        assert_eq!(
            result,
            Err(Ok(NavinError::ShipmentNotFound)),
            "get_status_hash on non-existent shipment must return ShipmentNotFound (#4)"
        );
    }

    /// get_status_hash for a status never updated on a valid shipment must return
    /// StatusHashNotFound (#44).
    #[test]
    fn test_get_status_hash_unset_status_returns_not_found() {
        use crate::NavinError;
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);

        // No status update has been made — Delivered hash must not exist.
        let result = client.try_get_status_hash(&shipment_id, &ShipmentStatus::Delivered);
        assert_eq!(
            result,
            Err(Ok(NavinError::StatusHashNotFound)),
            "get_status_hash for unset status must return StatusHashNotFound (#44)"
        );
    }

    /// get_status_hash for AtCheckpoint before that status was ever reached returns
    /// StatusHashNotFound — different unset status variant.
    #[test]
    fn test_get_status_hash_checkpoint_never_reached_returns_not_found() {
        use crate::NavinError;
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);

        // Update to InTransit only — AtCheckpoint hash must still be absent.
        let transit_hash = BytesN::from_array(&env, &[2u8; 32]);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &transit_hash,
        );

        let result = client.try_get_status_hash(&shipment_id, &ShipmentStatus::AtCheckpoint);
        assert_eq!(
            result,
            Err(Ok(NavinError::StatusHashNotFound)),
            "get_status_hash for AtCheckpoint (never reached) must return StatusHashNotFound (#44)"
        );
    }

    /// get_status_hash for Cancelled status before cancellation returns StatusHashNotFound.
    #[test]
    fn test_get_status_hash_cancelled_status_not_yet_set_returns_not_found() {
        use crate::NavinError;
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);

        // Shipment is Created — no Cancelled hash recorded yet.
        let result = client.try_get_status_hash(&shipment_id, &ShipmentStatus::Cancelled);
        assert_eq!(
            result,
            Err(Ok(NavinError::StatusHashNotFound)),
            "get_status_hash for Cancelled (not yet set) must return StatusHashNotFound (#44)"
        );
    }

    /// verify_data_hash on a non-existent shipment must return ShipmentNotFound (#4).
    #[test]
    fn test_verify_data_hash_nonexistent_shipment_returns_not_found() {
        use crate::NavinError;
        let (_env, client, admin, token_contract) = setup_test_env();
        client.initialize(&admin, &token_contract);

        let hash = BytesN::from_array(&_env, &[1u8; 32]);
        let result = client.try_verify_data_hash(&9999u64, &ShipmentStatus::InTransit, &hash);
        assert_eq!(
            result,
            Err(Ok(NavinError::ShipmentNotFound)),
            "verify_data_hash on non-existent shipment must return ShipmentNotFound (#4)"
        );
    }

    /// verify_data_hash for a status with no recorded hash must return StatusHashNotFound (#44).
    #[test]
    fn test_verify_data_hash_unset_status_returns_hash_not_found() {
        use crate::NavinError;
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);

        // No status update — verify_data_hash for Delivered must be StatusHashNotFound.
        let probe = BytesN::from_array(&env, &[5u8; 32]);
        let result = client.try_verify_data_hash(&shipment_id, &ShipmentStatus::Delivered, &probe);
        assert_eq!(
            result,
            Err(Ok(NavinError::StatusHashNotFound)),
            "verify_data_hash for unset status must return StatusHashNotFound (#44)"
        );
    }

    /// verify_data_hash for AtCheckpoint before that status was reached returns StatusHashNotFound.
    #[test]
    fn test_verify_data_hash_checkpoint_unset_returns_hash_not_found() {
        use crate::NavinError;
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);

        // Update to InTransit only — AtCheckpoint has no stored hash.
        let transit_hash = BytesN::from_array(&env, &[2u8; 32]);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &transit_hash,
        );

        let probe = BytesN::from_array(&env, &[9u8; 32]);
        let result =
            client.try_verify_data_hash(&shipment_id, &ShipmentStatus::AtCheckpoint, &probe);
        assert_eq!(
            result,
            Err(Ok(NavinError::StatusHashNotFound)),
            "verify_data_hash for AtCheckpoint (unset) must return StatusHashNotFound (#44)"
        );
    }

    /// After updating status, the correct hash is returned and StatusHashNotFound is
    /// not triggered — confirming the happy path is unaffected.
    #[test]
    fn test_get_status_hash_returns_correct_hash_after_update() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);

        let transit_hash = BytesN::from_array(&env, &[0xBBu8; 32]);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &transit_hash,
        );

        let result = client.try_get_status_hash(&shipment_id, &ShipmentStatus::InTransit);
        assert_eq!(
            result,
            Ok(transit_hash),
            "get_status_hash must return the correct hash after update"
        );
    }

    /// StatusHashNotFound is not returned for a previously-set status even after
    /// the shipment transitions to a later status — old hashes remain accessible.
    #[test]
    fn test_get_status_hash_historical_hash_still_accessible() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);

        // Record InTransit hash.
        let transit_hash = BytesN::from_array(&env, &[0xCCu8; 32]);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &transit_hash,
        );

        // Advance to AtCheckpoint.
        advance_past_rate_limit(&env);
        let checkpoint_hash = BytesN::from_array(&env, &[0xDDu8; 32]);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::AtCheckpoint,
            &checkpoint_hash,
        );

        // InTransit hash must still be retrievable even after moving to AtCheckpoint.
        let result = client.try_get_status_hash(&shipment_id, &ShipmentStatus::InTransit);
        assert_eq!(
            result,
            Ok(transit_hash),
            "historical InTransit hash must remain accessible after transition to AtCheckpoint"
        );

        // AtCheckpoint hash must also be correct.
        let cp_result = client.try_get_status_hash(&shipment_id, &ShipmentStatus::AtCheckpoint);
        assert_eq!(
            cp_result,
            Ok(checkpoint_hash),
            "AtCheckpoint hash must be correctly stored and retrieved"
        );
    }

    /// ShipmentNotFound is distinct from StatusHashNotFound: the former fires when
    /// the shipment doesn't exist, the latter when the shipment exists but lacks
    /// a hash for the queried status.
    #[test]
    fn test_get_status_hash_error_codes_are_distinct() {
        use crate::NavinError;
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);

        // Non-existent shipment → ShipmentNotFound.
        let not_found = client.try_get_status_hash(&9999u64, &ShipmentStatus::InTransit);
        assert_eq!(not_found, Err(Ok(NavinError::ShipmentNotFound)));

        // Existing shipment, unset status → StatusHashNotFound.
        let hash_not_found =
            client.try_get_status_hash(&shipment_id, &ShipmentStatus::Delivered);
        assert_eq!(hash_not_found, Err(Ok(NavinError::StatusHashNotFound)));

        // The two errors must be distinct values.
        assert_ne!(
            NavinError::ShipmentNotFound as u32,
            NavinError::StatusHashNotFound as u32,
            "ShipmentNotFound and StatusHashNotFound must have different discriminants"
        );
    }
}
