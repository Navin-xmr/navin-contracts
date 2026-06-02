//! Tests for pause/unpause emergency mechanism

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
    fn test_pause_success() {
        let (_env, client, admin, token_contract) = setup_test_env();

        client.initialize(&admin, &token_contract);

        // Pause the contract
        client.pause(&admin);

        // Verify contract is paused
        assert!(client.is_paused());
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #3)")]
    fn test_pause_unauthorized() {
        let (env, client, admin, token_contract) = setup_test_env();
        let non_admin = Address::generate(&env);

        client.initialize(&admin, &token_contract);

        // Non-admin tries to pause
        client.pause(&non_admin);
    }

    #[test]
    fn test_unpause_success() {
        let (_env, client, admin, token_contract) = setup_test_env();

        client.initialize(&admin, &token_contract);

        // Pause then unpause
        client.pause(&admin);
        assert!(client.is_paused());

        client.unpause(&admin);
        assert!(!client.is_paused());
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #43)")]
    fn test_create_shipment_fails_when_paused() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);

        // Pause the contract
        client.pause(&admin);

        // Try to create shipment while paused
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        client.create_shipment(&company, &receiver, &carrier, &hash, &milestones, &deadline);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #43)")]
    fn test_update_status_fails_when_paused() {
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

        // Pause the contract
        client.pause(&admin);

        // Try to update status while paused
        let new_hash = BytesN::from_array(&env, &[2u8; 32]);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &new_hash,
        );
    }

    #[test]
    fn test_read_operations_work_when_paused() {
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

        // Pause the contract
        client.pause(&admin);

        // Read operations should still work
        let shipment = client.get_shipment(&shipment_id);
        assert_eq!(shipment.id, shipment_id);

        let admin_result = client.get_admin();
        assert_eq!(admin_result, admin);

        let counter = client.get_shipment_counter();
        assert_eq!(counter, 1);
    }

    #[test]
    fn test_pause_unpause_operation_succeeds() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        let receiver = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        // Create shipment before pause
        let hash1 = BytesN::from_array(&env, &[1u8; 32]);
        let milestones = Vec::new(&env);
        let deadline = future_deadline(&env, 86400);

        let shipment_id1 = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &hash1,
            &milestones,
            &deadline,
        );

        // Pause
        client.pause(&admin);
        assert!(client.is_paused());

        // Unpause
        client.unpause(&admin);
        assert!(!client.is_paused());

        // Create shipment after unpause should work
        let hash2 = BytesN::from_array(&env, &[2u8; 32]);
        let shipment_id2 = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &hash2,
            &milestones,
            &deadline,
        );

        assert_eq!(shipment_id2, shipment_id1 + 1);
    }

    #[test]
    fn test_pause_guardian_success() {
        let (_env, client, admin, token_contract) = setup_test_env();
        let guardian = Address::generate(&_env);

        client.initialize(&admin, &token_contract);
        client.add_guardian(&admin, &guardian);

        // Guardian pauses the contract
        client.pause(&guardian);

        // Verify contract is paused
        assert!(client.is_paused());
    }

    #[test]
    fn test_unpause_guardian_success() {
        let (_env, client, admin, token_contract) = setup_test_env();
        let guardian = Address::generate(&_env);

        client.initialize(&admin, &token_contract);
        client.add_guardian(&admin, &guardian);

        // Pause then unpause by Guardian
        client.pause(&guardian);
        assert!(client.is_paused());

        client.unpause(&guardian);
        assert!(!client.is_paused());
    }

    // ── Finalization lock-out survives pause/unpause cycle (issue #446) ─────

    /// A finalized shipment must stay locked even after the contract is
    /// paused and then unpaused — the finalization flag is not cleared by
    /// the pause mechanism.
    #[test]
    fn test_finalized_shipment_stays_locked_after_unpause() {
        let (env, client, admin, token_contract) = setup_test_env();
        let company = Address::generate(&env);
        let receiver = Address::generate(&env);
        let carrier = Address::generate(&env);

        client.initialize(&admin, &token_contract);
        client.add_company(&admin, &company);

        let hash = BytesN::from_array(&env, &[0x99u8; 32]);
        let deadline = future_deadline(&env, 86400);
        let shipment_id =
            client.create_shipment(&company, &receiver, &carrier, &hash, &Vec::new(&env), &deadline);

        // Finalize the shipment via cancellation.
        client.cancel_shipment(&company, &shipment_id, &hash);
        assert!(
            client.get_shipment(&shipment_id).finalized,
            "shipment must be finalized after cancel"
        );

        // Pause then unpause the contract.
        client.pause(&admin);
        client.unpause(&admin);
        assert!(!client.is_paused(), "contract must be unpaused");

        // The finalized shipment must still reject mutating calls.
        let update_result =
            client.try_update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &hash);
        assert!(
            matches!(update_result, Err(Ok(crate::NavinError::ShipmentFinalized))),
            "update_status must still be rejected after pause/unpause cycle"
        );

        let deposit_result = client.try_deposit_escrow(&company, &shipment_id, &1_000_i128);
        assert!(
            matches!(deposit_result, Err(Ok(crate::NavinError::ShipmentFinalized))),
            "deposit_escrow must still be rejected after pause/unpause cycle"
        );
    }
}
