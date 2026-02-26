//! # Events Module — Hash-and-Emit Pattern
//!
//! The heart of Navin's off-chain data architecture. Instead of storing heavy
//! payloads (GPS traces, sensor readings, metadata) on-chain, the contract
//! emits structured events containing only the `shipment_id`, relevant
//! identifiers, and a `data_hash` (SHA-256 of the full off-chain payload).
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

use crate::types::{BreachType, ShipmentStatus};
use soroban_sdk::{Address, BytesN, Env, Symbol};

/// Emits a `shipment_created` event when a new shipment is registered.
///
/// # Event Data
///
/// | Field        | Type        | Description                                     |
/// |--------------|-------------|-------------------------------------------------|
/// | shipment_id  | `u64`       | Unique on-chain shipment identifier              |
/// | sender       | `Address`   | Company that created the shipment                |
/// | receiver     | `Address`   | Intended recipient of the goods                  |
/// | data_hash    | `BytesN<32>`| SHA-256 hash of the full off-chain shipment data |
///
/// # Listeners
///
/// - **Express backend**: Creates the initial shipment record in the DB.
/// - **Frontend**: Displays real-time shipment creation notifications.
///
/// # Arguments
/// * `env` - Extracted execution environment.
/// * `shipment_id` - ID of the created shipment.
/// * `sender` - Originating company.
/// * `receiver` - Target destination address.
/// * `data_hash` - The off-chain data hash tracking.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_shipment_created(&env, id, &sender, &receiver, &hash);
/// ```
pub fn emit_shipment_created(
    env: &Env,
    shipment_id: u64,
    sender: &Address,
    receiver: &Address,
    data_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, "shipment_created"),),
        (
            shipment_id,
            sender.clone(),
            receiver.clone(),
            data_hash.clone(),
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
}

/// Emits a `status_updated` event when a shipment transitions between lifecycle states.
///
/// # Event Data
///
/// | Field       | Type             | Description                                        |
/// |-------------|------------------|----------------------------------------------------|
/// | shipment_id | `u64`            | Shipment whose status changed                      |
/// | old_status  | `ShipmentStatus` | Previous lifecycle state                            |
/// | new_status  | `ShipmentStatus` | New lifecycle state after transition                |
/// | data_hash   | `BytesN<32>`     | SHA-256 hash of the updated off-chain payload       |
///
/// # Listeners
///
/// - **Express backend**: Updates shipment status in the DB and triggers webhooks.
/// - **Frontend**: Refreshes the shipment timeline in the tracking UI.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `shipment_id` - Assigned ID of the shipment.
/// * `old_status` - Replaced status.
/// * `new_status` - Promoted status.
/// * `data_hash` - Latest hash of off-chain records tracking.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_status_updated(&env, id, &ShipmentStatus::Created, &ShipmentStatus::InTransit, &hash);
/// ```
pub fn emit_status_updated(
    env: &Env,
    shipment_id: u64,
    old_status: &ShipmentStatus,
    new_status: &ShipmentStatus,
    data_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, "status_updated"),),
        (
            shipment_id,
            old_status.clone(),
            new_status.clone(),
            data_hash.clone(),
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
}

/// Emits a `milestone_recorded` event when a carrier reports a checkpoint.
///
/// Milestones are **never stored on-chain** — this is the canonical example
/// of the Hash-and-Emit pattern. The full milestone payload (GPS coordinates,
/// temperature readings, photos) lives off-chain; only its hash is emitted.
///
/// # Event Data
///
/// | Field       | Type         | Description                                       |
/// |-------------|--------------|---------------------------------------------------|
/// | shipment_id | `u64`        | Shipment this milestone belongs to                 |
/// | checkpoint  | `Symbol`     | Human-readable checkpoint name (e.g. "warehouse") |
/// | data_hash   | `BytesN<32>` | SHA-256 hash of the full off-chain milestone data  |
/// | reporter    | `Address`    | Carrier address that recorded the milestone        |
///
/// # Listeners
///
/// - **Express backend**: Stores the full milestone record and verifies the hash.
/// - **Frontend**: Adds a new point on the shipment tracking map.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - ID of the shipment.
/// * `checkpoint` - The target checkpoint recorded.
/// * `data_hash` - Encoded offchain metadata representation hashes.
/// * `reporter` - The active address recording milestone.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_milestone_recorded(&env, 1, &Symbol::new(&env, "warehouse"), &hash, &carrier);
/// ```
pub fn emit_milestone_recorded(
    env: &Env,
    shipment_id: u64,
    checkpoint: &Symbol,
    data_hash: &BytesN<32>,
    reporter: &Address,
) {
    env.events().publish(
        (Symbol::new(env, "milestone_recorded"),),
        (
            shipment_id,
            checkpoint.clone(),
            data_hash.clone(),
            reporter.clone(),
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
}

/// Emits an `escrow_deposited` event when funds are locked for a shipment.
///
/// # Event Data
///
/// | Field       | Type      | Description                                  |
/// |-------------|-----------|----------------------------------------------|
/// | shipment_id | `u64`     | Shipment the escrow is associated with        |
/// | from        | `Address` | Address that deposited the funds              |
/// | amount      | `i128`    | Amount deposited (in stroops)                 |
///
/// # Listeners
///
/// - **Express backend**: Updates the escrow ledger and notifies the carrier.
/// - **Frontend**: Shows the escrow status on the shipment detail page.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - Target shipment.
/// * `from` - Depositor address.
/// * `amount` - Escrow funds.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_escrow_deposited(&env, 1, &company_addr, 1000);
/// ```
#[allow(dead_code)]
pub fn emit_escrow_deposited(env: &Env, shipment_id: u64, from: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "escrow_deposited"),),
        (shipment_id, from.clone(), amount),
    );
}

