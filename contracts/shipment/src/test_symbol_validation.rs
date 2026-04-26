extern crate std;

use crate::errors::NavinError;
use crate::validation::{validate_metadata_symbols, validate_milestone_symbols, validate_symbol};
use soroban_sdk::{Env, Symbol, Vec};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn sym(env: &Env, s: &str) -> Symbol {
    Symbol::new(env, s)
}

// ── Valid symbols: boundary lengths ──────────────────────────────────────────

#[test]
fn test_valid_single_char_x() {
    let env = Env::default();
    assert_eq!(validate_symbol(&env, &sym(&env, "X")), Ok(()));
}

#[test]
fn test_valid_single_char_lowercase() {
    let env = Env::default();
    assert_eq!(validate_symbol(&env, &sym(&env, "a")), Ok(()));
}

#[test]
fn test_valid_shipment_8_chars() {
    let env = Env::default();
    assert_eq!(validate_symbol(&env, &sym(&env, "SHIPMENT")), Ok(()));
}

#[test]
fn test_valid_11_chars_below_boundary() {
    let env = Env::default();
    assert_eq!(validate_symbol(&env, &sym(&env, "ABCDEFGHIJK")), Ok(()));
}

#[test]
fn test_valid_12_chars_at_boundary() {
    let env = Env::default();
    // "VERYLONGNAME" is exactly 12 chars — the Stellar Symbol maximum
    assert_eq!(validate_symbol(&env, &sym(&env, "VERYLONGNAME")), Ok(()));
}

#[test]
fn test_valid_12_chars_digits() {
    let env = Env::default();
    assert_eq!(validate_symbol(&env, &sym(&env, "123456789012")), Ok(()));
}

// ── Valid symbols: character sets ─────────────────────────────────────────────

#[test]
fn test_valid_uppercase_only() {
    let env = Env::default();
    assert_eq!(validate_symbol(&env, &sym(&env, "ABCDEF")), Ok(()));
}

#[test]
fn test_valid_lowercase_only() {
    let env = Env::default();
    assert_eq!(validate_symbol(&env, &sym(&env, "abcdef")), Ok(()));
}

#[test]
fn test_valid_mixed_case() {
    let env = Env::default();
    assert_eq!(validate_symbol(&env, &sym(&env, "AbCdEfGh")), Ok(()));
}

#[test]
fn test_valid_alphanumeric_mixed() {
    let env = Env::default();
    assert_eq!(validate_symbol(&env, &sym(&env, "ABC123")), Ok(()));
}

#[test]
fn test_valid_digits_only() {
    let env = Env::default();
    assert_eq!(validate_symbol(&env, &sym(&env, "12345")), Ok(()));
}

#[test]
fn test_valid_underscore_allowed() {
    // Soroban Symbol allows [a-zA-Z0-9_]; underscore is a valid character.
    let env = Env::default();
    assert_eq!(validate_symbol(&env, &sym(&env, "ship_id")), Ok(()));
}

// ── Invalid symbols: too long ─────────────────────────────────────────────────

#[test]
fn test_invalid_13_chars_at_boundary() {
    let env = Env::default();
    // One char over the Stellar 12-char limit
    let s: std::string::String = "A".repeat(13);
    assert_eq!(
        validate_symbol(&env, &sym(&env, &s)),
        Err(NavinError::InvalidShipmentInput),
        "13-char symbol must be rejected"
    );
}

#[test]
fn test_invalid_17_chars_toolongsymbolname() {
    let env = Env::default();
    // "TOOLONGSYMBOLNAME" = 17 chars
    assert_eq!(
        validate_symbol(&env, &sym(&env, "TOOLONGSYMBNAME")),
        Err(NavinError::InvalidShipmentInput),
        "15-char symbol must be rejected"
    );
}

#[test]
fn test_invalid_30_chars_rejected() {
    // 30 chars: within Soroban SDK's limit but well above our 12-char max
    let env = Env::default();
    let s: std::string::String = "A".repeat(30);
    assert_eq!(
        validate_symbol(&env, &sym(&env, &s)),
        Err(NavinError::InvalidShipmentInput),
        "30-char symbol must be rejected"
    );
}

#[test]
fn test_invalid_25_chars_rejected() {
    let env = Env::default();
    let s: std::string::String = "B".repeat(25);
    assert_eq!(
        validate_symbol(&env, &sym(&env, &s)),
        Err(NavinError::InvalidShipmentInput),
        "25-char symbol must be rejected"
    );
}

// ── Error type verification ───────────────────────────────────────────────────

