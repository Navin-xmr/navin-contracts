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

/// Emitted when a shipment is successfully delivered.
pub const DELIVERY_SUCCESS: &str = "delivery_success";

// ── Escrow ────────────────────────────────────────────────────────────────────

/// Emitted when funds are locked into escrow for a shipment.
pub const ESCROW_DEPOSITED: &str = "escrow_deposited";

/// Emitted when escrowed funds are paid out to the carrier.
pub const ESCROW_RELEASED: &str = "escrow_released";

/// Emitted when escrowed funds are returned to the company.
pub const ESCROW_REFUNDED: &str = "escrow_refunded";

/// Emitted when a partial milestone-based escrow release is triggered.
pub const MILESTONE_PAYMENT_RELEASED: &str = "milestone_payment_released";

/// Emitted when a platform fee is collected from a deposit.
pub const PLATFORM_FEE_COLLECTED: &str = "platform_fee_collected";

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

/// Emitted when an admin forcibly releases escrow for a shipment to carrier (privileged path).
pub const FORCE_RELEASED: &str = "force_released";

/// Emitted when an admin forcibly refunds escrow for a shipment to company (privileged path).
pub const FORCE_REFUNDED: &str = "force_refunded";

/// Emitted when the platform fee configuration is updated.
pub const FEE_CONFIG_UPDATED: &str = "fee_config_updated";

/// Emitted once by `initialize`. `"init"` is the only topic string for this
/// event; indexers filter on it, so it must not be renamed.
pub const CONTRACT_INITIALIZED: &str = "init";

/// Emitted by `set_shipment_limit`. `"set_limit"` is the only topic string for
/// this event; indexers filter on it, so it must not be renamed.
pub const SHIPMENT_LIMIT_UPDATED: &str = "set_limit";

/// Emitted by `set_company_shipment_limit`. `"set_cmp_limit"` is the only topic
/// string for this event; indexers filter on it, so it must not be renamed.
pub const COMPANY_LIMIT_UPDATED: &str = "set_cmp_limit";

// ── RBAC ──────────────────────────────────────────────────────────────────────

/// Emitted when a role is revoked from an address.
pub const ROLE_REVOKED: &str = "role_revoked";

/// Emitted on every RBAC change (assign / revoke / suspend / reactivate).
pub const ROLE_CHANGED: &str = "role_changed";

/// Emitted when a carrier is added to a company's whitelist.
pub const WHITELIST_ADDED: &str = "add_wl";

/// Emitted when a carrier is removed from a company's whitelist.
pub const WHITELIST_REMOVED: &str = "rm_wl";

/// Emitted when multisig configuration is initialized or replaced.
pub const MULTISIG_INITIALIZED: &str = "ms_init";

/// Emitted when an admin approves a multisig proposal.
pub const PROPOSAL_APPROVED: &str = "approve";

/// Emitted when a multisig proposal is executed.
pub const PROPOSAL_EXECUTED: &str = "executed";

// ── Audit trail ───────────────────────────────────────────────────────────────

/// Emitted when an audit-log entry records a role assignment.
pub const AUDIT_ROLE_ASSIGNED: &str = "audit_role_assigned";

/// Emitted when an audit-log entry records a role revocation.
pub const AUDIT_ROLE_REVOKED: &str = "audit_role_revoked";

/// Emitted when an audit-log entry records a role suspension.
pub const AUDIT_ROLE_SUSPENDED: &str = "audit_role_suspended";

/// Emitted when an audit-log entry records a role reactivation.
pub const AUDIT_ROLE_REACTIVATED: &str = "audit_role_reactivated";

/// Emitted when an audit-log entry records an admin transfer.
pub const AUDIT_ADMIN_TRANSFERRED: &str = "audit_admin_transferred";

/// Emitted when an audit-log entry records a carrier whitelist addition.
pub const AUDIT_CARRIER_WHITELISTED: &str = "audit_carrier_whitelisted";

