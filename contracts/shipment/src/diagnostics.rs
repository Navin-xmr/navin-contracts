use crate::storage;
use crate::types::ShipmentStatus;
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

/// Executes a health check over all stored shipments.
/// This loops through all shipments up to the global counter, tallying
/// actively held escrow and watching for missing records or abnormal shipment states
/// (like lingering past a deadline).
pub fn run_system_health_check(env: &Env) -> SystemHealthStatus {
    let total_shipments = storage::get_shipment_count(env);

    let mut sum_of_escrow_balances: i128 = 0;
    let mut active_shipments_counted: u32 = 0;
    let mut anomalous_shipment_ids = Vec::new(env);
    let mut storage_inconsistencies = Vec::new(env);

    let current_timestamp = env.ledger().timestamp();

    for id in 1..=total_shipments {
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
            }
            None => {
                // Tracking a shipment internally that does not map to any persistent or archived storage
                storage_inconsistencies.push_back(id);
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
