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
<<<<<<< test/symbol-validation-boundaries

// ── Additional edge case tests ────────────────────────────────────────────────

#[test]
fn test_milestone_empty_vector_valid() {
    let env = Env::default();
    let milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    assert_eq!(
        validate_milestone_symbols(&env, &milestones),
        Ok(()),
        "Empty milestone vector should be valid"
    );
}

#[test]
fn test_milestone_single_symbol_100_percent_valid() {
    let env = Env::default();
    let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    milestones.push_back((sym(&env, "CHECKPOINT"), 100));
    assert_eq!(
        validate_milestone_symbols(&env, &milestones),
        Ok(()),
        "Single milestone with 100% should be valid"
    );
}

#[test]
fn test_metadata_single_char_key_and_value_valid() {
    let env = Env::default();
    let key = sym(&env, "K");
    let val = sym(&env, "V");
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Ok(()),
        "Single character key and value should be valid"
    );
}

#[test]
fn test_symbol_with_underscores_and_numbers_valid() {
    let env = Env::default();
    assert_eq!(
        validate_symbol(&env, &sym(&env, "ship_123_id")),
        Ok(()),
        "Symbol with underscores and numbers should be valid"
    );
}

#[test]
fn test_milestone_three_symbols_equal_split_valid() {
    let env = Env::default();
    let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    milestones.push_back((sym(&env, "PICKUP"), 33));
    milestones.push_back((sym(&env, "TRANSIT"), 33));
    milestones.push_back((sym(&env, "DELIVERY"), 34));
    assert_eq!(
        validate_milestone_symbols(&env, &milestones),
        Ok(()),
        "Three milestones with equal split (33-33-34) should be valid (regression)"
    );
}

// ── Regression tests added for issue #380 ────────────────────────────────────
// These cases expand coverage for exact-length boundaries, overlong symbols,
// and event-related symbol usage so that milestone, metadata, and event-topic
// symbols all keep the same bounded-input guarantees.

// ── Exact-length boundary: 2 chars ───────────────────────────────────────────

#[test]
fn test_valid_two_char_symbol() {
    let env = Env::default();
    assert_eq!(
        validate_symbol(&env, &sym(&env, "AB")),
        Ok(()),
        "2-char symbol must be accepted"
    );
}

#[test]
fn test_valid_two_char_numeric_symbol() {
    let env = Env::default();
    assert_eq!(
        validate_symbol(&env, &sym(&env, "99")),
        Ok(()),
        "2-char all-digit symbol must be accepted"
    );
}

// ── Exact-length boundary: 4 chars (XDR boundary word) ───────────────────────

#[test]
fn test_valid_four_char_symbol_abcd() {
    let env = Env::default();
    assert_eq!(validate_symbol(&env, &sym(&env, "ABCD")), Ok(()));
}

#[test]
fn test_valid_four_char_symbol_with_underscore() {
    let env = Env::default();
    assert_eq!(validate_symbol(&env, &sym(&env, "a_b_")), Ok(()));
}

// ── Exact-length boundary: 9 chars ───────────────────────────────────────────

#[test]
fn test_valid_nine_char_symbol() {
    let env = Env::default();
    assert_eq!(
        validate_symbol(&env, &sym(&env, "WAREHOUSE")),
        Ok(()),
        "9-char symbol 'WAREHOUSE' must be accepted"
    );
}

// ── Exact-length boundary: 10 chars ──────────────────────────────────────────

#[test]
fn test_valid_ten_char_symbol() {
    let env = Env::default();
    assert_eq!(
        validate_symbol(&env, &sym(&env, "CHECKPOINT")),
        Ok(()),
        "10-char symbol must be accepted"
    );
}

// ── Boundary just above limit: 13 chars  (regression guard) ──────────────────

#[test]
fn test_regression_13_chars_always_rejected() {
    let env = Env::default();
    // Regression: ensure the boundary has not drifted above 12.
    let s: std::string::String = "Z".repeat(13);
    assert_eq!(
        validate_symbol(&env, &sym(&env, &s)),
        Err(crate::errors::NavinError::InvalidShipmentInput),
        "regression: 13-char symbol must always map to InvalidShipmentInput"
    );
}

// ── Event-topic symbol regression ────────────────────────────────────────────
// Event topic names are Symbols and must follow the same length constraints.

#[test]
fn test_event_topic_transfer_valid() {
    let env = Env::default();
    assert_eq!(
        validate_symbol(&env, &sym(&env, "transfer")),
        Ok(()),
        "event topic 'transfer' (8 chars) must be valid"
    );
}

