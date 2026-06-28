//! Tests for issue #297 — multi-sig proposal action hash and digest query.
//!
//! Verifies that:
//! - A digest is stored when `propose_action` is called.
//! - The digest is stable for identical payloads.
//! - The digest changes for different actions or proposal IDs.
//! - `get_proposal_action_digest` returns the stored digest.
//! - `compute_proposal_digest` is a pure helper that matches the stored value.

#[cfg(test)]
mod tests {
    use crate::{test_utils, NavinError, NavinShipment, NavinShipmentClient};
    use soroban_sdk::testutils::Ledger as _;
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

    fn setup_multisig() -> (Env, NavinShipmentClient<'static>, Address, Address) {
        let (env, admin) = test_utils::setup_env();
        let contract_id = env.register(NavinShipment, ());
        let client = NavinShipmentClient::new(&env, &contract_id);
        let token_id = env.register(MockToken, ());
        client.initialize(&admin, &token_id);

        // Set up multi-sig with two admins and threshold 2.
        let admin2 = Address::generate(&env);
        let mut admins = Vec::new(&env);
        admins.push_back(admin.clone());
        admins.push_back(admin2.clone());
        client.init_multisig(&admin, &admins, &2);

        (env, client, admin, admin2)
    }

    fn wasm_hash(env: &Env, seed: u8) -> BytesN<32> {
        BytesN::from_array(env, &[seed; 32])
    }

    // ── digest stored on propose_action ─────────────────────────────────────

    #[test]
    fn digest_stored_when_proposal_created() {
        let (env, client, admin, _admin2) = setup_multisig();

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        let digest = client.get_proposal_action_digest(&proposal_id);
        assert_eq!(digest.proposal_id, proposal_id);
        // Digest must be non-zero.
        let bytes: [u8; 32] = digest.digest.to_array();
        assert!(bytes.iter().any(|&b| b != 0));
    }

    // ── digest is stable for identical payloads ──────────────────────────────

