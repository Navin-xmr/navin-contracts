//! # Configuration Module
//!
//! Centralizes all tuneable contract parameters to enable post-deployment
//! configuration updates without requiring WASM upgrades.
//!
//! ## Design Philosophy
//!
//! Instead of hard-coding operational parameters (TTL thresholds, rate limits,
//! batch sizes), this module stores them in instance storage, allowing the
//! admin to adjust them dynamically as network conditions or business
//! requirements evolve.
//!
//! ## Configuration Parameters
//!
//! | Parameter                    | Default | Description                                    |
//! |------------------------------|---------|------------------------------------------------|
//! | shipment_ttl_threshold       | 17,280  | Min ledgers before TTL extension (~1 day)      |
//! | shipment_ttl_extension       | 518,400 | Ledgers to extend TTL by (~30 days)            |
//! | min_status_update_interval   | 60      | Min seconds between status updates             |
//! | batch_operation_limit        | 10      | Max items per batch operation                  |
//! | max_metadata_entries         | 5       | Max metadata key-value pairs per shipment      |
//! | default_shipment_limit       | 100     | Default active shipments per company           |
//! | multisig_min_admins          | 2       | Min admins for multi-sig                       |
//! | multisig_max_admins          | 10      | Max admins for multi-sig                       |
//! | proposal_expiry_seconds      | 604,800 | Proposal expiry time (7 days)                  |

use crate::types::DataKey;
use soroban_sdk::{contracttype, Address, Env};

/// Contract configuration parameters stored in instance storage.
///
/// All fields use sensible defaults that can be overridden by the admin
/// post-deployment via the `update_config` function.
///
/// # Examples
/// ```rust
/// let config = ContractConfig::default();
/// assert_eq!(config.shipment_ttl_threshold, 17_280);
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ContractConfig {
    /// Minimum ledgers remaining before TTL extension is triggered.
    /// Default: 17,280 ledgers (~1 day at 5s/ledger).
    pub shipment_ttl_threshold: u32,

    /// Number of ledgers to extend TTL by when threshold is reached.
    /// Default: 518,400 ledgers (~30 days at 5s/ledger).
    pub shipment_ttl_extension: u32,

    /// Minimum seconds that must pass between status updates on the same shipment.
    /// Admin is exempt from this restriction.
    /// Default: 60 seconds (~10 ledgers).
    pub min_status_update_interval: u64,

    /// Maximum number of items allowed in batch operations (shipments, milestones).
    /// Default: 10 items per batch.
    pub batch_operation_limit: u32,

    /// Maximum number of metadata key-value pairs per shipment.
    /// Default: 5 entries.
    pub max_metadata_entries: u32,

    /// Default limit on active shipments per company.
    /// Can be overridden per-company via `set_shipment_limit`.
    /// Default: 100 active shipments.
    pub default_shipment_limit: u32,

    /// Minimum number of admins required for multi-sig configuration.
    /// Default: 2 admins.
    pub multisig_min_admins: u32,

    /// Maximum number of admins allowed for multi-sig configuration.
    /// Default: 10 admins.
    pub multisig_max_admins: u32,

    /// Number of seconds before a multi-sig proposal expires.
    /// Default: 604,800 seconds (7 days).
    pub proposal_expiry_seconds: u64,

    /// Optional governance token for token-weighted voting. When None, governance checks are disabled.
    pub governance_token: Option<Address>,

    /// Minimum token balance required to create a proposal. Ignored when governance_token is None. Default: 0.
    pub min_proposal_tokens: i128,

    /// Number of ledgers to lock voting power after an admin approves a proposal. Default: 0 (no lock).
    pub vote_lock_ledgers: u32,
}

impl Default for ContractConfig {
    /// Returns the default configuration with production-ready values.
    ///
    /// # Examples
    /// ```rust
    /// let config = ContractConfig::default();
    /// assert_eq!(config.batch_operation_limit, 10);
    /// ```
    fn default() -> Self {
        Self {
            shipment_ttl_threshold: 17_280,   // ~1 day
            shipment_ttl_extension: 518_400,  // ~30 days
            min_status_update_interval: 60,   // 60 seconds
            batch_operation_limit: 10,        // 10 items
            max_metadata_entries: 5,          // 5 entries
            default_shipment_limit: 100,      // 100 shipments
            multisig_min_admins: 2,           // 2 admins
            multisig_max_admins: 10,          // 10 admins
            proposal_expiry_seconds: 604_800, // 7 days
            governance_token: None,
            min_proposal_tokens: 0,
            vote_lock_ledgers: 0,
        }
    }
}

/// Retrieve the contract configuration from instance storage.
///
/// If no configuration has been set, returns the default configuration.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `ContractConfig` - The current configuration.
///
/// # Examples
/// ```rust
/// let config = config::get_config(&env);
/// assert!(config.shipment_ttl_threshold > 0);
/// ```
pub fn get_config(env: &Env) -> ContractConfig {
    env.storage()
        .instance()
        .get(&DataKey::ContractConfig)
        .unwrap_or_default()
}

