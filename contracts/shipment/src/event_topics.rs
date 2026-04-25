//! # Event Topic Constants
//!
//! Centralised `&str` constants for every event topic emitted by the Navin
//! Shipment contract.  Using named constants instead of inline string literals
//! prevents typo-drift, makes refactoring safe (a rename is a single-line
//! change here), and provides a single source of truth for off-chain consumers
//! that need to match topic names.
//!
//! ## Usage
//!
//! ```rust
//! use crate::event_topics;
//!
//! env.events().publish(
//!     (Symbol::new(env, event_topics::SHIPMENT_CREATED),),
//!     payload,
//! );
//! ```
//!
//! ## Backward Compatibility
//!
//! The string value of every constant **must** remain identical to what was
//! previously hard-coded at the call site.  Any change to a value is a
//! breaking change for off-chain indexers.

// ── Shipment lifecycle ────────────────────────────────────────────────────────

/// Emitted when a new shipment is registered on-chain.
pub const SHIPMENT_CREATED: &str = "shipment_created";

/// Emitted when a shipment transitions between lifecycle states.
pub const STATUS_UPDATED: &str = "status_updated";

/// Emitted when a carrier records a checkpoint milestone.
pub const MILESTONE_RECORDED: &str = "milestone_recorded";

/// Emitted when a shipment is cancelled (non-admin path).
pub const SHIPMENT_CANCELLED: &str = "shipment_cancelled";

/// Emitted when a shipment misses its deadline and is auto-cancelled.
pub const SHIPMENT_EXPIRED: &str = "shipment_expired";

/// Emitted when a shipment is moved to temporary (archived) storage.
pub const SHIPMENT_ARCHIVED: &str = "shipment_archived";

/// Emitted when a shipment is successfully delivered.
pub const DELIVERY_SUCCESS: &str = "delivery_success";

// ── Escrow ────────────────────────────────────────────────────────────────────

/// Emitted when funds are locked into escrow for a shipment.
pub const ESCROW_DEPOSITED: &str = "escrow_deposited";

/// Emitted when escrowed funds are paid out to the carrier.
pub const ESCROW_RELEASED: &str = "escrow_released";

/// Emitted when escrowed funds are returned to the company.
pub const ESCROW_REFUNDED: &str = "escrow_refunded";

// ── Disputes ──────────────────────────────────────────────────────────────────

/// Emitted when any party raises a dispute on a shipment.
pub const DISPUTE_RAISED: &str = "dispute_raised";

/// Emitted when an admin resolves a dispute.
pub const DISPUTE_RESOLVED: &str = "dispute_resolved";

// ── Condition breaches ────────────────────────────────────────────────────────

/// Emitted when a carrier reports an out-of-range sensor reading.
pub const CONDITION_BREACH: &str = "condition_breach";

// ── Carrier reputation ────────────────────────────────────────────────────────

/// Emitted to record a breach against the carrier's reputation index.
pub const CARRIER_BREACH: &str = "carrier_breach";

/// Emitted when a dispute is resolved against the carrier.
pub const CARRIER_DISPUTE_LOSS: &str = "carrier_dispute_loss";

/// Emitted when a carrier completes delivery after the deadline.
pub const CARRIER_LATE_DELIVERY: &str = "carrier_late_delivery";

/// Emitted when a carrier completes delivery on or before the deadline.
pub const CARRIER_ON_TIME_DELIVERY: &str = "carrier_on_time_delivery";

/// Emitted when a carrier-to-carrier handoff is completed.
pub const CARRIER_HANDOFF_COMPLETED: &str = "carrier_handoff_completed";

/// Emitted to track the ratio of checkpoints hit vs expected for a carrier.
pub const CARRIER_MILESTONE_RATE: &str = "carrier_milestone_rate";

// ── Admin & governance ────────────────────────────────────────────────────────

/// Emitted when a new administrator is proposed.
pub const ADMIN_PROPOSED: &str = "admin_proposed";