/// Emitted when an audit-log entry records a carrier whitelist removal.
pub const AUDIT_CARRIER_UNWHITELISTED: &str = "audit_carrier_unwhitelisted";

/// Emitted when an audit-log entry records a company suspension.
pub const AUDIT_COMPANY_SUSPENDED: &str = "audit_company_suspended";

/// Emitted when an audit-log entry records a company reactivation.
pub const AUDIT_COMPANY_REACTIVATED: &str = "audit_company_reactivated";

/// Emitted when an audit-log entry records a carrier suspension.
pub const AUDIT_CARRIER_SUSPENDED: &str = "audit_carrier_suspended";

/// Emitted when an audit-log entry records a carrier reactivation.
pub const AUDIT_CARRIER_REACTIVATED: &str = "audit_carrier_reactivated";

// ── Carrier handoff ───────────────────────────────────────────────────────────

/// Emitted when a shipment is handed off to a new carrier.
pub const CARRIER_HANDOFF: &str = "carrier_handoff";

// ── Notifications ─────────────────────────────────────────────────────────────

/// Emitted to trigger push notifications, emails, or in-app alerts.
pub const NOTIFICATION: &str = "notification";

// ── Evidence ─────────────────────────────────────────────────────────────────

/// Emitted when dispute evidence is appended (append-only).
pub const EVIDENCE_ADDED: &str = "evidence_added";

// ── Hash domain-separation prefixes by event family ──────────────────────────
//
// These `u8` tags are prepended to every idempotency-key hash input to
// bind each key to its event-family context.  Using a per-family tag means
// that the same external payload hash submitted to two different event
// families always produces distinct idempotency keys, preventing
// cross-context hash collisions.
//
// ## Callers computing off-chain hashes
//
// Off-chain clients SHOULD prefix their raw payload with the appropriate
// constant before hashing (e.g. `SHA-256(domain_tag_byte || raw_payload)`)
// so that hashes are naturally scoped to their family.
//
// ## Assignment rules
// - Each event family gets a unique, stable `u8` discriminant.
// - Values MUST NOT be reused or renumbered; doing so is a breaking change.

/// Domain tag for shipment-lifecycle events
/// (`shipment_created`, `status_updated`, `milestone_recorded`,
///  `shipment_cancelled`, `shipment_expired`, `delivery_success`).
pub const HASH_DOMAIN_SHIPMENT: u8 = 0x01;

/// Domain tag for escrow-operation events
/// (`escrow_deposited`, `escrow_released`, `escrow_refunded`).
pub const HASH_DOMAIN_ESCROW: u8 = 0x02;

/// Domain tag for dispute-related events
/// (`dispute_raised`, `dispute_resolved`).
pub const HASH_DOMAIN_DISPUTE: u8 = 0x03;

/// Domain tag for condition-breach / sensor-data events
/// (`condition_breach`, `carrier_breach`).
pub const HASH_DOMAIN_CONDITION: u8 = 0x04;

/// Domain tag for carrier-reputation events
/// (`carrier_late_delivery`, `carrier_on_time_delivery`,
///  `carrier_handoff`, `carrier_handoff_completed`,
///  `carrier_milestone_rate`, `carrier_dispute_loss`).
pub const HASH_DOMAIN_CARRIER: u8 = 0x05;

/// Domain tag for admin / governance events
/// (`admin_proposed`, `admin_transferred`, `contract_upgraded`,
///  `migration_reported`, `contract_paused`, `contract_unpaused`,
///  `force_cancelled`, `force_released`, `force_refunded`).
pub const HASH_DOMAIN_ADMIN: u8 = 0x06;

/// Domain tag for RBAC events
/// (`role_revoked`, `role_changed`).
pub const HASH_DOMAIN_RBAC: u8 = 0x07;

/// Domain tag for notification events (`notification`).
pub const HASH_DOMAIN_NOTIFICATION: u8 = 0x08;

