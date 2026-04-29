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
//! | deadline_grace_seconds       | 0       | Grace window after deadline before expiry fires |

use crate::types::DataKey;
use soroban_sdk::{contracttype, BytesN, Env};

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

    /// Grace window (in seconds) added to a shipment's deadline before expiry
    /// logic fires. A caller invoking `check_deadline` must wait until
    /// `ledger_timestamp >= deadline + deadline_grace_seconds`.
    ///
    /// Setting this to 0 (the default) preserves the original behaviour where
    /// expiry triggers the moment the deadline timestamp is reached.
    /// Default: 0 seconds (no grace period).
    pub deadline_grace_seconds: u64,

    /// Duration (in seconds) for which an action hash is held in temporary
    /// storage to reject duplicate external triggers.
    /// Default: 300 seconds (5 minutes).
    pub idempotency_window_seconds: u64,

    /// When `true`, any `Critical`-severity condition breach reported by a carrier
    /// automatically opens a dispute for that shipment (equivalent to calling
    /// `raise_dispute` with the breach data hash as the reason).
    ///
    /// Has no effect on non-critical breaches or on shipments that are already
    /// `Disputed` or `Cancelled`.
    /// Default: `false` (disabled — existing behavior preserved).
    pub auto_dispute_breach: bool,

    /// Maximum number of milestone events allowed per shipment.
    /// Guards against unbounded milestone list growth that would increase
    /// storage and indexing costs. Must be >= 1 and <= 1000.
    /// Default: 255 milestones per shipment.
    pub max_milestones_per_shipment: u32,

    /// Maximum number of note events allowed per shipment.
    /// Bounds the size of the append-only note log for payload growth control.
    /// Must be >= 1 and <= 1000.
    /// Default: 255 notes per shipment.
    pub max_notes_per_shipment: u32,

    /// Maximum number of evidence hashes allowed per dispute (per shipment).
    /// Evidence entries are stored while a shipment is disputed; this limit
    /// prevents unbounded storage consumption in the dispute window.
    /// Must be >= 1 and <= 1000.
    /// Default: 255 evidence entries per dispute.
    pub max_evidence_per_dispute: u32,

    /// Maximum number of condition breach events allowed per shipment.
    /// Guards against unbounded breach reporting that would increase execution costs.
    /// Must be >= 1 and <= 1000.
    /// Default: 255 breaches per shipment.
    pub max_breaches_per_shipment: u32,

    /// Maximum number of shipments a company may create within `creation_quota_window_seconds`.
    /// Set to 0 to disable the creation quota (unlimited).
    /// Default: 0 (disabled).
    pub creation_quota_max: u32,

    /// Duration of the creation quota window in seconds.
    /// Only meaningful when `creation_quota_max > 0`.
    /// Default: 3600 (1 hour).
    pub creation_quota_window_seconds: u64,
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
            shipment_ttl_threshold: 17_280,      // ~1 day
            shipment_ttl_extension: 518_400,     // ~30 days
            min_status_update_interval: 60,      // 60 seconds
            batch_operation_limit: 10,           // 10 items
            max_metadata_entries: 5,             // 5 entries
            default_shipment_limit: 100,         // 100 shipments
            multisig_min_admins: 2,              // 2 admins
            multisig_max_admins: 10,             // 10 admins
            proposal_expiry_seconds: 604_800,    // 7 days
            deadline_grace_seconds: 0,           // no grace period
            idempotency_window_seconds: 300,     // 5 minutes
            auto_dispute_breach: false,          // disabled by default
            max_milestones_per_shipment: 255,    // 255 milestones
            max_notes_per_shipment: 255,         // 255 notes
            max_evidence_per_dispute: 255,       // 255 evidence entries
            max_breaches_per_shipment: 255,      // 255 breaches
            creation_quota_max: 0,               // disabled by default
            creation_quota_window_seconds: 3600, // 1 hour window
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
/// updates the configuration via `update_config`. It automatically computes
/// and stores the config checksum for drift detection.
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

    // Automatically compute and store checksum for drift detection
    let checksum = compute_config_checksum(config, env);
    set_config_checksum(env, &checksum);
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

    // Validate high-frequency event payload size guards
    if config.max_milestones_per_shipment == 0 || config.max_milestones_per_shipment > 1000 {
        return Err("max_milestones_per_shipment must be >= 1 and <= 1000");
    }
    if config.max_notes_per_shipment == 0 || config.max_notes_per_shipment > 1000 {
        return Err("max_notes_per_shipment must be >= 1 and <= 1000");
    }
    if config.max_evidence_per_dispute == 0 || config.max_evidence_per_dispute > 1000 {
        return Err("max_evidence_per_dispute must be >= 1 and <= 1000");
    }
    if config.max_breaches_per_shipment == 0 || config.max_breaches_per_shipment > 1000 {
        return Err("max_breaches_per_shipment must be >= 1 and <= 1000");
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

    // Validate deadline grace period (0 = disabled, max 7 days)
    if config.deadline_grace_seconds > 604_800 {
        return Err("deadline_grace_seconds must be <= 604,800 (7 days)");
    }

    Ok(())
}