#[test]
fn test_event_topic_deposit_valid() {
    let env = Env::default();
    assert_eq!(
        validate_symbol(&env, &sym(&env, "deposit")),
        Ok(()),
        "event topic 'deposit' (7 chars) must be valid"
    );
}

#[test]
fn test_event_topic_status_update_valid() {
    let env = Env::default();
    assert_eq!(
        validate_symbol(&env, &sym(&env, "StatusUpd")),
        Ok(()),
        "event topic 'StatusUpd' (9 chars) must be valid"
    );
}

#[test]
fn test_event_topic_milestone_complete_valid() {
    let env = Env::default();
    // 12 chars — at the Stellar Symbol maximum
    assert_eq!(
        validate_symbol(&env, &sym(&env, "MilestoneDon")),
        Ok(()),
        "event topic 'MilestoneDon' (12 chars) must be valid"
    );
}

#[test]
fn test_event_topic_too_long_rejected() {
    let env = Env::default();
    // 13 chars — one over the Stellar Symbol maximum
    let s: std::string::String = "E".repeat(13);
    assert_eq!(
        validate_symbol(&env, &sym(&env, &s)),
        Err(crate::errors::NavinError::InvalidShipmentInput),
        "event topic symbol > 12 chars must be rejected"
    );
}

// ── Milestone: all 12-char symbols unique → accepted ─────────────────────────

#[test]
fn test_milestone_five_unique_12_char_symbols_valid() {
    let env = Env::default();
    let mut milestones: soroban_sdk::Vec<(soroban_sdk::Symbol, u32)> =
        soroban_sdk::Vec::new(&env);
    milestones.push_back((sym(&env, "VERYLONGNAM1"), 20));
    milestones.push_back((sym(&env, "VERYLONGNAM2"), 20));
    milestones.push_back((sym(&env, "VERYLONGNAM3"), 20));
    milestones.push_back((sym(&env, "VERYLONGNAM4"), 20));
    milestones.push_back((sym(&env, "VERYLONGNAM5"), 20));
    assert_eq!(
        validate_milestone_symbols(&env, &milestones),
        Ok(()),
        "Five unique 12-char milestone symbols must all pass validation"
    );
}

// ── Metadata: key at exact max, value at exact max ───────────────────────────

#[test]
fn test_metadata_both_12_chars_valid() {
    let env = Env::default();
    let key = sym(&env, "ABCDEFGHIJKL"); // 12 chars
    let val = sym(&env, "123456789012"); // 12 chars
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Ok(()),
        "Metadata key and value both at 12-char boundary must be valid"
    );
}

#[test]
fn test_metadata_key_at_12_value_at_1_valid() {
    let env = Env::default();
    let key = sym(&env, "ABCDEFGHIJKL");
    let val = sym(&env, "V");
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Ok(()),
        "12-char key with 1-char value must be valid"
    );
}

#[test]
fn test_metadata_key_at_1_value_at_12_valid() {
    let env = Env::default();
    let key = sym(&env, "K");
    let val = sym(&env, "ABCDEFGHIJKL");
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Ok(()),
        "1-char key with 12-char value must be valid"
    );
}

// ── Overlong metadata symbols ─────────────────────────────────────────────────

#[test]
fn test_metadata_key_13_chars_rejected() {
    let env = Env::default();
    let key = sym(&env, "AAAAAAAAAAAAA"); // 13 chars
    let val = sym(&env, "fine");
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Err(crate::errors::NavinError::InvalidShipmentInput),
        "13-char metadata key must be rejected"
    );
}

#[test]
fn test_metadata_value_20_chars_rejected() {
    let env = Env::default();
    let key = sym(&env, "fine");
    let val = sym(&env, "AAAAAAAAAAAAAAAAAAAA"); // 20 chars
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Err(crate::errors::NavinError::InvalidShipmentInput),
        "20-char metadata value must be rejected"
    );
}

// ── Idempotency of validate_symbol ────────────────────────────────────────────

#[test]
fn test_validate_symbol_is_idempotent() {
    let env = Env::default();
    // Calling validate_symbol twice on the same input must return identical results.
    let s = sym(&env, "SHIPMENT");
    let first = validate_symbol(&env, &s);
    let second = validate_symbol(&env, &s);
    assert_eq!(first, second, "validate_symbol must be idempotent");
}

#[test]
fn test_validate_symbol_overlong_is_idempotent() {
    let env = Env::default();
    let long: std::string::String = "X".repeat(15);
    let s = sym(&env, &long);
    let first = validate_symbol(&env, &s);
    let second = validate_symbol(&env, &s);
    assert_eq!(first, second, "validate_symbol (overlong) must be idempotent");
}

