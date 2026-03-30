//! # Circuit Breaker Module
//!
//! Implements circuit breaker pattern for external token transfer operations.
//! Prevents cascading failures by tracking consecutive failures and entering
//! "open" state to reject new attempts until recovery.
//!
//! ## States
//!
//! - **Closed**: Normal operation, requests pass through
//! - **Open**: Failures exceeded threshold, requests rejected
//! - **HalfOpen**: Recovery window active, testing if service recovered
//!
//! ## Features
//!
//! - Automatic recovery after time window
//! - Admin manual reset capability
//! - Comprehensive state transition tests
//! - Clear error messages

use crate::{errors::NavinError, types::*};
use soroban_sdk::{contracttype, Address, Env};

/// Circuit breaker states
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum CircuitBreakerState {
    /// Normal operation, requests pass through
    Closed,
    /// Failures exceeded threshold, requests rejected
    Open,
    /// Recovery window active, testing recovery
    HalfOpen,
}

/// Circuit breaker configuration
#[contracttype]
#[derive(Clone, Debug)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening
    pub failure_threshold: u32,
    /// Time window in seconds before attempting recovery
    pub recovery_timeout: u64,
    /// Maximum requests allowed in HalfOpen state
    pub half_open_max_requests: u32,
}

impl CircuitBreakerConfig {
    /// Create a new circuit breaker configuration
    pub fn new(failure_threshold: u32, recovery_timeout: u64, half_open_max_requests: u32) -> Self {
        CircuitBreakerConfig {
            failure_threshold,
            recovery_timeout,
            half_open_max_requests,
        }
    }

    /// Default configuration: 5 failures, 300 second recovery, 3 half-open requests
    pub fn default() -> Self {
        CircuitBreakerConfig {
            failure_threshold: 5,
            recovery_timeout: 300,
            half_open_max_requests: 3,
        }
    }

    /// Strict configuration: 3 failures, 600 second recovery, 1 half-open request
    pub fn strict() -> Self {
        CircuitBreakerConfig {
            failure_threshold: 3,
            recovery_timeout: 600,
            half_open_max_requests: 1,
        }
    }

    /// Permissive configuration: 10 failures, 60 second recovery, 5 half-open requests
    pub fn permissive() -> Self {
        CircuitBreakerConfig {
            failure_threshold: 10,
            recovery_timeout: 60,
            half_open_max_requests: 5,
        }
    }
}

/// Circuit breaker state tracker
#[contracttype]
#[derive(Clone, Debug)]
pub struct CircuitBreakerTracker {
    /// Current state
    pub state: CircuitBreakerState,
    /// Number of consecutive failures
    pub failure_count: u32,
    /// Timestamp when breaker was opened
    pub opened_at: u64,
    /// Number of requests in HalfOpen state
    pub half_open_requests: u32,
}

impl CircuitBreakerTracker {
    /// Create a new circuit breaker tracker in Closed state
    pub fn new() -> Self {
        CircuitBreakerTracker {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            opened_at: 0,
            half_open_requests: 0,
        }
    }

    /// Record a successful operation
    pub fn record_success(&mut self) {
        match self.state {
            CircuitBreakerState::Closed => {
                // Already closed, nothing to do
            }
            CircuitBreakerState::Open => {
                // Cannot succeed while open
            }
            CircuitBreakerState::HalfOpen => {
                // Success in HalfOpen means recovery succeeded
                self.state = CircuitBreakerState::Closed;
                self.failure_count = 0;
                self.half_open_requests = 0;
            }
        }
    }

