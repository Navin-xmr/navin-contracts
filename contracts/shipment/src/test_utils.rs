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
//! | [`checkpoint_symbol`] | Create deterministic checkpoint/milestone symbols |

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, Symbol,
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

// ── Symbol helpers ────────────────────────────────────────────────────────────

/// Create a deterministic checkpoint or milestone symbol for tests.
///
/// This helper provides a consistent, repeatable way to construct Symbol
/// instances for milestone and status checkpoint tests, avoiding duplicated
/// symbol construction code and ensuring deterministic test behavior.
///
/// # Naming Pattern
///
/// The helper supports common checkpoint/milestone naming conventions:
/// - **Descriptive names**: "warehouse", "port", "customs", "final"
/// - **Sequential names**: "M1", "M2", "M3" or "checkpoint1", "checkpoint2"
/// - **Short codes**: "pickup", "transit", "delivery"
///
/// All symbols must conform to Stellar Symbol constraints:
/// - Length: 1-12 characters
/// - Format: Alphanumeric and underscore only (A-Z, a-z, 0-9, _)
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `name` - The checkpoint/milestone name (1-12 chars, alphanumeric + underscore)
///
/// # Returns
/// A `Symbol` instance that can be used in milestone vectors or status updates
///
/// # Examples
/// ```rust
/// // Create milestone schedule with descriptive names
/// let mut milestones = Vec::new(&env);
/// milestones.push_back((checkpoint_symbol(&env, "warehouse"), 30));
/// milestones.push_back((checkpoint_symbol(&env, "port"), 30));
/// milestones.push_back((checkpoint_symbol(&env, "final"), 40));
///
/// // Create milestone schedule with sequential names
/// let mut milestones = Vec::new(&env);
/// milestones.push_back((checkpoint_symbol(&env, "M1"), 50));
/// milestones.push_back((checkpoint_symbol(&env, "M2"), 50));
///
/// // Record a milestone
/// client.record_milestone(
///     &carrier,
///     &shipment_id,
///     &checkpoint_symbol(&env, "warehouse"),
///     &data_hash,
/// );
/// ```
///
/// # Panics
/// Panics if the provided name exceeds 12 characters or contains invalid
/// characters (enforced by Stellar Symbol constraints).
pub fn checkpoint_symbol(env: &Env, name: &str) -> Symbol {
    Symbol::new(env, name)
}

