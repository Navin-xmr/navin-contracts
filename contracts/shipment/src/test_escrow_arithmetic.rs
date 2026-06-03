//! # i128 Boundary Tests for Escrow Arithmetic Guards
//!
//! Validates that the checked arithmetic helpers (`checked_add_i128`,
//! `checked_sub_i128`, `checked_mul_div_i128`) correctly return
//! `NavinError::ArithmeticError` at the i128 boundaries and succeed
//! for representable values.

use crate::errors::NavinError;
use crate::{checked_add_i128, checked_sub_i128, checked_mul_div_i128};

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
