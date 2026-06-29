/// Unit tests for ActorQuota sliding window replenishment mechanism.
///
/// This module verifies that the ActorQuota sliding window tracker correctly
/// replenishes available quotas when queried after the window duration has
/// fully reset in the ledger clock.

#[cfg(test)]
mod actor_quota_tests {
    use crate::rate_limit::{QuotaTracker, RateLimitConfig};

    /// Test that ActorQuota replenishes when window expires with exact boundary crossing.
    ///
    /// This test verifies that when an actor hits their quota limit and then
    /// sufficient time passes (>= window_seconds), the quota resets and the
    /// actor can perform operations again.
    #[test]
    fn test_actor_quota_replenishment_on_window_expiry() {
        // Setup: Create a rate limit config with 100 ops per 3600 second window
        let config = RateLimitConfig::default();
        assert_eq!(config.max_operations, 100);
        assert_eq!(config.window_seconds, 3600);

        // Create initial quota tracker at time 0
        let mut tracker = QuotaTracker::new(0);
        assert_eq!(tracker.operations_count, 0);
        assert_eq!(tracker.window_start, 0);

        // Simulate operations up to quota limit
        for _ in 0..config.max_operations {
            tracker
                .check_and_update(1, config.max_operations, config.window_seconds, 0)
                .expect("operations within limit should succeed");
        }

        // Verify quota is exhausted
        assert_eq!(tracker.operations_count, config.max_operations);
        assert_eq!(tracker.remaining_quota(&config), 0);

        // Attempt operation at boundary (window_start + window_seconds)
        let boundary_time = tracker.window_start + config.window_seconds;
        let result = tracker.check_and_update(
            1,
            config.max_operations,
            config.window_seconds,
            boundary_time,
        );

        // Should succeed because window has expired exactly at boundary
        assert!(
            result.is_ok(),
            "operation at window boundary should succeed"
        );

        // Verify window was reset and operations count cleared
        assert_eq!(tracker.window_start, boundary_time);
        assert_eq!(tracker.operations_count, 1); // One operation performed
        assert_eq!(tracker.remaining_quota(&config), config.max_operations - 1);
    }

    /// Test that ActorQuota replenishment works after window duration fully resets.
    ///
    /// Verifies that when the ledger clock advances significantly past the window
    /// expiry, the quota is properly replenished and operations can resume.
    #[test]
    fn test_actor_quota_replenishment_after_window_fully_reset() {
        let config = RateLimitConfig::default();

        // Create tracker at time 1000
        let mut tracker = QuotaTracker::new(1000);

        // Fill quota to limit
        for _ in 0..config.max_operations {
            tracker
                .check_and_update(1, config.max_operations, config.window_seconds, 1000)
                .expect("operations should succeed");
        }

        // Verify exhausted
        assert_eq!(tracker.operations_count, config.max_operations);
        assert_eq!(tracker.remaining_quota(&config), 0);

        // Advance time well past window expiry (add 2x window duration)
        let far_future = 1000 + (config.window_seconds * 2);

        // Perform operation at far future time
        let result =
            tracker.check_and_update(1, config.max_operations, config.window_seconds, far_future);

        assert!(
            result.is_ok(),
            "operation after window fully reset should succeed"
        );
        assert_eq!(tracker.window_start, far_future);
        assert_eq!(tracker.operations_count, 1);
    }

    /// Test that multiple operations succeed after replenishment.
    ///
    /// Ensures that after a quota window resets, the actor can perform multiple
    /// operations up to the new quota limit.
    #[test]
    fn test_multiple_operations_after_quota_replenishment() {
        let config = RateLimitConfig::default();

        let mut tracker = QuotaTracker::new(0);

        // Exhaust initial quota
        for _ in 0..config.max_operations {
            tracker
                .check_and_update(1, config.max_operations, config.window_seconds, 0)
                .expect("should succeed");
        }

        // Verify exhausted
        assert_eq!(tracker.remaining_quota(&config), 0);

        // Move past window expiry
        let new_window_start = config.window_seconds + 1;

        // Perform multiple operations in new window
        let ops_count = 50;
        for i in 0..ops_count {
            let result = tracker.check_and_update(
                1,
                config.max_operations,
                config.window_seconds,
                new_window_start,
            );
            assert!(
                result.is_ok(),
                "operation {} in replenished window should succeed",
                i + 1
            );
        }

        // Verify state
        assert_eq!(tracker.window_start, new_window_start);
        assert_eq!(tracker.operations_count, ops_count);
        assert_eq!(
            tracker.remaining_quota(&config),
            config.max_operations - ops_count
        );
    }

    /// Test quota replenishment with different rate limit configurations.
    ///
    /// Verifies that replenishment works correctly with strict, default, and
    /// permissive rate limit configurations.
    #[test]
    fn test_quota_replenishment_with_different_configs() {
        let configs = vec![
            ("strict", RateLimitConfig::strict()),
            ("default", RateLimitConfig::default()),
            ("permissive", RateLimitConfig::permissive()),
        ];

        for (config_name, config) in configs {
            // Create and exhaust quota
            let mut tracker = QuotaTracker::new(0);

            for _ in 0..config.max_operations {
                tracker
                    .check_and_update(1, config.max_operations, config.window_seconds, 0)
                    .expect(&format!("{} exhaustion failed", config_name));
            }

            // Move past window and try operation
            let new_time = config.window_seconds + 1;
            let result =
                tracker.check_and_update(1, config.max_operations, config.window_seconds, new_time);

            assert!(
                result.is_ok(),
                "{} config replenishment should succeed",
                config_name
            );
            assert_eq!(
                tracker.operations_count, 1,
                "{} config should reset counter",
                config_name
            );
        }
    }