/// Emitted when the administrator role transfer is accepted.
pub const ADMIN_TRANSFERRED: &str = "admin_transferred";

/// Emitted when the contract WASM is upgraded.
pub const CONTRACT_UPGRADED: &str = "contract_upgraded";

/// Emitted when a migration report is generated after an upgrade.
pub const MIGRATION_REPORTED: &str = "migration_reported";

/// Emitted when the contract is paused.
pub const CONTRACT_PAUSED: &str = "contract_paused";

/// Emitted when the contract is unpaused.
pub const CONTRACT_UNPAUSED: &str = "contract_unpaused";

/// Emitted when an admin forcibly cancels a shipment (privileged path).
pub const FORCE_CANCELLED: &str = "force_cancelled";

// ── RBAC ──────────────────────────────────────────────────────────────────────

/// Emitted when a role is revoked from an address.
pub const ROLE_REVOKED: &str = "role_revoked";

/// Emitted on every RBAC change (assign / revoke / suspend / reactivate).
pub const ROLE_CHANGED: &str = "role_changed";

// ── Carrier handoff ───────────────────────────────────────────────────────────

/// Emitted when a shipment is handed off to a new carrier.
pub const CARRIER_HANDOFF: &str = "carrier_handoff";

// ── Notifications ─────────────────────────────────────────────────────────────

/// Emitted to trigger push notifications, emails, or in-app alerts.
pub const NOTIFICATION: &str = "notification";

// ── Notes & evidence ─────────────────────────────────────────────────────────

/// Emitted when a hash-only note is appended to a shipment.
pub const NOTE_APPENDED: &str = "note_appended";

/// Emitted when dispute evidence is appended (append-only).
pub const EVIDENCE_ADDED: &str = "evidence_added";

#[cfg(test)]
mod tests {
    use super::*;

    // ── Length guard ─────────────────────────────────────────────────────────
    // Soroban Symbol values are limited to 32 characters.  This test catches
    // any constant that would silently fail at runtime.

    #[test]
    fn all_topic_constants_are_within_symbol_length_limit() {
        let topics = [
            SHIPMENT_CREATED,
            STATUS_UPDATED,
            MILESTONE_RECORDED,
            SHIPMENT_CANCELLED,
            SHIPMENT_EXPIRED,
            SHIPMENT_ARCHIVED,
            DELIVERY_SUCCESS,
            ESCROW_DEPOSITED,
            ESCROW_RELEASED,
            ESCROW_REFUNDED,
            DISPUTE_RAISED,
            DISPUTE_RESOLVED,
            CONDITION_BREACH,
            CARRIER_BREACH,
            CARRIER_DISPUTE_LOSS,
            CARRIER_LATE_DELIVERY,
            CARRIER_ON_TIME_DELIVERY,
            CARRIER_HANDOFF_COMPLETED,
            CARRIER_MILESTONE_RATE,
            ADMIN_PROPOSED,
            ADMIN_TRANSFERRED,
            CONTRACT_UPGRADED,
            CONTRACT_PAUSED,
            CONTRACT_UNPAUSED,
            FORCE_CANCELLED,
            ROLE_REVOKED,
            ROLE_CHANGED,
            CARRIER_HANDOFF,
            NOTIFICATION,
            NOTE_APPENDED,
            EVIDENCE_ADDED,
            MIGRATION_REPORTED,
        ];
        for topic in &topics {
            assert!(
                topic.len() <= 32,
                "Topic '{}' exceeds Soroban Symbol 32-char limit (len={})",
                topic,
                topic.len()
            );
        }
    }

    // ── Value regression guard ────────────────────────────────────────────────
    // These assertions ensure that no constant value is accidentally changed,
    // which would break off-chain indexers that match topic strings.

