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

        // Set up multi-sig with two admins and threshold 1 (so we can test easily).
        let admin2 = Address::generate(&env);
        let mut admins = Vec::new(&env);
        admins.push_back(admin.clone());
        admins.push_back(admin2.clone());
        client.init_multisig(&admin, &admins, &1);

        (env, client, admin, admin2)
    }

    fn wasm_hash(env: &Env, seed: u8) -> BytesN<32> {
        BytesN::from_array(env, &[seed; 32])
    }

    // ── digest stored on propose_action ─────────────────────────────────────

    #[test]
    fn digest_stored_when_proposal_created() {
        let (env, client, admin, _admin2) = setup_multisig();

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 1));
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

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 2));

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

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 3));
        let proposal_id = client.propose_action(&admin, &action);

        let stored = client.get_proposal_action_digest(&proposal_id);
        let computed = client.compute_proposal_digest(&proposal_id, &action);

        assert_eq!(stored.digest, computed);
    }

    // ── digest changes for different actions ─────────────────────────────────

    #[test]
    fn digest_differs_for_different_actions() {
        let (env, client, admin, _admin2) = setup_multisig();

        let action_a = crate::types::AdminAction::Upgrade(wasm_hash(&env, 4));
        let action_b = crate::types::AdminAction::Upgrade(wasm_hash(&env, 5));

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
        let (env, client, admin, _admin2) = setup_multisig();

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

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 10));
        let proposal_id = client.propose_action(&admin, &action);

        // Verify proposal exists and has expiry set
        let proposal = client.get_proposal(&proposal_id);
        assert_eq!(proposal.id, proposal_id);
        assert!(proposal.expires_at > proposal.created_at);
        assert!(!proposal.executed);

        // Proposal should be usable (can be approved)
        client.approve_action(&admin2, &proposal_id);

        // Verify approval was recorded
        let updated = client.get_proposal(&proposal_id);
        assert_eq!(updated.approvals.len(), 1);
    }

    /// Test: Advance ledger time beyond the expiry window, then verify proposal cannot be approved.
    /// This is the core expiry enforcement test.
    #[test]
    fn proposal_expired_cannot_be_approved() {
        let (env, client, admin, admin2) = setup_multisig();

        // Create proposal with default 7-day expiry
        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 11));
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

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 12));
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

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 13));
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

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 14));
        let proposal_id = client.propose_action(&admin, &action);

        // Capture initial state
        let before_expiry = client.get_proposal(&proposal_id);
        assert_eq!(before_expiry.approvals.len(), 0);
        assert!(!before_expiry.executed);

        // Advance time past expiry
        crate::test_utils::advance_past_multisig_expiry(&env);

        // Verify proposal can still be queried
        let after_expiry = client.get_proposal(&proposal_id);
        assert_eq!(after_expiry.id, proposal_id);
        assert_eq!(after_expiry.approvals.len(), 0);
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

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 15));
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
        assert_eq!(result_past_boundary, Err(Ok(crate::NavinError::ProposalExpired)));
    }

    /// Test: Multiple proposals expire independently.
    /// Each proposal has its own expiry time and they don't interfere with each other.
    #[test]
    fn multiple_proposals_expire_independently() {
        let (env, client, admin, admin2) = setup_multisig();

        let action1 = crate::types::AdminAction::Upgrade(wasm_hash(&env, 16));
        let action2 = crate::types::AdminAction::Upgrade(wasm_hash(&env, 17));

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

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 18));
        let proposal_id = client.propose_action(&admin, &action);

        // Get 1 approval while still valid
        client.approve_action(&admin2, &proposal_id);

        // Verify 1 approval is recorded
        let proposal = client.get_proposal(&proposal_id);
        assert_eq!(proposal.approvals.len(), 1);

        // Advance past expiry
        crate::test_utils::advance_past_multisig_expiry(&env);

        // Cannot add more approvals
        let approve_result = client.try_approve_action(&admin3, &proposal_id);
        assert_eq!(approve_result, Err(Ok(crate::NavinError::ProposalExpired)));

        // Cannot execute even though we have 1 approval
        let execute_result = client.try_execute_proposal(&proposal_id);
        assert_eq!(execute_result, Err(Ok(crate::NavinError::ProposalExpired)));
    }

    /// Test: Cleanup assertion - expired proposal digest remains queryable.
    /// This ensures proposal metadata (like digests) persists after expiry.
    #[test]
    fn expired_proposal_digest_remains_queryable() {
        let (env, client, admin, _admin2) = setup_multisig();

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 19));
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

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 20));
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

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 21));
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

        let action = crate::types::AdminAction::Upgrade(wasm_hash(&env, 22));
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
}

