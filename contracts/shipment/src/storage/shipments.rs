//! Persistent-storage helpers for shipment records, escrow, archival, delivery
//! proofs, status-hash logs, TTL management and shipment dependencies.

use crate::{
    errors::NavinError,
    types::{
        DataKey, DeliveryProofKey, EscrowFreezeReason, Shipment, ShipmentStatus, StatusHashKey,
        StatusHashLog, StatusHashRecord, MAX_PARTIAL_DELIVERY_PROOFS,
        MAX_STATUS_HASHES_PER_SHIPMENT,
    },
};
use soroban_sdk::{Address, BytesN, Env, Map, Symbol, Vec};

// ── Storage key helpers ───────────────────────────────────────────────────────

/// Construct a storage key for a shipment's data.
#[inline]
pub fn shipment_key(shipment_id: u64) -> DataKey {
    DataKey::Shipment(shipment_id)
}

/// Construct a storage key for a shipment's escrow amount.
#[inline]
pub fn escrow_key(shipment_id: u64) -> DataKey {
    DataKey::Escrow(shipment_id)
}

/// Construct a storage key for a shipment's confirmation hash.
#[inline]
pub fn confirmation_hash_key(shipment_id: u64) -> DataKey {
    DataKey::ConfirmationHash(shipment_id)
}

/// Construct a storage key for a shipment's escrow freeze reason.
#[inline]
pub fn escrow_freeze_reason_key(shipment_id: u64) -> DataKey {
    DataKey::EscrowFreezeReasonByShipment(shipment_id)
}

// ── Shipment read/write ───────────────────────────────────────────────────────

/// Canonical helper to retrieve a shipment by ID.
/// Checks persistent storage first, then falls back to temporary archived storage.
pub fn get_shipment(env: &Env, shipment_id: u64) -> Option<Shipment> {
    if let Some(shipment) = env
        .storage()
        .persistent()
        .get(&DataKey::Shipment(shipment_id))
    {
        return Some(shipment);
    }
    get_archived_shipment(env, shipment_id)
}

/// Check whether shipment payload exists in persistent storage.
pub fn has_persistent_shipment(env: &Env, shipment_id: u64) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Shipment(shipment_id))
}

/// Check whether escrow entry exists in persistent storage.
pub fn has_escrow_entry(env: &Env, shipment_id: u64) -> bool {
    env.storage().persistent().has(&escrow_key(shipment_id))
}

/// Check whether a per-shipment event-count key exists in persistent storage.
pub fn has_event_count_entry(env: &Env, shipment_id: u64) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::EventCount(shipment_id))
}

/// Check whether a confirmation-hash entry exists in persistent storage.
pub fn has_confirmation_hash_entry(env: &Env, shipment_id: u64) -> bool {
    env.storage()
        .persistent()
        .has(&confirmation_hash_key(shipment_id))
}

/// Check whether a last-status-update timestamp exists in persistent storage.
pub fn has_last_status_update_entry(env: &Env, shipment_id: u64) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::LastStatusUpdate(shipment_id))
}

/// Persist a shipment to persistent storage.
pub fn set_shipment(env: &Env, shipment: &Shipment) {
    env.storage()
        .persistent()
        .set(&shipment_key(shipment.id), shipment);
}

// ── Archived shipment (temporary storage) ────────────────────────────────────

/// Archive a shipment by moving it from persistent to temporary storage.
pub fn archive_shipment(env: &Env, shipment_id: u64, shipment: &Shipment) {
    let key = DataKey::ArchivedShipment(shipment_id);
    env.storage().temporary().set(&key, shipment);
    let config = crate::config::get_config(env);
    let (threshold, extend_to) = clamp_ttl(
        env,
        config.shipment_ttl_threshold,
        config.shipment_ttl_extension,
    );
    env.storage()
        .temporary()
        .extend_ttl(&key, threshold, extend_to);
    env.storage()
        .persistent()
        .remove(&DataKey::Shipment(shipment_id));
}

/// Get an archived shipment from temporary storage. Refreshes TTL on read.
pub fn get_archived_shipment(env: &Env, shipment_id: u64) -> Option<Shipment> {
    let key = DataKey::ArchivedShipment(shipment_id);
    let shipment: Option<Shipment> = env.storage().temporary().get(&key);
    if shipment.is_some() {
        let config = crate::config::get_config(env);
        let (threshold, extend_to) = clamp_ttl(
            env,
            config.shipment_ttl_threshold,
            config.shipment_ttl_extension,
        );
        env.storage()
            .temporary()
            .extend_ttl(&key, threshold, extend_to);
    }
    shipment
}

