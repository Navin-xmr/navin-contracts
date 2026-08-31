use crate::storage;
use crate::types::{DataKey, ShipmentStatus};
use soroban_sdk::{contracttype, Env, Vec};

/// Reusable response object representing the state of the contract's health.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SystemHealthStatus {
    pub total_shipments: u64,
    pub sum_of_escrow_balances: i128,
    pub active_shipments_counted: u32,
    pub anomalous_shipment_ids: Vec<u64>,
    pub storage_inconsistencies: Vec<u64>,
}

/// Default sample cap for system health checks (budget safety on large sets, matching `get_ttl_health_summary`).
pub const DEFAULT_HEALTH_SAMPLE_LIMIT: u64 = 100;

/// Executes a health check over stored shipments, capped at `DEFAULT_HEALTH_SAMPLE_LIMIT` for budget safety.
pub fn run_system_health_check(env: &Env) -> SystemHealthStatus {
    run_system_health_check_range(env, 1, DEFAULT_HEALTH_SAMPLE_LIMIT)
}

/// Executes a health check over a specific range of shipment IDs [start_id, start_id + limit - 1].
pub fn run_system_health_check_range(env: &Env, start_id: u64, limit: u64) -> SystemHealthStatus {
    let total_shipments = storage::get_shipment_count(env);

    let mut sum_of_escrow_balances: i128 = 0;
    let mut active_shipments_counted: u32 = 0;
    let mut anomalous_shipment_ids = Vec::new(env);
    let mut storage_inconsistencies = Vec::new(env);

    let current_timestamp = env.ledger().timestamp();

    if start_id > 0 && start_id <= total_shipments && limit > 0 {
        let end_id = start_id
            .saturating_add(limit)
            .saturating_sub(1)
            .min(total_shipments);

        for id in start_id..=end_id {
            let shipment_opt = storage::get_shipment(env, id);

            match shipment_opt {
                Some(shipment) => {
                    let is_terminal = shipment.status == ShipmentStatus::Delivered
                        || shipment.status == ShipmentStatus::Cancelled
                        || shipment.status == ShipmentStatus::Disputed;

                    if !is_terminal {
                        active_shipments_counted += 1;

                        // Anomaly Check: Stuck InTransit past deadline
                        if shipment.deadline < current_timestamp
                            && shipment.status == ShipmentStatus::InTransit
                            && !anomalous_shipment_ids.contains(id)
                        {
                            anomalous_shipment_ids.push_back(id);
                        }
                    }

                    // Escrow tally
                    sum_of_escrow_balances =
                        sum_of_escrow_balances.saturating_add(shipment.escrow_amount);

                    // Consistency verification against storage structure
                    let has_persist = storage::has_persistent_shipment(env, id);
                    let escrow_in_storage = storage::get_escrow(env, id);

                    // Consistency check: dual storage of escrow must match
                    if shipment.escrow_amount != escrow_in_storage
                        && !storage_inconsistencies.contains(id)
                    {
                        storage_inconsistencies.push_back(id);
                    }

                    // Non-terminal shipments must be resiliently stored
                    if !is_terminal && !has_persist && !storage_inconsistencies.contains(id) {
                        storage_inconsistencies.push_back(id);
                    }

                    // Archived (terminal) shipments must not have orphaned
                    // per-shipment counter or index keys in persistent storage.
                    // Any present key is a false-positive inconsistency that
                    // inflates rent costs over time.
                    let is_archived = !has_persist && is_terminal;
                    if is_archived
                        && has_orphaned_counters(env, id)
                        && !storage_inconsistencies.contains(id)
                    {
                        storage_inconsistencies.push_back(id);
                    }
                }
                None => {
                    // Tracking a shipment internally that does not map to any persistent or archived storage
                    storage_inconsistencies.push_back(id);
                }
            }
        }
    }

    SystemHealthStatus {
        total_shipments,
        sum_of_escrow_balances,
        active_shipments_counted,
        anomalous_shipment_ids,
        storage_inconsistencies,
    }
}

/// Returns `true` if any per-shipment counter or index key still exists in
/// persistent storage for a shipment that has already been archived.
///
/// Used by `run_system_health_check_range` to detect orphaned keys left
/// behind by a prior (unfixed) `archive_shipment` call.
fn has_orphaned_counters(env: &Env, shipment_id: u64) -> bool {
    // Scalar counter/index keys
    if storage::has_event_count_entry(env, shipment_id) {
        return true;
    }
    if storage::has_confirmation_hash_entry(env, shipment_id) {
        return true;
    }
    if storage::has_last_status_update_entry(env, shipment_id) {
        return true;
    }
    if storage::has_escrow_entry(env, shipment_id) {
        return true;
    }
    if env
        .storage()
        .persistent()
        .has(&DataKey::MilestoneEventCount(shipment_id))
    {
        return true;
    }
    if env
        .storage()
        .persistent()
        .has(&DataKey::BreachEventCount(shipment_id))
    {
        return true;
    }
    if env
        .storage()
        .persistent()
        .has(&DataKey::ActiveSettlement(shipment_id))
    {
        return true;
    }
    if env
        .storage()
        .persistent()
        .has(&storage::escrow_freeze_reason_key(shipment_id))
    {
        return true;
    }
    // Note count (implies note hashes also exist)
    if env
        .storage()
        .persistent()
        .has(&DataKey::ShipmentNoteCount(shipment_id))
    {
        return true;
    }
    // Evidence count (implies evidence hashes also exist)
    if env
        .storage()
        .persistent()
        .has(&DataKey::DisputeEvidenceCount(shipment_id))
    {
        return true;
    }
    // Recovery record count (implies record entries also exist)
    if env
        .storage()
        .persistent()
        .has(&DataKey::RecoveryRecordCount(shipment_id))
    {
        return true;
    }
    false
}