    /// Test that quota does not replenish if window has not fully expired.
    ///
    /// Verifies that the rate limiting correctly prevents operations when the
    /// window duration has not yet elapsed, even with just 1 second remaining.
    #[test]
    fn test_quota_replenishment_blocked_before_window_expires() {
        let config = RateLimitConfig::default();

        let mut tracker = QuotaTracker::new(0);

        // Exhaust quota
        for _ in 0..config.max_operations {
            tracker
                .check_and_update(1, config.max_operations, config.window_seconds, 0)
                .expect("exhaustion should succeed");
        }

        // Try operation just before window expires
        let almost_expired = 0 + config.window_seconds - 1;
        let result = tracker.check_and_update(
            1,
            config.max_operations,
            config.window_seconds,
            almost_expired,
        );

        // Should fail - window not yet expired
        assert!(
            result.is_err(),
            "operation before window expires should fail"
        );
        assert_eq!(
            tracker.operations_count, config.max_operations,
            "operations_count should not change"
        );
        assert_eq!(tracker.window_start, 0, "window_start should not change");
    }

    /// Test sequential quota exhaustion and replenishment cycles.
    ///
    /// Verifies that an actor can go through multiple cycles of exhausting
    /// quota and replenishing after window reset.
    #[test]
    fn test_sequential_quota_cycles() {
        let config = RateLimitConfig::default();

        let mut tracker = QuotaTracker::new(0);
        let cycle_count = 3;

        for cycle in 0..cycle_count {
            // Exhaust quota in this cycle
            for _ in 0..config.max_operations {
                let current_time = cycle as u64 * (config.window_seconds + 1);
                tracker
                    .check_and_update(
                        1,
                        config.max_operations,
                        config.window_seconds,
                        current_time,
                    )
                    .expect(&format!("cycle {} exhaustion should succeed", cycle));
            }

            // Verify exhausted
            assert_eq!(
                tracker.remaining_quota(&config),
                0,
                "cycle {} should be exhausted",
                cycle
            );

            // Move to next window
            let next_cycle_time = (cycle as u64 + 1) * (config.window_seconds + 1);
            let result = tracker.check_and_update(
                1,
                config.max_operations,
                config.window_seconds,
                next_cycle_time,
            );

            assert!(
                result.is_ok(),
                "cycle {} replenishment should succeed",
                cycle
            );
        }

        // Final state should show successful cycle completion
        assert_eq!(
            tracker.operations_count, 1,
            "final cycle should have 1 operation"
        );
    }

    /// Test window time remaining calculation after replenishment.
    ///
    /// Verifies that `window_time_remaining()` correctly reports the time
    /// remaining in the current window after a quota replenishment.
    #[test]
    fn test_window_time_remaining_after_replenishment() {
        let config = RateLimitConfig::default();

        let mut tracker = QuotaTracker::new(1000);

        // Exhaust quota
        for _ in 0..config.max_operations {
            tracker
                .check_and_update(1, config.max_operations, config.window_seconds, 1000)
                .expect("exhaustion should succeed");
        }

        // Move to new window
        let new_window_start = 1000 + config.window_seconds + 1;
        tracker
            .check_and_update(
                1,
                config.max_operations,
                config.window_seconds,
                new_window_start,
            )
            .expect("replenishment should succeed");

        // Check time remaining at various points in new window
        let time_at_start = new_window_start;
        let remaining_at_start = tracker.window_time_remaining(time_at_start, &config);
        assert_eq!(
            remaining_at_start, config.window_seconds,
            "time remaining at window start should equal window_seconds"
        );

        let time_at_mid = new_window_start + (config.window_seconds / 2);
        let remaining_at_mid = tracker.window_time_remaining(time_at_mid, &config);
        assert_eq!(
            remaining_at_mid,
            config.window_seconds / 2,
            "time remaining at window midpoint should be half"
        );

        let time_at_end = new_window_start + config.window_seconds - 1;
        let remaining_at_end = tracker.window_time_remaining(time_at_end, &config);
        assert_eq!(
            remaining_at_end, 1,
            "time remaining at window end should be 1"
        );
    }

    /// Test quota replenishment with zero-value operations.
    ///
    /// Ensures that the replenishment logic is robust when edge cases like
    /// attempting zero-value operations occur.
    #[test]
    fn test_quota_replenishment_robustness_zero_ops() {
        let config = RateLimitConfig::default();

        let mut tracker = QuotaTracker::new(0);

        // Exhaust quota with standard operations
        for _ in 0..config.max_operations {
            tracker
                .check_and_update(1, config.max_operations, config.window_seconds, 0)
                .expect("should succeed");
        }

        // Move past window and attempt zero-value operation
        let new_time = config.window_seconds + 1;
        let result =
            tracker.check_and_update(0, config.max_operations, config.window_seconds, new_time);

        // Zero-value operation should succeed and not consume quota
        assert!(result.is_ok(), "zero-value operation should succeed");
        assert_eq!(
            tracker.operations_count, 0,
            "zero-value operation should not increment counter"
        );
    }
}
