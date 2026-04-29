//! Shared test utilities for deterministic Soroban SDK testing.
//!
//! This module provides helper functions to set up test environments
//! with explicit protocol version, timestamp, and sequence number
//! to ensure deterministic behavior across all tests.
//!
//! # Usage pattern for time-sensitive tests
//!
//! All tests that exercise deadlines, penalties, rate limiting, or proposal
//! expiry **must** use these helpers instead of ad-hoc ledger mutations so
//! that the intent is self-documenting and reruns remain deterministic.
//!
//! ```text
//! // ✅ Preferred — intent is clear
//! test_utils::advance_past_rate_limit(&env);
//! client.update_status(&carrier, &id, &ShipmentStatus::AtCheckpoint, &h2);
//!
//! // ❌ Avoid — magic constant, intent is opaque
//! env.ledger().with_mut(|l| l.timestamp += 61);
//! client.update_status(&carrier, &id, &ShipmentStatus::AtCheckpoint, &h2);
//! ```
//!
//! ## Available helpers
//!
//! | Helper | Use case |
//! |---|---|
//! | [`advance_ledger_time`] | Generic N-second advance |
//! | [`set_ledger_time`] | Jump to a specific instant |
//! | [`advance_ledger_sequence`] | Bump sequence by N |
//! | [`set_ledger_sequence`] | Pin sequence to an exact value |
//! | [`advance_past_rate_limit`] | Clear 60-s `update_status` window |
//! | [`advance_past_multisig_expiry`] | Expire a multi-sig proposal |
//! | [`future_deadline`] | Compute a relative deadline timestamp |

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

#[cfg(any(test, feature = "testutils"))]
extern crate std;

/// Default protocol version for tests
pub const DEFAULT_PROTOCOL_VERSION: u32 = 22;

/// Default timestamp for tests (Unix epoch + 1 day)
pub const DEFAULT_TIMESTAMP: u64 = 86400;

/// Default sequence number for tests
pub const DEFAULT_SEQUENCE_NUMBER: u32 = 1;

/// Sets up a deterministic test environment with explicit protocol version,
/// timestamp, and sequence number.
///
/// # Returns
/// A tuple containing:
/// - `Env` - The configured Soroban environment
/// - `Address` - A generated admin address
///
/// # Example
/// ```rust
/// let (env, admin) = test_utils::setup_env();
/// ```
pub fn setup_env() -> (Env, Address) {
    let env = Env::default();

    // Set protocol version explicitly for deterministic behavior
    env.ledger().with_mut(|li| {
        li.protocol_version = DEFAULT_PROTOCOL_VERSION;
    });

    // Set explicit timestamp
    env.ledger().set_timestamp(DEFAULT_TIMESTAMP);

    // Set explicit sequence number
    env.ledger().with_mut(|li| {
        li.sequence_number = DEFAULT_SEQUENCE_NUMBER;
    });

    let admin = Address::generate(&env);
    env.mock_all_auths();

    (env, admin)
}

// ── Timestamp helpers ─────────────────────────────────────────────────────────

/// Advance the ledger timestamp by `seconds`.
///
/// Use this in place of ad-hoc `env.ledger().with_mut(|l| l.timestamp += N)`
/// so the intent is explicit and the pattern is consistent across all tests.
///
/// # Example
/// ```text
/// test_utils::advance_ledger_time(&env, 1001); // push past a 1000-s deadline
/// client.check_deadline(&shipment_id);
/// ```
pub fn advance_ledger_time(env: &Env, seconds: u64) {
    env.ledger().with_mut(|l| l.timestamp += seconds);
}

/// Set the ledger timestamp to an explicit absolute value.
///
/// Use when a test needs to jump to a specific instant rather than a relative
/// offset — for example, to land exactly on a deadline+grace boundary.
///
/// # Example
/// ```text
/// test_utils::set_ledger_time(&env, deadline + grace);
/// client.check_deadline(&shipment_id); // expires exactly at the boundary
/// ```
pub fn set_ledger_time(env: &Env, timestamp: u64) {
    env.ledger().set_timestamp(timestamp);
}