/// Check if a shipment is archived in temporary storage.
pub fn is_shipment_archived(env: &Env, shipment_id: u64) -> bool {
    env.storage()
        .temporary()
        .has(&DataKey::ArchivedShipment(shipment_id))
}

/// Extend TTL for an archived shipment in temporary storage.
pub fn extend_archived_shipment_ttl(env: &Env, shipment_id: u64, threshold: u32, extend_to: u32) {
    let key = DataKey::ArchivedShipment(shipment_id);
    if env.storage().temporary().has(&key) {
        env.storage()
            .temporary()
            .extend_ttl(&key, threshold, extend_to);
    }
}

// ── Escrow ────────────────────────────────────────────────────────────────────

/// Get escrow amount for a shipment from persistent storage. Returns 0 if unset.
pub fn get_escrow(env: &Env, shipment_id: u64) -> i128 {
    env.storage()
        .persistent()
        .get(&escrow_key(shipment_id))
        .unwrap_or(0)
}

/// Backwards-compatible wrapper.
pub fn get_escrow_balance(env: &Env, shipment_id: u64) -> i128 {
    get_escrow(env, shipment_id)
}

/// Check if the shipment's struct escrow_amount differs from dedicated storage.
pub fn has_escrow_mismatch(env: &Env, shipment_id: u64, struct_escrow_amount: i128) -> bool {
    get_escrow(env, shipment_id) != struct_escrow_amount
}

/// Set escrow amount for a shipment in persistent storage.
pub fn set_escrow(env: &Env, shipment_id: u64, amount: i128) {
    env.storage()
        .persistent()
        .set(&escrow_key(shipment_id), &amount);
}

/// Get the latest escrow freeze reason code for a shipment.
pub fn get_escrow_freeze_reason(env: &Env, shipment_id: u64) -> Option<EscrowFreezeReason> {
    env.storage()
        .persistent()
        .get(&escrow_freeze_reason_key(shipment_id))
}

/// Store the latest escrow freeze reason code for a shipment.
pub fn set_escrow_freeze_reason(env: &Env, shipment_id: u64, reason: &EscrowFreezeReason) {
    env.storage()
        .persistent()
        .set(&escrow_freeze_reason_key(shipment_id), reason);
}

/// Remove escrow for a shipment from persistent storage.
pub fn remove_escrow(env: &Env, shipment_id: u64) {
    env.storage().persistent().remove(&escrow_key(shipment_id));
}

/// Backwards-compatible name used by tests.
pub fn remove_escrow_balance(env: &Env, shipment_id: u64) {
    remove_escrow(env, shipment_id);
}

// ── Confirmation hash ─────────────────────────────────────────────────────────

/// Store confirmation hash for a shipment in persistent storage.
pub fn set_confirmation_hash(env: &Env, shipment_id: u64, hash: &BytesN<32>) {
    let key = DataKey::ConfirmationHash(shipment_id);
    env.storage().persistent().set(&key, hash);
}

/// Retrieve confirmation hash for a shipment from persistent storage.
pub fn get_confirmation_hash(env: &Env, shipment_id: u64) -> Option<BytesN<32>> {
    let key = DataKey::ConfirmationHash(shipment_id);
    env.storage().persistent().get(&key)
}

// ── Partial delivery proofs ───────────────────────────────────────────────────

/// Read every partial-delivery proof hash recorded for a shipment, oldest first.
pub fn get_partial_delivery_proofs(env: &Env, shipment_id: u64) -> Vec<BytesN<32>> {
    env.storage()
        .persistent()
        .get(&DeliveryProofKey::PartialLog(shipment_id))
        .unwrap_or_else(|| Vec::new(env))
}

/// Read the partial-delivery proof hash recorded at `index` (zero-based).
pub fn get_partial_delivery_proof(env: &Env, shipment_id: u64, index: u32) -> Option<BytesN<32>> {
    get_partial_delivery_proofs(env, shipment_id).get(index)
}

