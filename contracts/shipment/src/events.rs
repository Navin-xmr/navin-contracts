//! # Events Module — Hash-and-Emit Pattern
//!
//! The heart of Navin's off-chain data architecture. Instead of storing heavy
//! payloads (GPS traces, sensor readings, metadata) on-chain, the contract
//! emits structured events containing only the `shipment_id`, relevant
//! identifiers, and a `data_hash` (SHA-256 of the full off-chain payload).
//!
//! ## Shipment Lifecycle Event Schema
//!
//! Every shipment lifecycle event carries a minimal tuple:
//! `(shipment_id, status, data_hash, timestamp, actor)`
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
use soroban_sdk::{Address, BytesN, Env, Symbol};

#[cfg(test)]
use soroban_sdk::Bytes;

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
#[cfg(test)]
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

/// Emits a `shipment_created` event when a new shipment is registered.
///
/// # Event Payload
///
/// | Field       | Type          | Description                                     |
/// |-------------|---------------|-------------------------------------------------|
/// | shipment_id | `u64`         | Unique on-chain shipment identifier              |
/// | status      | `ShipmentStatus` | Always `Created`                              |
/// | data_hash   | `BytesN<32>`  | SHA-256 hash of the full off-chain shipment data |
/// | timestamp   | `u64`         | Ledger timestamp at creation                     |
/// | actor       | `Address`     | Company address that created the shipment        |
pub fn emit_shipment_created(env: &Env, shipment_id: u64, actor: &Address, data_hash: &BytesN<32>) {
    let zero_hash: BytesN<32> = BytesN::from_array(env, &[0u8; 32]);
    let _ = &zero_hash; // unused if data_hash is provided
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
/// # Event Payload
///
/// | Field       | Type          | Description                                        |
/// |-------------|---------------|----------------------------------------------------|
/// | shipment_id | `u64`         | Shipment whose status changed                      |
/// | status      | `ShipmentStatus` | New lifecycle state after transition            |
/// | data_hash   | `BytesN<32>`  | SHA-256 hash of the updated off-chain payload       |
/// | timestamp   | `u64`         | Ledger timestamp of the transition                  |
/// | actor       | `Address`     | Address that triggered the status change            |
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
/// # Event Payload
///
/// | Field       | Type          | Description                                       |
/// |-------------|---------------|---------------------------------------------------|
/// | shipment_id | `u64`         | Shipment this milestone belongs to                 |
/// | status      | `ShipmentStatus` | Current status (implied, not changed)           |
/// | data_hash   | `BytesN<32>`  | SHA-256 hash of the full off-chain milestone data  |
/// | timestamp   | `u64`         | Ledger timestamp of the milestone                  |
/// | actor       | `Address`     | Carrier address that recorded the milestone        |
/// | checkpoint  | `Symbol`      | Human-readable checkpoint name (e.g. "warehouse")  |
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
/// # Event Payload
///
/// | Field       | Type          | Description                                  |
/// |-------------|---------------|----------------------------------------------|
/// | shipment_id | `u64`         | Shipment the escrow is associated with        |
/// | status      | `ShipmentStatus` | Current shipment status                   |
/// | data_hash   | `BytesN<32>`  | Zero hash (not applicable)                    |
/// | timestamp   | `u64`         | Ledger timestamp of the deposit               |
/// | actor       | `Address`     | Address that deposited the funds              |
/// | amount      | `i128`        | Amount deposited (in stroops)                 |
#[allow(dead_code)]
pub fn emit_escrow_deposited(env: &Env, shipment_id: u64, from: &Address, amount: i128) {
    let zero_hash: BytesN<32> = BytesN::from_array(env, &[0u8; 32]);
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ESCROW_DEPOSITED),),
        (
            shipment_id,
            ShipmentStatus::Created,
            zero_hash,
            env.ledger().timestamp(),
            from.clone(),
            amount,
        ),
    );
}

