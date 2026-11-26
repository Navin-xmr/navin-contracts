use crate::test_helpers::*;
use crate::NavinError;

// ── Issue #434: Settlement counter overflow protection tests ──────────────────

/// Seed the counter near its upper bound and verify each increment produces
/// the expected deterministic value — counter growth must be safe and monotonic.
#[test]
fn test_settlement_counter_near_boundary_increments_safely() {
    let (env, client, _admin, _token_contract) = setup_initialized_shipment_env();

    // Seed the counter to u64::MAX - 2 via internal storage so we can observe
    // the last two safe increments without requiring many settlement operations.
    let near_max: u64 = u64::MAX - 2;
    env.as_contract(&client.address, || {
        crate::storage::set_settlement_counter(&env, near_max);
    });

    // Verify the seeded value is reflected in the public query.
    assert_eq!(client.get_settlement_count(), near_max);

    // First increment: near_max + 1  — must succeed.
    env.as_contract(&client.address, || {
        let next = crate::storage::increment_settlement_counter(&env).unwrap();
        assert_eq!(next, near_max + 1);
    });
    assert_eq!(client.get_settlement_count(), near_max + 1);

    // Second increment: near_max + 2 = u64::MAX - 1 — still within bounds.
    env.as_contract(&client.address, || {
        let next = crate::storage::increment_settlement_counter(&env).unwrap();
        assert_eq!(next, near_max + 2);
    });
    assert_eq!(client.get_settlement_count(), near_max + 2);
}

/// Boundary test: at exactly u64::MAX the counter must not wrap or reuse the
/// last ID. `checked_add(1)` returns `None` at saturation, so the increment
/// fails with `CounterOverflow` and the stored counter stays at u64::MAX.
#[test]
fn test_settlement_counter_saturates_at_u64_max() {
    let (env, client, _admin, _token_contract) = setup_initialized_shipment_env();

    // Seed counter at the absolute maximum.
    env.as_contract(&client.address, || {
        crate::storage::set_settlement_counter(&env, u64::MAX);
    });

    assert_eq!(client.get_settlement_count(), u64::MAX);

    // Attempting to increment at saturation must fail instead of handing out
    // the already-used ID u64::MAX again.
    env.as_contract(&client.address, || {
        let result = crate::storage::increment_settlement_counter(&env);
        assert_eq!(result, Err(NavinError::CounterOverflow));
    });

    // Public count must still reflect u64::MAX, not 0.
    assert_eq!(client.get_settlement_count(), u64::MAX);
}

/// Failure-path assertion: verifies that neither the rollover sentinel (0
/// after MAX) nor a reused ID is reachable via the contract's increment path.
/// This test must fail if the overflow guard (`checked_add`) is ever removed.
#[test]
fn test_settlement_counter_rollover_is_impossible() {
    let (env, client, _admin, _token_contract) = setup_initialized_shipment_env();

    env.as_contract(&client.address, || {
        crate::storage::set_settlement_counter(&env, u64::MAX);
        let after_increment = crate::storage::increment_settlement_counter(&env);
        // The only acceptable post-MAX outcome is `CounterOverflow`. An `Ok`
        // means the counter either wrapped or reused an existing ID.
        assert_eq!(
            after_increment,
            Err(NavinError::CounterOverflow),
            "Counter must reject increments past u64::MAX"
        );
        assert_eq!(crate::storage::get_settlement_counter(&env), u64::MAX);
    });
}
