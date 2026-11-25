//! Persistent-storage helpers for settlement records and the active/latest
//! settlement indices.

use crate::{
    errors::NavinError,
    types::{DataKey, SettlementKey, SettlementRecord},
};
use soroban_sdk::{Env, Vec};

/// Get the settlement counter value from instance storage. Defaults to 0.
pub fn get_settlement_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::SettlementCounter)
        .unwrap_or(0)
}

/// Set the settlement counter.
pub fn set_settlement_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&DataKey::SettlementCounter, &counter);
}

/// Increment the settlement counter by 1 and return the new value.
///
/// Returns `NavinError::CounterOverflow` if the counter is already at `u64::MAX`.
/// The stored counter is left unchanged so an existing settlement ID is never reused.
pub fn increment_settlement_counter(env: &Env) -> Result<u64, NavinError> {
    let next = get_settlement_counter(env)
        .checked_add(1)
        .ok_or(NavinError::CounterOverflow)?;
    set_settlement_counter(env, next);
    Ok(next)
}

/// Get a settlement record by ID from persistent storage.
pub fn get_settlement(env: &Env, settlement_id: u64) -> Option<SettlementRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::Settlement(settlement_id))
}

/// Persist a settlement record to persistent storage.
///
/// Also updates the per-shipment `SettlementKey::Latest` and
/// `SettlementKey::Ids` indices so TTL extension can find all settlement records
/// for a shipment without a scan.
pub fn set_settlement(env: &Env, settlement: &SettlementRecord) {
    let persistent = env.storage().persistent();
    persistent.set(&DataKey::Settlement(settlement.settlement_id), settlement);
    persistent.set(
        &SettlementKey::Latest(settlement.shipment_id),
        &settlement.settlement_id,
    );
    let ids_key = SettlementKey::Ids(settlement.shipment_id);
    let mut ids: Vec<u64> = persistent.get(&ids_key).unwrap_or_else(|| Vec::new(env));
    if !ids.iter().any(|id| id == settlement.settlement_id) {
        ids.push_back(settlement.settlement_id);
    }
    persistent.set(&ids_key, &ids);
}

/// Get the most recent settlement ID recorded for a shipment.
///
/// Unlike [`get_active_settlement`], this survives settlement completion or
/// failure, so it can be used to renew TTL alongside the shipment.
pub fn get_latest_settlement(env: &Env, shipment_id: u64) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&SettlementKey::Latest(shipment_id))
}

/// Get the active settlement ID for a shipment.
pub fn get_active_settlement(env: &Env, shipment_id: u64) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::ActiveSettlement(shipment_id))
}

/// Set the active settlement ID for a shipment.
pub fn set_active_settlement(env: &Env, shipment_id: u64, settlement_id: u64) {
    env.storage()
        .persistent()
        .set(&DataKey::ActiveSettlement(shipment_id), &settlement_id);
}

/// Clear the active settlement for a shipment.
pub fn clear_active_settlement(env: &Env, shipment_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::ActiveSettlement(shipment_id));
}
