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

use crate::types::{
    BreachType, EscrowFreezeReason, MigrationReport, Role, RoleChangeAction, Severity,
    ShipmentStatus,
};
use soroban_sdk::{Address, Bytes, BytesN, Env, Symbol};

pub const EVENT_SCHEMA_VERSION: u32 = 2;

fn next_event_counter(env: &Env, shipment_id: u64) -> u32 {
    crate::storage::get_event_count(env, shipment_id).saturating_add(1)
}

fn append_len_prefixed_bytes(env: &Env, payload: &mut Bytes, data: &[u8]) {
    payload.append(&Bytes::from_array(env, &(data.len() as u32).to_be_bytes()));
    payload.append(&Bytes::from_slice(env, data));
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
///
/// This structured encoding prevents ambiguous concatenation and guarantees
/// domain separation across event families.
pub fn generate_idempotency_key(
    env: &Env,
    domain: u8,
    shipment_id: u64,
    event_type: &str,
    event_counter: u32,
) -> BytesN<32> {
    let mut payload = Bytes::new(env);

    // Build a length-delimited preimage to avoid ambiguous concatenation.
    let domain_bytes = domain.to_be_bytes();
    append_len_prefixed_bytes(env, &mut payload, &domain_bytes);

    payload.append(&Bytes::from_array(env, &shipment_id.to_be_bytes()));

    let event_type_bytes = event_type.as_bytes();
    append_len_prefixed_bytes(env, &mut payload, event_type_bytes);

    payload.append(&Bytes::from_array(env, &event_counter.to_be_bytes()));
    env.crypto().sha256(&payload).into()
}

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
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        shipment_id,
        crate::event_topics::SHIPMENT_CREATED,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::SHIPMENT_CREATED),),
        (
            shipment_id,
            sender.clone(),
            receiver.clone(),
            data_hash.clone(),
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
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
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        shipment_id,
        crate::event_topics::STATUS_UPDATED,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::STATUS_UPDATED),),
        (
            shipment_id,
            old_status.clone(),
            new_status.clone(),
            data_hash.clone(),
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
}

/// Emits a `milestone_recorded` event when a carrier reports a checkpoint.
///
/// Milestones are **never stored on-chain** — this is the canonical example
/// of the Hash-and-Emit pattern. The full milestone payload (GPS coordinates,
/// temperature readings, photos) lives off-chain; only its hash is published.
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
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        shipment_id,
        crate::event_topics::MILESTONE_RECORDED,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::MILESTONE_RECORDED),),
        (
            shipment_id,
            checkpoint.clone(),
            data_hash.clone(),
            reporter.clone(),
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
    // Also track milestone-specific count for payload size guard
    crate::storage::increment_milestone_event_count(env, shipment_id);
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
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_ESCROW,
        shipment_id,
        crate::event_topics::ESCROW_DEPOSITED,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ESCROW_DEPOSITED),),
        (
            shipment_id,
            from.clone(),
            amount,
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
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
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_ESCROW,
        shipment_id,
        crate::event_topics::ESCROW_RELEASED,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ESCROW_RELEASED),),
        (
            shipment_id,
            to.clone(),
            amount,
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
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
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_ESCROW,
        shipment_id,
        crate::event_topics::ESCROW_REFUNDED,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::ESCROW_REFUNDED),),
        (
            shipment_id,
            to.clone(),
            amount,
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
}

/// Emits an `observer_assigned` event when an observer role is assigned to an address.
pub fn emit_observer_assigned(
    env: &Env,
    shipment_id: u64,
    observer: &Address,
    assigned_by: &Address,
) {
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_RBAC,
        shipment_id,
        crate::event_topics::OBSERVER_ASSIGNED,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::OBSERVER_ASSIGNED),),
        (
            shipment_id,
            observer.clone(),
            assigned_by.clone(),
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
}

