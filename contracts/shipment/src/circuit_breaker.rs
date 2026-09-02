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
#[derive(Clone, Debug, PartialEq)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening
    pub failure_threshold: u32,
    /// Time window in seconds before attempting recovery
    pub recovery_timeout: u64,
    /// Maximum requests allowed in HalfOpen state
    pub half_open_max_requests: u32,
}

impl Default for CircuitBreakerConfig {
    /// Default configuration: 5 failures, 300 second recovery, 3 half-open requests
    fn default() -> Self {
        CircuitBreakerConfig {
            failure_threshold: 5,
            recovery_timeout: 300,
            half_open_max_requests: 3,
        }
    }
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

/// Selectable presets for [`set_circuit_breaker_config`].
///
/// Exposing named presets keeps the common cases a single argument, while
/// [`CircuitBreakerPreset::Custom`] still allows explicit tuning.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum CircuitBreakerPreset {
    /// 5 failures, 300s recovery, 3 half-open requests.
    Default,
    /// 3 failures, 600s recovery, 1 half-open request.
    Strict,
    /// 10 failures, 60s recovery, 5 half-open requests.
    Permissive,
    /// Explicit `(failure_threshold, recovery_timeout, half_open_max_requests)`.
    Custom(u32, u64, u32),
}

/// Upper bounds for `Custom`, so a mistyped value cannot wedge transfers.
/// A zero `failure_threshold` would open the breaker immediately and block
/// every transfer; an unbounded `recovery_timeout` would keep it open.
const MAX_FAILURE_THRESHOLD: u32 = 1_000;
const MAX_RECOVERY_TIMEOUT: u64 = 30 * 24 * 60 * 60; // 30 days
const MAX_HALF_OPEN_REQUESTS: u32 = 1_000;

impl CircuitBreakerPreset {
    /// Resolve to a concrete config, validating `Custom` values.
    pub fn resolve(&self) -> Result<CircuitBreakerConfig, NavinError> {
        match self {
            CircuitBreakerPreset::Default => Ok(CircuitBreakerConfig::default()),
            CircuitBreakerPreset::Strict => Ok(CircuitBreakerConfig::strict()),
            CircuitBreakerPreset::Permissive => Ok(CircuitBreakerConfig::permissive()),
            CircuitBreakerPreset::Custom(failure_threshold, recovery_timeout, half_open_max) => {
                if *failure_threshold == 0
                    || *failure_threshold > MAX_FAILURE_THRESHOLD
                    || *recovery_timeout > MAX_RECOVERY_TIMEOUT
                    || *half_open_max == 0
                    || *half_open_max > MAX_HALF_OPEN_REQUESTS
                {
                    return Err(NavinError::InvalidConfig);
                }
                Ok(CircuitBreakerConfig::new(
                    *failure_threshold,
                    *recovery_timeout,
                    *half_open_max,
                ))
            }
        }
    }
}

/// Persist the admin-selected circuit breaker configuration.
pub fn set_config(env: &Env, config: &CircuitBreakerConfig) {
    env.storage()
        .persistent()
        .set(&DataKey::CircuitBreakerConfig, config);
}