// 0x09 was `HASH_DOMAIN_NOTE` (for the removed `note_appended` topic). It is
// retired and must not be reused.

/// Domain tag for platform-level events (`platform_fee_collected`, `fee_config_updated`).
pub const HASH_DOMAIN_PLATFORM: u8 = 0x0B;

// ── Escrow freeze ─────────────────────────────────────────────────────────────

/// Emitted when escrow is frozen due to a dispute or safety control.
/// Contains a structured reason code (`EscrowFreezeReason`) so that
/// indexers can classify the freeze without parsing free-form text.
pub const ESCROW_FROZEN: &str = "escrow_frozen";

/// Emitted when the settlement-proof NFT could not be minted for a final
/// settlement. The settlement itself still completes.
pub const PROOF_MINT_FAILED: &str = "proof_mint_failed";

pub const CARRIER_SUSPENDED: &str = "carrier_suspended";
pub const CARRIER_REACTIVATED: &str = "carrier_reactivated";
pub const DELIVERY_CONFIRMED: &str = "delivery_confirmed";
pub const GEOFENCE_EVENT: &str = "geofence_event";
pub const ETA_UPDATED: &str = "eta_updated";
pub const PROPOSAL_DIGEST: &str = "proposal_digest";
pub const CONFIG_UPDATED: &str = "config_updated";
pub const QUOTA_SET: &str = "quota_set";

/// Emitted when the shipment admin sets the base URI on the settlement-proof NFT.
pub const NFT_BASE_URI_SET: &str = "nft_base_uri_set";

/// Emitted when the shipment admin pauses the settlement-proof NFT contract.
pub const NFT_PAUSED: &str = "nft_paused";

/// Emitted when the shipment admin unpauses the settlement-proof NFT contract.
pub const NFT_UNPAUSED: &str = "nft_unpaused";

/// Emitted when the shipment admin initiates a transfer of NFT admin custody.
pub const NFT_ADMIN_TRANSFERRED: &str = "nft_admin_transferred";

/// Every event topic this contract emits. Tests iterate it so a new topic is
/// checked against the Symbol length limit and `compute_idempotency_key`.
#[cfg(test)]
pub(crate) const ALL_TOPICS: &[&str] = &[
    SHIPMENT_CREATED,
    STATUS_UPDATED,
    MILESTONE_RECORDED,
    SHIPMENT_CANCELLED,
    SHIPMENT_EXPIRED,
    DELIVERY_SUCCESS,
    ESCROW_DEPOSITED,
    ESCROW_RELEASED,
    ESCROW_REFUNDED,
    MILESTONE_PAYMENT_RELEASED,
    PLATFORM_FEE_COLLECTED,
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
    MIGRATION_REPORTED,
    CONTRACT_PAUSED,
    CONTRACT_UNPAUSED,
    FORCE_CANCELLED,
    FORCE_RELEASED,
    FORCE_REFUNDED,
    FEE_CONFIG_UPDATED,
    CONTRACT_INITIALIZED,
    SHIPMENT_LIMIT_UPDATED,
    COMPANY_LIMIT_UPDATED,
    ROLE_REVOKED,
    ROLE_CHANGED,
    WHITELIST_ADDED,
    WHITELIST_REMOVED,
    MULTISIG_INITIALIZED,
    PROPOSAL_APPROVED,
    PROPOSAL_EXECUTED,
    AUDIT_ROLE_ASSIGNED,
    AUDIT_ROLE_REVOKED,
    AUDIT_ROLE_SUSPENDED,
    AUDIT_ROLE_REACTIVATED,
    AUDIT_ADMIN_TRANSFERRED,
    AUDIT_CARRIER_WHITELISTED,
    AUDIT_CARRIER_UNWHITELISTED,
    AUDIT_COMPANY_SUSPENDED,
    AUDIT_COMPANY_REACTIVATED,
    AUDIT_CARRIER_SUSPENDED,
    AUDIT_CARRIER_REACTIVATED,
    CARRIER_HANDOFF,
    NOTIFICATION,
    EVIDENCE_ADDED,
    ESCROW_FROZEN,
    PROOF_MINT_FAILED,
    CARRIER_SUSPENDED,
    CARRIER_REACTIVATED,
    DELIVERY_CONFIRMED,
    GEOFENCE_EVENT,
    ETA_UPDATED,
    PROPOSAL_DIGEST,
    CONFIG_UPDATED,
    QUOTA_SET,
    NFT_BASE_URI_SET,
    NFT_PAUSED,
    NFT_UNPAUSED,
    NFT_ADMIN_TRANSFERRED,
];

