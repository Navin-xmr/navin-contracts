//! # Events Module — Hash-and-Emit Pattern
//!
//! The heart of Navin's off-chain data architecture. Instead of storing heavy
//! payloads (GPS traces, sensor readings, metadata) on-chain, the contract
//! emits structured events containing only the `shipment_id`, relevant
//! identifiers, and a `data_hash` (SHA-256 of the full off-chain payload).
//!
//! ## Event Schemas
//!
//! There is **no single schema shared by every event**. Payload shapes fall
//! into the families below; the rustdoc table on each `emit_*` function is
//! the authoritative field list for that event.
//!
//! ### Shipment lifecycle — the 5-field tuple
//!
//! `(shipment_id, status, data_hash, timestamp, actor)`
//!
//! Exactly this shape: `emit_shipment_created`, `emit_status_updated`,
//! `emit_shipment_cancelled` (hash is the reason hash),
//! `emit_shipment_expired`, `emit_delivery_success` (timestamp is the
//! delivery time), `emit_delivery_confirmed`.
//!
//! The 5-field prefix followed by extra trailing fields:
//!
//! | Emitter                                  | Trailing fields            |
//! |------------------------------------------|----------------------------|
//! | `emit_milestone_recorded`                | `checkpoint`               |
//! | `emit_eta_updated`                       | `new_eta`                  |
//! | `emit_escrow_deposited` / `_released` / `_refunded` | `amount`        |
//! | `emit_platform_fee_collected`            | `amount`                   |
//! | `emit_milestone_payment_released`        | `milestone`, `amount`      |
//! | `emit_force_cancelled` / `_released` / `_refunded` | escrow amount    |
//!
//! `emit_geofence_event` keeps the 5-field layout but carries a `zone_type`
//! in the `status` slot.
//!
//! ### Other families — their own shapes
//!
//! These do **not** follow the 5-field tuple; see each function's docs:
//!
//! - **Disputes:** `emit_dispute_raised`, `emit_dispute_resolved`,
//!   `emit_escrow_frozen`.
//! - **Conditions and handoffs:** `emit_condition_breach`,
//!   `emit_carrier_handoff`, `emit_carrier_handoff_completed`.
//! - **Carrier reputation:** `emit_carrier_breach`,
//!   `emit_carrier_dispute_loss`, `emit_carrier_late_delivery`,
//!   `emit_carrier_on_time_delivery`, `emit_carrier_milestone_rate`,
//!   `emit_carrier_suspended`, `emit_carrier_reactivated`.
//! - **Admin and governance:** `emit_contract_initialized`,
//!   `emit_contract_upgraded`, `emit_migration_report`,
//!   `emit_admin_proposed`, `emit_admin_transferred`,
//!   `emit_contract_paused`, `emit_contract_unpaused`,
//!   `emit_shipment_limit_updated`, `emit_company_limit_updated`,
//!   `emit_config_updated`, `emit_proposal_digest`, `emit_quota_set`,
//!   `emit_fee_config_updated`.
//! - **RBAC:** `emit_role_revoked`, `emit_role_changed`.
//! - **Notifications:** `emit_notification`.
//!
//! ## Listeners
//!
//! | Consumer          | Purpose                                          |
//! |-------------------|--------------------------------------------------|
//! | Express backend   | Indexes events into the off-chain database        |
//! | Frontend (React)  | Verifies events directly via Stellar RPC node     |
//! | Analytics pipeline| Aggregates shipment lifecycle metrics              |
//!
//! ## Topic Convention
//!
//! Each event uses a single descriptive `Symbol` as its topic so that
//! consumers can filter by topic when subscribing to contract events.

use crate::types::{
    BreachType, EscrowFreezeReason, MigrationReport, Role, RoleChangeAction, Severity,
    ShipmentStatus,
};
use soroban_sdk::{xdr::ToXdr, Address, Bytes, BytesN, Env, Symbol};

/// Version of the tuple suffix used by schema-aware events.
pub const EVENT_SCHEMA_VERSION: u32 = 2;

