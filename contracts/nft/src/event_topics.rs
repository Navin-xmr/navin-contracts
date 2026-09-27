//! # Event Topic Constants
//!
//! Centralised topic constants for every event emitted by the Navin Shipment
//! NFT contract, mirroring `contracts/token/src/event_topics.rs`. Using named
//! constants instead of inline `symbol_short!` literals prevents typo-drift,
//! makes refactoring safe, and provides a single source of truth for
//! off-chain indexers that match topic names.
//!
//! ## Schema versioning
//!
//! Every event is published with a two-element topic tuple:
//!
//! ```text
//! env.events().publish(
//!     (event_topics::MINT, event_topics::EVENT_SCHEMA_VERSION),
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
//! The string value of every event-name constant **must** remain identical to
//! what was previously hard-coded at the call site. Any change to a value is a
//! breaking change for off-chain indexers.

use soroban_sdk::{symbol_short, Symbol};

/// Schema version string carried by every NFT event as the second topic
/// element. Bump only when a payload shape changes in a way indexers must
/// branch on.
pub const EVENT_SCHEMA_VERSION_STR: &str = "v1";

/// Schema version symbol carried by every NFT event as the second topic
/// element. Must always match [`EVENT_SCHEMA_VERSION_STR`].
pub const EVENT_SCHEMA_VERSION: Symbol = symbol_short!("v1");

// ── Lifecycle ────────────────────────────────────────────────────────────────

/// Contract initialization.
pub const INIT: Symbol = symbol_short!("init");

// ── Supply ───────────────────────────────────────────────────────────────────

/// Token minted. Payload: `(token_id, to, timestamp)`.
pub const MINT: Symbol = symbol_short!("mint");

/// Token burned. Payload: `(token_id, caller, timestamp)`.
pub const BURN: Symbol = symbol_short!("burn");

// ── Transfers ────────────────────────────────────────────────────────────────

/// Token transferred. Payload: `(token_id, from, to, timestamp)`.
pub const TRANSFER: Symbol = symbol_short!("xfer");

// ── Admin ────────────────────────────────────────────────────────────────────

/// Admin role transferred. Payload: `(old_admin, new_admin, timestamp)`.
pub const ADMIN_TRANSFER: Symbol = symbol_short!("admin");
