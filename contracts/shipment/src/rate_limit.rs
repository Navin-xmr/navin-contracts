//! # Rate Limiting Module
//!
//! Implements sliding window rate limiting and quota enforcement for resource-intensive
//! operations like batch shipment creation and bulk status updates.
//!
//! ## Design
//!
//! - Per-actor rate limits with configurable windows
//! - Sliding window quota tracking
//! - Admin configuration of limits
//! - Clear error messages on quota exceeded
//! - No panic paths in rate limit code

use crate::{errors::NavinError, types::*};
use soroban_sdk::{contracttype, Address, Env};

/// Rate limit configuration for an actor (company or carrier)
#[contracttype]
#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    /// Maximum operations allowed per window
    pub max_operations: u32,
    /// Time window in seconds
    pub window_seconds: u64,
}

impl RateLimitConfig {
    /// Create a new rate limit configuration
    pub fn new(max_operations: u32, window_seconds: u64) -> Self {
        RateLimitConfig {
            max_operations,
            window_seconds,
        }
    }

    /// Default rate limit: 100 operations per hour
    pub fn default() -> Self {
        RateLimitConfig {
            max_operations: 100,
            window_seconds: 3600,
        }
    }

    /// Strict rate limit: 10 operations per hour
    pub fn strict() -> Self {
        RateLimitConfig {
            max_operations: 10,
            window_seconds: 3600,
        }
    }

    /// Permissive rate limit: 1000 operations per hour
    pub fn permissive() -> Self {
        RateLimitConfig {
            max_operations: 1000,
            window_seconds: 3600,
        }
    }
}

/// Rate limit quota tracking for an actor
#[contracttype]
#[derive(Clone, Debug)]
pub struct QuotaTracker {
    /// Number of operations in current window
    pub operations_count: u32,
    /// Timestamp when current window started
    pub window_start: u64,
}

impl QuotaTracker {
    /// Create a new quota tracker
    pub fn new(current_time: u64) -> Self {
        QuotaTracker {
            operations_count: 0,
            window_start: current_time,
        }
    }

    /// Check if quota is exceeded and update tracker
    pub fn check_and_update(
        &mut self,
        current_time: u64,
        config: &RateLimitConfig,
        operations: u32,
    ) -> Result<(), NavinError> {
        // Check if window has expired
        if current_time >= self.window_start + config.window_seconds {
            // Reset window
            self.window_start = current_time;
            self.operations_count = 0;
        }

        // Check if adding operations would exceed limit
        let new_count = self
            .operations_count
            .checked_add(operations)
            .ok_or(NavinError::CounterOverflow)?;

        if new_count > config.max_operations {
            return Err(NavinError::RateLimitExceeded);
        }

        // Update tracker
        self.operations_count = new_count;
        Ok(())
    }

    /// Get remaining quota in current window
    pub fn remaining_quota(&self, config: &RateLimitConfig) -> u32 {
        config.max_operations.saturating_sub(self.operations_count)
    }

    /// Get time remaining in current window (seconds)
    pub fn window_time_remaining(&self, current_time: u64, config: &RateLimitConfig) -> u64 {
        let window_end = self.window_start + config.window_seconds;
        window_end.saturating_sub(current_time)
    }
}

/// Check rate limit for batch operations
///
/// # Arguments
/// * `env` - The execution environment
/// * `actor` - The address performing the operation
/// * `operation_count` - Number of operations in batch
/// * `config` - Rate limit configuration
///
/// # Returns
/// * `Ok(())` if within quota
/// * `Err(NavinError::RateLimitExceeded)` if quota exceeded
///
/// # Examples
/// ```rust
/// // check_rate_limit(&env, &company, 5, &RateLimitConfig::default())?;
/// ```
#[allow(dead_code)]
pub fn check_rate_limit(
    env: &Env,
    actor: &Address,
    operation_count: u32,
    config: &RateLimitConfig,
) -> Result<(), NavinError> {
    if operation_count == 0 {
        return Ok(());
    }

    let current_time = env.ledger().timestamp();
    let quota_key = DataKey::ActorQuota(actor.clone());

    // Get or create quota tracker
    let mut tracker: QuotaTracker = env
        .storage()
        .persistent()
        .get(&quota_key)
        .unwrap_or_else(|| QuotaTracker::new(current_time));

    // Check and update quota
    tracker.check_and_update(current_time, config, operation_count)?;

    // Persist updated tracker
    env.storage().persistent().set(&quota_key, &tracker);

    Ok(())
}

/// Reset quota for an actor (admin-only)
///
/// # Arguments
/// * `env` - The execution environment
/// * `admin` - The admin address
/// * `actor` - The actor to reset quota for
///
/// # Returns
/// * `Ok(())` on success
/// * `Err(NavinError)` if not authorized
#[allow(dead_code)]
pub fn reset_quota(env: &Env, admin: &Address, actor: &Address) -> Result<(), NavinError> {
    // Verify admin authorization
    admin.require_auth();
    if !crate::storage::is_admin(env, admin) {
        return Err(NavinError::Unauthorized);
    }

    let quota_key = DataKey::ActorQuota(actor.clone());
    env.storage().persistent().remove(&quota_key);

    Ok(())
}