    /// Record a failed operation
    pub fn record_failure(&mut self, config: &CircuitBreakerConfig, current_time: u64) {
        match self.state {
            CircuitBreakerState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= config.failure_threshold {
                    self.state = CircuitBreakerState::Open;
                    self.opened_at = current_time;
                }
            }
            CircuitBreakerState::Open => {
                // Already open, increment failure count
                self.failure_count += 1;
            }
            CircuitBreakerState::HalfOpen => {
                // Failure in HalfOpen means recovery failed
                self.state = CircuitBreakerState::Open;
                self.opened_at = current_time;
                self.half_open_requests = 0;
            }
        }
    }

    /// Check if request should be allowed
    pub fn should_allow_request(
        &mut self,
        config: &CircuitBreakerConfig,
        current_time: u64,
    ) -> Result<(), NavinError> {
        match self.state {
            CircuitBreakerState::Closed => Ok(()),
            CircuitBreakerState::Open => {
                // Check if recovery timeout has passed
                if current_time >= self.opened_at + config.recovery_timeout {
                    // Transition to HalfOpen
                    self.state = CircuitBreakerState::HalfOpen;
                    self.half_open_requests = 1;
                    Ok(())
                } else {
                    Err(NavinError::CircuitBreakerOpen)
                }
            }
            CircuitBreakerState::HalfOpen => {
                // Allow limited requests in HalfOpen
                if self.half_open_requests < config.half_open_max_requests {
                    self.half_open_requests += 1;
                    Ok(())
                } else {
                    Err(NavinError::CircuitBreakerOpen)
                }
            }
        }
    }

    /// Get current state
    pub fn get_state(&self) -> CircuitBreakerState {
        self.state.clone()
    }

    /// Get failure count
    pub fn get_failure_count(&self) -> u32 {
        self.failure_count
    }

    /// Get time until recovery attempt (0 if already in recovery)
    pub fn get_recovery_time_remaining(
        &self,
        config: &CircuitBreakerConfig,
        current_time: u64,
    ) -> u64 {
        match self.state {
            CircuitBreakerState::Closed => 0,
            CircuitBreakerState::Open => {
                let recovery_time = self.opened_at + config.recovery_timeout;
                recovery_time.saturating_sub(current_time)
            }
            CircuitBreakerState::HalfOpen => 0,
        }
    }
}

/// Check if a token transfer operation should be allowed
///
/// # Arguments
/// * `env` - The execution environment
/// * `config` - Circuit breaker configuration
///
/// # Returns
/// * `Ok(())` if operation should proceed
/// * `Err(NavinError::CircuitBreakerOpen)` if breaker is open
#[allow(dead_code)]
pub fn check_transfer_allowed(env: &Env, config: &CircuitBreakerConfig) -> Result<(), NavinError> {
    let current_time = env.ledger().timestamp();
    let breaker_key = DataKey::CircuitBreakerState;

    let mut breaker: CircuitBreakerTracker = env
        .storage()
        .persistent()
        .get(&breaker_key)
        .unwrap_or_else(CircuitBreakerTracker::new);

    breaker.should_allow_request(config, current_time)?;

    // Persist updated breaker state
    env.storage().persistent().set(&breaker_key, &breaker);

    Ok(())
}

/// Record a successful token transfer
///
/// # Arguments
/// * `env` - The execution environment
#[allow(dead_code)]
pub fn record_transfer_success(env: &Env) {
    let breaker_key = DataKey::CircuitBreakerState;

    let mut breaker: CircuitBreakerTracker = env
        .storage()
        .persistent()
        .get(&breaker_key)
        .unwrap_or_else(CircuitBreakerTracker::new);

    breaker.record_success();

    // Persist updated breaker state
    env.storage().persistent().set(&breaker_key, &breaker);
}

/// Record a failed token transfer
///
/// # Arguments
/// * `env` - The execution environment
/// * `config` - Circuit breaker configuration
#[allow(dead_code)]
pub fn record_transfer_failure(env: &Env, config: &CircuitBreakerConfig) {
    let current_time = env.ledger().timestamp();
    let breaker_key = DataKey::CircuitBreakerState;

    let mut breaker: CircuitBreakerTracker = env
        .storage()
        .persistent()
        .get(&breaker_key)
        .unwrap_or_else(CircuitBreakerTracker::new);

    breaker.record_failure(config, current_time);

    // Emit circuit breaker event if state changed to Open
    if breaker.state == CircuitBreakerState::Open {
        emit_breaker_opened_event(env, breaker.failure_count);
    }

    // Persist updated breaker state
    env.storage().persistent().set(&breaker_key, &breaker);
}

/// Manually reset the circuit breaker (admin-only)
///
/// # Arguments
/// * `env` - The execution environment
/// * `admin` - The admin address
///
/// # Returns
/// * `Ok(())` on success
/// * `Err(NavinError)` if not authorized
#[allow(dead_code)]
pub fn manual_reset(env: &Env, admin: &Address) -> Result<(), NavinError> {
    // Verify admin authorization
    admin.require_auth();
    if !crate::storage::is_admin(env, admin) {
        return Err(NavinError::Unauthorized);
    }

    let breaker_key = DataKey::CircuitBreakerState;
    let new_breaker = CircuitBreakerTracker::new();

    env.storage().persistent().set(&breaker_key, &new_breaker);

    // Emit reset event
    emit_breaker_reset_event(env, admin);

    Ok(())
}