/// Topics whose hash domain is *not* the shipment default, paired with the
/// domain they belong to.
///
/// Shipment-lifecycle topics are intentionally absent: they resolve through the
/// fallback, which keeps their previously emitted keys byte-identical.
const NON_DEFAULT_HASH_DOMAINS: &[(&str, u8)] = &[
    // Escrow operations
    (ESCROW_DEPOSITED, HASH_DOMAIN_ESCROW),
    (ESCROW_RELEASED, HASH_DOMAIN_ESCROW),
    (ESCROW_REFUNDED, HASH_DOMAIN_ESCROW),
    (MILESTONE_PAYMENT_RELEASED, HASH_DOMAIN_ESCROW),
    (ESCROW_FROZEN, HASH_DOMAIN_ESCROW),
    (PROOF_MINT_FAILED, HASH_DOMAIN_ESCROW),
    // Disputes
    (DISPUTE_RAISED, HASH_DOMAIN_DISPUTE),
    (DISPUTE_RESOLVED, HASH_DOMAIN_DISPUTE),
    (EVIDENCE_ADDED, HASH_DOMAIN_DISPUTE),
    // Condition / sensor breaches
    (CONDITION_BREACH, HASH_DOMAIN_CONDITION),
    (CARRIER_BREACH, HASH_DOMAIN_CONDITION),
    (GEOFENCE_EVENT, HASH_DOMAIN_CONDITION),
    // Carrier reputation and lifecycle
    (CARRIER_DISPUTE_LOSS, HASH_DOMAIN_CARRIER),
    (CARRIER_LATE_DELIVERY, HASH_DOMAIN_CARRIER),
    (CARRIER_ON_TIME_DELIVERY, HASH_DOMAIN_CARRIER),
    (CARRIER_HANDOFF, HASH_DOMAIN_CARRIER),
    (CARRIER_HANDOFF_COMPLETED, HASH_DOMAIN_CARRIER),
    (CARRIER_MILESTONE_RATE, HASH_DOMAIN_CARRIER),
    (CARRIER_SUSPENDED, HASH_DOMAIN_CARRIER),
    (CARRIER_REACTIVATED, HASH_DOMAIN_CARRIER),
    // Admin / governance
    (ADMIN_PROPOSED, HASH_DOMAIN_ADMIN),
    (ADMIN_TRANSFERRED, HASH_DOMAIN_ADMIN),
    (CONTRACT_UPGRADED, HASH_DOMAIN_ADMIN),
    (MIGRATION_REPORTED, HASH_DOMAIN_ADMIN),
    (CONTRACT_PAUSED, HASH_DOMAIN_ADMIN),
    (CONTRACT_UNPAUSED, HASH_DOMAIN_ADMIN),
    (FORCE_CANCELLED, HASH_DOMAIN_ADMIN),
    (FORCE_RELEASED, HASH_DOMAIN_ADMIN),
    (FORCE_REFUNDED, HASH_DOMAIN_ADMIN),
    (CONTRACT_INITIALIZED, HASH_DOMAIN_ADMIN),
    (SHIPMENT_LIMIT_UPDATED, HASH_DOMAIN_ADMIN),
    (COMPANY_LIMIT_UPDATED, HASH_DOMAIN_ADMIN),
    (PROPOSAL_DIGEST, HASH_DOMAIN_ADMIN),
    (CONFIG_UPDATED, HASH_DOMAIN_ADMIN),
    (QUOTA_SET, HASH_DOMAIN_ADMIN),
    (NFT_BASE_URI_SET, HASH_DOMAIN_ADMIN),
    (NFT_PAUSED, HASH_DOMAIN_ADMIN),
    (NFT_UNPAUSED, HASH_DOMAIN_ADMIN),
    (NFT_ADMIN_TRANSFERRED, HASH_DOMAIN_ADMIN),
    // RBAC
    (ROLE_REVOKED, HASH_DOMAIN_RBAC),
    (ROLE_CHANGED, HASH_DOMAIN_RBAC),
    // Notifications
    (NOTIFICATION, HASH_DOMAIN_NOTIFICATION),
    // Platform-level fee events
    (PLATFORM_FEE_COLLECTED, HASH_DOMAIN_PLATFORM),
    (FEE_CONFIG_UPDATED, HASH_DOMAIN_PLATFORM),
];