/// Normalizes non-deterministic fields in a JSON snapshot.
///
/// # Normalized Fields
///
/// This sanitizer removes volatility from snapshot tests by normalizing
/// ledger-dependent and non-deterministic fields that would otherwise create
/// noisy diffs on every test run.
///
/// ## Ledger State Fields
/// - `generators.address` → `0` (test address counter)
/// - `generators.nonce` → `0` (test nonce counter)
/// - `ledger.timestamp` → `86400` (canonical 1-day offset)
/// - `ledger.sequence_number` → `1` (canonical sequence)
/// - `ledger_key_nonce.nonce` → `0` (storage nonces)
///
/// ## Event Fields
/// - `event.contract_id` → `"0000...0000"` (32-byte zero hash)
/// - Event idempotency keys (last `bytes` in event data) → `"0000...0000"`
///
/// ## Design Rationale
///
/// True event changes (topics, data structure, ordering) still produce diffs,
/// while ledger-specific noise is eliminated. This keeps snapshot tests
/// readable and focused on contract behavior rather than test harness state.
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

                // Sanitize event contract_id (generated contract addresses)
                if map.contains_key("event") {
                    if let Some(event_obj) = map.get_mut("event").and_then(|e| e.as_object_mut()) {
                        if event_obj.contains_key("contract_id") {
                            event_obj.insert(
                                "contract_id".to_string(),
                                Value::from("0000000000000000000000000000000000000000000000000000000000000000"),
                            );
                        }
                    }
                }

                // Sanitize event idempotency keys
                // Events emit idempotency keys as the last element in their data vec
                // These are SHA-256 hashes that include ledger-dependent values
                if map.contains_key("body") {
                    if let Some(body) = map.get_mut("body").and_then(|b| b.as_object_mut()) {
                        if let Some(v0) = body.get_mut("v0").and_then(|v| v.as_object_mut()) {
                            if let Some(data) = v0.get_mut("data").and_then(|d| d.as_object_mut()) {
                                if let Some(vec) = data.get_mut("vec").and_then(|v| v.as_array_mut()) {
                                    // Check if the last element is a bytes field (potential idempotency key)
                                    if let Some(last) = vec.last_mut() {
                                        if let Some(obj) = last.as_object_mut() {
                                            if obj.contains_key("bytes") {
                                                // Normalize to zero hash (64 hex chars = 32 bytes)
                                                if let Some(bytes_val) = obj.get("bytes").and_then(|b| b.as_str()) {
                                                    if bytes_val.len() == 64 {
                                                        obj.insert(
                                                            "bytes".to_string(),
                                                            Value::from("0000000000000000000000000000000000000000000000000000000000000000"),
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
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
    fn test_checkpoint_symbol_creates_valid_symbol() {
        let (env, _) = setup_env();
        let sym = checkpoint_symbol(&env, "warehouse");
        assert_eq!(sym, Symbol::new(&env, "warehouse"));
    }

    #[test]
    fn test_checkpoint_symbol_short_names() {
        let (env, _) = setup_env();
        let m1 = checkpoint_symbol(&env, "M1");
        let m2 = checkpoint_symbol(&env, "M2");
        assert_eq!(m1, Symbol::new(&env, "M1"));
        assert_eq!(m2, Symbol::new(&env, "M2"));
    }

    #[test]
    fn test_checkpoint_symbol_max_length() {
        let (env, _) = setup_env();
        // 12 characters is the Stellar Symbol maximum
        let sym = checkpoint_symbol(&env, "VERYLONGNAME");
        assert_eq!(sym, Symbol::new(&env, "VERYLONGNAME"));
    }

    #[test]
    fn test_checkpoint_symbol_with_underscore() {
        let (env, _) = setup_env();
        let sym = checkpoint_symbol(&env, "port_arrival");
        assert_eq!(sym, Symbol::new(&env, "port_arrival"));
    }

    #[test]
    fn test_checkpoint_symbol_deterministic() {
        let (env, _) = setup_env();
        let sym1 = checkpoint_symbol(&env, "warehouse");
        let sym2 = checkpoint_symbol(&env, "warehouse");
        assert_eq!(sym1, sym2, "Same name should produce identical symbols");
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

    #[test]
    fn test_sanitize_json_snapshot_normalizes_event_contract_ids() {
        let json = r#"{
            "events": [
                {
                    "event": {
                        "contract_id": "0000000000000000000000000000000000000000000000000000000000000006",
                        "type_": "contract"
                    }
                }
            ]
        }"#;
        let sanitized = sanitize_json_snapshot(json);
        assert!(
            sanitized.contains(r#""contract_id": "0000000000000000000000000000000000000000000000000000000000000000""#),
            "Event contract_id should be normalized to zero hash"
        );
    }

    #[test]
    fn test_sanitize_json_snapshot_normalizes_event_idempotency_keys() {
        let json = r#"{
            "events": [
                {
                    "event": {
                        "body": {
                            "v0": {
                                "topics": [{"symbol": "shipment_created"}],
                                "data": {
                                    "vec": [
                                        {"u64": 1},
                                        {"address": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4"},
                                        {"u32": 2},
                                        {"u32": 1},
                                        {"bytes": "4d665e5885d370938b6ef4915d3e18cce2280979a315d468afc7bef8d99362b4"}
                                    ]
                                }
                            }
                        }
                    }
                }
            ]
        }"#;
        let sanitized = sanitize_json_snapshot(json);
        assert!(
            sanitized.contains(r#""bytes": "0000000000000000000000000000000000000000000000000000000000000000""#),
            "Event idempotency key (last bytes field) should be normalized to zero hash"
        );
    }

    #[test]
    fn test_sanitize_json_snapshot_preserves_non_idempotency_bytes() {
        let json = r#"{
            "events": [
                {
                    "event": {
                        "body": {
                            "v0": {
                                "topics": [{"symbol": "shipment_created"}],
                                "data": {
                                    "vec": [
                                        {"bytes": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
                                        {"u64": 1},
                                        {"bytes": "4d665e5885d370938b6ef4915d3e18cce2280979a315d468afc7bef8d99362b4"}
                                    ]
                                }
                            }
                        }
                    }
                }
            ]
        }"#;
        let sanitized = sanitize_json_snapshot(json);
        // First bytes field (data hash) should be preserved
        assert!(
            sanitized.contains(r#""bytes": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa""#),
            "Non-idempotency bytes fields should be preserved"
        );
        // Last bytes field (idempotency key) should be normalized
        assert!(
            sanitized.contains(r#""bytes": "0000000000000000000000000000000000000000000000000000000000000000""#),
            "Last bytes field (idempotency key) should be normalized"
        );
    }

    #[test]
    fn test_sanitize_json_snapshot_handles_multiple_events() {
        let json = r#"{
            "events": [
                {
                    "event": {
                        "contract_id": "0000000000000000000000000000000000000000000000000000000000000001",
                        "body": {
                            "v0": {
                                "data": {
                                    "vec": [
                                        {"u64": 1},
                                        {"bytes": "1111111111111111111111111111111111111111111111111111111111111111"}
                                    ]
                                }
                            }
                        }
                    }
                },
                {
                    "event": {
                        "contract_id": "0000000000000000000000000000000000000000000000000000000000000002",
                        "body": {
                            "v0": {
                                "data": {
                                    "vec": [
                                        {"u64": 2},
                                        {"bytes": "2222222222222222222222222222222222222222222222222222222222222222"}
                                    ]
                                }
                            }
                        }
                    }
                }
            ]
        }"#;
        let sanitized = sanitize_json_snapshot(json);
        // All contract_ids should be normalized
        let zero_hash = r#""contract_id": "0000000000000000000000000000000000000000000000000000000000000000""#;
        assert_eq!(
            sanitized.matches(zero_hash).count(),
            2,
            "All event contract_ids should be normalized"
        );
        // All idempotency keys should be normalized
        let zero_bytes = r#""bytes": "0000000000000000000000000000000000000000000000000000000000000000""#;
        assert_eq!(
            sanitized.matches(zero_bytes).count(),
            2,
            "All event idempotency keys should be normalized"
        );
    }
}