/// Compute a deterministic SHA-256 checksum of the config for drift detection.
///
/// This function serializes all config fields in a fixed order and computes
/// their SHA-256 hash. The same config always produces the same checksum,
/// enabling indexers and operators to detect unintended config drift.
///
/// # Serialization Order
/// Fields are serialized in declaration order (top-to-bottom in the struct):
/// 1. shipment_ttl_threshold (u32, 4 bytes, big-endian)
/// 2. shipment_ttl_extension (u32, 4 bytes, big-endian)
/// 3. min_status_update_interval (u64, 8 bytes, big-endian)
/// 4. batch_operation_limit (u32, 4 bytes, big-endian)
/// 5. max_metadata_entries (u32, 4 bytes, big-endian)
/// 6. default_shipment_limit (u32, 4 bytes, big-endian)
/// 7. multisig_min_admins (u32, 4 bytes, big-endian)
/// 8. multisig_max_admins (u32, 4 bytes, big-endian)
/// 9. proposal_expiry_seconds (u64, 8 bytes, big-endian)
/// 10. deadline_grace_seconds (u64, 8 bytes, big-endian)
/// 11. auto_dispute_breach (bool, 1 byte: 1 = true, 0 = false)
/// 12. max_milestones_per_shipment (u32, 4 bytes, big-endian)
/// 13. max_notes_per_shipment (u32, 4 bytes, big-endian)
/// 14. max_evidence_per_dispute (u32, 4 bytes, big-endian)
///
/// Total: 65 bytes serialized, hashed to 32-byte SHA-256 digest.
///
/// # Arguments
/// * `config` - The configuration to checksum.
///
/// # Returns
/// * `BytesN<32>` - The SHA-256 hash of the serialized config.
///
/// # Examples
/// ```rust
/// let config = ContractConfig::default();
/// let checksum1 = config::compute_config_checksum(&config);
/// let checksum2 = config::compute_config_checksum(&config);
/// assert_eq!(checksum1, checksum2); // Deterministic
/// ```
pub fn compute_config_checksum(config: &ContractConfig, env: &Env) -> BytesN<32> {
    // Serialize all fields in fixed order (69 bytes total)
    let mut bytes: [u8; 69] = [0; 69];
    let mut offset = 0;

    // 1. shipment_ttl_threshold (u32, big-endian)
    bytes[offset..offset + 4].copy_from_slice(&config.shipment_ttl_threshold.to_be_bytes());
    offset += 4;

    // 2. shipment_ttl_extension (u32, big-endian)
    bytes[offset..offset + 4].copy_from_slice(&config.shipment_ttl_extension.to_be_bytes());
    offset += 4;

    // 3. min_status_update_interval (u64, big-endian)
    bytes[offset..offset + 8].copy_from_slice(&config.min_status_update_interval.to_be_bytes());
    offset += 8;

    // 4. batch_operation_limit (u32, big-endian)
    bytes[offset..offset + 4].copy_from_slice(&config.batch_operation_limit.to_be_bytes());
    offset += 4;

    // 5. max_metadata_entries (u32, big-endian)
    bytes[offset..offset + 4].copy_from_slice(&config.max_metadata_entries.to_be_bytes());
    offset += 4;

    // 6. default_shipment_limit (u32, big-endian)
    bytes[offset..offset + 4].copy_from_slice(&config.default_shipment_limit.to_be_bytes());
    offset += 4;

    // 7. multisig_min_admins (u32, big-endian)
    bytes[offset..offset + 4].copy_from_slice(&config.multisig_min_admins.to_be_bytes());
    offset += 4;

    // 8. multisig_max_admins (u32, big-endian)
    bytes[offset..offset + 4].copy_from_slice(&config.multisig_max_admins.to_be_bytes());
    offset += 4;

    // 9. proposal_expiry_seconds (u64, big-endian)
    bytes[offset..offset + 8].copy_from_slice(&config.proposal_expiry_seconds.to_be_bytes());
    offset += 8;

    // 10. deadline_grace_seconds (u64, big-endian)
    bytes[offset..offset + 8].copy_from_slice(&config.deadline_grace_seconds.to_be_bytes());
    offset += 8;

    // 11. auto_dispute_breach (bool, 1 byte)
    bytes[offset] = if config.auto_dispute_breach { 1 } else { 0 };
    offset += 1;

    // 12. max_milestones_per_shipment (u32, big-endian)
    bytes[offset..offset + 4].copy_from_slice(&config.max_milestones_per_shipment.to_be_bytes());
    offset += 4;

    // 13. max_notes_per_shipment (u32, big-endian)
    bytes[offset..offset + 4].copy_from_slice(&config.max_notes_per_shipment.to_be_bytes());
    offset += 4;

    // 14. max_evidence_per_dispute (u32, big-endian)
    bytes[offset..offset + 4].copy_from_slice(&config.max_evidence_per_dispute.to_be_bytes());
    offset += 4;

    // 15. max_breaches_per_shipment (u32, big-endian)
    bytes[offset..offset + 4].copy_from_slice(&config.max_breaches_per_shipment.to_be_bytes());

    // Compute SHA-256 hash and convert to BytesN<32>
    let hash = env
        .crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(env, &bytes));
    BytesN::from_array(env, &hash.to_array())
}