/// `data_hash` for events whose payload lives entirely on-chain (escrow
/// movements, expiry, fees): SHA-256 over
/// `topic.to_xdr() || shipment_id (u64 BE) || actor.to_xdr() || amount (i128 BE)`,
/// followed by `milestone.to_xdr()` when present. Distinct payloads therefore
/// never share a hash, and consumers can recompute it from the event fields.
fn payload_hash(
    env: &Env,
    topic: &str,
    shipment_id: u64,
    actor: &Address,
    amount: i128,
    milestone: Option<&Symbol>,
) -> BytesN<32> {
    let mut payload = Symbol::new(env, topic).to_xdr(env);
    payload.append(&Bytes::from_array(env, &shipment_id.to_be_bytes()));
    payload.append(&actor.clone().to_xdr(env));
    payload.append(&Bytes::from_array(env, &amount.to_be_bytes()));
    if let Some(milestone) = milestone {
        payload.append(&milestone.clone().to_xdr(env));
    }
    env.crypto().sha256(&payload).into()
}

/// Compute the canonical idempotency key for an event.
///
/// The idempotency key is a SHA-256 hash of a canonical binary payload
/// consisting of length-delimited `domain` and `event_type` fields, with
/// fixed-width numeric fields in between:
///
/// 1. `domain_len` (u32 big-endian), `domain_bytes`
/// 2. `shipment_id` as big-endian u64 (8 bytes)
/// 3. `topic_len` (u32 big-endian), `topic_bytes`
/// 4. `event_counter` as big-endian u32 (4 bytes)
pub fn generate_idempotency_key(
    env: &Env,
    domain: u8,
    shipment_id: u64,
    event_type: &str,
    event_counter: u32,
) -> BytesN<32> {
    let mut payload = Bytes::new(env);

    let domain_bytes = domain.to_be_bytes();
    payload.append(&Bytes::from_array(
        env,
        &(domain_bytes.len() as u32).to_be_bytes(),
    ));
    payload.append(&Bytes::from_slice(env, &domain_bytes));

    payload.append(&Bytes::from_array(env, &shipment_id.to_be_bytes()));

    payload.append(&Bytes::from_array(
        env,
        &(event_type.len() as u32).to_be_bytes(),
    ));
    payload.append(&Bytes::from_slice(env, event_type.as_bytes()));

    payload.append(&Bytes::from_array(env, &event_counter.to_be_bytes()));
    env.crypto().sha256(&payload).into()
}

fn next_global_event_metadata(env: &Env, topic: &str, domain: u8) -> (u32, BytesN<32>) {
    let event_counter = crate::storage::increment_global_event_count(env);
    let idempotency_key =
        generate_idempotency_key(env, domain, event_counter as u64, topic, event_counter);
    (event_counter, idempotency_key)
}

pub fn emit_whitelist_added(env: &Env, company: &Address, carrier: &Address) {
    let (event_counter, idempotency_key) = next_global_event_metadata(
        env,
        crate::event_topics::WHITELIST_ADDED,
        crate::event_topics::HASH_DOMAIN_RBAC,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::WHITELIST_ADDED),),
        (
            company.clone(),
            carrier.clone(),
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
}

pub fn emit_whitelist_removed(env: &Env, company: &Address, carrier: &Address) {
    let (event_counter, idempotency_key) = next_global_event_metadata(
        env,
        crate::event_topics::WHITELIST_REMOVED,
        crate::event_topics::HASH_DOMAIN_RBAC,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::WHITELIST_REMOVED),),
        (
            company.clone(),
            carrier.clone(),
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
}

pub fn emit_multisig_initialized(env: &Env, admin_count: u32, threshold: u32) {
    let (event_counter, idempotency_key) = next_global_event_metadata(
        env,
        crate::event_topics::MULTISIG_INITIALIZED,
        crate::event_topics::HASH_DOMAIN_ADMIN,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::MULTISIG_INITIALIZED),),
        (
            admin_count,
            threshold,
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
}

pub fn emit_proposal_approved(
    env: &Env,
    proposal_id: u64,
    approver: &Address,
    approval_count: u32,
) {
    let (event_counter, idempotency_key) = next_global_event_metadata(
        env,
        crate::event_topics::PROPOSAL_APPROVED,
        crate::event_topics::HASH_DOMAIN_ADMIN,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::PROPOSAL_APPROVED),),
        (
            proposal_id,
            approver.clone(),
            approval_count,
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
}

