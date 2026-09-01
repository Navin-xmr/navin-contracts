//! # Event Topic Constants
//!
//! Centralised `&str` constants for every event topic emitted by the Navin
//! Token contract, mirroring `contracts/shipment/src/event_topics.rs`. Using
//! named constants instead of inline `symbol_short!` literals prevents
//! typo-drift, makes refactoring safe, and provides a single source of truth
//! for off-chain indexers that match topic names.
//!
//! ## Schema versioning
//!
//! Every event is published with a two-element topic tuple:
//!
//! ```rust
//! env.events().publish(
//!     (
//!         Symbol::new(env, event_topics::TRANSFER),
//!         Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
//!     ),
//!     payload,
//! );
//! ```
//!
//! The first element is the event name (unchanged from the historical
//! `symbol_short!` literal, so existing indexers keep matching); the second
//! element is the schema version, giving indexers a stable way to detect
//! payload shape changes without breaking on the name alone.
//!
//! ## Backward Compatibility
//!
//! The string value of every constant **must** remain identical to what was
//! previously hard-coded at the call site. Any change to a value is a
//! breaking change for off-chain indexers.

/// Schema version string carried by every token event as the second topic
/// element. Bump only when a payload shape changes in a way indexers must
/// branch on.
pub const EVENT_SCHEMA_VERSION_STR: &str = "v1";

// ── Lifecycle ────────────────────────────────────────────────────────────────

/// Contract initialization.
pub const INIT: &str = "init";

// ── Transfers ────────────────────────────────────────────────────────────────

/// Single-party transfer (`transfer`).
pub const TRANSFER: &str = "transfer";

/// Two-party transfer (`transfer_from`).
pub const TRANSFER_FROM: &str = "tr_from";

/// Batch transfer (`batch_transfer`).
pub const BATCH_TRANSFER: &str = "batch_tr";

// ── Allowances ───────────────────────────────────────────────────────────────

/// Allowance approval.
pub const APPROVE: &str = "approve";

/// Allowance increased.
pub const ALLOWANCE_INCREASED: &str = "inc_alw";

/// Allowance decreased.
pub const ALLOWANCE_DECREASED: &str = "dec_alw";

// ── Admin ────────────────────────────────────────────────────────────────────

/// Admin transferred.
pub const ADMIN_TRANSFERRED: &str = "admin_tr";

// ── Supply ───────────────────────────────────────────────────────────────────

/// Tokens minted.
pub const MINT: &str = "mint";

/// Admin burn (`burn` by admin).
pub const ADMIN_BURN: &str = "adm_burn";

/// Token burn (`burn_from`).
pub const BURN: &str = "burn";

/// Burn by spender (`burn_from` variant).
pub const BURN_FROM: &str = "burn_from";

// ── Pause ────────────────────────────────────────────────────────────────────

/// Contract paused.
pub const PAUSED: &str = "paused";

/// Contract unpaused.
pub const UNPAUSED: &str = "unpaused";

// ── Metadata ─────────────────────────────────────────────────────────────────

/// Metadata key added.
pub const METADATA_ADDED: &str = "meta_add";

/// Metadata key removed.
pub const METADATA_REMOVED: &str = "meta_rm";

/// Metadata key set.
pub const METADATA_SET: &str = "meta_set";

/// Metadata deleted.
pub const METADATA_DELETED: &str = "meta_del";