// ── Milestone: exact-length boundary through helper ──────────────────────────

#[test]
fn test_milestone_single_char_symbol_valid() {
    let env = Env::default();
    let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    milestones.push_back((sym(&env, "X"), 100));
    assert_eq!(
        validate_milestone_symbols(&env, &milestones),
        Ok(()),
        "Milestone with 1-char symbol must be accepted"
    );
}

#[test]
fn test_milestone_two_char_symbol_valid() {
    let env = Env::default();
    let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    milestones.push_back((sym(&env, "AB"), 50));
    milestones.push_back((sym(&env, "CD"), 50));
    assert_eq!(
        validate_milestone_symbols(&env, &milestones),
        Ok(()),
        "Milestones with 2-char symbols must be accepted"
    );
}

// ── Milestone: overlong symbols through helper ──────────────────────────────

#[test]
fn test_milestone_17_char_symbol_rejected() {
    let env = Env::default();
    let long: std::string::String = "M".repeat(17);
    let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    milestones.push_back((sym(&env, &long), 100));
    assert_eq!(
        validate_milestone_symbols(&env, &milestones),
        Err(NavinError::InvalidShipmentInput),
        "Milestone with 17-char symbol must be rejected"
    );
}

#[test]
fn test_milestone_25_char_symbol_rejected() {
    let env = Env::default();
    let long: std::string::String = "M".repeat(25);
    let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    milestones.push_back((sym(&env, &long), 100));
    assert_eq!(
        validate_milestone_symbols(&env, &milestones),
        Err(NavinError::InvalidShipmentInput),
        "Milestone with 25-char symbol must be rejected"
    );
}

#[test]
fn test_milestone_30_char_symbol_rejected() {
    let env = Env::default();
    let long: std::string::String = "M".repeat(30);
    let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    milestones.push_back((sym(&env, &long), 100));
    assert_eq!(
        validate_milestone_symbols(&env, &milestones),
        Err(NavinError::InvalidShipmentInput),
        "Milestone with 30-char symbol must be rejected"
    );
}

// ── Metadata: exact-length boundary through helper ──────────────────────────

#[test]
fn test_metadata_single_char_key_single_char_value_valid() {
    let env = Env::default();
    let key = sym(&env, "K");
    let val = sym(&env, "V");
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Ok(()),
        "1-char key and 1-char value must be accepted"
    );
}

#[test]
fn test_metadata_two_char_key_two_char_value_valid() {
    let env = Env::default();
    let key = sym(&env, "AB");
    let val = sym(&env, "XY");
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Ok(()),
        "2-char key and 2-char value must be accepted"
    );
}

// ── Metadata: overlong symbols through helper ───────────────────────────────

#[test]
fn test_metadata_key_17_chars_rejected() {
    let env = Env::default();
    let key = sym(&env, &std::string::String::from("K").repeat(17));
    let val = sym(&env, "fine");
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Err(NavinError::InvalidShipmentInput),
        "17-char metadata key must be rejected"
    );
}

#[test]
fn test_metadata_value_17_chars_rejected() {
    let env = Env::default();
    let key = sym(&env, "fine");
    let val = sym(&env, &std::string::String::from("V").repeat(17));
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Err(NavinError::InvalidShipmentInput),
        "17-char metadata value must be rejected"
    );
}

#[test]
fn test_metadata_key_25_chars_rejected() {
    let env = Env::default();
    let key = sym(&env, &std::string::String::from("K").repeat(25));
    let val = sym(&env, "fine");
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Err(NavinError::InvalidShipmentInput),
        "25-char metadata key must be rejected"
    );
}

#[test]
fn test_metadata_value_25_chars_rejected() {
    let env = Env::default();
    let key = sym(&env, "fine");
    let val = sym(&env, &std::string::String::from("V").repeat(25));
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Err(NavinError::InvalidShipmentInput),
        "25-char metadata value must be rejected"
    );
}

#[test]
fn test_metadata_key_30_chars_rejected() {
    let env = Env::default();
    let key = sym(&env, &std::string::String::from("K").repeat(30));
    let val = sym(&env, "fine");
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Err(NavinError::InvalidShipmentInput),
        "30-char metadata key must be rejected"
    );
}

#[test]
fn test_metadata_value_30_chars_rejected() {
    let env = Env::default();
    let key = sym(&env, "fine");
    let val = sym(&env, &std::string::String::from("V").repeat(30));
    assert_eq!(
        validate_metadata_symbols(&env, &key, &val),
        Err(NavinError::InvalidShipmentInput),
        "30-char metadata value must be rejected"
    );
}
=======
>>>>>>> main