pub fn emit_proposal_executed(env: &Env, proposal_id: u64, action: &crate::types::AdminAction) {
    let (event_counter, idempotency_key) = next_global_event_metadata(
        env,
        crate::event_topics::PROPOSAL_EXECUTED,
        crate::event_topics::HASH_DOMAIN_ADMIN,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::PROPOSAL_EXECUTED),),
        (
            proposal_id,
            action.clone(),
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
}

pub fn emit_circuit_breaker_config_updated(
    env: &Env,
    failure_threshold: u32,
    recovery_timeout: u64,
    half_open_max_requests: u32,
) {
    let (event_counter, idempotency_key) = next_global_event_metadata(
        env,
        crate::event_topics::CONFIG_UPDATED,
        crate::event_topics::HASH_DOMAIN_ADMIN,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CONFIG_UPDATED),),
        (
            Symbol::new(env, "circuit_breaker"),
            failure_threshold,
            recovery_timeout,
            half_open_max_requests,
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
}

/// Emits a `shipment_created` event when a new shipment is registered.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_shipment_created(env: &Env, shipment_id: u64, actor: &Address, data_hash: &BytesN<32>) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::SHIPMENT_CREATED),),
        (
            shipment_id,
            ShipmentStatus::Created,
            data_hash.clone(),
            env.ledger().timestamp(),
            actor.clone(),
        ),
    );
}

/// Emits a `status_updated` event when a shipment transitions between lifecycle states.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_status_updated(
    env: &Env,
    shipment_id: u64,
    status: &ShipmentStatus,
    data_hash: &BytesN<32>,
    actor: &Address,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::STATUS_UPDATED),),
        (
            shipment_id,
            status.clone(),
            data_hash.clone(),
            env.ledger().timestamp(),
            actor.clone(),
        ),
    );
}

/// Emits a `milestone_recorded` event when a carrier reports a checkpoint.
///
/// Milestones are **never stored on-chain** — this is the canonical example
/// of the Hash-and-Emit pattern. The full milestone payload (GPS coordinates,
/// temperature readings, photos) lives off-chain; only its hash is published.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_milestone_recorded(
    env: &Env,
    shipment_id: u64,
    checkpoint: &Symbol,
    data_hash: &BytesN<32>,
    reporter: &Address,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::MILESTONE_RECORDED),),
        (
            shipment_id,
            ShipmentStatus::InTransit,
            data_hash.clone(),
            env.ledger().timestamp(),
            reporter.clone(),
            checkpoint.clone(),
        ),
    );
    crate::storage::increment_milestone_event_count(env, shipment_id);
}

/// Emits an `escrow_deposited` event when funds are locked for a shipment.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_escrow_deposited(env: &Env, shipment_id: u64, from: &Address, amount: i128) {
    let data_hash = payload_hash(
        env,
        crate::event_topics::ESCROW_DEPOSITED,
        shipment_id,
        from,
        amount,
        None,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ESCROW_DEPOSITED),),
        (
            shipment_id,
            ShipmentStatus::Created,
            data_hash,
            env.ledger().timestamp(),
            from.clone(),
            amount,
        ),
    );
}

/// Emits an `escrow_released` event when escrowed funds are paid out.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_escrow_released(env: &Env, shipment_id: u64, to: &Address, amount: i128) {
    let data_hash = payload_hash(
        env,
        crate::event_topics::ESCROW_RELEASED,
        shipment_id,
        to,
        amount,
        None,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ESCROW_RELEASED),),
        (
            shipment_id,
            ShipmentStatus::Delivered,
            data_hash,
            env.ledger().timestamp(),
            to.clone(),
            amount,
        ),
    );
}

/// Emits an `escrow_refunded` event when escrowed funds are returned to the company.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_escrow_refunded(env: &Env, shipment_id: u64, to: &Address, amount: i128) {
    let data_hash = payload_hash(
        env,
        crate::event_topics::ESCROW_REFUNDED,
        shipment_id,
        to,
        amount,
        None,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ESCROW_REFUNDED),),
        (
            shipment_id,
            ShipmentStatus::Cancelled,
            data_hash,
            env.ledger().timestamp(),
            to.clone(),
            amount,
        ),
    );
}