/// Emits an `observer_revoked` event when an observer role is revoked from an address.
pub fn emit_observer_revoked(
    env: &Env,
    shipment_id: u64,
    observer: &Address,
    revoked_by: &Address,
) {
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_RBAC,
        shipment_id,
        crate::event_topics::OBSERVER_REVOKED,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::OBSERVER_REVOKED),),
        (
            shipment_id,
            observer.clone(),
            revoked_by.clone(),
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
}

/// Emits an `escrow_partially_refunded` event when a partial refund is executed.
pub fn emit_escrow_partially_refunded(
    env: &Env,
    shipment_id: u64,
    sender_refund: i128,
    carrier_compensation: i128,
    refund_percentage: u32,
) {
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_ESCROW,
        shipment_id,
        crate::event_topics::ESCROW_PARTIALLY_REFUNDED,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(
            env,
            crate::event_topics::ESCROW_PARTIALLY_REFUNDED,
        ),),
        (
            shipment_id,
            sender_refund,
            carrier_compensation,
            refund_percentage,
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
}

/// Emits a `milestone_payment_released` event when a partial escrow release occurs.
pub fn emit_milestone_payment_released(
    env: &Env,
    shipment_id: u64,
    milestone: &Symbol,
    amount: i128,
    to: &Address,
) {
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_ESCROW,
        shipment_id,
        crate::event_topics::MILESTONE_PAYMENT_RELEASED,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(
            env,
            crate::event_topics::MILESTONE_PAYMENT_RELEASED,
        ),),
        (
            shipment_id,
            milestone.clone(),
            amount,
            to.clone(),
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
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
        (Symbol::new(env, crate::event_topics::DISPUTE_RAISED),),
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
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        shipment_id,
        crate::event_topics::SHIPMENT_CANCELLED,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::SHIPMENT_CANCELLED),),
        (
            shipment_id,
            caller.clone(),
            reason_hash.clone(),
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
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
///
/// # Arguments
/// * `env` - Execution environment.
/// * `report` - Structured migration metrics.
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
///
/// # Listeners
///
/// - **Express backend**: Records the breach event and triggers alerts.
/// - **Frontend**: Flags the shipment with a condition-breach warning badge.
/// - **Indexer**: Filters and aggregates breaches by severity for analytics.
///
/// # Arguments
/// * `env` - Invoker mapping of standard SDK elements mappings
/// * `shipment_id` - Primary index resolving context arrays mappings reference.
/// * `carrier` - Invoking controller array mappings identifiers scope handlers.
/// * `breach_type` - Type tracking parameter reference format mapping instances.
/// * `severity` - Severity level for filtering and prioritization.
/// * `data_hash` - External proof pointer array.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_condition_breach(&env, 1, &carrier_addr, &BreachType::TemperatureHigh, &Severity::High, &hash);
/// ```
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
/// # Event Data
///
/// | Field       | Type   | Description                                     |
/// |-------------|--------|-------------------------------------------------|
/// | shipment_id | `u64`  | Cancelled shipment identifier                   |
pub fn emit_shipment_expired(env: &Env, shipment_id: u64) {
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        shipment_id,
        crate::event_topics::SHIPMENT_EXPIRED,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::SHIPMENT_EXPIRED),),
        (
            shipment_id,
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
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
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        shipment_id,
        crate::event_topics::DELIVERY_SUCCESS,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::DELIVERY_SUCCESS),),
        (
            carrier.clone(),
            shipment_id,
            delivery_time,
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
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
/// | severity    | `Severity`   | Severity level for analytics and alerting       |
///
/// # Listeners
/// - **Express backend**: Increments breach counter for the carrier's reputation record.
/// - **Indexer**: Filters and aggregates breaches by severity for analytics.
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
///
/// # Listeners
/// - **Express backend**: Increments dispute-loss counter in carrier reputation index.
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
        (Symbol::new(env, crate::event_topics::NOTIFICATION),),
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
        (Symbol::new(env, crate::event_topics::SHIPMENT_ARCHIVED),),
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
///
/// # Arguments
/// * `env` - The execution environment.
/// * `admin` - The admin who revoked the role.
/// * `target` - The address whose role was revoked.
/// * `role` - The role that was revoked.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_role_revoked(&env, &admin, &target, &Role::Company);
/// ```
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
///
/// # Listeners
///
/// - **Express backend**: Maintains a role-history index for each address.
/// - **Compliance**: Audits all RBAC changes for regulatory requirements.
/// - **Frontend**: Displays role change timeline in admin dashboard.
/// - **Analytics**: Tracks role distribution and changes over time.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `action` - The type of role change action.
/// * `admin` - The admin who performed the action.
/// * `target` - The address whose role was changed.
/// * `role` - The role that was affected.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_role_changed(&env, &RoleChangeAction::Assigned, &admin, &target, &Role::Company);
/// ```
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
/// This is a dedicated, immutable audit trail for emergency admin-only cancellations.
/// It is intentionally separate from `shipment_cancelled` so that off-chain indexers
/// can distinguish routine cancellations from privileged force-cancels.
///
/// # Event Data
///
/// | Field       | Type         | Description                                              |
/// |-------------|--------------|----------------------------------------------------------|
/// | shipment_id | `u64`        | Forcibly cancelled shipment identifier                   |
/// | admin       | `Address`    | Admin or multi-sig address that triggered the cancel     |
/// | reason_hash | `BytesN<32>` | SHA-256 hash of the mandatory off-chain reason document  |
/// | escrow_refunded | `i128`   | Amount refunded to the company (0 if no escrow held)     |
///
/// # Listeners
/// - **Express backend**: Creates a force-cancel audit record and triggers compliance alerts.
/// - **Frontend**: Flags the shipment with an admin-override badge.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `shipment_id` - ID of the force-cancelled shipment.
/// * `admin` - Admin address that executed the force-cancel.
/// * `reason_hash` - Mandatory SHA-256 hash of the off-chain reason document.
/// * `escrow_refunded` - Amount refunded to the company.
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
            admin.clone(),
            reason_hash.clone(),
            escrow_refunded,
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
}