/// Emits an `escrow_released` event when escrowed funds are paid out.
///
/// # Event Payload
///
/// | Field       | Type          | Description                                  |
/// |-------------|---------------|----------------------------------------------|
/// | shipment_id | `u64`         | Shipment the escrow was held for              |
/// | status      | `ShipmentStatus` | Current shipment status                   |
/// | data_hash   | `BytesN<32>`  | Zero hash (not applicable)                    |
/// | timestamp   | `u64`         | Ledger timestamp of the release               |
/// | actor       | `Address`     | Address receiving the released funds          |
/// | amount      | `i128`        | Amount released (in stroops)                  |
pub fn emit_escrow_released(env: &Env, shipment_id: u64, to: &Address, amount: i128) {
    let zero_hash: BytesN<32> = BytesN::from_array(env, &[0u8; 32]);
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ESCROW_RELEASED),),
        (
            shipment_id,
            ShipmentStatus::Delivered,
            zero_hash,
            env.ledger().timestamp(),
            to.clone(),
            amount,
        ),
    );
}

/// Emits an `escrow_refunded` event when escrowed funds are returned to the company.
///
/// # Event Payload
///
/// | Field       | Type          | Description                                  |
/// |-------------|---------------|----------------------------------------------|
/// | shipment_id | `u64`         | Shipment the escrow was held for              |
/// | status      | `ShipmentStatus` | Current shipment status                   |
/// | data_hash   | `BytesN<32>`  | Zero hash (not applicable)                    |
/// | timestamp   | `u64`         | Ledger timestamp of the refund                |
/// | actor       | `Address`     | Company address receiving the refund          |
/// | amount      | `i128`        | Amount refunded (in stroops)                  |
pub fn emit_escrow_refunded(env: &Env, shipment_id: u64, to: &Address, amount: i128) {
    let zero_hash: BytesN<32> = BytesN::from_array(env, &[0u8; 32]);
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ESCROW_REFUNDED),),
        (
            shipment_id,
            ShipmentStatus::Cancelled,
            zero_hash,
            env.ledger().timestamp(),
            to.clone(),
            amount,
        ),
    );
}