/// Emits a `milestone_payment_released` event when a partial escrow release occurs.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_milestone_payment_released(
    env: &Env,
    shipment_id: u64,
    milestone: &Symbol,
    amount: i128,
    to: &Address,
) {
    let data_hash = payload_hash(
        env,
        crate::event_topics::MILESTONE_PAYMENT_RELEASED,
        shipment_id,
        to,
        amount,
        Some(milestone),
    );
    env.events().publish(
        (Symbol::new(
            env,
            crate::event_topics::MILESTONE_PAYMENT_RELEASED,
        ),),
        (
            shipment_id,
            ShipmentStatus::InTransit,
            data_hash,
            env.ledger().timestamp(),
            to.clone(),
            milestone.clone(),
            amount,
        ),
    );
}

/// Emits a `dispute_raised` event when a party disputes a shipment.
///
/// The `reason_hash` follows the same Hash-and-Emit pattern: the full dispute
/// description (text, evidence, photos) is stored off-chain, and only its
/// SHA-256 hash is published on the ledger for tamper-proof auditability.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_dispute_raised(
    env: &Env,
    shipment_id: u64,
    raised_by: &Address,
    reason_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::DISPUTE_RAISED),),
        (shipment_id, raised_by.clone(), reason_hash.clone()),
    );
}

/// Emits a `shipment_cancelled` event when a shipment is cancelled.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_shipment_cancelled(
    env: &Env,
    shipment_id: u64,
    caller: &Address,
    reason_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::SHIPMENT_CANCELLED),),
        (
            shipment_id,
            ShipmentStatus::Cancelled,
            reason_hash.clone(),
            env.ledger().timestamp(),
            caller.clone(),
        ),
    );
}

/// Emits a `contract_upgraded` event when the contract WASM is upgraded.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_contract_upgraded(
    env: &Env,
    admin: &Address,
    new_wasm_hash: &BytesN<32>,
    version: u32,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CONTRACT_UPGRADED),),
        (admin.clone(), new_wasm_hash.clone(), version),
    );
}

/// Emits a `migration_reported` event summarizing the impact of an upgrade.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_migration_report(env: &Env, report: &MigrationReport) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::MIGRATION_REPORTED),),
        (
            report.current_version,
            report.target_version,
            report.affected_shipments,
        ),
    );
}

/// Emits a `carrier_handoff` event when a shipment is transferred between carriers.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_carrier_handoff(
    env: &Env,
    shipment_id: u64,
    from_carrier: &Address,
    to_carrier: &Address,
    handoff_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CARRIER_HANDOFF),),
        (
            shipment_id,
            from_carrier.clone(),
            to_carrier.clone(),
            handoff_hash.clone(),
        ),
    );
}

/// Emits a `condition_breach` event when a carrier detects an out-of-range sensor reading.
///
/// The full sensor payload remains off-chain; only the `data_hash` is emitted.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_condition_breach(
    env: &Env,
    shipment_id: u64,
    carrier: &Address,
    breach_type: &BreachType,
    severity: &Severity,
    data_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CONDITION_BREACH),),
        (
            shipment_id,
            carrier.clone(),
            breach_type.clone(),
            severity.clone(),
            data_hash.clone(),
        ),
    );
}

/// Emits an `admin_proposed` event when a new administrator is proposed.
pub fn emit_admin_proposed(env: &Env, current_admin: &Address, proposed_admin: &Address) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ADMIN_PROPOSED),),
        (current_admin.clone(), proposed_admin.clone()),
    );
}

/// Emits an `admin_transferred` event when the administrator role is successfully transferred.
pub fn emit_admin_transferred(env: &Env, old_admin: &Address, new_admin: &Address) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ADMIN_TRANSFERRED),),
        (old_admin.clone(), new_admin.clone()),
    );
}