// ── Sequence helpers ──────────────────────────────────────────────────────────

/// Advance the ledger sequence number by `count`.
///
/// Useful for tests that verify nonce-dependent behaviour or integration
/// identifiers that increment with each ledger sequence step.
pub fn advance_ledger_sequence(env: &Env, count: u32) {
    env.ledger().with_mut(|l| l.sequence_number += count);
}

/// Set the ledger sequence number to an explicit value.
///
/// Pin the sequence to a known state so that tests depending on sequence-based
/// logic are fully deterministic across reruns.
pub fn set_ledger_sequence(env: &Env, sequence: u32) {
    env.ledger().with_mut(|l| l.sequence_number = sequence);
}

// ── Named scenario helpers ────────────────────────────────────────────────────

/// Advance ledger time by **61 seconds** — just past the 60-second
/// `min_status_update_interval` — to clear the rate-limit window on
/// `update_status` calls made by non-admin callers.
///
/// # Example
/// ```text
/// client.update_status(&carrier, &id, &ShipmentStatus::InTransit, &h1);
/// test_utils::advance_past_rate_limit(&env);
/// client.update_status(&carrier, &id, &ShipmentStatus::AtCheckpoint, &h2); // ok
/// ```
pub fn advance_past_rate_limit(env: &Env) {
    advance_ledger_time(env, 61);
}

/// Advance ledger time by **7 days + 1 second** (604 801 s) — past the
/// multi-sig proposal expiry window — so that subsequent `approve_action`
/// or `execute_proposal` calls return `ProposalExpired` (#24).
///
/// # Example
/// ```text
/// let proposal_id = client.propose_action(&admin1, &action);
/// test_utils::advance_past_multisig_expiry(&env);
/// // now approve_action / execute_proposal will return ProposalExpired
/// ```
pub fn advance_past_multisig_expiry(env: &Env) {
    advance_ledger_time(env, 7 * 24 * 60 * 60 + 1); // 604_801 s
}

/// Return a **future deadline timestamp** that is `secs_from_now` seconds
/// ahead of the current ledger time.
///
/// Prefer this over `env.ledger().timestamp() + N` literals so the
/// relationship between "now" and the deadline is always obvious.
///
/// # Example
/// ```text
/// let deadline = test_utils::future_deadline(&env, 3_600); // 1 h from now
/// let id = client.create_shipment(&company, &receiver, &carrier,
///                                  &hash, &milestones, &deadline, &None);
/// ```
pub fn future_deadline(env: &Env, secs_from_now: u64) -> u64 {
    env.ledger().timestamp() + secs_from_now
}