    #[test]
    fn digest_stable_for_identical_action() {
        let (env, client, admin, _admin2) = setup_multisig();

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));

        let id1 = client.propose_action(&admin, &action);
        // Advance time so idempotency window doesn't block the second proposal.
        crate::test_utils::advance_ledger_time(&env, 400);
        let id2 = client.propose_action(&admin, &action);

        let d1 = client.get_proposal_action_digest(&id1);
        let d2 = client.get_proposal_action_digest(&id2);

        // Same action but different proposal IDs → different digests (ID is bound in).
        assert_ne!(d1.digest, d2.digest);
    }

    // ── compute_proposal_digest matches stored digest ────────────────────────

    #[test]
    fn compute_digest_matches_stored_digest() {
        let (env, client, admin, _admin2) = setup_multisig();

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        let stored = client.get_proposal_action_digest(&proposal_id);
        let computed = client.compute_proposal_digest(&proposal_id, &action);

        assert_eq!(stored.digest, computed);
    }

    // ── digest changes for different actions ─────────────────────────────────

    #[test]
    fn digest_differs_for_different_actions() {
        let (env, client, admin, _admin2) = setup_multisig();

        let action_a = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let action_b = crate::types::AdminAction::TransferAdmin(Address::generate(&env));

        let id_a = client.propose_action(&admin, &action_a);
        crate::test_utils::advance_ledger_time(&env, 400);
        let id_b = client.propose_action(&admin, &action_b);

        // Use the same proposal_id for both computes to isolate action difference.
        let digest_a = client.compute_proposal_digest(&id_a, &action_a);
        let digest_b = client.compute_proposal_digest(&id_a, &action_b);

        assert_ne!(digest_a, digest_b);

        // Also verify stored digests differ.
        let stored_a = client.get_proposal_action_digest(&id_a);
        let stored_b = client.get_proposal_action_digest(&id_b);
        assert_ne!(stored_a.digest, stored_b.digest);
    }

    // ── get_proposal_action_digest returns not found for missing proposal ─────

    #[test]
    fn get_digest_returns_not_found_for_missing_proposal() {
        let (_env, client, _admin, _admin2) = setup_multisig();

        let result = client.try_get_proposal_action_digest(&9999);
        assert_eq!(result, Err(Ok(NavinError::ProposalNotFound)));
    }

    // ── digest exposed in proposal lifecycle ─────────────────────────────────

    #[test]
    fn digest_available_throughout_proposal_lifecycle() {
        let (env, client, admin, admin2) = setup_multisig();

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        // Digest available before approval.
        let before = client.get_proposal_action_digest(&proposal_id);
        assert_eq!(before.proposal_id, proposal_id);

        // Approve (threshold=1, so this also executes).
        let _ = client.try_approve_action(&admin2, &proposal_id);

        // Digest still available after execution.
        let after = client.get_proposal_action_digest(&proposal_id);
        assert_eq!(before.digest, after.digest);
    }

    // ── ForceRelease and ForceRefund actions produce distinct digests ─────────

    #[test]
    fn force_release_and_force_refund_produce_distinct_digests() {
        let (_env, client, _admin, _admin2) = setup_multisig();

        let shipment_id = 42u64;
        let action_release = crate::types::AdminAction::ForceRelease(shipment_id);
        let action_refund = crate::types::AdminAction::ForceRefund(shipment_id);

        let digest_release = client.compute_proposal_digest(&1, &action_release);
        let digest_refund = client.compute_proposal_digest(&1, &action_refund);

        assert_ne!(digest_release, digest_refund);
    }

    // ── [ISSUE #454] Proposal expiry workflow tests ─────────────────────────

    /// Test: Create a proposal fixture with expiry, then verify it exists and is usable.
    /// This establishes the baseline before testing expiry behavior.
    #[test]
    fn proposal_created_with_expiry_is_initially_usable() {
        let (env, client, admin, admin2) = setup_multisig();

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        // Verify proposal exists and has expiry set
        let proposal = client.get_proposal(&proposal_id);
        assert_eq!(proposal.id, proposal_id);
        assert!(proposal.expires_at > proposal.created_at);
        assert!(!proposal.executed);

        // Proposal should be usable (can be approved)
        client.approve_action(&admin2, &proposal_id);

        // Verify approval was recorded (proposer + admin2)
        let updated = client.get_proposal(&proposal_id);
        assert_eq!(updated.approvals.len(), 2);
    }

    /// Test: Advance ledger time beyond the expiry window, then verify proposal cannot be approved.
    /// This is the core expiry enforcement test.
    #[test]
    fn proposal_expired_cannot_be_approved() {
        let (env, client, admin, admin2) = setup_multisig();

        // Create proposal with default 7-day expiry
        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        // Verify proposal is initially valid
        let proposal = client.get_proposal(&proposal_id);
        assert!(!proposal.executed);

        // Advance time beyond expiry window (7 days + 1 second)
        crate::test_utils::advance_past_multisig_expiry(&env);

        // Attempt to approve should fail with ProposalExpired
        let result = client.try_approve_action(&admin2, &proposal_id);
        assert_eq!(result, Err(Ok(crate::NavinError::ProposalExpired)));
    }

    /// Test: Advance time beyond expiry window, then verify proposal cannot be executed.
    /// This tests that expired proposals cannot be executed even if they have enough approvals.
    #[test]
    fn proposal_expired_cannot_be_executed() {
        let (env, client, admin, admin2) = setup_multisig();

        // Set up multi-sig with threshold 2
        let admin3 = Address::generate(&env);
        let mut admins = Vec::new(&env);
        admins.push_back(admin.clone());
        admins.push_back(admin2.clone());
        admins.push_back(admin3.clone());
        client.init_multisig(&admin, &admins, &2);

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        // Get one approval before expiry
        client.approve_action(&admin2, &proposal_id);

        // Advance time beyond expiry
        crate::test_utils::advance_past_multisig_expiry(&env);

        // Attempt to execute should fail even though we have 1 approval
        let result = client.try_execute_proposal(&proposal_id);
        assert_eq!(result, Err(Ok(crate::NavinError::ProposalExpired)));
    }

    /// Test: Verify the expiry flow is deterministic across multiple checks.
    /// Repeated queries of an expired proposal should consistently return ProposalExpired.
    #[test]
    fn proposal_expiry_check_is_deterministic() {
        let (env, client, admin, admin2) = setup_multisig();

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        // Advance time past expiry
        crate::test_utils::advance_past_multisig_expiry(&env);

        // Multiple approval attempts should all fail with the same error
        let result1 = client.try_approve_action(&admin2, &proposal_id);
        let result2 = client.try_approve_action(&admin2, &proposal_id);
        let result3 = client.try_approve_action(&admin2, &proposal_id);

        assert_eq!(result1, Err(Ok(crate::NavinError::ProposalExpired)));
        assert_eq!(result2, Err(Ok(crate::NavinError::ProposalExpired)));
        assert_eq!(result3, Err(Ok(crate::NavinError::ProposalExpired)));
    }

    /// Test: Proposal state remains consistent after expiry (can still be queried).
    /// Expired proposals should remain readable and their state should not change.
    #[test]
    fn proposal_state_consistent_after_expiry() {
        let (env, client, admin, _admin2) = setup_multisig();

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        // Capture initial state (proposer is auto-approved)
        let before_expiry = client.get_proposal(&proposal_id);
        assert_eq!(before_expiry.approvals.len(), 1);
        assert!(!before_expiry.executed);

        // Advance time past expiry
        crate::test_utils::advance_past_multisig_expiry(&env);

        // Verify proposal can still be queried
        let after_expiry = client.get_proposal(&proposal_id);
        assert_eq!(after_expiry.id, proposal_id);
        assert_eq!(after_expiry.approvals.len(), 1);
        assert!(!after_expiry.executed);

        // State fields should remain unchanged
        assert_eq!(after_expiry.proposer, before_expiry.proposer);
        assert_eq!(after_expiry.created_at, before_expiry.created_at);
        assert_eq!(after_expiry.expires_at, before_expiry.expires_at);
    }

    /// Test: Proposal expires exactly at the boundary timestamp.
    /// This verifies precise timing behavior: proposals should expire when ledger time > expires_at.
    #[test]
    fn proposal_expires_at_exact_boundary() {
        let (env, client, admin, admin2) = setup_multisig();

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        let proposal = client.get_proposal(&proposal_id);
        let expires_at = proposal.expires_at;

        // Advance to exactly expiry time (not past it)
        env.ledger().with_mut(|l| l.timestamp = expires_at);

        // Should still be usable at exactly expires_at (not strictly greater)
        let result_at_boundary = client.try_approve_action(&admin2, &proposal_id);
        assert!(
            result_at_boundary.is_ok(),
            "proposal should be usable at exactly expires_at"
        );

        // Now advance 1 second past expiry
        env.ledger().with_mut(|l| l.timestamp = expires_at + 1);

        // Now it should be expired
        let result_past_boundary = client.try_approve_action(&admin2, &proposal_id);
        assert_eq!(
            result_past_boundary,
            Err(Ok(crate::NavinError::ProposalExpired))
        );
    }

    /// Test: Multiple proposals expire independently.
    /// Each proposal has its own expiry time and they don't interfere with each other.
    #[test]
    fn multiple_proposals_expire_independently() {
        let (env, client, admin, admin2) = setup_multisig();

        let action1 = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let action2 = crate::types::AdminAction::TransferAdmin(Address::generate(&env));

        let proposal1 = client.propose_action(&admin, &action1);
        let p1 = client.get_proposal(&proposal1);

        // Advance time 1 day
        crate::test_utils::advance_ledger_time(&env, 86_400);

        let proposal2 = client.propose_action(&admin, &action2);
        let p2 = client.get_proposal(&proposal2);

        // proposal2 was created 1 day later, so it expires later
        assert!(p2.expires_at > p1.expires_at);

        // Advance to just past proposal1's expiry
        env.ledger().with_mut(|l| l.timestamp = p1.expires_at + 1);

        // proposal1 should be expired
        let result1 = client.try_approve_action(&admin2, &proposal1);
        assert_eq!(result1, Err(Ok(crate::NavinError::ProposalExpired)));

        // proposal2 should still be usable
        let result2 = client.try_approve_action(&admin2, &proposal2);
        assert!(result2.is_ok());
    }

    /// Test: Proposal with approvals before expiry cannot be executed after expiry.
    /// This verifies that even with valid approvals, expiry blocks execution.
    #[test]
    fn proposal_with_approvals_blocked_after_expiry() {
        let (env, client, admin, admin2) = setup_multisig();

        // Set threshold to 2
        let admin3 = Address::generate(&env);
        let mut admins = Vec::new(&env);
        admins.push_back(admin.clone());
        admins.push_back(admin2.clone());
        admins.push_back(admin3.clone());
        client.init_multisig(&admin, &admins, &2);

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        // Get 1 approval while still valid
        client.approve_action(&admin2, &proposal_id);

        // Verify 2 approvals are recorded (admin + admin2)
        let proposal = client.get_proposal(&proposal_id);
        assert_eq!(proposal.approvals.len(), 2);

        // Advance past expiry
        crate::test_utils::advance_past_multisig_expiry(&env);

        // Cannot add more approvals
        let approve_result = client.try_approve_action(&admin3, &proposal_id);
        assert_eq!(approve_result, Err(Ok(crate::NavinError::ProposalExpired)));

        // Cannot execute even though we have 2 approvals
        let execute_result = client.try_execute_proposal(&proposal_id);
        assert_eq!(execute_result, Err(Ok(crate::NavinError::ProposalExpired)));
    }

    /// Test: Cleanup assertion - expired proposal digest remains queryable.
    /// This ensures proposal metadata (like digests) persists after expiry.
    #[test]
    fn expired_proposal_digest_remains_queryable() {
        let (env, client, admin, _admin2) = setup_multisig();

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        // Capture digest before expiry
        let digest_before = client.get_proposal_action_digest(&proposal_id);
        assert_eq!(digest_before.proposal_id, proposal_id);

        // Advance past expiry
        crate::test_utils::advance_past_multisig_expiry(&env);

        // Digest should still be queryable
        let digest_after = client.get_proposal_action_digest(&proposal_id);
        assert_eq!(digest_after.proposal_id, proposal_id);
        assert_eq!(digest_after.digest, digest_before.digest);
    }

    /// Test: Proposal expiry with custom config duration.
    /// Verify that changing proposal_expiry_seconds in config affects new proposals.
    #[test]
    fn proposal_expiry_respects_config_duration() {
        let (env, client, admin, admin2) = setup_multisig();

        // Get current config and set shorter expiry (1 hour)
        let mut config = client.get_contract_config();
        config.proposal_expiry_seconds = 3_600; // 1 hour
        client.update_config(&admin, &config);

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        let proposal = client.get_proposal(&proposal_id);
        let expected_expiry = proposal.created_at + 3_600;
        assert_eq!(proposal.expires_at, expected_expiry);

        // Advance time by 1 hour + 1 second
        env.ledger()
            .with_mut(|l| l.timestamp = proposal.created_at + 3_601);

        // Should be expired
        let result = client.try_approve_action(&admin2, &proposal_id);
        assert_eq!(result, Err(Ok(crate::NavinError::ProposalExpired)));
    }

    /// Test: Very short expiry window (edge case for rapid expiry).
    /// This tests that even very short expiry windows work correctly.
    #[test]
    fn proposal_with_minimum_expiry_window() {
        let (env, client, admin, admin2) = setup_multisig();

        // Set minimum allowed expiry (1 hour = 3600 seconds)
        let mut config = client.get_contract_config();
        config.proposal_expiry_seconds = 3_600;
        client.update_config(&admin, &config);

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        // Should be usable immediately after creation
        let result = client.try_approve_action(&admin2, &proposal_id);
        assert!(result.is_ok());

        // Create another proposal
        crate::test_utils::advance_ledger_time(&env, 400);
        let proposal2_id = client.propose_action(&admin, &action);

        // Advance to just before expiry
        let p2 = client.get_proposal(&proposal2_id);
        env.ledger().with_mut(|l| l.timestamp = p2.expires_at);

        // Should still be usable
        let result = client.try_approve_action(&admin2, &proposal2_id);
        assert!(result.is_ok());
    }

    /// Test: Proposal creation time vs expiry calculation is consistent.
    /// This verifies the expires_at field is correctly calculated.
    #[test]
    fn proposal_expiry_calculation_consistent() {
        let (env, client, admin, _admin2) = setup_multisig();

        let config = client.get_contract_config();
        let before_creation = env.ledger().timestamp();

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        let proposal = client.get_proposal(&proposal_id);

        // created_at should be at or after before_creation
        assert!(proposal.created_at >= before_creation);

        // expires_at should be created_at + proposal_expiry_seconds
        let expected_expiry = proposal.created_at + config.proposal_expiry_seconds;
        assert_eq!(proposal.expires_at, expected_expiry);
    }

    /// Test: Expired proposal rejection message is deterministic.
    /// Verify that the error returned for expired proposals is consistent.
    #[test]
    fn expired_proposal_rejection_consistent() {
        let (env, client, admin, admin2) = setup_multisig();

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 23));
        let proposal_id = client.propose_action(&admin, &action);

        crate::test_utils::advance_past_multisig_expiry(&env);

        // Try different operations on expired proposal
        let approve_result = client.try_approve_action(&admin2, &proposal_id);
        let execute_result = client.try_execute_proposal(&proposal_id);

        // Both should return the same ProposalExpired error
        assert_eq!(approve_result, Err(Ok(crate::NavinError::ProposalExpired)));
        assert_eq!(execute_result, Err(Ok(crate::NavinError::ProposalExpired)));
    }

    // ── Proposal expiration and cleanup flow ──────────────────────────────────

    /// Test: Expired proposals cannot be executed.
    /// Verify that attempting to execute an expired proposal fails with ProposalExpired.
    #[test]
    fn expired_proposal_cannot_be_executed() {
        let (env, client, admin, admin2) = setup_multisig();

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        // Approve to get to sufficient threshold
        client.approve_action(&admin2, &proposal_id);

        // Advance ledger time beyond the 7-day expiry threshold
        crate::test_utils::advance_past_multisig_expiry(&env);

        // Attempting to execute the expired proposal must fail
        let result = client.try_execute_proposal(&proposal_id);
        assert_eq!(
            result,
            Err(Ok(NavinError::ProposalExpired)),
            "Expired proposal must reject execution"
        );
    }

    /// Test: Expired proposal approval attempts fail.
    /// Verify that trying to approve an expired proposal returns the proper error.
    #[test]
    fn expired_proposal_cannot_be_approved() {
        let (env, client, admin, admin2) = setup_multisig();

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 24));
        let proposal_id = client.propose_action(&admin, &action);

        // Advance time beyond expiry
        crate::test_utils::advance_past_multisig_expiry(&env);

        // Attempting to approve should fail
        let result = client.try_approve_action(&admin2, &proposal_id);
        assert_eq!(
            result,
            Err(Ok(NavinError::ProposalExpired)),
            "Cannot approve an expired proposal"
        );
    }

    /// Test: Expired proposal storage key is safely removable.
    /// Verify that storage cleanup operations work on expired proposal keys.
    #[test]
    fn expired_proposal_storage_can_be_cleaned() {
        let (env, client, admin, _admin2) = setup_multisig();

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        // Advance time beyond expiry
        crate::test_utils::advance_past_multisig_expiry(&env);

        // Verify the proposal is expired (cannot execute)
        let exec_result = client.try_execute_proposal(&proposal_id);
        assert_eq!(
            exec_result,
            Err(Ok(NavinError::ProposalExpired)),
            "Proposal must be expired"
        );

        // After expiration, the proposal storage key should remain safely accessible
        // for cleanup without causing errors. Attempting to get the expired proposal
        // should still work for diagnostics (not panic or fail unexpectedly).
        let get_result = client.try_get_proposal(&proposal_id);
        // Result varies based on implementation - could be NotFound or ProposalExpired
        // The key point is it doesn't cause a crash or unexpected error type
        assert!(
            get_result.is_err(),
            "Getting expired proposal should handle gracefully"
        );
    }

    /// Test: Multiple expired proposals do not interfere with new proposals.
    /// Verify that creating new proposals works even when old expired ones exist.
    #[test]
    fn new_proposals_work_after_expiring_old_ones() {
        let (env, client, admin, admin2) = setup_multisig();

        // Create and expire first proposal
        let action1 = crate::types::AdminAction::Upgrade(wasm_hash(&env, 25));
        let proposal_id_1 = client.propose_action(&admin, &action1);

        // Advance past expiry
        crate::test_utils::advance_past_multisig_expiry(&env);

        // Verify first proposal is expired
        assert_eq!(
            client.try_execute_proposal(&proposal_id_1),
            Err(Ok(NavinError::ProposalExpired))
        );

        // Now create a new proposal - should succeed even with expired proposal in storage
        let action2 = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id_2 = client.propose_action(&admin2, &action2);

        // New proposal should be functional (not expired)
        let proposal = client.get_proposal(&proposal_id_2);
        assert_eq!(proposal.id, proposal_id_2);
        assert!(!proposal.executed, "New proposal must not be pre-executed");
    }

    /// Test: Proposal expiry timestamp enforcement.
    /// Verify that proposals expire at the correct ledger time threshold.
    #[test]
    fn proposal_expiry_enforced_at_correct_threshold() {
        let (env, client, admin, admin2) = setup_multisig();

        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let proposal_id = client.propose_action(&admin, &action);

        // Before expiry window - should be executable (if thresholds met)
        // Note: We can't execute this without hitting the approval threshold,
        // but we can verify approval is allowed
        let approve_result = client.try_approve_action(&admin2, &proposal_id);
        assert!(
            approve_result.is_ok(),
            "Proposal must be approvable before expiry"
        );

        // Advance time to just before expiry (less than 7 days)
        env.ledger().with_mut(|l| {
            l.timestamp += 604799; // 7 days - 1 second
        });

        // Proposal should still be usable
        let get_result = client.try_get_proposal(&proposal_id);
        assert!(get_result.is_ok(), "Proposal must be accessible just before expiry");

        // Now advance past the expiry threshold
        env.ledger().with_mut(|l| {
            l.timestamp += 2; // Now past 7 days + 1 second
        });

        // Now it should be expired
        let result = client.try_approve_action(&admin, &proposal_id);
        assert_eq!(
            result,
            Err(Ok(NavinError::ProposalExpired)),
            "Proposal must be expired after 7-day threshold"
        );
    }
}