/// Append one partial-delivery proof hash to a shipment's append-only log.
pub fn append_partial_delivery_proof(
    env: &Env,
    shipment_id: u64,
    hash: &BytesN<32>,
) -> Result<u32, NavinError> {
    let mut proofs = get_partial_delivery_proofs(env, shipment_id);
    let index = proofs.len();
    if index >= MAX_PARTIAL_DELIVERY_PROOFS {
        return Err(NavinError::CounterOverflow);
    }
    proofs.push_back(hash.clone());
    env.storage()
        .persistent()
        .set(&DeliveryProofKey::PartialLog(shipment_id), &proofs);
    Ok(index)
}

// ── Last status update timestamp ──────────────────────────────────────────────

/// Retrieve the timestamp of the last status update for a shipment.
pub fn get_last_status_update(env: &Env, shipment_id: u64) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::LastStatusUpdate(shipment_id))
}

/// Persist the timestamp of the last status update for a shipment.
pub fn set_last_status_update(env: &Env, shipment_id: u64, timestamp: u64) {
    env.storage()
        .persistent()
        .set(&DataKey::LastStatusUpdate(shipment_id), &timestamp);
}

// ── TTL helpers ───────────────────────────────────────────────────────────────

/// Clamp a TTL threshold/extend_to pair to the network's maximum entry TTL.
pub fn clamp_ttl(env: &Env, threshold: u32, extend_to: u32) -> (u32, u32) {
    let max_ttl = env.storage().max_ttl();
    let extend_to = extend_to.min(max_ttl);
    let threshold = threshold.min(extend_to);
    (threshold, extend_to)
}

/// Extend the TTL of a shipment and all its related storage entries.
pub fn extend_shipment_ttl(env: &Env, shipment_id: u64, threshold: u32, extend_to: u32) {
    let (threshold, extend_to) = clamp_ttl(env, threshold, extend_to);
    let key = shipment_key(shipment_id);
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, threshold, extend_to);
    }

    let ek = escrow_key(shipment_id);
    if env.storage().persistent().has(&ek) {
        env.storage()
            .persistent()
            .extend_ttl(&ek, threshold, extend_to);
    }

    let hash_key = confirmation_hash_key(shipment_id);
    if env.storage().persistent().has(&hash_key) {
        env.storage()
            .persistent()
            .extend_ttl(&hash_key, threshold, extend_to);
    }

    extend_archived_shipment_ttl(env, shipment_id, threshold, extend_to);

    let freeze_reason_key = escrow_freeze_reason_key(shipment_id);
    if env.storage().persistent().has(&freeze_reason_key) {
        env.storage()
            .persistent()
            .extend_ttl(&freeze_reason_key, threshold, extend_to);
    }

    let last_status_key = DataKey::LastStatusUpdate(shipment_id);
    if env.storage().persistent().has(&last_status_key) {
        env.storage()
            .persistent()
            .extend_ttl(&last_status_key, threshold, extend_to);
    }

    let dependents_key = DataKey::ShipmentDependents(shipment_id);
    if env.storage().persistent().has(&dependents_key) {
        env.storage()
            .persistent()
            .extend_ttl(&dependents_key, threshold, extend_to);
    }

    let metadata_key = DataKey::ShipmentMetadata(shipment_id);
    if env.storage().persistent().has(&metadata_key) {
        env.storage()
            .persistent()
            .extend_ttl(&metadata_key, threshold, extend_to);
    }

    let persistent = env.storage().persistent();
    let counter_keys = [
        DataKey::MilestoneEventCount(shipment_id),
        DataKey::BreachEventCount(shipment_id),
        DataKey::DeadlineWarningEmitted(shipment_id),
        DataKey::ActiveSettlement(shipment_id),
    ];
    for key in counter_keys.iter() {
        if persistent.has(key) {
            persistent.extend_ttl(key, threshold, extend_to);
        }
    }

    let latest_key = crate::types::SettlementKey::Latest(shipment_id);
    if persistent.has(&latest_key) {
        persistent.extend_ttl(&latest_key, threshold, extend_to);
    }
    let settlement_ids_key = crate::types::SettlementKey::Ids(shipment_id);
    if persistent.has(&settlement_ids_key) {
        persistent.extend_ttl(&settlement_ids_key, threshold, extend_to);
    }
    let settlement_ids: Vec<u64> = persistent
        .get(&settlement_ids_key)
        .unwrap_or_else(|| Vec::new(env));
    for id in settlement_ids.iter() {
        let key = DataKey::Settlement(id);
        if persistent.has(&key) {
            persistent.extend_ttl(&key, threshold, extend_to);
        }
    }
    for id in [
        super::settlements::get_latest_settlement(env, shipment_id),
        super::settlements::get_active_settlement(env, shipment_id),
    ]
    .into_iter()
    .flatten()
    {
        let key = DataKey::Settlement(id);
        if persistent.has(&key) {
            persistent.extend_ttl(&key, threshold, extend_to);
        }
    }

    extend_status_hash_ttl(env, shipment_id, threshold, extend_to);

    let partial_proofs_key = DeliveryProofKey::PartialLog(shipment_id);
    if persistent.has(&partial_proofs_key) {
        persistent.extend_ttl(&partial_proofs_key, threshold, extend_to);
    }
}