/// Normalizes non-deterministic fields in a JSON snapshot.
#[cfg(any(test, feature = "testutils"))]
pub fn sanitize_json_snapshot(json: &str) -> std::string::String {
    use serde_json::Value;
    use std::string::ToString;

    let mut v: Value = serde_json::from_str(json).expect("Invalid JSON for sanitization");

    fn walk(v: &mut Value) {
        match v {
            Value::Object(map) => {
                // Sanitize known non-deterministic fields
                if map.contains_key("ledger_key_nonce") {
                    if let Some(nonce_obj) = map
                        .get_mut("ledger_key_nonce")
                        .and_then(|n| n.as_object_mut())
                    {
                        nonce_obj.insert("nonce".to_string(), Value::from(0));
                    }
                }

                // Sanitize generators
                if let Some(gen) = map.get_mut("generators").and_then(|g| g.as_object_mut()) {
                    if gen.contains_key("address") {
                        gen.insert("address".to_string(), Value::from(0));
                    }
                    if gen.contains_key("nonce") {
                        gen.insert("nonce".to_string(), Value::from(0));
                    }
                }

                // Sanitize ledger
                if let Some(ledger) = map.get_mut("ledger").and_then(|l| l.as_object_mut()) {
                    if ledger.contains_key("timestamp") {
                        ledger.insert("timestamp".to_string(), Value::from(86400));
                    }
                    if ledger.contains_key("sequence_number") {
                        ledger.insert("sequence_number".to_string(), Value::from(1));
                    }
                }

                for value in map.values_mut() {
                    walk(value);
                }
            }
            Value::Array(arr) => {
                for value in arr.iter_mut() {
                    walk(value);
                }
            }
            _ => {}
        }
    }

    walk(&mut v);
    serde_json::to_string_pretty(&v).expect("Failed to serialize sanitized JSON")
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_env_sets_protocol_version() {
        let (env, _admin) = setup_env();
        env.ledger().with_mut(|li| {
            assert_eq!(li.protocol_version, DEFAULT_PROTOCOL_VERSION);
        });
    }

    #[test]
    fn test_setup_env_sets_timestamp() {
        let (env, _admin) = setup_env();
        assert_eq!(env.ledger().timestamp(), DEFAULT_TIMESTAMP);
    }

    #[test]
    fn test_setup_env_sets_sequence_number() {
        let (env, _admin) = setup_env();
        env.ledger().with_mut(|li| {
            assert_eq!(li.sequence_number, DEFAULT_SEQUENCE_NUMBER);
        });
    }

    #[test]
    fn test_advance_ledger_time_increments_timestamp() {
        let (env, _) = setup_env();
        let before = env.ledger().timestamp();
        advance_ledger_time(&env, 100);
        assert_eq!(env.ledger().timestamp(), before + 100);
    }

    #[test]
    fn test_set_ledger_time_pins_timestamp() {
        let (env, _) = setup_env();
        set_ledger_time(&env, 999_999);
        assert_eq!(env.ledger().timestamp(), 999_999);
    }

    #[test]
    fn test_advance_ledger_sequence_increments() {
        let (env, _) = setup_env();
        let before = env.ledger().sequence();
        advance_ledger_sequence(&env, 5);
        assert_eq!(env.ledger().sequence(), before + 5);
    }

    #[test]
    fn test_set_ledger_sequence_pins_value() {
        let (env, _) = setup_env();
        set_ledger_sequence(&env, 42);
        assert_eq!(env.ledger().sequence(), 42);
    }

    #[test]
    fn test_advance_past_rate_limit_adds_61_seconds() {
        let (env, _) = setup_env();
        let before = env.ledger().timestamp();
        advance_past_rate_limit(&env);
        assert_eq!(env.ledger().timestamp(), before + 61);
    }

    #[test]
    fn test_advance_past_multisig_expiry_adds_seven_days_plus_one() {
        let (env, _) = setup_env();
        let before = env.ledger().timestamp();
        advance_past_multisig_expiry(&env);
        assert_eq!(env.ledger().timestamp(), before + 604_801);
    }

    #[test]
    fn test_future_deadline_returns_offset_from_now() {
        let (env, _) = setup_env();
        let now = env.ledger().timestamp();
        let dl = future_deadline(&env, 3_600);
        assert_eq!(dl, now + 3_600);
    }

    #[test]
    fn test_sanitize_json_snapshot() {
        let json = r#"{
            "generators": { "address": 10, "nonce": 5 },
            "ledger": { "timestamp": 123456, "sequence_number": 99 },
            "ledger_entries": [
                {
                    "ledger_key_nonce": { "nonce": 987654321 }
                }
            ]
        }"#;
        let sanitized = sanitize_json_snapshot(json);
        assert!(sanitized.contains(r#""timestamp": 86400"#));
        assert!(sanitized.contains(r#""sequence_number": 1"#));
        assert!(sanitized.contains(r#""address": 0"#));
        assert!(sanitized.contains(r#""nonce": 0"#));
    }
}