/// Get current quota status for an actor
///
/// # Arguments
/// * `env` - The execution environment
/// * `actor` - The actor to check quota for
/// * `config` - Rate limit configuration
///
/// # Returns
/// * `(used, remaining, window_time_remaining)` tuple
#[allow(dead_code)]
pub fn get_quota_status(env: &Env, actor: &Address, config: &RateLimitConfig) -> (u32, u32, u64) {
    let current_time = env.ledger().timestamp();
    let quota_key = DataKey::ActorQuota(actor.clone());

    let tracker: QuotaTracker = env
        .storage()
        .persistent()
        .get(&quota_key)
        .unwrap_or_else(|| QuotaTracker::new(current_time));

    let remaining = tracker.remaining_quota(config);
    let window_remaining = tracker.window_time_remaining(current_time, config);

    (tracker.operations_count, remaining, window_remaining)
}

/// Enforce rate limit for batch shipment creation
///
/// # Arguments
/// * `env` - The execution environment
/// * `company` - The company creating shipments
/// * `batch_size` - Number of shipments in batch
///
/// # Returns
/// * `Ok(())` if within quota
/// * `Err(NavinError)` if quota exceeded or not authorized
#[allow(dead_code)]
pub fn enforce_batch_creation_limit(
    env: &Env,
    company: &Address,
    batch_size: u32,
) -> Result<(), NavinError> {
    // Get default rate limit config (100 per hour)
    let config = RateLimitConfig::default();
    check_rate_limit(env, company, batch_size, &config)
}

/// Enforce rate limit for bulk status updates
///
/// # Arguments
/// * `env` - The execution environment
/// * `carrier` - The carrier updating statuses
/// * `update_count` - Number of status updates
///
/// # Returns
/// * `Ok(())` if within quota
/// * `Err(NavinError)` if quota exceeded or not authorized
#[allow(dead_code)]
pub fn enforce_bulk_update_limit(
    env: &Env,
    carrier: &Address,
    update_count: u32,
) -> Result<(), NavinError> {
    // Get stricter rate limit for status updates (50 per hour)
    let config = RateLimitConfig::new(50, 3600);
    check_rate_limit(env, carrier, update_count, &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_tracker_new() {
        let tracker = QuotaTracker::new(1000);
        assert_eq!(tracker.operations_count, 0);
        assert_eq!(tracker.window_start, 1000);
    }

    #[test]
    fn test_quota_tracker_check_and_update_within_limit() {
        let mut tracker = QuotaTracker::new(1000);
        let config = RateLimitConfig::new(10, 3600);

        let result = tracker.check_and_update(1000, &config, 5);
        assert!(result.is_ok());
        assert_eq!(tracker.operations_count, 5);
    }

    #[test]
    fn test_quota_tracker_check_and_update_exceeds_limit() {
        let mut tracker = QuotaTracker::new(1000);
        let config = RateLimitConfig::new(10, 3600);

        // First batch OK
        let _ = tracker.check_and_update(1000, &config, 8);

        // Second batch exceeds limit
        let result = tracker.check_and_update(1000, &config, 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_quota_tracker_window_reset() {
        let mut tracker = QuotaTracker::new(1000);
        let config = RateLimitConfig::new(10, 3600);

        // Fill quota
        let _ = tracker.check_and_update(1000, &config, 10);
        assert_eq!(tracker.operations_count, 10);

        // Window expires, quota resets
        let result = tracker.check_and_update(5000, &config, 5);
        assert!(result.is_ok());
        assert_eq!(tracker.operations_count, 5);
        assert_eq!(tracker.window_start, 5000);
    }

    #[test]
    fn test_remaining_quota() {
        let mut tracker = QuotaTracker::new(1000);
        let config = RateLimitConfig::new(10, 3600);

        let _ = tracker.check_and_update(1000, &config, 3);
        assert_eq!(tracker.remaining_quota(&config), 7);
    }

    #[test]
    fn test_window_time_remaining() {
        let tracker = QuotaTracker::new(1000);
        let config = RateLimitConfig::new(10, 3600);

        // 1000 seconds into window
        assert_eq!(tracker.window_time_remaining(1000, &config), 3600);

        // 2000 seconds into window
        assert_eq!(tracker.window_time_remaining(2000, &config), 2600);

        // Window expired
        assert_eq!(tracker.window_time_remaining(5000, &config), 0);
    }

    #[test]
    fn test_rate_limit_configs() {
        let default = RateLimitConfig::default();
        assert_eq!(default.max_operations, 100);
        assert_eq!(default.window_seconds, 3600);

        let strict = RateLimitConfig::strict();
        assert_eq!(strict.max_operations, 10);

        let permissive = RateLimitConfig::permissive();
        assert_eq!(permissive.max_operations, 1000);
    }
}
