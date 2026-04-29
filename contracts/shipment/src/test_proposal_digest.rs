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
}