/// Emits a `note_appended` event when a new hash-only note is added to a shipment.
///
/// This follows the Hash-and-Emit pattern for shipment commentary. The actual
/// text of the note is stored off-chain (e.g., in IPFS or a private database),
/// while the SHA-256 hash is recorded on-chain for tamper-proof auditability.
///
/// # Event Data
///
/// | Field       | Type         | Description                                       |
/// |-------------|--------------|---------------------------------------------------|
/// | shipment_id | `u64`        | Shipment this note belongs to                      |
/// | note_index  | `u32`        | Sequence number of the note for this shipment      |
/// | note_hash   | `BytesN<32>` | SHA-256 hash of the off-chain note text            |
/// | reporter    | `Address`    | Address that appended the note                     |
///
/// # Arguments
/// * `env` - Execution environment.
/// * `shipment_id` - ID of the shipment.
/// * `note_index` - Cumulative count/index of the note for this shipment.
/// * `note_hash` - The hash of the off-chain commentary.
/// * `reporter` - The address that provided the note.
pub fn emit_note_appended(
    env: &Env,
    shipment_id: u64,
    note_index: u32,
    note_hash: &BytesN<32>,
    reporter: &Address,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::NOTE_APPENDED),),
        (shipment_id, note_index, note_hash.clone(), reporter.clone()),
    );
    crate::storage::increment_event_count(env, shipment_id);
}