/// Emits a `shipment_expired` event when a shipment misses its deadline and is auto-cancelled.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_shipment_expired(env: &Env, shipment_id: u64, admin: &Address) {
    let data_hash = payload_hash(
        env,
        crate::event_topics::SHIPMENT_EXPIRED,
        shipment_id,
        admin,
        0,
        None,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::SHIPMENT_EXPIRED),),
        (
            shipment_id,
            ShipmentStatus::Cancelled,
            data_hash,
            env.ledger().timestamp(),
            admin.clone(),
        ),
    );
}

/// Emits a `delivery_success` event when a shipment is successfully delivered.
///
/// The backend indexes this event to increment the carrier's on-time delivery
/// count and compute punctuality metrics relative to the shipment deadline.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_delivery_success(env: &Env, carrier: &Address, shipment_id: u64, delivery_time: u64) {
    let zero_hash: BytesN<32> = BytesN::from_array(env, &[0u8; 32]);
    env.events().publish(
        (Symbol::new(env, crate::event_topics::DELIVERY_SUCCESS),),
        (
            shipment_id,
            ShipmentStatus::Delivered,
            zero_hash,
            delivery_time,
            carrier.clone(),
        ),
    );
}

/// Emits a `carrier_breach` event when a carrier reports a condition breach.
///
/// The backend indexes this event to increment the carrier's breach count and
/// adjust the reliability score accordingly.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_carrier_breach(
    env: &Env,
    carrier: &Address,
    shipment_id: u64,
    breach_type: &BreachType,
    severity: &Severity,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CARRIER_BREACH),),
        (
            carrier.clone(),
            shipment_id,
            breach_type.clone(),
            severity.clone(),
        ),
    );
}

/// Emits a `carrier_dispute_loss` event when a dispute is resolved against the
/// carrier (i.e., `DisputeResolution::RefundToCompany`).
///
/// The backend indexes this event to penalise the carrier's reputation score.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_carrier_dispute_loss(env: &Env, carrier: &Address, shipment_id: u64) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CARRIER_DISPUTE_LOSS),),
        (carrier.clone(), shipment_id),
    );
}

/// Emits a `notification` event for backend indexing to trigger push notifications,
/// emails, or in-app alerts.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_notification(
    env: &Env,
    recipient: &Address,
    notification_type: crate::types::NotificationType,
    shipment_id: u64,
    data_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::NOTIFICATION),),
        (
            recipient.clone(),
            notification_type,
            shipment_id,
            data_hash.clone(),
        ),
    );
}

/// Emits a `carrier_late_delivery` event when a carrier completes delivery after the deadline.
pub fn emit_carrier_late_delivery(
    env: &Env,
    carrier: &Address,
    shipment_id: u64,
    deadline: u64,
    actual_delivery_time: u64,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CARRIER_LATE_DELIVERY),),
        (carrier.clone(), shipment_id, deadline, actual_delivery_time),
    );
}

/// Emits a `carrier_on_time_delivery` event when a carrier completes delivery on or before the deadline.
pub fn emit_carrier_on_time_delivery(env: &Env, carrier: &Address, shipment_id: u64) {
    env.events().publish(
        (Symbol::new(
            env,
            crate::event_topics::CARRIER_ON_TIME_DELIVERY,
        ),),
        (carrier.clone(), shipment_id),
    );
}

/// Emits a `carrier_handoff_completed` event when a shipment is transferred between carriers.
pub fn emit_carrier_handoff_completed(
    env: &Env,
    from_carrier: &Address,
    to_carrier: &Address,
    shipment_id: u64,
) {
    env.events().publish(
        (Symbol::new(
            env,
            crate::event_topics::CARRIER_HANDOFF_COMPLETED,
        ),),
        (from_carrier.clone(), to_carrier.clone(), shipment_id),
    );
}

/// Emits a `role_revoked` event when an admin revokes a role from an address.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_role_revoked(env: &Env, admin: &Address, target: &Address, role: &crate::types::Role) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ROLE_REVOKED),),
        (admin.clone(), target.clone(), role.clone()),
    );
}