// ── Status hash log ───────────────────────────────────────────────────────────

/// Read a shipment's status-hash log, or an empty log if it has none.
pub fn get_status_hash_log(env: &Env, shipment_id: u64) -> StatusHashLog {
    let key = StatusHashKey::Log(shipment_id);
    if let Some(log) = env
        .storage()
        .persistent()
        .get::<StatusHashKey, StatusHashLog>(&key)
    {
        return log;
    }
    env.storage()
        .temporary()
        .get(&key)
        .unwrap_or_else(|| StatusHashLog {
            next_seq: 0,
            entries: Map::new(env),
            visits: Map::new(env),
        })
}

/// Persist a shipment's status-hash log with the shipment TTL window.
fn set_status_hash_log(env: &Env, shipment_id: u64, log: &StatusHashLog) {
    let config = crate::config::get_config(env);
    let (threshold, extend_to) = clamp_ttl(
        env,
        config.shipment_ttl_threshold,
        config.shipment_ttl_extension,
    );
    let key = StatusHashKey::Log(shipment_id);
    env.storage().persistent().set(&key, log);
    env.storage()
        .persistent()
        .extend_ttl(&key, threshold, extend_to);
}

/// Append one visit's data hash to a shipment's bounded status-hash log.
pub fn append_status_hash(
    env: &Env,
    shipment_id: u64,
    status: &ShipmentStatus,
    data_hash: &BytesN<32>,
    actor: &Address,
) -> StatusHashRecord {
    let mut log = get_status_hash_log(env, shipment_id);

    let seq = log.next_seq;
    let visit_index = log.visits.get(status.clone()).unwrap_or(0);

    let record = StatusHashRecord {
        status: status.clone(),
        data_hash: data_hash.clone(),
        visit_index,
        seq,
        timestamp: env.ledger().timestamp(),
        actor: actor.clone(),
    };

    log.entries.set(seq, record.clone());
    log.visits
        .set(status.clone(), visit_index.saturating_add(1));
    log.next_seq = seq.saturating_add(1);

    let evicted = log.next_seq.checked_sub(MAX_STATUS_HASHES_PER_SHIPMENT + 1);
    if let Some(seq) = evicted {
        if seq < log.next_seq {
            log.entries.remove(seq);
        }
    }

    set_status_hash_log(env, shipment_id, &log);
    record
}

/// Look up the recorded data hash for one visit to a status.
pub fn find_status_hash(
    env: &Env,
    shipment_id: u64,
    status: &ShipmentStatus,
    visit_index: u32,
) -> Option<StatusHashRecord> {
    if visit_index
        >= get_status_hash_log(env, shipment_id)
            .visits
            .get(status.clone())?
    {
        return None;
    }
    status_hash_history_for(env, shipment_id, status)
        .into_iter()
        .find(|record| record.visit_index == visit_index)
}

/// Return every retained entry of a shipment's status-hash log, oldest first.
pub fn status_hash_history(env: &Env, shipment_id: u64) -> Vec<StatusHashRecord> {
    let log = get_status_hash_log(env, shipment_id);
    let oldest = log.next_seq.saturating_sub(MAX_STATUS_HASHES_PER_SHIPMENT);

    let mut out = Vec::new(env);
    for seq in oldest..log.next_seq {
        if let Some(record) = log.entries.get(seq) {
            out.push_back(record);
        }
    }
    out
}