/// Emits an `evidence_added` event when a new hash-only evidence is added to a shipment dispute.
///
/// # Event Data
///
/// | Field       | Type         | Description                                       |
/// |-------------|--------------|---------------------------------------------------|
/// | shipment_id | `u64`        | Shipment under dispute                             |
/// | evidence_index | `u32`      | Sequence number of the evidence for this shipment  |
/// | evidence_hash | `BytesN<32>`| SHA-256 hash of the off-chain evidence             |
/// | reporter    | `Address`    | Address that added the evidence                    |
pub fn emit_evidence_added(
    env: &Env,
    shipment_id: u64,
    evidence_index: u32,
    evidence_hash: &BytesN<32>,
    reporter: &Address,
) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::EVIDENCE_ADDED),),
        (
            shipment_id,
            evidence_index,
            evidence_hash.clone(),
            reporter.clone(),
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
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
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_DISPUTE,
        shipment_id,
        crate::event_topics::DISPUTE_RESOLVED,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(env, crate::event_topics::DISPUTE_RESOLVED),),
        (
            shipment_id,
            resolution.clone(),
            reason_hash.clone(),
            admin.clone(),
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
}

/// Emits a `contract_paused` event when the contract is paused by an admin.
///
/// # Event Data
///
/// | Field     | Type      | Description                                |
/// |-----------|-----------|-------------------------------------------|
/// | admin     | `Address` | Admin that paused the contract             |
/// | timestamp | `u64`     | Ledger timestamp when pause occurred       |
///
/// # Listeners
/// - **Express backend**: Updates contract status and alerts operators.
/// - **Frontend**: Displays maintenance mode banner.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `admin` - Admin address that paused the contract.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_contract_paused(&env, &admin);
/// ```
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
///
/// # Listeners
/// - **Express backend**: Updates contract status and resumes operations.
/// - **Frontend**: Removes maintenance mode banner.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `admin` - Admin address that unpaused the contract.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_contract_unpaused(&env, &admin);
/// ```
pub fn emit_contract_unpaused(env: &Env, admin: &Address) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::CONTRACT_UNPAUSED),),
        (admin.clone(), env.ledger().timestamp()),
    );
}