/// Emits a `role_changed` event for the complete RBAC audit trail.
///
/// This event is emitted on every role assignment, revocation, suspension,
/// and reactivation. It provides a complete history stream for compliance,
/// analytics, and off-chain indexing.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_role_changed(
    env: &Env,
    action: &RoleChangeAction,
    admin: &Address,
    target: &Address,
    role: &Role,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ROLE_CHANGED),),
        (
            action.clone(),
            admin.clone(),
            target.clone(),
            role.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emits a `carrier_milestone_rate` event to track completeness of checkpoint reporting.
pub fn emit_carrier_milestone_rate(
    env: &Env,
    carrier: &Address,
    shipment_id: u64,
    milestones_hit: u32,
    total_milestones: u32,
) {
    env.events().publish(
        (Symbol::new(
            env,
            crate::event_topics::CARRIER_MILESTONE_RATE,
        ),),
        (
            carrier.clone(),
            shipment_id,
            milestones_hit,
            total_milestones,
        ),
    );
}

/// Emits a `force_cancelled` event when an admin or multi-sig forcibly cancels a shipment.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_force_cancelled(
    env: &Env,
    shipment_id: u64,
    admin: &Address,
    reason_hash: &BytesN<32>,
    escrow_refunded: i128,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::FORCE_CANCELLED),),
        (
            shipment_id,
            ShipmentStatus::Cancelled,
            reason_hash.clone(),
            env.ledger().timestamp(),
            admin.clone(),
            escrow_refunded,
        ),
    );
}

/// Emits a `force_released` event when an admin or multi-sig forcibly releases escrow for a shipment to carrier.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_force_released(
    env: &Env,
    shipment_id: u64,
    admin: &Address,
    reason_hash: &BytesN<32>,
    escrow_released: i128,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::FORCE_RELEASED),),
        (
            shipment_id,
            ShipmentStatus::Delivered,
            reason_hash.clone(),
            env.ledger().timestamp(),
            admin.clone(),
            escrow_released,
        ),
    );
}

/// Emits a `force_refunded` event when an admin or multi-sig forcibly refunds escrow for a shipment to company.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_force_refunded(
    env: &Env,
    shipment_id: u64,
    admin: &Address,
    reason_hash: &BytesN<32>,
    escrow_refunded: i128,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::FORCE_REFUNDED),),
        (
            shipment_id,
            ShipmentStatus::Cancelled,
            reason_hash.clone(),
            env.ledger().timestamp(),
            admin.clone(),
            escrow_refunded,
        ),
    );
}

/// Emits a `dispute_resolved` event when an admin settles a shipment dispute.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_dispute_resolved(
    env: &Env,
    shipment_id: u64,
    resolution: &crate::types::DisputeResolution,
    reason_hash: &BytesN<32>,
    admin: &Address,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::DISPUTE_RESOLVED),),
        (
            shipment_id,
            resolution.clone(),
            reason_hash.clone(),
            admin.clone(),
        ),
    );
}

/// Emits a `contract_paused` event when the contract is paused by an admin.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_contract_paused(env: &Env, admin: &Address) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CONTRACT_PAUSED),),
        (admin.clone(), env.ledger().timestamp()),
    );
}

/// Emits a `contract_unpaused` event when the contract is unpaused by an admin.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_contract_unpaused(env: &Env, admin: &Address) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CONTRACT_UNPAUSED),),
        (admin.clone(), env.ledger().timestamp()),
    );
}

/// Emits an `escrow_frozen` event when escrow is blocked due to a dispute or safety control.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_escrow_frozen(
    env: &Env,
    shipment_id: u64,
    reason: EscrowFreezeReason,
    caller: &Address,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ESCROW_FROZEN),),
        (
            shipment_id,
            reason,
            caller.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emits a `platform_fee_collected` event when a fee is deducted from a deposit.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_platform_fee_collected(env: &Env, shipment_id: u64, treasury: &Address, amount: i128) {
    let data_hash = payload_hash(
        env,
        crate::event_topics::PLATFORM_FEE_COLLECTED,
        shipment_id,
        treasury,
        amount,
        None,
    );
    env.events().publish(
        (Symbol::new(
            env,
            crate::event_topics::PLATFORM_FEE_COLLECTED,
        ),),
        (
            shipment_id,
            ShipmentStatus::Created,
            data_hash,
            env.ledger().timestamp(),
            treasury.clone(),
            amount,
        ),
    );
}

/// Emits a `fee_config_updated` event when the platform fee configuration changes.
pub fn emit_fee_config_updated(env: &Env, admin: &Address, fee_bps: u32, treasury: &Address) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::FEE_CONFIG_UPDATED),),
        (admin.clone(), fee_bps, treasury.clone()),
    );
}