/// Emits an `escrow_released` event when escrowed funds are paid out.
///
/// # Event Data
///
/// | Field       | Type      | Description                                  |
/// |-------------|-----------|----------------------------------------------|
/// | shipment_id | `u64`     | Shipment the escrow was held for              |
/// | to          | `Address` | Address receiving the released funds          |
/// | amount      | `i128`    | Amount released (in stroops)                  |
///
/// # Listeners
///
/// - **Express backend**: Finalizes the payment record and triggers settlement.
/// - **Frontend**: Confirms payment completion to both parties.
///
/// # Arguments
/// * `env` - Extracted execution environment
/// * `shipment_id` - Corresponding shipment target identifier
/// * `to` - Receivers payment delivery destination
/// * `amount` - Transfer quantifiers emitted.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_escrow_released(&env, 1, &carrier_addr, 1000);
/// ```
pub fn emit_escrow_released(env: &Env, shipment_id: u64, to: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "escrow_released"),),
        (shipment_id, to.clone(), amount),
    );
}

/// Emits an `escrow_refunded` event when escrowed funds are returned to the company.
///
/// # Event Data
///
/// | Field       | Type      | Description                                  |
/// |-------------|-----------|----------------------------------------------|
/// | shipment_id | `u64`     | Shipment the escrow was held for              |
/// | to          | `Address` | Company address receiving the refund          |
/// | amount      | `i128`    | Amount refunded (in stroops)                  |
///
/// # Listeners
///
/// - **Express backend**: Updates the escrow ledger and notifies the company.
/// - **Frontend**: Shows the refund status on the shipment detail page.
///
/// # Arguments
/// * `env` - Execution environment references
/// * `shipment_id` - Bound identifier
/// * `to` - Bound targets receiving refunds.
/// * `amount` - Total refund magnitude.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_escrow_refunded(&env, 1, &company_addr, 1000);
/// ```
pub fn emit_escrow_refunded(env: &Env, shipment_id: u64, to: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "escrow_refunded"),),
        (shipment_id, to.clone(), amount),
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
///
/// # Listeners
///
/// - **Express backend**: Creates a dispute case and alerts the admin.
/// - **Frontend**: Opens the dispute resolution workflow for both parties.
///
/// # Arguments
/// * `env` - Operating environment mappings
/// * `shipment_id` - Identifier tracking dispute
/// * `raised_by` - Object instance generating dispute action
/// * `reason_hash` - Formatted storage mapping to offchain dispute proof
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_dispute_raised(&env, 1, &caller, &hash);
/// ```
pub fn emit_dispute_raised(
    env: &Env,
    shipment_id: u64,
    raised_by: &Address,
    reason_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, "dispute_raised"),),
        (shipment_id, raised_by.clone(), reason_hash.clone()),
    );
}

