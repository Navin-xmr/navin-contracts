//! # i128 Boundary Tests for Escrow Arithmetic Guards
//!
//! Validates that the checked arithmetic helpers (`checked_add_i128`,
//! `checked_sub_i128`, `checked_mul_div_i128`) correctly return
//! `NavinError::ArithmeticError` at the i128 boundaries and succeed
//! for representable values.

use crate::errors::NavinError;
use crate::{checked_add_i128, checked_mul_div_i128, checked_sub_i128};

// ── checked_add_i128 ─────────────────────────────────────────────────────────

#[test]
fn test_checked_add_zero_plus_zero() {
    assert_eq!(checked_add_i128(0, 0), Ok(0));
}

#[test]
fn test_checked_add_positive_values() {
    assert_eq!(checked_add_i128(100, 200), Ok(300));
}

#[test]
fn test_checked_add_max_plus_zero() {
    assert_eq!(checked_add_i128(i128::MAX, 0), Ok(i128::MAX));
}

#[test]
fn test_checked_add_max_plus_one_overflows() {
    assert_eq!(
        checked_add_i128(i128::MAX, 1),
        Err(NavinError::ArithmeticError),
        "i128::MAX + 1 must overflow"
    );
}

#[test]
fn test_checked_add_max_plus_max_overflows() {
    assert_eq!(
        checked_add_i128(i128::MAX, i128::MAX),
        Err(NavinError::ArithmeticError),
        "i128::MAX + i128::MAX must overflow"
    );
}

#[test]
fn test_checked_add_min_plus_negative_one_overflows() {
    assert_eq!(
        checked_add_i128(i128::MIN, -1),
        Err(NavinError::ArithmeticError),
        "i128::MIN + (-1) must underflow"
    );
}

#[test]
fn test_checked_add_min_plus_zero() {
    assert_eq!(checked_add_i128(i128::MIN, 0), Ok(i128::MIN));
}

#[test]
fn test_checked_add_negative_values() {
    assert_eq!(checked_add_i128(-50, -30), Ok(-80));
}

#[test]
fn test_checked_add_positive_and_negative_cancel() {
    assert_eq!(checked_add_i128(i128::MAX, i128::MIN), Ok(-1));
}

// ── checked_sub_i128 ─────────────────────────────────────────────────────────

#[test]
fn test_checked_sub_zero_minus_zero() {
    assert_eq!(checked_sub_i128(0, 0), Ok(0));
}

#[test]
fn test_checked_sub_positive_values() {
    assert_eq!(checked_sub_i128(300, 100), Ok(200));
}

#[test]
fn test_checked_sub_min_minus_one_overflows() {
    assert_eq!(
        checked_sub_i128(i128::MIN, 1),
        Err(NavinError::ArithmeticError),
        "i128::MIN - 1 must underflow"
    );
}

#[test]
fn test_checked_sub_max_minus_negative_one_overflows() {
    assert_eq!(
        checked_sub_i128(i128::MAX, -1),
        Err(NavinError::ArithmeticError),
        "i128::MAX - (-1) must overflow"
    );
}

#[test]
fn test_checked_sub_min_minus_zero() {
    assert_eq!(checked_sub_i128(i128::MIN, 0), Ok(i128::MIN));
}

#[test]
fn test_checked_sub_max_minus_zero() {
    assert_eq!(checked_sub_i128(i128::MAX, 0), Ok(i128::MAX));
}

#[test]
fn test_checked_sub_max_minus_max() {
    assert_eq!(checked_sub_i128(i128::MAX, i128::MAX), Ok(0));
}

#[test]
fn test_checked_sub_min_minus_min() {
    assert_eq!(checked_sub_i128(i128::MIN, i128::MIN), Ok(0));
}

// ── checked_mul_div_i128 ─────────────────────────────────────────────────────

#[test]
fn test_checked_mul_div_basic() {
    // 100 * 50 / 100 = 50
    assert_eq!(checked_mul_div_i128(100, 50, 100), Ok(50));
}

#[test]
fn test_checked_mul_div_divide_by_zero() {
    assert_eq!(
        checked_mul_div_i128(100, 50, 0),
        Err(NavinError::ArithmeticError),
        "Division by zero must return ArithmeticError"
    );
}

#[test]
fn test_checked_mul_div_max_times_one_over_one() {
    assert_eq!(checked_mul_div_i128(i128::MAX, 1, 1), Ok(i128::MAX));
}

#[test]
fn test_checked_mul_div_max_times_two_overflows() {
    assert_eq!(
        checked_mul_div_i128(i128::MAX, 2, 1),
        Err(NavinError::ArithmeticError),
        "i128::MAX * 2 must overflow before division"
    );
}

#[test]
fn test_checked_mul_div_max_times_max_overflows() {
    assert_eq!(
        checked_mul_div_i128(i128::MAX, i128::MAX, 1),
        Err(NavinError::ArithmeticError),
        "i128::MAX * i128::MAX must overflow"
    );
}

#[test]
fn test_checked_mul_div_min_times_negative_one_overflows() {
    // i128::MIN * (-1) overflows because |i128::MIN| > i128::MAX
    assert_eq!(
        checked_mul_div_i128(i128::MIN, -1, 1),
        Err(NavinError::ArithmeticError),
        "i128::MIN * (-1) must overflow"
    );
}