/// Read the active circuit breaker configuration.
///
/// Falls back to [`CircuitBreakerConfig::default`] when an admin has never set
/// one, so existing deployments keep their current behaviour.
pub fn get_config(env: &Env) -> CircuitBreakerConfig {
    env.storage()
        .persistent()
        .get(&DataKey::CircuitBreakerConfig)
        .unwrap_or_default()
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

impl Default for CircuitBreakerTracker {
    fn default() -> Self {
        Self::new()
    }
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
pub fn check_transfer_allowed(env: &Env, config: &CircuitBreakerConfig) -> Result<(), NavinError> {
    let current_time = env.ledger().timestamp();
    let breaker_key = DataKey::CircuitBreakerState;

    let mut breaker: CircuitBreakerTracker = env
        .storage()
        .persistent()
        .get(&breaker_key)
        .unwrap_or_default();

    breaker.should_allow_request(config, current_time)?;

    // Persist updated breaker state
    env.storage().persistent().set(&breaker_key, &breaker);

    Ok(())
}

/// Record a successful token transfer
///
/// # Arguments
/// * `env` - The execution environment
pub fn record_transfer_success(env: &Env) {
    let breaker_key = DataKey::CircuitBreakerState;

    let mut breaker: CircuitBreakerTracker = env
        .storage()
        .persistent()
        .get(&breaker_key)
        .unwrap_or_default();

    breaker.record_success();

    // Persist updated breaker state
    env.storage().persistent().set(&breaker_key, &breaker);
}

/// Record a failed token transfer
///
/// # Arguments
/// * `env` - The execution environment
/// * `config` - Circuit breaker configuration
pub fn record_transfer_failure(env: &Env, config: &CircuitBreakerConfig) {
    let current_time = env.ledger().timestamp();
    let breaker_key = DataKey::CircuitBreakerState;

    let mut breaker: CircuitBreakerTracker = env
        .storage()
        .persistent()
        .get(&breaker_key)
        .unwrap_or_default();

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
        .unwrap_or_default();

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

    // ── [ISSUE #597] CircuitBreakerOpen error variant tests ──────────────────
    //
    // CircuitBreakerOpen (#46) is returned by should_allow_request / check_transfer_allowed
    // when the breaker is in Open state and the recovery timeout has not elapsed,
    // or when in HalfOpen state and the max probe requests have been exhausted.

    /// Error code pin: CircuitBreakerOpen discriminant must be exactly 46.
    #[test]
    fn test_circuit_breaker_open_error_code_is_46() {
        use crate::NavinError;
        assert_eq!(
            NavinError::CircuitBreakerOpen as u32,
            46,
            "CircuitBreakerOpen discriminant must be 46"
        );
    }

    /// A breaker in Open state before the recovery timeout must return
    /// CircuitBreakerOpen from should_allow_request.
    #[test]
    fn test_circuit_breaker_open_returns_circuit_breaker_open_error() {
        use crate::NavinError;
        let mut breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::new(1, 300, 3);

        // One failure opens the breaker (threshold = 1).
        breaker.record_failure(&config, 1000);
        assert_eq!(breaker.state, CircuitBreakerState::Open);

        // Still within recovery window — must be rejected.
        let result = breaker.should_allow_request(&config, 1200);
        assert_eq!(
            result,
            Err(NavinError::CircuitBreakerOpen),
            "Open breaker before recovery timeout must return CircuitBreakerOpen"
        );
    }

    /// A closed breaker must allow requests (no CircuitBreakerOpen error).
    #[test]
    fn test_closed_breaker_allows_requests() {
        let breaker = CircuitBreakerTracker::new();
        assert_eq!(breaker.state, CircuitBreakerState::Closed);

        let mut b = breaker;
        let config = CircuitBreakerConfig::default();
        let result = b.should_allow_request(&config, 1000);
        assert!(result.is_ok(), "Closed breaker must allow requests");
    }

    /// After the recovery timeout elapses, the first request transitions the
    /// breaker to HalfOpen and is allowed through.
    #[test]
    fn test_open_breaker_allows_request_after_recovery_timeout() {
        let mut breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::new(1, 300, 3);

        breaker.record_failure(&config, 1000);
        assert_eq!(breaker.state, CircuitBreakerState::Open);

        // Exactly at timeout boundary (1000 + 300 = 1300 — still open).
        let at_boundary = breaker.should_allow_request(&config, 1300);
        assert!(
            at_boundary.is_ok(),
            "Request at recovery boundary must be allowed"
        );
        assert_eq!(breaker.state, CircuitBreakerState::HalfOpen);
    }

    /// In HalfOpen state, requests up to half_open_max_requests are allowed;
    /// excess requests return CircuitBreakerOpen.
    #[test]
    fn test_half_open_exhausted_returns_circuit_breaker_open() {
        use crate::NavinError;
        let mut breaker = CircuitBreakerTracker::new();
        // threshold=1, recovery=300, max_half_open=2
        let config = CircuitBreakerConfig::new(1, 300, 2);

        breaker.record_failure(&config, 1000);
        // Advance past recovery timeout → HalfOpen on first probe.
        breaker.should_allow_request(&config, 1301).unwrap();
        assert_eq!(breaker.state, CircuitBreakerState::HalfOpen);

        // Second probe — still within limit.
        breaker.should_allow_request(&config, 1302).unwrap();

        // Third probe — exceeds half_open_max_requests=2.
        let result = breaker.should_allow_request(&config, 1303);
        assert_eq!(
            result,
            Err(NavinError::CircuitBreakerOpen),
            "HalfOpen state with exhausted probes must return CircuitBreakerOpen"
        );
    }

    /// A success in HalfOpen closes the breaker and subsequent requests are allowed.
    #[test]
    fn test_half_open_success_closes_breaker_and_allows_transfers() {
        let mut breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::new(1, 300, 3);

        breaker.record_failure(&config, 1000);
        // Transition to HalfOpen.
        breaker.should_allow_request(&config, 1400).unwrap();
        assert_eq!(breaker.state, CircuitBreakerState::HalfOpen);

        // Record success → Closed.
        breaker.record_success();
        assert_eq!(breaker.state, CircuitBreakerState::Closed);
        assert_eq!(breaker.failure_count, 0);

        // Requests must now be allowed without error.
        let result = breaker.should_allow_request(&config, 1500);
        assert!(
            result.is_ok(),
            "Closed breaker after recovery must allow requests"
        );
    }

    /// A failure in HalfOpen re-opens the breaker and subsequent requests are
    /// rejected with CircuitBreakerOpen.
    #[test]
    fn test_half_open_failure_reopens_and_rejects_transfers() {
        use crate::NavinError;
        let mut breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::new(1, 300, 3);

        breaker.record_failure(&config, 1000);
        // Advance past timeout to HalfOpen.
        breaker.should_allow_request(&config, 1400).unwrap();
        assert_eq!(breaker.state, CircuitBreakerState::HalfOpen);

        // Record another failure → re-opens.
        breaker.record_failure(&config, 1401);
        assert_eq!(breaker.state, CircuitBreakerState::Open);

        // Immediately after re-opening, within the new recovery window.
        let result = breaker.should_allow_request(&config, 1402);
        assert_eq!(
            result,
            Err(NavinError::CircuitBreakerOpen),
            "Re-opened breaker must reject with CircuitBreakerOpen"
        );
    }

    /// reset_circuit_breaker via manual_reset closes the breaker and clears
    /// failure count — subsequent requests are allowed.
    #[test]
    fn test_manual_reset_closes_breaker() {
        let mut breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::new(1, 300, 3);

        breaker.record_failure(&config, 1000);
        assert_eq!(breaker.state, CircuitBreakerState::Open);

        // Simulate a manual reset by replacing with a fresh tracker.
        breaker = CircuitBreakerTracker::new();
        assert_eq!(breaker.state, CircuitBreakerState::Closed);
        assert_eq!(breaker.failure_count, 0);

        let result = breaker.should_allow_request(&config, 1500);
        assert!(
            result.is_ok(),
            "Breaker must allow requests after manual reset"
        );
    }

    /// Failures below the threshold must not open the breaker.
    #[test]
    fn test_failures_below_threshold_do_not_open_breaker() {
        let mut breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::new(5, 300, 3);

        for _ in 0..4 {
            breaker.record_failure(&config, 1000);
        }
        assert_eq!(
            breaker.state,
            CircuitBreakerState::Closed,
            "4 failures with threshold=5 must not open the breaker"
        );
        assert_eq!(breaker.failure_count, 4);

        // Requests must still be allowed.
        let result = breaker.should_allow_request(&config, 1000);
        assert!(result.is_ok());
    }

    /// The failure that exactly hits the threshold opens the breaker.
    #[test]
    fn test_failure_at_threshold_opens_breaker() {
        let mut breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::new(3, 300, 3);

        breaker.record_failure(&config, 1000);
        breaker.record_failure(&config, 1000);
        assert_eq!(breaker.state, CircuitBreakerState::Closed);

        breaker.record_failure(&config, 1000); // third = threshold
        assert_eq!(
            breaker.state,
            CircuitBreakerState::Open,
            "Third failure at threshold=3 must open the breaker"
        );
    }

    /// recovery_time_remaining returns 0 when the breaker is Closed.
    #[test]
    fn test_recovery_time_remaining_zero_when_closed() {
        let breaker = CircuitBreakerTracker::new();
        let config = CircuitBreakerConfig::default();
        assert_eq!(breaker.get_recovery_time_remaining(&config, 5000), 0);
    }

    /// CircuitBreakerOpen (#46) is distinct from all other token transfer errors.
    #[test]
    fn test_circuit_breaker_open_error_is_distinct() {
        use crate::NavinError;
        assert_ne!(
            NavinError::CircuitBreakerOpen as u32,
            NavinError::TokenTransferFailed as u32,
        );
        assert_ne!(
            NavinError::CircuitBreakerOpen as u32,
            NavinError::ShipmentNotFound as u32,
        );
        assert_eq!(NavinError::CircuitBreakerOpen as u32, 46);
    }

    // ── issue #639: preset resolution and validation ─────────────────────────

    #[test]
    fn preset_resolves_to_matching_config() {
        assert_eq!(
            CircuitBreakerPreset::Default
                .resolve()
                .unwrap()
                .failure_threshold,
            CircuitBreakerConfig::default().failure_threshold
        );
        assert_eq!(
            CircuitBreakerPreset::Strict
                .resolve()
                .unwrap()
                .failure_threshold,
            3
        );
        assert_eq!(
            CircuitBreakerPreset::Permissive
                .resolve()
                .unwrap()
                .failure_threshold,
            10
        );
    }

    #[test]
    fn custom_preset_resolves_to_supplied_values() {
        let config = CircuitBreakerPreset::Custom(7, 120, 2).resolve().unwrap();
        assert_eq!(config.failure_threshold, 7);
        assert_eq!(config.recovery_timeout, 120);
        assert_eq!(config.half_open_max_requests, 2);
    }

    /// A zero threshold would open the breaker immediately and block every
    /// transfer, so it must be rejected rather than persisted.
    #[test]
    fn custom_preset_rejects_zero_threshold() {
        assert_eq!(
            CircuitBreakerPreset::Custom(0, 300, 3).resolve(),
            Err(NavinError::InvalidConfig)
        );
    }

    #[test]
    fn custom_preset_rejects_zero_half_open_requests() {
        assert_eq!(
            CircuitBreakerPreset::Custom(5, 300, 0).resolve(),
            Err(NavinError::InvalidConfig)
        );
    }

    #[test]
    fn custom_preset_rejects_out_of_range_values() {
        assert_eq!(
            CircuitBreakerPreset::Custom(MAX_FAILURE_THRESHOLD + 1, 300, 3).resolve(),
            Err(NavinError::InvalidConfig)
        );
        assert_eq!(
            CircuitBreakerPreset::Custom(5, MAX_RECOVERY_TIMEOUT + 1, 3).resolve(),
            Err(NavinError::InvalidConfig)
        );
        assert_eq!(
            CircuitBreakerPreset::Custom(5, 300, MAX_HALF_OPEN_REQUESTS + 1).resolve(),
            Err(NavinError::InvalidConfig)
        );
    }

    /// With nothing stored, the active config must be the built-in default —
    /// existing deployments keep their behaviour.
    #[test]
    fn get_config_falls_back_to_default() {
        let env = Env::default();
        let contract_id = env.register(crate::NavinShipment, ());

        env.as_contract(&contract_id, || {
            let config = get_config(&env);
            assert_eq!(config.failure_threshold, 5);
            assert_eq!(config.recovery_timeout, 300);
        });
    }

    #[test]
    fn set_config_is_read_back_by_get_config() {
        let env = Env::default();
        let contract_id = env.register(crate::NavinShipment, ());

        env.as_contract(&contract_id, || {
            set_config(&env, &CircuitBreakerConfig::strict());
            let config = get_config(&env);
            assert_eq!(config.failure_threshold, 3);
            assert_eq!(config.recovery_timeout, 600);
            assert_eq!(config.half_open_max_requests, 1);
        });
    }

    /// The stored config must actually drive breaker behaviour: under the
    /// strict preset the breaker opens after 3 failures rather than 5.
    #[test]
    fn stored_config_changes_when_breaker_opens() {
        let env = Env::default();
        let contract_id = env.register(crate::NavinShipment, ());

        env.as_contract(&contract_id, || {
            set_config(&env, &CircuitBreakerConfig::strict());
            let config = get_config(&env);

            let mut breaker = CircuitBreakerTracker::new();
            for _ in 0..3 {
                breaker.record_failure(&config, 1_000);
            }

            assert_eq!(
                breaker.state,
                CircuitBreakerState::Open,
                "strict preset must open the breaker after 3 failures"
            );
        });
    }
}

// ── End-to-end reset_circuit_breaker coverage (issue #704) ─────────────────────
//
// The public `reset_circuit_breaker` entry point was previously untested. Soroban
// rolls back all storage writes when a contract call returns Err, so the breaker
// cannot be tripped by repeatedly calling a failing transfer through the client.
// We therefore inject the Open state directly (matching the existing
// cross-contract integration pattern) and then exercise the admin reset path
// through the contract client.

#[cfg(test)]
mod reset_integration_tests {
    use crate::{
        CircuitBreakerState, NavinError, NavinShipment, NavinShipmentClient, ShipmentStatus,
    };
    use soroban_sdk::{
        contract, contractimpl,
        testutils::Address as _,
        Address, BytesN, Env, Vec,
    };

    /// Token whose `transfer` always succeeds, so post-reset transfers go through.
    #[contract]
    struct WorkingToken;

    #[contractimpl]
    impl WorkingToken {
        pub fn decimals(_env: Env) -> u32 {
            7
        }
        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    }

    fn setup() -> (Env, NavinShipmentClient<'static>, Address) {
        let env = Env::default();
        let admin = Address::generate(&env);
        let token = env.register(WorkingToken {}, ());
        let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
        client.initialize(&admin, &token);

        // `reset_circuit_breaker` authorizes via `is_admin`, which consults the
        // admin list (populated through `init_multisig` in production). Seed it
        // so the admin is recognized as an admin for the test.
        let mut admins = soroban_sdk::Vec::new(&env);
        admins.push_back(admin.clone());
        env.as_contract(&client.address, || {
            crate::storage::set_admin_list(&env, &admins);
        });

        (env, client, admin)
    }

    /// Inject an already-tripped (Open) circuit breaker directly into storage.
    fn inject_open_breaker(env: &Env, client: &NavinShipmentClient<'static>) {
        env.as_contract(&client.address, || {
            let tracker = crate::circuit_breaker::CircuitBreakerTracker {
                state: CircuitBreakerState::Open,
                failure_count: 5,
                opened_at: env.ledger().timestamp(),
                half_open_requests: 0,
            };
            env.storage()
                .persistent()
                .set(&crate::types::DataKey::CircuitBreakerState, &tracker);
        });
    }

    #[test]
    fn test_reset_circuit_breaker_closes_breaker() {
        let (env, client, admin) = setup();
        env.mock_all_auths();
        inject_open_breaker(&env, &client);

        // Sanity: breaker reports Open before the reset.
        let (state_before, _, _) = client.get_circuit_breaker_status();
        assert_eq!(state_before, CircuitBreakerState::Open);

        client.reset_circuit_breaker(&admin);

        let (state_after, failures, _) = client.get_circuit_breaker_status();
        assert_eq!(state_after, CircuitBreakerState::Closed);
        assert_eq!(failures, 0);
    }

    #[test]
    fn test_reset_circuit_breaker_rejects_non_admin() {
        let (env, client, admin) = setup();
        env.mock_all_auths();
        inject_open_breaker(&env, &client);

        let non_admin = Address::generate(&env);
        assert_ne!(non_admin, admin);

        let result = client.try_reset_circuit_breaker(&non_admin);
        assert!(
            matches!(result, Err(Ok(NavinError::Unauthorized))),
            "non-admin reset must be rejected with Unauthorized"
        );

        // Breaker must remain Open after the rejected attempt.
        let (state, _, _) = client.get_circuit_breaker_status();
        assert_eq!(state, CircuitBreakerState::Open);
    }

    #[test]
    fn test_reset_circuit_breaker_allows_subsequent_transfer() {
        let (env, client, admin) = setup();
        env.mock_all_auths();

        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);
        client.add_carrier_to_whitelist(&company, &carrier);

        inject_open_breaker(&env, &client);

        // Reset the breaker; it should now permit transfers again.
        client.reset_circuit_breaker(&admin);
        let (state, _, _) = client.get_circuit_breaker_status();
        assert_eq!(state, CircuitBreakerState::Closed);

        // Build a shipment with escrow and drive it to Delivered.
        let deadline = env.ledger().timestamp() + 3600;
        let data_hash = BytesN::from_array(&env, &[7u8; 32]);
        let receiver = Address::generate(&env);
        let id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &Vec::new(&env),
            &deadline,
        );

        env.as_contract(&client.address, || {
            let mut s = crate::storage::get_shipment(&env, id).unwrap();
            s.escrow_amount = 100;
            s.total_escrow = 100;
            crate::storage::set_shipment(&env, &s);
            crate::storage::set_escrow(&env, id, 100);
        });

        crate::test_utils::advance_past_rate_limit(&env);
        client.update_status(
            &carrier,
            &id,
            &ShipmentStatus::InTransit,
            &BytesN::from_array(&env, &[8u8; 32]),
        );
        crate::test_utils::advance_past_rate_limit(&env);
        client.update_status(
            &carrier,
            &id,
            &ShipmentStatus::Delivered,
            &BytesN::from_array(&env, &[9u8; 32]),
        );

        // Escrow release must now succeed (breaker closed + working token).
        client.release_escrow(&admin, &id);

        let shipment = client.get_shipment(&id);
        assert_eq!(
            shipment.escrow_amount, 0,
            "escrow must be released after reset"
        );
    }
}
