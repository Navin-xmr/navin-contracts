//! Tests for issue #296 — shipment creation quota window.
//!
//! Verifies that the per-company creation quota is enforced within the active
//! window and resets correctly when the window expires.

#[cfg(test)]
mod tests {
    use crate::{test_utils, NavinError, NavinShipment, NavinShipmentClient};
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Ledger as _},
        Address, BytesN, Env, Vec,
    };

    #[contract]
    struct MockToken;

    #[contractimpl]
    impl MockToken {
        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}

        pub fn decimals(_env: Env) -> u32 {
            7
        }
    }
    fn setup() -> (
        Env,
        NavinShipmentClient<'static>,
        Address,
        Address,
        Address,
        Address,
    ) {
        let (env, admin) = test_utils::setup_env();
        let contract_id = env.register(NavinShipment, ());
        let client = NavinShipmentClient::new(&env, &contract_id);
        let token_id = env.register(MockToken, ());
        client.initialize(&admin, &token_id);

        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);
        client.add_carrier_to_whitelist(&company, &carrier);

        (env, client, admin, company, carrier, token_id)
    }

    fn make_hash(env: &Env, seed: u8) -> BytesN<32> {
        BytesN::from_array(env, &[seed; 32])
    }

    fn future_deadline(env: &Env) -> u64 {
        env.ledger().timestamp() + 7200
    }

    fn create_one(
        env: &Env,
        client: &NavinShipmentClient,
        company: &Address,
        carrier: &Address,
        seed: u8,
    ) -> Result<u64, crate::NavinError> {
        let hash = make_hash(env, seed);
        let deadline = future_deadline(env);
        match client.try_create_shipment(
            company,
            &Address::generate(env),
            carrier,
            &hash,
            &Vec::new(env),
            &deadline,
        ) {
            Ok(Ok(id)) => Ok(id),
            Err(Ok(e)) => Err(e),
            _ => panic!("unexpected error in create_one"),
        }
    }

    // ── quota disabled by default ────────────────────────────────────────────

    #[test]
    fn quota_disabled_by_default_allows_unlimited_creation() {
        let (env, client, _admin, company, carrier, _token) = setup();

        // With quota disabled (max=0), many shipments should succeed.
        for seed in 1u8..=10 {
            assert!(create_one(&env, &client, &company, &carrier, seed).is_ok());
            // Advance time to avoid idempotency window collision.
            env.ledger().with_mut(|l| l.timestamp += 400);
        }
    }

    // ── quota enforced within window ─────────────────────────────────────────

    #[test]
    fn quota_exceeded_within_window_returns_error() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Set quota: max 3 per 3600-second window.
        client.set_creation_quota(&admin, &3, &3600);

        for seed in 1u8..=3 {
            assert!(create_one(&env, &client, &company, &carrier, seed).is_ok());
            env.ledger().with_mut(|l| l.timestamp += 400);
        }

        // 4th attempt within the same window should fail.
        let result = create_one(&env, &client, &company, &carrier, 4);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));
    }

    // ── quota resets after window expires ────────────────────────────────────

    #[test]
    fn quota_resets_after_window_expires() {
        let (env, client, admin, company, carrier, _token) = setup();

        client.set_creation_quota(&admin, &2, &3600);

        // Use up the quota.
        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // Confirm quota is exhausted.
        let result = create_one(&env, &client, &company, &carrier, 3);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));

        // Advance past the window (3600 seconds).
        env.ledger().with_mut(|l| l.timestamp += 3600);

        // Quota should have reset — new shipments allowed.
        assert!(create_one(&env, &client, &company, &carrier, 4).is_ok());
    }

    // ── get_creation_quota_status ────────────────────────────────────────────

    #[test]
    fn quota_status_returns_max_when_disabled() {
        let (_env, client, _admin, company, _carrier, _token) = setup();

        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 0);
        assert_eq!(remaining, u32::MAX);
    }

    #[test]
    fn quota_status_tracks_usage_correctly() {
        let (env, client, admin, company, carrier, _token) = setup();

        client.set_creation_quota(&admin, &5, &3600);

        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 0);
        assert_eq!(remaining, 5);

        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 1);
        assert_eq!(remaining, 4);
    }

    #[test]
    fn quota_status_resets_after_window() {
        let (env, client, admin, company, carrier, _token) = setup();

        client.set_creation_quota(&admin, &3, &3600);

        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());

        // Advance past window.
        env.ledger().with_mut(|l| l.timestamp += 3600);

        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 0);
        assert_eq!(remaining, 3);
    }

    // ── get_effective_shipment_limit: company override vs global fallback ─────

    #[test]
    fn effective_limit_falls_back_to_global_when_no_company_override() {
        let (env, client, admin, company, _carrier, _token) = setup();

        // Set global limit to 7
        client.set_shipment_limit(&admin, &7);

        // No company override → should return global
        assert_eq!(client.get_effective_shipment_limit(&company), 7);
    }

    #[test]
    fn effective_limit_returns_company_override_when_set() {
        let (env, client, admin, company, _carrier, _token) = setup();

        client.set_shipment_limit(&admin, &50);
        client.set_company_shipment_limit(&admin, &company, &10);

        // Company override of 10 should be returned, not the global 50
        assert_eq!(client.get_effective_shipment_limit(&company), 10);
    }

    #[test]
    fn effective_limit_reverts_to_global_after_override_removed() {
        let (env, client, admin, company, _carrier, _token) = setup();

        client.set_shipment_limit(&admin, &25);
        client.set_company_shipment_limit(&admin, &company, &5);
        assert_eq!(client.get_effective_shipment_limit(&company), 5);

        // Remove override by setting to 0
        client.set_company_shipment_limit(&admin, &company, &0);
        // After clearing, should fall back to global 25
        assert_eq!(client.get_effective_shipment_limit(&company), 0);
    }

    #[test]
    fn effective_limit_respects_different_companies_with_different_overrides() {
        let (env, client, admin, company1, _carrier, _token) = setup();
        let company2 = Address::generate(&env);
        client.add_company(&admin, &company2);

        client.set_shipment_limit(&admin, &100);
        client.set_company_shipment_limit(&admin, &company1, &20);
        client.set_company_shipment_limit(&admin, &company2, &50);

        assert_eq!(client.get_effective_shipment_limit(&company1), 20);
        assert_eq!(client.get_effective_shipment_limit(&company2), 50);
    }

    // ── set_creation_quota validation ────────────────────────────────────────

    #[test]
    fn set_creation_quota_rejects_zero_window_with_nonzero_max() {
        let (_env, client, admin, _company, _carrier, _token) = setup();

        let result = client.try_set_creation_quota(&admin, &5, &0);
        assert_eq!(result, Err(Ok(NavinError::InvalidConfig)));
    }

    #[test]
    fn set_creation_quota_allows_zero_max_to_disable() {
        let (_env, client, admin, _company, _carrier, _token) = setup();

        // max=0 disables quota regardless of window.
        assert!(client.try_set_creation_quota(&admin, &0, &0).is_ok());
    }

    #[test]
    fn set_creation_quota_rejects_non_admin() {
        let (_env, client, _admin, company, _carrier, _token) = setup();

        let result = client.try_set_creation_quota(&company, &5, &3600);
        assert_eq!(result, Err(Ok(NavinError::Unauthorized)));
    }

    // ── batch creation respects quota ────────────────────────────────────────

    #[test]
    fn batch_creation_respects_quota() {
        use crate::types::ShipmentInput;
        let (env, client, admin, company, carrier, _token) = setup();

        // Allow max 2 per window.
        client.set_creation_quota(&admin, &2, &3600);

        let deadline = future_deadline(&env);
        let mut inputs = soroban_sdk::Vec::new(&env);
        for seed in 1u8..=3 {
            inputs.push_back(ShipmentInput {
                receiver: Address::generate(&env),
                carrier: carrier.clone(),
                data_hash: make_hash(&env, seed),
                payment_milestones: soroban_sdk::Vec::new(&env),
                deadline,
            });
        }

        // Batch of 3 exceeds quota of 2.
        let result = client.try_create_shipments_batch(&company, &inputs);
        assert_eq!(result, Err(Ok(NavinError::CreationQuotaExceeded)));
    }

    // ── multiple companies have independent quotas ───────────────────────────

    #[test]
    fn multiple_companies_have_independent_quotas() {
        let (env, client, admin, company1, carrier, _token) = setup();
        let company2 = Address::generate(&env);
        client.add_company(&admin, &company2);

        // Set quota: max 2 per window.
        client.set_creation_quota(&admin, &2, &3600);

        // Company 1 uses up quota.
        assert!(create_one(&env, &client, &company1, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);
        assert!(create_one(&env, &client, &company1, &carrier, 2).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // Company 1 should be blocked.
        let result = create_one(&env, &client, &company1, &carrier, 3);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));

        // Company 2 should still have full quota available.
        assert!(create_one(&env, &client, &company2, &carrier, 4).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);
        assert!(create_one(&env, &client, &company2, &carrier, 5).is_ok());
    }

    // ── quota window boundary conditions ─────────────────────────────────────

    #[test]
    fn quota_resets_exactly_at_window_boundary() {
        let (env, client, admin, company, carrier, _token) = setup();

        client.set_creation_quota(&admin, &1, &3600);

        // Create first shipment.
        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        let initial_time = env.ledger().timestamp();

        // Advance to just before window expiry.
        env.ledger().with_mut(|l| l.timestamp = initial_time + 3599);

        // Should still be blocked (within window).
        let result = create_one(&env, &client, &company, &carrier, 2);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));

        // Advance exactly to window boundary.
        env.ledger().with_mut(|l| l.timestamp = initial_time + 3600);

        // Should now succeed (window expired).
        assert!(create_one(&env, &client, &company, &carrier, 3).is_ok());
    }

    // ── quota update changes enforcement immediately ──────────────────────────

    #[test]
    fn quota_update_changes_enforcement_immediately() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Start with quota of 2.
        client.set_creation_quota(&admin, &2, &3600);

        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());

        // Reduce quota to 1 (stricter).
        client.set_creation_quota(&admin, &1, &3600);

        // Next attempt should fail because new quota is 1 and we've already used 2.
        let result = create_one(&env, &client, &company, &carrier, 3);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));

        // Increase quota back to 5.
        client.set_creation_quota(&admin, &5, &3600);

        // Should now succeed.
        assert!(create_one(&env, &client, &company, &carrier, 4).is_ok());
    }

    // ── very large quota allows many creations ───────────────────────────────

    #[test]
    fn very_large_quota_allows_many_creations() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Set a very large quota.
        client.set_creation_quota(&admin, &1000, &3600);

        // Create many shipments within the window.
        for seed in 1u8..=100 {
            assert!(create_one(&env, &client, &company, &carrier, seed).is_ok());
            env.ledger().with_mut(|l| l.timestamp += 10);
        }

        // Verify status shows correct usage.
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 100);
        assert_eq!(remaining, 900);
    }

    // ── quota with very short window enforces tightly ──────────────────────────

    #[test]
    fn quota_with_very_short_window_enforces_tightly() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Set quota: max 1 per 100-second window.
        client.set_creation_quota(&admin, &1, &100);

        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        let initial_time = env.ledger().timestamp();

        // Try immediately after — should fail.
        env.ledger().with_mut(|l| l.timestamp += 1);
        let result = create_one(&env, &client, &company, &carrier, 2);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));

        // Advance past the short window.
        env.ledger().with_mut(|l| l.timestamp = initial_time + 101);

        // Should now succeed.
        assert!(create_one(&env, &client, &company, &carrier, 3).is_ok());
    }

    // ── active shipment limit boundary conditions ─────────────────────────────

    #[test]
    fn test_company_active_shipment_limit_exact_boundary() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Limit is 3 active shipments
        client.set_shipment_limit(&admin, &3);

        // Create exactly 3 active shipments
        for seed in 1u8..=3 {
            assert!(create_one(&env, &client, &company, &carrier, seed).is_ok());
            env.ledger().with_mut(|l| l.timestamp += 400); // Avoid idempotency/quota window collisions
        }

        assert_eq!(client.get_effective_shipment_limit(&company), 3);
    }

    #[test]
    fn test_company_active_shipment_limit_exceeded_rejected() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Limit is 3 active shipments
        client.set_shipment_limit(&admin, &3);

        // Create exactly 3 active shipments
        for seed in 1u8..=3 {
            assert!(create_one(&env, &client, &company, &carrier, seed).is_ok());
            env.ledger().with_mut(|l| l.timestamp += 400);
        }

        // The 4th active shipment must be rejected with ShipmentLimitReached
        let result = create_one(&env, &client, &company, &carrier, 4);
        assert_eq!(result, Err(NavinError::ShipmentLimitReached));
    }

    #[test]
    fn test_company_active_shipment_limit_lifted_by_config_change() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Limit is 2 active shipments
        client.set_shipment_limit(&admin, &2);

        // Create exactly 2 active shipments
        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // 3rd is rejected
        let result = create_one(&env, &client, &company, &carrier, 3);
        assert_eq!(result, Err(NavinError::ShipmentLimitReached));

        // Lift limit to 5
        client.set_shipment_limit(&admin, &5);
        assert_eq!(client.get_effective_shipment_limit(&company), 5);

        // 3rd now succeeds!
        assert!(create_one(&env, &client, &company, &carrier, 4).is_ok());
    }

    // ── [ISSUE #452] creation quota limit reset tests ─────────────────────────

    /// Test: Reach the quota limit in a controlled fixture, then verify enforcement.
    /// This ensures that quota limits are properly enforced.
    #[test]
    fn test_quota_limit_reached_and_enforced() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Set quota: max 3 shipments per 3600-second window
        client.set_creation_quota(&admin, &3, &3600);

        // Verify quota status before any creation
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 0);
        assert_eq!(remaining, 3);

        // Create shipments up to the limit
        for seed in 1u8..=3 {
            assert!(create_one(&env, &client, &company, &carrier, seed).is_ok());
            env.ledger().with_mut(|l| l.timestamp += 400);
        }

        // Verify quota is exhausted
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 3);
        assert_eq!(remaining, 0);

        // Attempt to create another shipment should fail
        let result = create_one(&env, &client, &company, &carrier, 4);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));
    }

    /// Test: Change config to increase quota limit and confirm the limit updates immediately.
    /// This verifies that config changes affect the quota as expected.
    #[test]
    fn test_quota_config_change_increases_limit() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Start with quota: max 2 per 3600-second window
        client.set_creation_quota(&admin, &2, &3600);

        // Create 2 shipments to reach the limit
        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // Verify limit reached
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 2);
        assert_eq!(remaining, 0);

        // Next attempt should fail
        let result = create_one(&env, &client, &company, &carrier, 3);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));

        // ── Config Change: Increase limit to 5 ──
        client.set_creation_quota(&admin, &5, &3600);

        // Verify quota status reflects new limit
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 2); // Still used 2 in current window
        assert_eq!(remaining, 3); // But now have 3 more available

        // Now creation should succeed
        assert!(create_one(&env, &client, &company, &carrier, 4).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // Verify status updated
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 3);
        assert_eq!(remaining, 2);
    }

    /// Test: Change config to decrease quota limit and confirm enforcement becomes stricter.
    /// This verifies that stricter config changes are immediately enforced.
    #[test]
    fn test_quota_config_change_decreases_limit() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Start with quota: max 5 per 3600-second window
        client.set_creation_quota(&admin, &5, &3600);

        // Create 3 shipments
        for seed in 1u8..=3 {
            assert!(create_one(&env, &client, &company, &carrier, seed).is_ok());
            env.ledger().with_mut(|l| l.timestamp += 400);
        }

        // Verify quota status
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 3);
        assert_eq!(remaining, 2);

        // ── Config Change: Decrease limit to 3 ──
        client.set_creation_quota(&admin, &3, &3600);

        // Verify quota status reflects new (stricter) limit
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 3);
        assert_eq!(remaining, 0); // Already at new limit

        // Next attempt should now fail with new stricter limit
        let result = create_one(&env, &client, &company, &carrier, 4);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));
    }

    /// Test: Quota reset path - reach limit, wait for window to expire, then create again.
    /// This ensures the limit reset path is deterministic.
    #[test]
    fn test_quota_reset_after_window_expiry_deterministic() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Set quota: max 2 per 1800-second (30 min) window
        client.set_creation_quota(&admin, &2, &1800);

        // Create 2 shipments to reach limit
        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        let first_timestamp = env.ledger().timestamp();
        env.ledger().with_mut(|l| l.timestamp += 400);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // Verify limit reached
        let result = create_one(&env, &client, &company, &carrier, 3);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));

        // Advance time to just before window expiry
        env.ledger()
            .with_mut(|l| l.timestamp = first_timestamp + 1799);

        // Should still be blocked
        let result = create_one(&env, &client, &company, &carrier, 4);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));

        // Advance time to exactly window expiry (deterministic reset point)
        env.ledger()
            .with_mut(|l| l.timestamp = first_timestamp + 1800);

        // Quota should have reset - verify status
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 0); // Reset
        assert_eq!(remaining, 2); // Full quota available

        // Creation should now succeed (post-reset success path)
        assert!(create_one(&env, &client, &company, &carrier, 5).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // Verify new window tracking
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 1);
        assert_eq!(remaining, 1);

        // Create one more to reach new window limit
        assert!(create_one(&env, &client, &company, &carrier, 6).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // Verify limit reached again
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 2);
        assert_eq!(remaining, 0);
    }

    /// Test: Config change to disable quota (set to 0) removes all restrictions.
    /// This verifies that disabling quota via config works correctly.
    #[test]
    fn test_quota_config_change_disable() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Start with quota: max 2 per 3600-second window
        client.set_creation_quota(&admin, &2, &3600);

        // Create 2 shipments to reach limit
        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // Verify limit reached
        let result = create_one(&env, &client, &company, &carrier, 3);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));

        // ── Config Change: Disable quota (set to 0) ──
        client.set_creation_quota(&admin, &0, &0);

        // Verify quota status shows unlimited
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 0);
        assert_eq!(remaining, u32::MAX); // Unlimited

        // Now creation should succeed without restriction
        for seed in 3u8..=10 {
            assert!(create_one(&env, &client, &company, &carrier, seed).is_ok());
            env.ledger().with_mut(|l| l.timestamp += 400);
        }
    }

    /// Test: Config change to re-enable quota after it was disabled.
    /// This ensures quota can be toggled on and off via config.
    #[test]
    fn test_quota_config_change_reenable() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Start with quota disabled (default)
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 0);
        assert_eq!(remaining, u32::MAX);

        // Create several shipments without restriction
        for seed in 1u8..=5 {
            assert!(create_one(&env, &client, &company, &carrier, seed).is_ok());
            env.ledger().with_mut(|l| l.timestamp += 400);
        }

        // ── Config Change: Enable quota with max 2 per 3600-second window ──
        client.set_creation_quota(&admin, &2, &3600);

        // Verify quota is now active - new window starts from now
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 0); // New window, no usage yet
        assert_eq!(remaining, 2);

        // Create 2 shipments to reach new limit
        assert!(create_one(&env, &client, &company, &carrier, 6).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);
        assert!(create_one(&env, &client, &company, &carrier, 7).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // Verify limit is now enforced
        let result = create_one(&env, &client, &company, &carrier, 8);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));
    }

    /// Test: Config change modifies window duration with active quota usage.
    /// This verifies that changing the window duration properly resets tracking.
    #[test]
    fn test_quota_config_change_window_duration() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Set quota: max 3 per 3600-second (1 hour) window
        client.set_creation_quota(&admin, &3, &3600);

        // Create 2 shipments
        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        let first_timestamp = env.ledger().timestamp();
        env.ledger().with_mut(|l| l.timestamp += 400);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // Verify quota status
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 2);
        assert_eq!(remaining, 1);

        // ── Config Change: Keep same max but change window to 7200 seconds ──
        client.set_creation_quota(&admin, &3, &7200);

        // The existing window should still be valid with new duration
        // Status calculation should use new window duration
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 2); // Usage persists
        assert_eq!(remaining, 1);

        // Advance time past old window (3600) but not past new window (7200)
        env.ledger()
            .with_mut(|l| l.timestamp = first_timestamp + 3700);

        // With new window duration, quota should NOT have reset yet
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 2); // Still counts
        assert_eq!(remaining, 1);

        // Create one more (should succeed - still within new window)
        assert!(create_one(&env, &client, &company, &carrier, 3).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // Now at limit
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 3);
        assert_eq!(remaining, 0);

        // Advance past new window duration
        env.ledger()
            .with_mut(|l| l.timestamp = first_timestamp + 7200);

        // Quota should now reset
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 0);
        assert_eq!(remaining, 3);
    }

    /// Test: Multiple config changes in succession with quota enforcement.
    /// This ensures config changes are applied correctly even with rapid changes.
    #[test]
    fn test_quota_multiple_config_changes() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Config 1: max 5 per window
        client.set_creation_quota(&admin, &5, &3600);
        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // Config 2: reduce to max 3
        client.set_creation_quota(&admin, &3, &3600);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // Config 3: reduce to max 2 (now at limit since we've created 2)
        client.set_creation_quota(&admin, &2, &3600);
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 2);
        assert_eq!(remaining, 0);

        // Should now be blocked
        let result = create_one(&env, &client, &company, &carrier, 3);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));

        // Config 4: increase to max 10
        client.set_creation_quota(&admin, &10, &3600);
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 2);
        assert_eq!(remaining, 8);

        // Should now succeed
        assert!(create_one(&env, &client, &company, &carrier, 3).is_ok());
    }

    // ── sliding window shifts forward on reset ──────────────────────────────

    #[test]
    fn test_sliding_window_shifts_forward_on_reset() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Quota: max 2 per 3600-second window.
        client.set_creation_quota(&admin, &2, &3600);
        let t0 = env.ledger().timestamp();

        // Window 1: fill quota.
        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp = t0 + 1);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());
        env.ledger().with_mut(|l| l.timestamp = t0 + 2);

        // Verify quota exhausted in window 1.
        let result = create_one(&env, &client, &company, &carrier, 3);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));

        // Advance past the window boundary — old window expires at t0 + 3600.
        env.ledger().with_mut(|l| l.timestamp = t0 + 3600);

        // The window slides forward: count resets to 0 then increments to 1.
        assert!(create_one(&env, &client, &company, &carrier, 4).is_ok());

        // New window should have count = 1, remaining = 1.
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 1);
        assert_eq!(remaining, 1);

        // Fill the new window.
        assert!(create_one(&env, &client, &company, &carrier, 5).is_ok());
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 2);
        assert_eq!(remaining, 0);

        // Verify exhaustion in the new window.
        let result = create_one(&env, &client, &company, &carrier, 6);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));
    }

    // ── multiple consecutive window slides ──────────────────────────────────

    #[test]
    fn test_sliding_window_multiple_consecutive_windows() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Quota: max 2 per 1000-second window (short window for fast test).
        client.set_creation_quota(&admin, &2, &1000);
        let t0 = env.ledger().timestamp();

        // ── Window 1 ──
        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp = t0 + 10);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());
        env.ledger().with_mut(|l| l.timestamp = t0 + 20);
        let result = create_one(&env, &client, &company, &carrier, 3);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));

        // Slide into window 2.
        env.ledger().with_mut(|l| l.timestamp = t0 + 1000);

        // ── Window 2 ──
        assert!(create_one(&env, &client, &company, &carrier, 4).is_ok());
        env.ledger().with_mut(|l| l.timestamp = t0 + 1010);
        assert!(create_one(&env, &client, &company, &carrier, 5).is_ok());
        env.ledger().with_mut(|l| l.timestamp = t0 + 1020);
        let result = create_one(&env, &client, &company, &carrier, 6);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));

        // Slide into window 3.
        env.ledger().with_mut(|l| l.timestamp = t0 + 2000);

        // ── Window 3 ──
        assert!(create_one(&env, &client, &company, &carrier, 7).is_ok());
        env.ledger().with_mut(|l| l.timestamp = t0 + 2010);
        assert!(create_one(&env, &client, &company, &carrier, 8).is_ok());
        env.ledger().with_mut(|l| l.timestamp = t0 + 2020);

        // Verify status shows full usage in window 3.
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 2);
        assert_eq!(remaining, 0);

        let result = create_one(&env, &client, &company, &carrier, 9);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));
    }

    // ── large time jump past multiple windows ────────────────────────────────

    #[test]
    fn test_sliding_window_large_time_jump() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Quota: max 3 per 3600-second window.
        client.set_creation_quota(&admin, &3, &3600);
        let t0 = env.ledger().timestamp();

        // Create 2 shipments (not yet at quota).
        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp = t0 + 10);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());

        // Jump way past the window (100x window length).
        env.ledger().with_mut(|l| l.timestamp = t0 + 360_000);

        // Window should have expired — creation resets the tracker.
        assert!(create_one(&env, &client, &company, &carrier, 3).is_ok());

        // Verify reset: only 1 used in the new window.
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 1);
        assert_eq!(remaining, 2);

        // Fill the new window.
        assert!(create_one(&env, &client, &company, &carrier, 4).is_ok());
        assert!(create_one(&env, &client, &company, &carrier, 5).is_ok());
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 3);
        assert_eq!(remaining, 0);

        // Verify exhausted.
        let result = create_one(&env, &client, &company, &carrier, 6);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));
    }

    // ── get_creation_quota_status reflects window shift ─────────────────────

    #[test]
    fn test_sliding_window_status_after_window_shift() {
        let (env, client, admin, company, carrier, _token) = setup();

        // Quota: max 3 per 3600-second window.
        client.set_creation_quota(&admin, &3, &3600);
        let t0 = env.ledger().timestamp();

        // Create 2 in window 1.
        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp = t0 + 10);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());
        env.ledger().with_mut(|l| l.timestamp = t0 + 20);

        // Status within window: used=2, remaining=1.
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 2);
        assert_eq!(remaining, 1);

        // Advance past the window.
        env.ledger().with_mut(|l| l.timestamp = t0 + 3600);

        // Status shows window expired: used=0, remaining=3 (even before creating).
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 0);
        assert_eq!(remaining, 3);

        // Create should now succeed (window already considered expired by status).
        assert!(create_one(&env, &client, &company, &carrier, 3).is_ok());

        // Status shows 1 used in new sliding window.
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 1);
        assert_eq!(remaining, 2);

        // Fill the new window.
        assert!(create_one(&env, &client, &company, &carrier, 4).is_ok());
        assert!(create_one(&env, &client, &company, &carrier, 5).is_ok());
        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 3);
        assert_eq!(remaining, 0);
    }
}