/// Store the contract configuration in instance storage.
///
/// This function is called during initialization and when the admin
/// updates the configuration via `update_config`.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `config` - The configuration to store.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// let mut config = ContractConfig::default();
/// config.batch_operation_limit = 20;
/// config::set_config(&env, &config);
/// ```
pub fn set_config(env: &Env, config: &ContractConfig) {
    env.storage()
        .instance()
        .set(&DataKey::ContractConfig, config);
}

/// Validate configuration parameters to ensure they are within acceptable ranges.
///
/// # Arguments
/// * `config` - The configuration to validate.
///
/// # Returns
/// * `Result<(), &'static str>` - Ok if valid, Err with message if invalid.
///
/// # Validation Rules
/// - `shipment_ttl_threshold` must be > 0 and <= 1,000,000
/// - `shipment_ttl_extension` must be > 0 and <= 10,000,000
/// - `min_status_update_interval` must be >= 10 and <= 86,400 (1 day)
/// - `batch_operation_limit` must be >= 1 and <= 100
/// - `max_metadata_entries` must be >= 1 and <= 50
/// - `default_shipment_limit` must be >= 1 and <= 10,000
/// - `multisig_min_admins` must be >= 2
/// - `multisig_max_admins` must be >= `multisig_min_admins` and <= 50
/// - `proposal_expiry_seconds` must be >= 3,600 (1 hour) and <= 2,592,000 (30 days)
///
/// # Examples
/// ```rust
/// let config = ContractConfig::default();
/// assert!(config::validate_config(&config).is_ok());
/// ```
pub fn validate_config(config: &ContractConfig) -> Result<(), &'static str> {
    // Validate TTL parameters
    if config.shipment_ttl_threshold == 0 || config.shipment_ttl_threshold > 1_000_000 {
        return Err("shipment_ttl_threshold must be > 0 and <= 1,000,000");
    }

    if config.shipment_ttl_extension == 0 || config.shipment_ttl_extension > 10_000_000 {
        return Err("shipment_ttl_extension must be > 0 and <= 10,000,000");
    }

    // Validate rate limiting
    if config.min_status_update_interval < 10 || config.min_status_update_interval > 86_400 {
        return Err("min_status_update_interval must be >= 10 and <= 86,400");
    }

    // Validate batch limits
    if config.batch_operation_limit == 0 || config.batch_operation_limit > 100 {
        return Err("batch_operation_limit must be >= 1 and <= 100");
    }

    // Validate metadata limits
    if config.max_metadata_entries == 0 || config.max_metadata_entries > 50 {
        return Err("max_metadata_entries must be >= 1 and <= 50");
    }

    // Validate shipment limits
    if config.default_shipment_limit == 0 || config.default_shipment_limit > 10_000 {
        return Err("default_shipment_limit must be >= 1 and <= 10,000");
    }

    // Validate multi-sig parameters
    if config.multisig_min_admins < 2 {
        return Err("multisig_min_admins must be >= 2");
    }

    if config.multisig_max_admins < config.multisig_min_admins || config.multisig_max_admins > 50 {
        return Err("multisig_max_admins must be >= multisig_min_admins and <= 50");
    }

    // Validate proposal expiry
    if config.proposal_expiry_seconds < 3_600 || config.proposal_expiry_seconds > 2_592_000 {
        return Err("proposal_expiry_seconds must be >= 3,600 and <= 2,592,000");
    }

    // Governance token is optional (None allowed).
    if config.min_proposal_tokens < 0 {
        return Err("min_proposal_tokens must be >= 0");
    }
    if config.vote_lock_ledgers > 10_000_000 {
        return Err("vote_lock_ledgers must be <= 10,000,000");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = ContractConfig::default();
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_ttl_threshold() {
        // Invalid: zero
        let config = ContractConfig {
            shipment_ttl_threshold: 0,
            ..Default::default()
        };
        assert!(validate_config(&config).is_err());

        // Invalid: too large
        let config = ContractConfig {
            shipment_ttl_threshold: 1_000_001,
            ..Default::default()
        };
        assert!(validate_config(&config).is_err());

        // Valid
        let config = ContractConfig {
            shipment_ttl_threshold: 50_000,
            ..Default::default()
        };
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_batch_limit() {
        // Invalid: zero
        let config = ContractConfig {
            batch_operation_limit: 0,
            ..Default::default()
        };
        assert!(validate_config(&config).is_err());

        // Invalid: too large
        let config = ContractConfig {
            batch_operation_limit: 101,
            ..Default::default()
        };
        assert!(validate_config(&config).is_err());

        // Valid
        let config = ContractConfig {
            batch_operation_limit: 50,
            ..Default::default()
        };
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_multisig_admins() {
        // Invalid: min < 2
        let config = ContractConfig {
            multisig_min_admins: 1,
            ..Default::default()
        };
        assert!(validate_config(&config).is_err());

        // Invalid: max < min
        let config = ContractConfig {
            multisig_min_admins: 5,
            multisig_max_admins: 4,
            ..Default::default()
        };
        assert!(validate_config(&config).is_err());

        // Valid
        let config = ContractConfig {
            multisig_min_admins: 3,
            multisig_max_admins: 7,
            ..Default::default()
        };
        assert!(validate_config(&config).is_ok());
    }
}