/// Emits a `milestone_payment_released` event when a partial escrow release occurs.
///
/// # Event Payload
///
/// | Field       | Type          | Description                                  |
/// |-------------|---------------|----------------------------------------------|
/// | shipment_id | `u64`         | Shipment the milestone belongs to             |
/// | status      | `ShipmentStatus` | Current shipment status                   |
/// | data_hash   | `BytesN<32>`  | Zero hash (not applicable)                    |
/// | timestamp   | `u64`         | Ledger timestamp of the release               |
/// | actor       | `Address`     | Carrier receiving the payment                 |
/// | milestone   | `Symbol`      | Checkpoint that triggered the release         |
/// | amount      | `i128`        | Amount released (in stroops)                  |
pub fn emit_milestone_payment_released(
    env: &Env,
    shipment_id: u64,
    milestone: &Symbol,
    amount: i128,
    to: &Address,
) {
    let zero_hash: BytesN<32> = BytesN::from_array(env, &[0u8; 32]);
    env.events().publish(
        (Symbol::new(
            env,
            crate::event_topics::MILESTONE_PAYMENT_RELEASED,
        ),),
        (
            shipment_id,
            ShipmentStatus::InTransit,
            zero_hash,
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
/// # Event Data
///
/// | Field       | Type         | Description                                      |
/// |-------------|--------------|--------------------------------------------------|
/// | shipment_id | `u64`        | Shipment under dispute                            |
/// | raised_by   | `Address`    | Address that initiated the dispute                |
/// | reason_hash | `BytesN<32>` | SHA-256 hash of the off-chain dispute evidence    |
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
/// # Event Payload
///
/// | Field       | Type          | Description                                   |
/// |-------------|---------------|-----------------------------------------------|
/// | shipment_id | `u64`         | Cancelled shipment identifier                  |
/// | status      | `ShipmentStatus` | Always `Cancelled`                          |
/// | data_hash   | `BytesN<32>`  | SHA-256 hash of the off-chain cancellation reason |
/// | timestamp   | `u64`         | Ledger timestamp of the cancellation           |
/// | actor       | `Address`     | Company or Admin that cancelled the shipment   |
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
/// # Event Data
///
/// | Field         | Type         | Description                    |
/// |---------------|--------------|--------------------------------|
/// | admin         | `Address`    | Admin that triggered the upgrade |
/// | new_wasm_hash | `BytesN<32>` | Hash of the new contract WASM   |
/// | version       | `u32`        | Contract version after upgrade  |
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
/// # Event Data
///
/// | Field            | Type              | Description                                |
/// |------------------|-------------------|--------------------------------------------|
/// | current_version  | `u32`             | Version before migration                    |
/// | target_version   | `u32`             | Version after migration                     |
/// | affected_entries | `u64`             | Count of entries involved in the migration  |
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
/// # Event Data
///
/// | Field        | Type         | Description                                    |
/// |--------------|--------------|------------------------------------------------|
/// | shipment_id  | `u64`        | Shipment being handed off                      |
/// | from_carrier | `Address`    | Current carrier handing off the shipment        |
/// | to_carrier   | `Address`    | New carrier receiving the shipment             |
/// | handoff_hash | `BytesN<32>` | SHA-256 hash of the off-chain handoff data     |
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
/// # Event Data
///
/// | Field        | Type         | Description                                          |
/// |--------------|--------------|------------------------------------------------------|
/// | shipment_id  | `u64`        | Shipment where the breach occurred                   |
/// | carrier      | `Address`    | Carrier that reported the breach                     |
/// | breach_type  | `BreachType` | Category of the condition breach                     |
/// | severity     | `Severity`   | Severity level for downstream analytics and alerting |
/// | data_hash    | `BytesN<32>` | SHA-256 hash of the off-chain sensor data payload    |
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
/// # Event Payload
///
/// | Field       | Type          | Description                                     |
/// |-------------|---------------|-------------------------------------------------|
/// | shipment_id | `u64`         | Cancelled shipment identifier                   |
/// | status      | `ShipmentStatus` | Always `Cancelled`                           |
/// | data_hash   | `BytesN<32>`  | Zero hash (not applicable)                      |
/// | timestamp   | `u64`         | Ledger timestamp of the expiry                  |
/// | actor       | `Address`     | Admin/system address that triggered auto-cancel |
pub fn emit_shipment_expired(env: &Env, shipment_id: u64, admin: &Address) {
    let zero_hash: BytesN<32> = BytesN::from_array(env, &[0u8; 32]);
    env.events().publish(
        (Symbol::new(env, crate::event_topics::SHIPMENT_EXPIRED),),
        (
            shipment_id,
            ShipmentStatus::Cancelled,
            zero_hash,
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
/// # Event Payload
///
/// | Field       | Type          | Description                                      |
/// |-------------|---------------|--------------------------------------------------|
/// | shipment_id | `u64`         | Shipment that was delivered                       |
/// | status      | `ShipmentStatus` | Always `Delivered`                            |
/// | data_hash   | `BytesN<32>`  | Zero hash (delivery data is off-chain)            |
/// | timestamp   | `u64`         | Ledger timestamp at the moment of delivery        |
/// | actor       | `Address`     | Carrier that completed the delivery               |
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
/// # Event Data
///
/// | Field       | Type         | Description                                    |
/// |-------------|--------------|------------------------------------------------|
/// | carrier     | `Address`    | Carrier that reported (and caused) the breach   |
/// | shipment_id | `u64`        | Shipment where the breach occurred              |
/// | breach_type | `BreachType` | Category of the condition breach                |
/// | severity    | `Severity`   | Severity level for analytics and alerting       |
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
/// # Event Data
///
/// | Field       | Type      | Description                                     |
/// |-------------|-----------|-------------------------------------------------|
/// | carrier     | `Address` | Carrier that lost the dispute                    |
/// | shipment_id | `u64`     | Shipment the dispute was raised on               |
pub fn emit_carrier_dispute_loss(env: &Env, carrier: &Address, shipment_id: u64) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CARRIER_DISPUTE_LOSS),),
        (carrier.clone(), shipment_id),
    );
}

/// Emits a `notification` event for backend indexing to trigger push notifications,
/// emails, or in-app alerts.
///
/// # Event Data
///
/// | Field             | Type               | Description                                    |
/// |-------------------|--------------------|------------------------------------------------|
/// | recipient         | `Address`          | Address to receive the notification             |
/// | notification_type | `NotificationType` | Type of notification event                      |
/// | shipment_id       | `u64`              | Related shipment ID                             |
/// | data_hash         | `BytesN<32>`       | SHA-256 hash of notification payload            |
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
/// # Event Data
///
/// | Field   | Type      | Description                                |
/// |---------|-----------|--------------------------------------------|
/// | admin   | `Address` | Admin that performed the revocation         |
/// | target  | `Address` | Address whose role was revoked              |
/// | role    | `Role`    | The role that was revoked                   |
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
/// # Event Data (Payload Schema)
///
/// | Field       | Type                | Description                                    |
/// |-------------|---------------------|------------------------------------------------|
/// | action      | `RoleChangeAction`  | The type of change (Assigned/Revoked/Suspended/Reactivated) |
/// | admin       | `Address`           | Admin who performed the action                 |
/// | target      | `Address`           | Address whose role was changed                 |
/// | role        | `Role`              | The role that was affected                     |
/// | timestamp   | `u64`               | Ledger timestamp of the change                 |
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
/// # Event Payload
///
/// | Field            | Type          | Description                                              |
/// |------------------|---------------|----------------------------------------------------------|
/// | shipment_id      | `u64`         | Forcibly cancelled shipment identifier                   |
/// | status           | `ShipmentStatus` | Always `Cancelled`                                    |
/// | data_hash        | `BytesN<32>`  | SHA-256 hash of the mandatory off-chain reason document  |
/// | timestamp        | `u64`         | Ledger timestamp of the force-cancel                     |
/// | actor            | `Address`     | Admin or multi-sig address that triggered the cancel     |
/// | escrow_refunded  | `i128`        | Amount refunded to the company (0 if no escrow held)     |
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
/// # Event Payload
///
/// | Field           | Type          | Description                                              |
/// |-----------------|---------------|----------------------------------------------------------|
/// | shipment_id     | `u64`         | Shipment for which escrow is being released               |
/// | status          | `ShipmentStatus` | Always `Delivered`                                     |
/// | data_hash       | `BytesN<32>`  | SHA-256 hash of the mandatory off-chain reason document  |
/// | timestamp       | `u64`         | Ledger timestamp of the force-release                     |
/// | actor           | `Address`     | Admin address authorizing the force release               |
/// | escrow_released | `i128`        | Amount released to the carrier                            |
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
/// # Event Payload
///
/// | Field           | Type          | Description                                              |
/// |-----------------|---------------|----------------------------------------------------------|
/// | shipment_id     | `u64`         | Shipment for which escrow is being refunded               |
/// | status          | `ShipmentStatus` | Always `Cancelled`                                     |
/// | data_hash       | `BytesN<32>`  | SHA-256 hash of the mandatory off-chain reason document  |
/// | timestamp       | `u64`         | Ledger timestamp of the force-refund                      |
/// | actor           | `Address`     | Admin address authorizing the force refund                |
/// | escrow_refunded | `i128`        | Amount refunded to the company                            |
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
/// # Event Data
///
/// | Field       | Type              | Description                                       |
/// |-------------|-------------------|---------------------------------------------------|
/// | shipment_id | `u64`             | Shipment that was disputed                         |
/// | resolution  | `DisputeResolution` | The final settlement choice (Carrier or Company)  |
/// | reason_hash | `BytesN<32>`      | SHA-256 hash of the off-chain settlement rationale |
/// | admin       | `Address`         | Admin address that resolved the dispute            |
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
/// # Event Data
///
/// | Field     | Type      | Description                                |
/// |-----------|-----------|-------------------------------------------|
/// | admin     | `Address` | Admin that paused the contract             |
/// | timestamp | `u64`     | Ledger timestamp when pause occurred       |
pub fn emit_contract_paused(env: &Env, admin: &Address) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CONTRACT_PAUSED),),
        (admin.clone(), env.ledger().timestamp()),
    );
}