pub fn emit_contract_initialized(env: &Env, admin: &Address, token_contract: &Address) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CONTRACT_INITIALIZED),),
        (admin.clone(), token_contract.clone()),
    );
}

pub fn emit_shipment_limit_updated(env: &Env, admin: &Address, limit: u32) {
    env.events().publish(
        (Symbol::new(
            env,
            crate::event_topics::SHIPMENT_LIMIT_UPDATED,
        ),),
        (admin.clone(), limit),
    );
}

pub fn emit_company_limit_updated(env: &Env, admin: &Address, company: &Address, limit: u32) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::COMPANY_LIMIT_UPDATED),),
        (admin.clone(), company.clone(), limit),
    );
}

pub fn emit_carrier_suspended(env: &Env, admin: &Address, carrier: &Address) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CARRIER_SUSPENDED),),
        (admin.clone(), carrier.clone()),
    );
}

pub fn emit_carrier_reactivated(env: &Env, admin: &Address, carrier: &Address) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CARRIER_REACTIVATED),),
        (admin.clone(), carrier.clone()),
    );
}

/// Emits a `delivery_confirmed` event when a receiver confirms delivery.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_delivery_confirmed(
    env: &Env,
    shipment_id: u64,
    receiver: &Address,
    data_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::DELIVERY_CONFIRMED),),
        (
            shipment_id,
            ShipmentStatus::Delivered,
            data_hash.clone(),
            env.ledger().timestamp(),
            receiver.clone(),
        ),
    );
}

/// Emits a `geofence_event` when a carrier crosses a geofence boundary.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_geofence_event(
    env: &Env,
    shipment_id: u64,
    zone_type: crate::types::GeofenceEvent,
    data_hash: &BytesN<32>,
    actor: &Address,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::GEOFENCE_EVENT),),
        (
            shipment_id,
            zone_type,
            data_hash.clone(),
            env.ledger().timestamp(),
            actor.clone(),
        ),
    );
}

/// Emits an `eta_updated` event when the estimated arrival time changes.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_eta_updated(
    env: &Env,
    shipment_id: u64,
    new_eta: u64,
    data_hash: &BytesN<32>,
    actor: &Address,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ETA_UPDATED),),
        (
            shipment_id,
            ShipmentStatus::InTransit,
            data_hash.clone(),
            env.ledger().timestamp(),
            actor.clone(),
            new_eta,
        ),
    );
}

pub fn emit_proposal_digest(env: &Env, proposal_id: u64, digest: BytesN<32>, computed_at: u64) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::PROPOSAL_DIGEST),),
        (proposal_id, digest, computed_at),
    );
}

pub fn emit_config_updated(env: &Env, admin: &Address, new_config: &crate::ContractConfig) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CONFIG_UPDATED),),
        (admin.clone(), new_config.clone()),
    );
}

pub fn emit_quota_set(env: &Env, company: &Address, count: u32, window_start: u64) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::QUOTA_SET),),
        (company.clone(), count, window_start),
    );
}

/// Emits an `evidence_added` event when a dispute participant attaches evidence.
///
/// Follows the contract's hash-and-emit model: the evidence document itself
/// (photos, inspection reports, correspondence) is stored off-chain and only
/// its SHA-256 hash is published on the ledger, so the submission is
/// timestamped and tamper-evident without putting the payload on-chain.
///
/// See `docs/events.md` for the full payload layout.
pub fn emit_evidence_added(
    env: &Env,
    shipment_id: u64,
    submitted_by: &Address,
    index: u32,
    evidence_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::EVIDENCE_ADDED),),
        (
            shipment_id,
            submitted_by.clone(),
            index,
            evidence_hash.clone(),
        ),
    );
}

pub fn emit_shipment_archived(env: &Env, shipment_id: u64, timestamp: u64) {
    env.events().publish(
        (Symbol::new(env, "shipment_archived"),),
        (shipment_id, timestamp),
    );
}