/// Emits a `shipment_cancelled` event when a shipment is cancelled.
///
/// # Event Data
///
/// | Field       | Type         | Description                                   |
/// |-------------|--------------|-----------------------------------------------|
/// | shipment_id | `u64`        | Cancelled shipment identifier                  |
/// | caller      | `Address`    | Company or Admin that cancelled the shipment   |
/// | reason_hash | `BytesN<32>` | SHA-256 hash of the off-chain cancellation reason |
///
/// # Arguments
/// * `env` - Binding caller environment context map
/// * `shipment_id` - ID specifying cancelled shipment instance
/// * `caller` - Requestor generating cancellations
/// * `reason_hash` - The mapped hash associated to the cancellation context.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_shipment_cancelled(&env, 1, &caller, &hash);
/// ```
pub fn emit_shipment_cancelled(
    env: &Env,
    shipment_id: u64,
    caller: &Address,
    reason_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, "shipment_cancelled"),),
        (shipment_id, caller.clone(), reason_hash.clone()),
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
///
/// # Arguments
/// * `env` - Env runtime context tracker
/// * `admin` - Contract mapping triggering the event notification
/// * `new_wasm_hash` - Reference byte arrays mapping the deployed WASM context
/// * `version` - Deployment identifier index context
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_contract_upgraded(&env, &admin, &hash, 2);
/// ```
pub fn emit_contract_upgraded(
    env: &Env,
    admin: &Address,
    new_wasm_hash: &BytesN<32>,
    version: u32,
) {
    env.events().publish(
        (Symbol::new(env, "contract_upgraded"),),
        (admin.clone(), new_wasm_hash.clone(), version),
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
///
/// # Listeners
///
/// - **Express backend**: Updates carrier assignment and triggers notifications.
/// - **Frontend**: Shows carrier change in shipment tracking UI.
///
/// # Arguments
/// * `env` - Invoker environment handler instance
/// * `shipment_id` - Target referencing the handoff sequence
/// * `from_carrier` - Initial handler returning mapping to shipment ID sequence
/// * `to_carrier` - Target updated recipient acting as carrier
/// * `handoff_hash` - Validation signature array mapping references.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_carrier_handoff(&env, 1, &curr_carr, &new_carr, &hash);
/// ```
pub fn emit_carrier_handoff(
    env: &Env,
    shipment_id: u64,
    from_carrier: &Address,
    to_carrier: &Address,
    handoff_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, "carrier_handoff"),),
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
/// | data_hash    | `BytesN<32>` | SHA-256 hash of the off-chain sensor data payload    |
///
/// # Listeners
///
/// - **Express backend**: Records the breach event and triggers alerts.
/// - **Frontend**: Flags the shipment with a condition-breach warning badge.
///
/// # Arguments
/// * `env` - Invoker mapping of standard SDK elements mappings
/// * `shipment_id` - Primary index resolving context arrays mappings reference.
/// * `carrier` - Invoking controller array mappings identifiers scope handlers.
/// * `breach_type` - Type tracking parameter reference format mapping instances.
/// * `data_hash` - External proof pointer array.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_condition_breach(&env, 1, &carrier_addr, &BreachType::TemperatureHigh, &hash);
/// ```
pub fn emit_condition_breach(
    env: &Env,
    shipment_id: u64,
    carrier: &Address,
    breach_type: &BreachType,
    data_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, "condition_breach"),),
        (
            shipment_id,
            carrier.clone(),
            breach_type.clone(),
            data_hash.clone(),
        ),
    );
}

/// Emits an `admin_proposed` event when a new administrator is proposed.
pub fn emit_admin_proposed(env: &Env, current_admin: &Address, proposed_admin: &Address) {
    env.events().publish(
        (Symbol::new(env, "admin_proposed"),),
        (current_admin.clone(), proposed_admin.clone()),
    );
}

/// Emits an `admin_transferred` event when the administrator role is successfully transferred.
pub fn emit_admin_transferred(env: &Env, old_admin: &Address, new_admin: &Address) {
    env.events().publish(
        (Symbol::new(env, "admin_transferred"),),
        (old_admin.clone(), new_admin.clone()),
    );
}

/// Emits a `shipment_expired` event when a shipment misses its deadline and is auto-cancelled.
///
/// # Event Data
///
/// | Field       | Type   | Description                                     |
/// |-------------|--------|-------------------------------------------------|
/// | shipment_id | `u64`  | Cancelled shipment identifier                   |
pub fn emit_shipment_expired(env: &Env, shipment_id: u64) {
    env.events()
        .publish((Symbol::new(env, "shipment_expired"),), (shipment_id,));
}

/// Emits a `contract_paused` event when the contract is paused by an admin.
///
/// # Event Data
///
/// | Field   | Type      | Description               |
/// |---------|-----------|---------------------------|
/// | `admin` | `Address` | Admin who paused it       |
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_contract_paused(&env, &admin);
/// ```
pub fn emit_contract_paused(env: &Env, admin: &Address) {
    let payload = admin.clone();
    env.events()
        .publish((Symbol::new(env, "contract_paused"),), payload);
}

