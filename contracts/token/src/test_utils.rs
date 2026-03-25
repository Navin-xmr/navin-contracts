//! Shared test utilities for deterministic Soroban SDK testing.
//!
//! This module provides helper functions to set up test environments
//! with explicit protocol version, timestamp, and sequence number
//! to ensure deterministic behavior across all tests.

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

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

#[cfg(test)]
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
}