/// Get current circuit breaker status
///
/// # Arguments
/// * `env` - The execution environment
/// * `config` - Circuit breaker configuration
///
/// # Returns
/// * `(state, failure_count, recovery_time_remaining)` tuple
#[allow(dead_code)]
pub fn get_breaker_status(
    env: &Env,
    config: &CircuitBreakerConfig,
) -> (CircuitBreakerState, u32, u64) {
    let current_time = env.ledger().timestamp();
    let breaker_key = DataKey::CircuitBreakerState;

    let breaker: CircuitBreakerTracker = env
        .storage()
        .persistent()
        .get(&breaker_key)
        .unwrap_or_else(CircuitBreakerTracker::new);

    let recovery_time = breaker.get_recovery_time_remaining(config, current_time);

    (breaker.state, breaker.failure_count, recovery_time)
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

fn emit_breaker_opened_event(env: &Env, failure_count: u32) {
    env.events().publish(
        (soroban_sdk::Symbol::new(env, "circuit_breaker_opened"),),
        (failure_count, env.ledger().timestamp()),
    );
}

fn emit_breaker_reset_event(env: &Env, admin: &Address) {
    env.events().publish(
        (soroban_sdk::Symbol::new(env, "circuit_breaker_reset"),),
        (admin.clone(), env.ledger().timestamp()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_new() {
        let breaker = CircuitBreakerTracker::new();
        assert_eq!(breaker.state, CircuitBreakerState::Closed);
        assert_eq!(breaker.failure_count, 0);
    }

    #[test]
    fn test_circuit_breaker_closed_allows_requests() {
        let mut breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::default();

        let result = breaker.should_allow_request(&config, 1000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_circuit_breaker_opens_on_threshold() {
        let mut breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::new(3, 300, 3);

        // Record failures
        breaker.record_failure(&config, 1000);
        assert_eq!(breaker.state, CircuitBreakerState::Closed);

        breaker.record_failure(&config, 1000);
        assert_eq!(breaker.state, CircuitBreakerState::Closed);

        breaker.record_failure(&config, 1000);
        assert_eq!(breaker.state, CircuitBreakerState::Open);
    }

    #[test]
    fn test_circuit_breaker_rejects_when_open() {
        let mut breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::new(1, 300, 3);

        breaker.record_failure(&config, 1000);
        assert_eq!(breaker.state, CircuitBreakerState::Open);

        let result = breaker.should_allow_request(&config, 1100);
        assert!(result.is_err());
    }

    #[test]
    fn test_circuit_breaker_half_open_after_timeout() {
        let mut breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::new(1, 300, 3);

        breaker.record_failure(&config, 1000);
        assert_eq!(breaker.state, CircuitBreakerState::Open);

        // Before timeout
        let result = breaker.should_allow_request(&config, 1200);
        assert!(result.is_err());

        // After timeout
        let result = breaker.should_allow_request(&config, 1400);
        assert!(result.is_ok());
        assert_eq!(breaker.state, CircuitBreakerState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_success_closes() {
        let mut breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::default();

        breaker.record_failure(&config, 1000);
        breaker.state = CircuitBreakerState::HalfOpen;

        breaker.record_success();
        assert_eq!(breaker.state, CircuitBreakerState::Closed);
        assert_eq!(breaker.failure_count, 0);
    }

    #[test]
    fn test_circuit_breaker_failure_in_half_open_reopens() {
        let mut breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::default();

        breaker.state = CircuitBreakerState::HalfOpen;
        breaker.record_failure(&config, 1000);

        assert_eq!(breaker.state, CircuitBreakerState::Open);
    }

    #[test]
    fn test_circuit_breaker_configs() {
        let default = CircuitBreakerConfig::default();
        assert_eq!(default.failure_threshold, 5);
        assert_eq!(default.recovery_timeout, 300);

        let strict = CircuitBreakerConfig::strict();
        assert_eq!(strict.failure_threshold, 3);

        let permissive = CircuitBreakerConfig::permissive();
        assert_eq!(permissive.failure_threshold, 10);
    }

    #[test]
    fn test_recovery_time_remaining() {
        let mut breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::new(1, 300, 3);

        breaker.record_failure(&config, 1000);
        assert_eq!(breaker.state, CircuitBreakerState::Open);

        // 100 seconds after opening
        let remaining = breaker.get_recovery_time_remaining(&config, 1100);
        assert_eq!(remaining, 200);

        // After timeout
        let remaining = breaker.get_recovery_time_remaining(&config, 1400);
        assert_eq!(remaining, 0);
    }
}