/// Emits a `contract_unpaused` event when the contract is unpaused by an admin.
///
/// # Event Data
///
/// | Field   | Type      | Description               |
/// |---------|-----------|---------------------------|
/// | `admin` | `Address` | Admin who unpaused it     |
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_contract_unpaused(&env, &admin);
/// ```
pub fn emit_contract_unpaused(env: &Env, admin: &Address) {
    let payload = admin.clone();
    env.events()
        .publish((Symbol::new(env, "contract_unpaused"),), payload);
}

// ─── Paste these three functions at the BOTTOM of src/events.rs ──────────────

/// Emits a `delivery_success` event when a shipment is successfully delivered.
///
/// The backend indexes this event to increment the carrier's on-time delivery
/// count and compute punctuality metrics relative to the shipment deadline.
///
/// # Event Data
///
/// | Field         | Type      | Description                                      |
/// |---------------|-----------|--------------------------------------------------|
/// | carrier       | `Address` | Carrier that completed the delivery               |
/// | shipment_id   | `u64`     | Shipment that was delivered                       |
/// | delivery_time | `u64`     | Ledger timestamp at the moment of delivery        |
///
/// # Listeners
/// - **Express backend**: Increments on-time delivery counter in carrier reputation index.
pub fn emit_delivery_success(env: &Env, carrier: &Address, shipment_id: u64, delivery_time: u64) {
    env.events().publish(
        (Symbol::new(env, "delivery_success"),),
        (carrier.clone(), shipment_id, delivery_time),
    );
    crate::storage::increment_event_count(env, shipment_id);
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
///
/// # Listeners
/// - **Express backend**: Increments breach counter for the carrier's reputation record.
pub fn emit_carrier_breach(
    env: &Env,
    carrier: &Address,
    shipment_id: u64,
    breach_type: &BreachType,
) {
    env.events().publish(
        (Symbol::new(env, "carrier_breach"),),
        (carrier.clone(), shipment_id, breach_type.clone()),
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
///
/// # Listeners
/// - **Express backend**: Increments dispute-loss counter in carrier reputation index.
pub fn emit_carrier_dispute_loss(env: &Env, carrier: &Address, shipment_id: u64) {
    env.events().publish(
        (Symbol::new(env, "carrier_dispute_loss"),),
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
///
/// # Listeners
/// - **Express backend**: Triggers push notifications, emails, or in-app alerts.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `recipient` - Address to receive the notification.
/// * `notification_type` - Type of notification.
/// * `shipment_id` - Related shipment ID.
/// * `data_hash` - Hash of notification data.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_notification(&env, &receiver, NotificationType::ShipmentCreated, 1, &hash);
/// ```
pub fn emit_notification(
    env: &Env,
    recipient: &Address,
    notification_type: crate::types::NotificationType,
    shipment_id: u64,
    data_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, "notification"),),
        (
            recipient.clone(),
            notification_type,
            shipment_id,
            data_hash.clone(),
        ),
    );
}

/// Emits a `shipment_archived` event when a shipment is moved to temporary storage.
///
/// # Event Data
///
/// | Field       | Type   | Description                                     |
/// |-------------|--------|-------------------------------------------------|
/// | shipment_id | `u64`  | ID of the archived shipment                     |
/// | timestamp   | `u64`  | Ledger timestamp when archival occurred         |
///
/// # Listeners
/// - **Express backend**: Updates shipment status to archived in the database.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `shipment_id` - ID of the archived shipment.
/// * `timestamp` - Timestamp of archival.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_shipment_archived(&env, 1, 1234567890);
/// ```
pub fn emit_shipment_archived(env: &Env, shipment_id: u64, timestamp: u64) {
    env.events().publish(
        (Symbol::new(env, "shipment_archived"),),
        (shipment_id, timestamp),
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
        (Symbol::new(env, "carrier_late_delivery"),),
        (carrier.clone(), shipment_id, deadline, actual_delivery_time),
    );
}

/// Emits a `carrier_on_time_delivery` event when a carrier completes delivery on or before the deadline.
pub fn emit_carrier_on_time_delivery(env: &Env, carrier: &Address, shipment_id: u64) {
    env.events().publish(
        (Symbol::new(env, "carrier_on_time_delivery"),),
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
        (Symbol::new(env, "carrier_handoff_completed"),),
        (from_carrier.clone(), to_carrier.clone(), shipment_id),
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
        (Symbol::new(env, "carrier_milestone_rate"),),
        (
            carrier.clone(),
            shipment_id,
            milestones_hit,
            total_milestones,
        ),
    );
}