/// Resolves the hash domain tag for an event topic `Symbol`.
///
/// This is the on-chain entry point: `Symbol` cannot be read back as a `&str`
/// inside the guest, so each candidate topic is re-interned and compared.
/// Only non-default domains are in the table, so shipment-lifecycle events —
/// the hot path — fall through without a match.
pub fn hash_domain_for_symbol(env: &soroban_sdk::Env, event_type: &soroban_sdk::Symbol) -> u8 {
    for (topic, domain) in NON_DEFAULT_HASH_DOMAINS {
        if *event_type == soroban_sdk::Symbol::new(env, topic) {
            return *domain;
        }
    }
    HASH_DOMAIN_SHIPMENT
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Length guard ─────────────────────────────────────────────────────────
    // Soroban Symbol values are limited to 32 characters.  This test catches
    // any constant that would silently fail at runtime.

    #[test]
    fn all_topic_constants_are_within_symbol_length_limit() {
        for topic in ALL_TOPICS {
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
        // Pinned string values, in the same order as `ALL_TOPICS`. Adding a
        // topic to `ALL_TOPICS` without pinning its value here fails the
        // length check below.
        let expected: &[&str] = &[
            "shipment_created",
            "status_updated",
            "milestone_recorded",
            "shipment_cancelled",
            "shipment_expired",
            "delivery_success",
            "escrow_deposited",
            "escrow_released",
            "escrow_refunded",
            "milestone_payment_released",
            "platform_fee_collected",
            "dispute_raised",
            "dispute_resolved",
            "condition_breach",
            "carrier_breach",
            "carrier_dispute_loss",
            "carrier_late_delivery",
            "carrier_on_time_delivery",
            "carrier_handoff_completed",
            "carrier_milestone_rate",
            "admin_proposed",
            "admin_transferred",
            "contract_upgraded",
            "migration_reported",
            "contract_paused",
            "contract_unpaused",
            "force_cancelled",
            "force_released",
            "force_refunded",
            "fee_config_updated",
            "init",
            "set_limit",
            "set_cmp_limit",
            "role_revoked",
            "role_changed",
            "add_wl",
            "rm_wl",
            "ms_init",
            "approve",
            "executed",
            "audit_role_assigned",
            "audit_role_revoked",
            "audit_role_suspended",
            "audit_role_reactivated",
            "audit_admin_transferred",
            "audit_carrier_whitelisted",
            "audit_carrier_unwhitelisted",
            "audit_company_suspended",
            "audit_company_reactivated",
            "audit_carrier_suspended",
            "audit_carrier_reactivated",
            "carrier_handoff",
            "notification",
            "evidence_added",
            "escrow_frozen",
            "carrier_suspended",
            "carrier_reactivated",
            "delivery_confirmed",
            "geofence_event",
            "eta_updated",
            "proposal_digest",
            "config_updated",
            "quota_set",
            "nft_base_uri_set",
            "nft_paused",
            "nft_unpaused",
            "nft_admin_transferred",
        ];
        assert_eq!(
            ALL_TOPICS.len(),
            expected.len(),
            "ALL_TOPICS and the pinned values list must stay in sync"
        );
        for (topic, value) in ALL_TOPICS.iter().zip(expected) {
            assert_eq!(
                topic, value,
                "Topic value changed: '{}' != '{}'",
                topic, value
            );
        }
    }

    #[test]
    fn all_topic_constants_are_unique() {
        let mut topics = [""; ALL_TOPICS.len()];
        topics.copy_from_slice(ALL_TOPICS);
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

    /// Every `pub const ...: &str` declared in this file must be listed in
    /// `ALL_TOPICS`, so the guard tests above cannot silently skip a topic.
    #[test]
    fn all_topics_covers_every_topic_constant() {
        let source = include_str!("event_topics.rs");
        let mut declared = 0;
        for line in source.lines() {
            let Some(rest) = line.strip_prefix("pub const ") else {
                continue;
            };
            let Some((_, value)) = rest.split_once(": &str = \"") else {
                continue;
            };
            let value = value.trim_end_matches("\";");
            declared += 1;
            assert!(
                ALL_TOPICS.contains(&value),
                "Topic '{}' is declared but missing from ALL_TOPICS",
                value
            );
        }
        assert_eq!(declared, ALL_TOPICS.len());
    }

    #[test]
    fn force_actions_use_admin_hash_domain() {
        let env = soroban_sdk::Env::default();
        for topic in [FORCE_CANCELLED, FORCE_RELEASED, FORCE_REFUNDED] {
            assert_eq!(
                hash_domain_for_symbol(&env, &soroban_sdk::Symbol::new(&env, topic)),
                HASH_DOMAIN_ADMIN,
                "'{}' must hash under the admin domain",
                topic
            );
        }
    }

    /// Verifies that every hash domain-separation prefix has a unique `u8` value.
    ///
    /// Adding a new family domain tag must not reuse an existing discriminant;
    /// this test is the compile-time-adjacent guard against that mistake.
    #[test]
    fn all_hash_domain_constants_are_unique() {
        let mut domains = [
            HASH_DOMAIN_SHIPMENT,
            HASH_DOMAIN_ESCROW,
            HASH_DOMAIN_DISPUTE,
            HASH_DOMAIN_CONDITION,
            HASH_DOMAIN_CARRIER,
            HASH_DOMAIN_ADMIN,
            HASH_DOMAIN_RBAC,
            HASH_DOMAIN_NOTIFICATION,
            HASH_DOMAIN_PLATFORM,
        ];
        domains.sort_unstable();
        for pair in domains.windows(2) {
            assert_ne!(
                pair[0], pair[1],
                "Duplicate HASH_DOMAIN constant value detected: 0x{:02X}",
                pair[0]
            );
        }
    }

    /// Verifies that each domain constant has the exact value specified in the
    /// design.  Changing a value is a breaking change for off-chain indexers.
    #[test]
    fn hash_domain_constant_values_are_stable() {
        assert_eq!(HASH_DOMAIN_SHIPMENT, 0x01);
        assert_eq!(HASH_DOMAIN_ESCROW, 0x02);
        assert_eq!(HASH_DOMAIN_DISPUTE, 0x03);
        assert_eq!(HASH_DOMAIN_CONDITION, 0x04);
        assert_eq!(HASH_DOMAIN_CARRIER, 0x05);
        assert_eq!(HASH_DOMAIN_ADMIN, 0x06);
        assert_eq!(HASH_DOMAIN_RBAC, 0x07);
        assert_eq!(HASH_DOMAIN_NOTIFICATION, 0x08);
        assert_eq!(HASH_DOMAIN_PLATFORM, 0x0B);
    }
}