#[test]
fn test_oversized_symbol_returns_invalid_input_error() {
    let env = Env::default();
    let s: std::string::String = "X".repeat(13);
    let err = validate_symbol(&env, &sym(&env, &s)).unwrap_err();
    assert_eq!(
        err,
        NavinError::InvalidShipmentInput,
        "Oversized symbol must map to InvalidShipmentInput, not any other error variant"
    );
}

#[test]
fn test_valid_boundary_symbols_return_ok() {
    let env = Env::default();
    for name in &["X", "SHIPMENT", "VERYLONGNAME"] {
        assert_eq!(
            validate_symbol(&env, &sym(&env, name)),
            Ok(()),
            "'{}' should return Ok(())",
            name
        );
    }
}

// ── Milestone symbol validation ───────────────────────────────────────────────

#[test]
fn test_milestone_with_12_char_symbols_valid() {
    let env = Env::default();
    let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    milestones.push_back((sym(&env, "VERYLONGNAME"), 50));
    milestones.push_back((sym(&env, "ABCDEFGHIJKL"), 50));
    assert_eq!(validate_milestone_symbols(&env, &milestones), Ok(()));
}

#[test]
fn test_milestone_with_13_char_symbol_rejected() {
    let env = Env::default();
    let long_name: std::string::String = "A".repeat(13);
    let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    milestones.push_back((sym(&env, &long_name), 100));
    assert_eq!(
        validate_milestone_symbols(&env, &milestones),
        Err(NavinError::InvalidShipmentInput),
        "Milestone with 13-char symbol must be rejected"
    );
}

#[test]
fn test_milestone_duplicate_12_char_symbols_rejected() {
    let env = Env::default();
    let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    milestones.push_back((sym(&env, "VERYLONGNAME"), 50));
    milestones.push_back((sym(&env, "VERYLONGNAME"), 50));
    assert_eq!(
        validate_milestone_symbols(&env, &milestones),
        Err(NavinError::InvalidShipmentInput),
        "Duplicate 12-char milestone symbols must be rejected"
    );
}

#[test]
fn test_milestone_mixed_valid_lengths_pass() {
    let env = Env::default();
    let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    milestones.push_back((sym(&env, "X"), 10));
    milestones.push_back((sym(&env, "SHIPMENT"), 40));
    milestones.push_back((sym(&env, "VERYLONGNAME"), 50));
    assert_eq!(validate_milestone_symbols(&env, &milestones), Ok(()));
}

// ── Metadata symbol validation ────────────────────────────────────────────────

#[test]
fn test_metadata_with_12_char_key_and_value_valid() {
    let env = Env::default();
    let key = sym(&env, "VERYLONGNAME");
    let val = sym(&env, "ABCDEFGHIJKL");
    assert_eq!(validate_metadata_symbols(&env, &key, &val), Ok(()));
}

#[test]
fn test_metadata_oversized_key_rejected() {
    let env = Env::default();
    let long: std::string::String = "K".repeat(13);
    let key = sym(&env, &long);
    let val = sym(&env, "OK");
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Err(NavinError::InvalidShipmentInput),
        "Metadata with oversized key must be rejected"
    );
}

#[test]
fn test_metadata_oversized_value_rejected() {
    let env = Env::default();
    let key = sym(&env, "weight");
    let long: std::string::String = "V".repeat(13);
    let val = sym(&env, &long);
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Err(NavinError::InvalidShipmentInput),
        "Metadata with oversized value must be rejected"
    );
}

#[test]
fn test_metadata_both_oversized_rejected() {
    let env = Env::default();
    let k: std::string::String = "K".repeat(13);
    let v: std::string::String = "V".repeat(13);
    let key = sym(&env, &k);
    let val = sym(&env, &v);
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Err(NavinError::InvalidShipmentInput),
        "Metadata with both oversized key and value must be rejected"
    );
}

// ── Additional coverage ───────────────────────────────────────────────────────

#[test]
fn test_all_12_char_alphanumeric_patterns_valid() {
    let env = Env::default();
    let names = [
        "SYMBOL123456", // mixed alphanumeric uppercase
        "symbol123456", // mixed alphanumeric lowercase
        "SymBol123456", // mixed case
    ];
    for name in &names {
        assert_eq!(
            validate_symbol(&env, &sym(&env, name)),
            Ok(()),
            "'{}' should be valid",
            name
        );
    }
}

#[test]
fn test_lengths_13_to_17_all_rejected() {
    let env = Env::default();
    for len in 13..=17usize {
        let s: std::string::String = "A".repeat(len);
        assert_eq!(
            validate_symbol(&env, &sym(&env, &s)),
            Err(NavinError::InvalidShipmentInput),
            "Symbol of length {} must be rejected",
            len
        );
    }
}