/// Emits a `recovery_event` when a shipment is recovered from a stuck state.
///
/// # Event Data
///
/// | Field       | Type             | Description                                    |
/// |-------------|------------------|------------------------------------------------|
/// | shipment_id | `u64`            | Shipment being recovered                        |
/// | admin       | `Address`        | Admin performing the recovery                   |
/// | old_status  | `ShipmentStatus` | Previous status before recovery                 |
/// | new_status  | `ShipmentStatus` | New status after recovery                       |
/// | reason_hash | `BytesN<32>`     | SHA-256 hash of recovery reason                 |
///
/// # Arguments
/// * `env` - Execution environment.
/// * `shipment_id` - ID of the recovered shipment.
/// * `admin` - Admin address performing recovery.
/// * `old_status` - Previous shipment status.
/// * `new_status` - New shipment status.
/// * `reason_hash` - Hash of recovery reason.
///
/// # Returns
/// No value returned.
pub fn emit_recovery_event(
    env: &Env,
    shipment_id: u64,
    admin: &Address,
    old_status: &ShipmentStatus,
    new_status: &ShipmentStatus,
    reason_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, "recovery_event"),),
        (
            shipment_id,
            admin.clone(),
            old_status.clone(),
            new_status.clone(),
            reason_hash.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emits an `escrow_unlock_event` when escrow is unlocked during recovery.
///
/// # Event Data
///
/// | Field       | Type         | Description                                    |
/// |-------------|--------------|------------------------------------------------|
/// | shipment_id | `u64`        | Shipment with unlocked escrow                   |
/// | admin       | `Address`    | Admin performing the unlock                     |
/// | old_amount  | `i128`       | Previous escrow amount                          |
/// | reason_hash | `BytesN<32>` | SHA-256 hash of unlock reason                   |
///
/// # Arguments
/// * `env` - Execution environment.
/// * `shipment_id` - ID of the shipment.
/// * `admin` - Admin address performing unlock.
/// * `old_amount` - Previous escrow amount.
/// * `reason_hash` - Hash of unlock reason.
///
/// # Returns
/// No value returned.
pub fn emit_escrow_unlock_event(
    env: &Env,
    shipment_id: u64,
    admin: &Address,
    old_amount: i128,
    reason_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, "escrow_unlock_event"),),
        (
            shipment_id,
            admin.clone(),
            old_amount,
            reason_hash.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emits a `finalization_clear_event` when finalization flag is cleared.
///
/// # Event Data
///
/// | Field       | Type         | Description                                    |
/// |-------------|--------------|------------------------------------------------|
/// | shipment_id | `u64`        | Shipment with cleared finalization              |
/// | admin       | `Address`    | Admin performing the clear                      |
/// | reason_hash | `BytesN<32>` | SHA-256 hash of clear reason                    |
///
/// # Arguments
/// * `env` - Execution environment.
/// * `shipment_id` - ID of the shipment.
/// * `admin` - Admin address performing clear.
/// * `reason_hash` - Hash of clear reason.
///
/// # Returns
/// No value returned.
pub fn emit_finalization_clear_event(
    env: &Env,
    shipment_id: u64,
    admin: &Address,
    reason_hash: &BytesN<32>,
) {
    env.events().publish(
        (Symbol::new(env, "finalization_clear_event"),),
        (
            shipment_id,
            admin.clone(),
            reason_hash.clone(),
            env.ledger().timestamp(),
        ),
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
///
/// # Arguments
/// * `env`         - Execution environment.
/// * `shipment_id` - ID of the shipment with frozen escrow.
/// * `reason`      - `EscrowFreezeReason` variant classifying the freeze.
/// * `caller`      - The address that triggered the freeze action.
///
/// # Returns
/// No value returned.
///
/// # Examples
/// ```rust
/// // events::emit_escrow_frozen(&env, shipment_id, EscrowFreezeReason::DisputeRaised, &caller);
/// ```
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

/// Emits a `shipment_blocked` event when a shipment cannot transition to InTransit or Delivered
/// due to unmet dependencies.
///
/// # Event Data
///
/// | Field                   | Type        | Description                                |
/// |-------------------------|-------------|--------------------------------------------|
/// | shipment_id             | `u64`       | Shipment that cannot proceed                |
/// | caller                  | `Address`   | Address that attempted the transition      |
/// | unmet_dependencies      | `Vec<u64>`  | Prerequisite shipment IDs not yet complete |
pub fn emit_shipment_blocked(
    env: &Env,
    shipment_id: u64,
    caller: &Address,
    unmet_dependencies: soroban_sdk::Vec<u64>,
) {
    env.events().publish(
        (crate::event_topics::SHIPMENT_BLOCKED,),
        (shipment_id, caller.clone(), unmet_dependencies),
          );
}

/// Emits a `platform_fee_collected` event when a fee is deducted from a deposit.
pub fn emit_platform_fee_collected(env: &Env, shipment_id: u64, treasury: &Address, amount: i128) {
    let event_counter = next_event_counter(env, shipment_id);
    let idempotency_key = generate_idempotency_key(
        env,
        crate::event_topics::HASH_DOMAIN_PLATFORM,
        shipment_id,
        crate::event_topics::PLATFORM_FEE_COLLECTED,
        event_counter,
    );
    env.events().publish(
        (Symbol::new(
            env,
            crate::event_topics::PLATFORM_FEE_COLLECTED,
        ),),
        (
            shipment_id,
            treasury.clone(),
            amount,
            EVENT_SCHEMA_VERSION,
            event_counter,
            idempotency_key,
        ),
    );
    crate::storage::increment_event_count(env, shipment_id);
}

/// Emits a `fee_config_updated` event when the platform fee configuration changes.
pub fn emit_fee_config_updated(env: &Env, admin: &Address, fee_bps: u32, treasury: &Address) {
    env.events().publish(
        (Symbol::new(env, crate::event_topics::FEE_CONFIG_UPDATED),),
        (
            admin.clone(),
            fee_bps,
            treasury.clone(),
            EVENT_SCHEMA_VERSION,
        ),
    );
}