/// Return the retained visits to a single status, oldest first.
pub fn status_hash_history_for(
    env: &Env,
    shipment_id: u64,
    status: &ShipmentStatus,
) -> Vec<StatusHashRecord> {
    let mut out = Vec::new(env);
    for record in status_hash_history(env, shipment_id).iter() {
        if &record.status == status {
            out.push_back(record);
        }
    }
    out
}

/// Extend the TTL of a shipment's entire status-hash log.
pub fn extend_status_hash_ttl(env: &Env, shipment_id: u64, threshold: u32, extend_to: u32) {
    let key = StatusHashKey::Log(shipment_id);
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, threshold, extend_to);
    }
}

/// Remove a shipment's entire status-hash log.
pub fn purge_status_hash_log(env: &Env, shipment_id: u64) {
    let key = StatusHashKey::Log(shipment_id);
    env.storage().persistent().remove(&key);
    env.storage().temporary().remove(&key);
}

/// Purge per-shipment hash/state entries when a shipment reaches a terminal state.
pub fn purge_status_hashes(env: &Env, shipment_id: u64) {
    let status_opt: Option<ShipmentStatus> = match get_shipment(env, shipment_id) {
        Some(shipment) => Some(shipment.status),
        None => get_archived_shipment(env, shipment_id).map(|shipment| shipment.status),
    };

    let is_terminal = matches!(
        status_opt,
        Some(ShipmentStatus::Delivered)
            | Some(ShipmentStatus::Cancelled)
            | Some(ShipmentStatus::PartiallyRefunded)
    );

    if !is_terminal {
        return;
    }

    env.storage()
        .persistent()
        .remove(&DataKey::ConfirmationHash(shipment_id));
    env.storage()
        .persistent()
        .remove(&DataKey::LastStatusUpdate(shipment_id));
    env.storage()
        .persistent()
        .remove(&DataKey::DeadlineWarningEmitted(shipment_id));
    env.storage()
        .persistent()
        .remove(&DataKey::BreachEventCount(shipment_id));
    env.storage()
        .persistent()
        .remove(&DataKey::MilestoneEventCount(shipment_id));
    env.storage()
        .persistent()
        .remove(&DataKey::EventCount(shipment_id));
    env.storage()
        .persistent()
        .remove(&DataKey::ShipmentDependents(shipment_id));
    env.storage()
        .persistent()
        .remove(&DataKey::EscrowFreezeReasonByShipment(shipment_id));
    env.storage()
        .persistent()
        .remove(&DataKey::ActiveSettlement(shipment_id));
    env.storage()
        .persistent()
        .remove(&DataKey::ShipmentMetadata(shipment_id));
    remove_shipment_dependents(env, shipment_id);
    purge_status_hash_log(env, shipment_id);
    env.storage()
        .persistent()
        .remove(&DeliveryProofKey::PartialLog(shipment_id));
}

// ── Shipment dependencies ─────────────────────────────────────────────────────

/// Get the prerequisite IDs for a dependent shipment.
pub fn get_shipment_dependents(env: &Env, dependent_id: u64) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::ShipmentDependents(dependent_id))
        .unwrap_or(Vec::new(env))
}

/// Add a prerequisite for a dependent shipment and return the updated list.
pub fn set_shipment_dependency(env: &Env, dependent_id: u64, prereq_id: u64) {
    let mut prereqs: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::ShipmentDependents(dependent_id))
        .unwrap_or(Vec::new(env));
    prereqs.push_back(prereq_id);
    env.storage()
        .persistent()
        .set(&DataKey::ShipmentDependents(dependent_id), &prereqs);
}

/// Remove the prerequisite list for a dependent shipment from persistent storage.
pub fn remove_shipment_dependents(env: &Env, dependent_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::ShipmentDependents(dependent_id));
}

// ── Shipment metadata ─────────────────────────────────────────────────────────

/// Read the key-value metadata map stored for a shipment.
///
/// Returns an empty map when no metadata has been recorded yet.
pub fn get_shipment_metadata(env: &Env, shipment_id: u64) -> Map<Symbol, Symbol> {
    env.storage()
        .persistent()
        .get(&DataKey::ShipmentMetadata(shipment_id))
        .unwrap_or_else(|| Map::new(env))
}

/// Persist the key-value metadata map for a shipment.
pub fn set_shipment_metadata(env: &Env, shipment_id: u64, metadata: &Map<Symbol, Symbol>) {
    env.storage()
        .persistent()
        .set(&DataKey::ShipmentMetadata(shipment_id), metadata);
}