/// Emits a `contract_unpaused` event when the contract is unpaused by an admin.
///
/// # Event Data
///
/// | Field     | Type      | Description                                |
/// |-----------|-----------|-------------------------------------------|
/// | admin     | `Address` | Admin that unpaused the contract           |
/// | timestamp | `u64`     | Ledger timestamp when unpause occurred     |
pub fn emit_contract_unpaused(env: &Env, admin: &Address) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CONTRACT_UNPAUSED),),
        (admin.clone(), env.ledger().timestamp()),
    );
}

/// Emits an `escrow_frozen` event when escrow is blocked due to a dispute or safety control.
///
/// # Event Data
///
/// | Field       | Type                | Description                                       |
/// |-------------|---------------------|---------------------------------------------------|
/// | shipment_id | `u64`               | Shipment whose escrow is now frozen                |
/// | reason      | `EscrowFreezeReason`| Structured code explaining why escrow was frozen  |
/// | caller      | `Address`           | Address that triggered the freeze (e.g. disputer) |
/// | timestamp   | `u64`               | Ledger timestamp of the freeze                    |
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
/// # Event Payload
///
/// | Field       | Type          | Description                                  |
/// |-------------|---------------|----------------------------------------------|
/// | shipment_id | `u64`         | Shipment the fee is associated with           |
/// | status      | `ShipmentStatus` | Current shipment status                   |
/// | data_hash   | `BytesN<32>`  | Zero hash (not applicable)                    |
/// | timestamp   | `u64`         | Ledger timestamp of the fee collection        |
/// | actor       | `Address`     | Treasury address receiving the fee            |
/// | amount      | `i128`        | Fee amount collected (in stroops)             |
pub fn emit_platform_fee_collected(env: &Env, shipment_id: u64, treasury: &Address, amount: i128) {
    let zero_hash: BytesN<32> = BytesN::from_array(env, &[0u8; 32]);
    env.events().publish(
        (Symbol::new(
            env,
            crate::event_topics::PLATFORM_FEE_COLLECTED,
        ),),
        (
            shipment_id,
            ShipmentStatus::Created,
            zero_hash,
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
/// # Event Payload
///
/// | Field       | Type          | Description                                  |
/// |-------------|---------------|----------------------------------------------|
/// | shipment_id | `u64`         | Shipment that was delivered                   |
/// | status      | `ShipmentStatus` | Always `Delivered`                        |
/// | data_hash   | `BytesN<32>`  | SHA-256 hash of the delivery confirmation     |
/// | timestamp   | `u64`         | Ledger timestamp of the confirmation          |
/// | actor       | `Address`     | Receiver address that confirmed delivery      |
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
/// # Event Payload
///
/// | Field       | Type          | Description                                  |
/// |-------------|---------------|----------------------------------------------|
/// | shipment_id | `u64`         | Shipment the geofence event belongs to        |
/// | zone_type   | `GeofenceEvent` | Type of geofence boundary crossed           |
/// | data_hash   | `BytesN<32>`  | SHA-256 hash of the off-chain location data   |
/// | timestamp   | `u64`         | Ledger timestamp of the geofence event        |
/// | actor       | `Address`     | Carrier that triggered the geofence event     |
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
/// # Event Payload
///
/// | Field       | Type          | Description                                  |
/// |-------------|---------------|----------------------------------------------|
/// | shipment_id | `u64`         | Shipment the ETA update belongs to            |
/// | status      | `ShipmentStatus` | Current shipment status                   |
/// | data_hash   | `BytesN<32>`  | SHA-256 hash of the off-chain ETA data        |
/// | timestamp   | `u64`         | Ledger timestamp of the ETA update            |
/// | actor       | `Address`     | Carrier that updated the ETA                  |
/// | new_eta     | `u64`         | New estimated time of arrival                 |
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
/// # Event Data
///
/// | Field         | Type         | Description                                   |
/// |---------------|--------------|-----------------------------------------------|
/// | shipment_id   | `u64`        | Shipment whose dispute the evidence belongs to |
/// | submitted_by  | `Address`    | Address that submitted the evidence            |
/// | index         | `u32`        | Zero-based index of the evidence entry         |
/// | evidence_hash | `BytesN<32>` | SHA-256 hash of the off-chain evidence document |
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