#[test]
fn test_checked_mul_div_zero_numerator() {
    assert_eq!(checked_mul_div_i128(0, i128::MAX, 1), Ok(0));
}

#[test]
fn test_checked_mul_div_zero_multiplier() {
    assert_eq!(checked_mul_div_i128(i128::MAX, 0, 1), Ok(0));
}

#[test]
fn test_checked_mul_div_large_but_representable() {
    // (i128::MAX / 2) * 2 / 1 should succeed (product is i128::MAX - 1)
    let half = i128::MAX / 2;
    assert_eq!(checked_mul_div_i128(half, 2, 1), Ok(half * 2));
}

#[test]
fn test_checked_mul_div_escrow_percentage_calculation() {
    // Simulates a milestone release: 1_000_000 * 30 / 100 = 300_000
    assert_eq!(
        checked_mul_div_i128(1_000_000, 30, 100),
        Ok(300_000),
        "Milestone percentage calculation must be exact"
    );
}

#[test]
fn test_checked_mul_div_truncates_remainder() {
    // 10 * 3 / 7 = 30/7 = 4 (truncated, not rounded)
    assert_eq!(
        checked_mul_div_i128(10, 3, 7),
        Ok(4),
        "Integer division must truncate toward zero"
    );
}

// ── TotalEscrowVolume overflow boundary tests (issue #519) ───────────────────
//
// These tests verify that the contract-wide volume tracker (`TotalEscrowVolume`)
// uses checked arithmetic and returns `ArithmeticError` rather than panicking
// when additions would overflow the i128 boundary.

mod total_escrow_volume_overflow {
    use crate::errors::NavinError;
    use crate::{storage, NavinShipment, NavinShipmentClient};
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env};

    #[contract]
    struct MockToken;

    #[contractimpl]
    impl MockToken {
        pub fn decimals(_env: Env) -> u32 {
            7
        }
        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    }

    fn setup() -> (Env, NavinShipmentClient<'static>, Address) {
        let (env, admin) = crate::test_utils::setup_env();
        let token = env.register(MockToken, ());
        let cid = env.register(NavinShipment, ());
        let client = NavinShipmentClient::new(&env, &cid);
        client.initialize(&admin, &token);
        (env, client, admin)
    }

    /// Seeding TotalEscrowVolume to i128::MAX then adding 1 must return
    /// ArithmeticError — overflow is caught by checked_add.
    #[test]
    fn test_overflow_at_max_plus_one_returns_arithmetic_error() {
        let (env, client, _admin) = setup();
        let cid = client.address.clone();

        // Seed the tracker to i128::MAX.
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&crate::DataKey::TotalEscrowVolume, &i128::MAX);
        });

        let result = env.as_contract(&cid, || storage::add_total_escrow_volume(&env, 1));

        assert_eq!(
            result,
            Err(NavinError::ArithmeticError),
            "adding 1 to i128::MAX must overflow and return ArithmeticError"
        );
    }

    /// i128::MAX + i128::MAX is the worst-case overflow; must not panic.
    #[test]
    fn test_overflow_max_plus_max_returns_arithmetic_error() {
        let (env, client, _admin) = setup();
        let cid = client.address.clone();

        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&crate::DataKey::TotalEscrowVolume, &i128::MAX);
        });

        let result =
            env.as_contract(&cid, || storage::add_total_escrow_volume(&env, i128::MAX));

        assert_eq!(
            result,
            Err(NavinError::ArithmeticError),
            "i128::MAX + i128::MAX must return ArithmeticError"
        );
    }

    /// Filling the tracker to exactly i128::MAX in two steps must succeed.
    #[test]
    fn test_near_max_accumulation_succeeds_then_overflows() {
        let (env, client, _admin) = setup();
        let cid = client.address.clone();

        // Seed to i128::MAX - 100.
        env.as_contract(&cid, || {
            env.storage()
                .instance()
                .set(&crate::DataKey::TotalEscrowVolume, &(i128::MAX - 100));
        });

        // Adding exactly 100 fills to MAX — must succeed.
        let ok = env.as_contract(&cid, || storage::add_total_escrow_volume(&env, 100));
        assert_eq!(ok, Ok(()), "filling to i128::MAX must succeed");

        // Verify the stored value.
        let volume =
            env.as_contract(&cid, || storage::get_total_escrow_volume(&env));
        assert_eq!(volume, i128::MAX);

        // One more unit overflows.
        let overflow =
            env.as_contract(&cid, || storage::add_total_escrow_volume(&env, 1));
        assert_eq!(
            overflow,
            Err(NavinError::ArithmeticError),
            "adding 1 past i128::MAX must return ArithmeticError"
        );
    }

    /// From the default zero state, normal accumulation must work correctly.
    #[test]
    fn test_volume_tracker_accumulates_from_zero() {
        let (env, client, _admin) = setup();
        let cid = client.address.clone();

        let result = env.as_contract(&cid, || storage::add_total_escrow_volume(&env, 5_000));
        assert_eq!(result, Ok(()));

        let volume = env.as_contract(&cid, || storage::get_total_escrow_volume(&env));
        assert_eq!(volume, 5_000);
    }
}