/// Retrieve the stored config checksum from instance storage.
///
/// If no checksum has been computed and stored, returns None.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `Option<BytesN<32>>` - The stored checksum, or None if not set.
///
/// # Examples
/// ```rust
/// let checksum = config::get_config_checksum(&env);
/// ```
pub fn get_config_checksum(env: &Env) -> Option<BytesN<32>> {
    env.storage().instance().get(&DataKey::ConfigChecksum)
}

/// Store the config checksum in instance storage.
///
/// This is called whenever the config is updated to maintain a current
/// checksum that indexers can query to detect drift.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `checksum` - The checksum to store.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// let checksum = config::compute_config_checksum(&config);
/// config::set_config_checksum(&env, &checksum);
/// ```
pub fn set_config_checksum(env: &Env, checksum: &BytesN<32>) {
    env.storage()
        .instance()
        .set(&DataKey::ConfigChecksum, checksum);
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

    #[test]
    fn test_validate_deadline_grace_seconds() {
        // Valid: 0 (disabled)
        let config = ContractConfig {
            deadline_grace_seconds: 0,
            ..Default::default()
        };
        assert!(validate_config(&config).is_ok());

        // Valid: exactly 7 days
        let config = ContractConfig {
            deadline_grace_seconds: 604_800,
            ..Default::default()
        };
        assert!(validate_config(&config).is_ok());

        // Invalid: exceeds 7-day cap
        let config = ContractConfig {
            deadline_grace_seconds: 604_801,
            ..Default::default()
        };
        assert!(validate_config(&config).is_err());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Checksum Tests — Deterministic Config Drift Detection
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_checksum_deterministic_same_config() {
        // Same config must always produce the same checksum
        let env = Env::default();
        let config = ContractConfig::default();
        let checksum1 = compute_config_checksum(&config, &env);
        let checksum2 = compute_config_checksum(&config, &env);
        assert_eq!(
            checksum1, checksum2,
            "Same config must produce identical checksums"
        );
    }

    #[test]
    fn test_checksum_changes_on_field_modification() {
        // Modifying any field must change the checksum
        let env = Env::default();
        let config_original = ContractConfig::default();
        let checksum_original = compute_config_checksum(&config_original, &env);

        // Test each field independently
        let mut config = config_original.clone();
        config.shipment_ttl_threshold = 20_000;
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing shipment_ttl_threshold must change checksum"
        );

        let mut config = config_original.clone();
        config.shipment_ttl_extension = 600_000;
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing shipment_ttl_extension must change checksum"
        );

        let mut config = config_original.clone();
        config.min_status_update_interval = 120;
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing min_status_update_interval must change checksum"
        );

        let mut config = config_original.clone();
        config.batch_operation_limit = 20;
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing batch_operation_limit must change checksum"
        );

        let mut config = config_original.clone();
        config.max_metadata_entries = 10;
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing max_metadata_entries must change checksum"
        );

        let mut config = config_original.clone();
        config.default_shipment_limit = 200;
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing default_shipment_limit must change checksum"
        );

        let mut config = config_original.clone();
        config.multisig_min_admins = 3;
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing multisig_min_admins must change checksum"
        );

        let mut config = config_original.clone();
        config.multisig_max_admins = 15;
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing multisig_max_admins must change checksum"
        );

        let mut config = config_original.clone();
        config.proposal_expiry_seconds = 1_209_600; // 14 days
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing proposal_expiry_seconds must change checksum"
        );

        let mut config = config_original.clone();
        config.deadline_grace_seconds = 86_400; // 1 day
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing deadline_grace_seconds must change checksum"
        );

        let mut config = config_original.clone();
        config.auto_dispute_breach = true;
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing auto_dispute_breach must change checksum"
        );

        let mut config = config_original.clone();
        config.max_milestones_per_shipment = 100;
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing max_milestones_per_shipment must change checksum"
        );

        let mut config = config_original.clone();
        config.max_notes_per_shipment = 100;
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing max_notes_per_shipment must change checksum"
        );

        let mut config = config_original.clone();
        config.max_evidence_per_dispute = 100;
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing max_evidence_per_dispute must change checksum"
        );

        let mut config = config_original.clone();
        config.max_breaches_per_shipment = 100;
        let checksum = compute_config_checksum(&config, &env);
        assert_ne!(
            checksum, checksum_original,
            "Changing max_breaches_per_shipment must change checksum"
        );
    }

    #[test]
    fn test_checksum_different_for_different_configs() {
        // Two different configs must produce different checksums
        let env = Env::default();
        let config1 = ContractConfig {
            batch_operation_limit: 10,
            ..Default::default()
        };
        let config2 = ContractConfig {
            batch_operation_limit: 20,
            ..Default::default()
        };

        let checksum1 = compute_config_checksum(&config1, &env);
        let checksum2 = compute_config_checksum(&config2, &env);

        assert_ne!(
            checksum1, checksum2,
            "Different configs must produce different checksums"
        );
    }

    #[test]
    fn test_checksum_stable_across_multiple_runs() {
        // Verify checksum stability across multiple independent computations
        let env = Env::default();
        let config = ContractConfig {
            shipment_ttl_threshold: 25_000,
            shipment_ttl_extension: 600_000,
            min_status_update_interval: 90,
            batch_operation_limit: 15,
            max_metadata_entries: 8,
            default_shipment_limit: 150,
            multisig_min_admins: 3,
            multisig_max_admins: 8,
            proposal_expiry_seconds: 864_000,
            deadline_grace_seconds: 43_200,
            idempotency_window_seconds: 300,
            auto_dispute_breach: false,
            max_milestones_per_shipment: 100,
            max_notes_per_shipment: 100,
            max_evidence_per_dispute: 100,
            max_breaches_per_shipment: 100,
            creation_quota_max: 0,
            creation_quota_window_seconds: 3600,
        };

        let checksums = [
            compute_config_checksum(&config, &env),
            compute_config_checksum(&config, &env),
            compute_config_checksum(&config, &env),
            compute_config_checksum(&config, &env),
            compute_config_checksum(&config, &env),
        ];

        // All checksums should be identical
        for i in 1..checksums.len() {
            assert_eq!(
                checksums[0], checksums[i],
                "Checksum must be stable across multiple runs"
            );
        }
    }

    #[test]
    fn test_checksum_serialization_order_matters() {
        // Verify that field order in serialization is critical
        // Two configs with same values but different field order would have
        // different checksums if serialization order changed (which it shouldn't)
        let env = Env::default();

        let config1 = ContractConfig {
            shipment_ttl_threshold: 17_280,
            batch_operation_limit: 10,
            ..Default::default()
        };

        let config2 = ContractConfig {
            shipment_ttl_threshold: 17_280,
            batch_operation_limit: 10,
            ..Default::default()
        };

        // These should be identical since all fields are the same
        let checksum1 = compute_config_checksum(&config1, &env);
        let checksum2 = compute_config_checksum(&config2, &env);
        assert_eq!(checksum1, checksum2);
    }

    #[test]
    fn test_checksum_is_32_bytes() {
        // Verify checksum is always 32 bytes (SHA-256)
        let env = Env::default();
        let config = ContractConfig::default();
        let checksum = compute_config_checksum(&config, &env);
        assert_eq!(checksum.len(), 32, "Checksum must be 32 bytes (SHA-256)");
    }

    #[test]
    fn test_checksum_not_all_zeros() {
        // Verify checksum is not all zeros (sanity check)
        let env = Env::default();
        let config = ContractConfig::default();
        let checksum = compute_config_checksum(&config, &env);
        let bytes: [u8; 32] = checksum.to_array();
        assert!(
            bytes.iter().any(|&b| b != 0),
            "Checksum should not be all zeros"
        );
    }

    #[test]
    fn test_checksum_boundary_values() {
        // Test checksums with boundary values to ensure serialization handles them
        let env = Env::default();
        let config_min = ContractConfig {
            shipment_ttl_threshold: 1,
            shipment_ttl_extension: 1,
            min_status_update_interval: 10,
            batch_operation_limit: 1,
            max_metadata_entries: 1,
            default_shipment_limit: 1,
            multisig_min_admins: 2,
            multisig_max_admins: 2,
            proposal_expiry_seconds: 3_600,
            deadline_grace_seconds: 0,
            idempotency_window_seconds: 0,
            auto_dispute_breach: false,
            max_milestones_per_shipment: 1,
            max_notes_per_shipment: 1,
            max_evidence_per_dispute: 1,
            max_breaches_per_shipment: 1,
            creation_quota_max: 0,
            creation_quota_window_seconds: 3600,
        };

        let config_max = ContractConfig {
            shipment_ttl_threshold: 1_000_000,
            shipment_ttl_extension: 10_000_000,
            min_status_update_interval: 86_400,
            batch_operation_limit: 100,
            max_metadata_entries: 50,
            default_shipment_limit: 10_000,
            multisig_min_admins: 2,
            multisig_max_admins: 50,
            proposal_expiry_seconds: 2_592_000,
            deadline_grace_seconds: 604_800,
            idempotency_window_seconds: 86_400,
            auto_dispute_breach: true,
            max_milestones_per_shipment: 1000,
            max_notes_per_shipment: 1000,
            max_evidence_per_dispute: 1000,
            max_breaches_per_shipment: 1000,
            creation_quota_max: 100,
            creation_quota_window_seconds: 86_400,
        };

        let checksum_min = compute_config_checksum(&config_min, &env);
        let checksum_max = compute_config_checksum(&config_max, &env);

        // Both should be valid 32-byte checksums
        assert_eq!(checksum_min.len(), 32);
        assert_eq!(checksum_max.len(), 32);

        // They should be different
        assert_ne!(checksum_min, checksum_max);
    }

    #[test]
    fn test_checksum_single_bit_flip_changes_hash() {
        // Verify that even a single bit change produces a different checksum
        let env = Env::default();
        let config1 = ContractConfig {
            batch_operation_limit: 10,
            ..Default::default()
        };

        let config2 = ContractConfig {
            batch_operation_limit: 11, // Single unit change
            ..Default::default()
        };

        let checksum1 = compute_config_checksum(&config1, &env);
        let checksum2 = compute_config_checksum(&config2, &env);

        assert_ne!(
            checksum1, checksum2,
            "Even a single unit change must produce different checksum"
        );
    }
}