    #[test]
    fn topic_values_are_backward_compatible() {
        assert_eq!(SHIPMENT_CREATED, "shipment_created");
        assert_eq!(STATUS_UPDATED, "status_updated");
        assert_eq!(MILESTONE_RECORDED, "milestone_recorded");
        assert_eq!(SHIPMENT_CANCELLED, "shipment_cancelled");
        assert_eq!(SHIPMENT_EXPIRED, "shipment_expired");
        assert_eq!(SHIPMENT_ARCHIVED, "shipment_archived");
        assert_eq!(DELIVERY_SUCCESS, "delivery_success");
        assert_eq!(ESCROW_DEPOSITED, "escrow_deposited");
        assert_eq!(ESCROW_RELEASED, "escrow_released");
        assert_eq!(ESCROW_REFUNDED, "escrow_refunded");
        assert_eq!(DISPUTE_RAISED, "dispute_raised");
        assert_eq!(DISPUTE_RESOLVED, "dispute_resolved");
        assert_eq!(CONDITION_BREACH, "condition_breach");
        assert_eq!(CARRIER_BREACH, "carrier_breach");
        assert_eq!(CARRIER_DISPUTE_LOSS, "carrier_dispute_loss");
        assert_eq!(CARRIER_LATE_DELIVERY, "carrier_late_delivery");
        assert_eq!(CARRIER_ON_TIME_DELIVERY, "carrier_on_time_delivery");
        assert_eq!(CARRIER_HANDOFF_COMPLETED, "carrier_handoff_completed");
        assert_eq!(CARRIER_MILESTONE_RATE, "carrier_milestone_rate");
        assert_eq!(ADMIN_PROPOSED, "admin_proposed");
        assert_eq!(ADMIN_TRANSFERRED, "admin_transferred");
        assert_eq!(CONTRACT_UPGRADED, "contract_upgraded");
        assert_eq!(CONTRACT_PAUSED, "contract_paused");
        assert_eq!(CONTRACT_UNPAUSED, "contract_unpaused");
        assert_eq!(FORCE_CANCELLED, "force_cancelled");
        assert_eq!(ROLE_REVOKED, "role_revoked");
        assert_eq!(ROLE_CHANGED, "role_changed");
        assert_eq!(CARRIER_HANDOFF, "carrier_handoff");
        assert_eq!(NOTIFICATION, "notification");
        assert_eq!(NOTE_APPENDED, "note_appended");
        assert_eq!(EVIDENCE_ADDED, "evidence_added");
        assert_eq!(MIGRATION_REPORTED, "migration_reported");
    }

    #[test]
    fn all_topic_constants_are_unique() {
        let mut topics = [
            SHIPMENT_CREATED,
            STATUS_UPDATED,
            MILESTONE_RECORDED,
            SHIPMENT_CANCELLED,
            SHIPMENT_EXPIRED,
            SHIPMENT_ARCHIVED,
            DELIVERY_SUCCESS,
            ESCROW_DEPOSITED,
            ESCROW_RELEASED,
            ESCROW_REFUNDED,
            DISPUTE_RAISED,
            DISPUTE_RESOLVED,
            CONDITION_BREACH,
            CARRIER_BREACH,
            CARRIER_DISPUTE_LOSS,
            CARRIER_LATE_DELIVERY,
            CARRIER_ON_TIME_DELIVERY,
            CARRIER_HANDOFF_COMPLETED,
            CARRIER_MILESTONE_RATE,
            ADMIN_PROPOSED,
            ADMIN_TRANSFERRED,
            CONTRACT_UPGRADED,
            CONTRACT_PAUSED,
            CONTRACT_UNPAUSED,
            FORCE_CANCELLED,
            ROLE_REVOKED,
            ROLE_CHANGED,
            CARRIER_HANDOFF,
            NOTIFICATION,
            NOTE_APPENDED,
            EVIDENCE_ADDED,
            MIGRATION_REPORTED,
        ];
        topics.sort_unstable();
        // After sorting, any duplicates are adjacent — windows(2) catches them.
        for pair in topics.windows(2) {
            assert_ne!(
                pair[0], pair[1],
                "Duplicate topic constant value detected: '{}'",
                pair[0]
            );
        }
    }
}
