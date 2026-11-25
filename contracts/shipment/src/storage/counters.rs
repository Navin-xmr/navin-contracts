//! Instance and persistent-storage helpers for all counters: shipment count,
//! event counts, milestone/breach counts, analytics, shipment limits, active
//! shipment counts, creation quotas, fee config and idempotency windows.

use crate::{
    errors::NavinError,
    types::{DataKey, FeeConfig, ShipmentStatus},
};
use soroban_sdk::{panic_with_error, Address, BytesN, Env, TryFromVal, Val};

// ── Shipment counter ──────────────────────────────────────────────────────────

/// Get the current shipment counter from instance storage.
pub fn get_shipment_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::ShipmentCount)
        .unwrap_or(0)
}

/// Set the shipment counter in instance storage.
pub fn set_shipment_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&DataKey::ShipmentCount, &counter);
}

/// Alias used by the consistency and diagnostics modules.
pub fn get_shipment_count(env: &Env) -> u64 {
    get_shipment_counter(env)
}

// ── Milestone and breach event counters ───────────────────────────────────────

/// Get the milestone event count for a shipment. Returns 0 if none yet.
pub fn get_milestone_event_count(env: &Env, shipment_id: u64) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::MilestoneEventCount(shipment_id))
        .unwrap_or(0)
}

/// Increment the milestone event count for a shipment.
pub fn increment_milestone_event_count(env: &Env, shipment_id: u64) {
    let current = get_milestone_event_count(env, shipment_id);
    env.storage().persistent().set(
        &DataKey::MilestoneEventCount(shipment_id),
        &current.saturating_add(1),
    );
}

/// Get the condition breach event count for a shipment.
pub fn get_breach_event_count(env: &Env, shipment_id: u64) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::BreachEventCount(shipment_id))
        .unwrap_or(0)
}

/// Increment the condition breach event count for a shipment.
pub fn increment_breach_event_count(env: &Env, shipment_id: u64) {
    let current = get_breach_event_count(env, shipment_id);
    env.storage().persistent().set(
        &DataKey::BreachEventCount(shipment_id),
        &current.saturating_add(1),
    );
}

// ── Per-shipment event count ──────────────────────────────────────────────────

/// Get the event count for a shipment. Returns 0 if none yet.
pub fn get_event_count(env: &Env, shipment_id: u64) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::EventCount(shipment_id))
        .unwrap_or(0)
}

// ── Global event counter ──────────────────────────────────────────────────────

/// Return the contract-wide event counter.
///
/// Reads legacy `u32` values transparently and returns them as `u64`.
pub fn get_global_event_count(env: &Env) -> u64 {
    let raw: Option<Val> = env.storage().instance().get(&DataKey::GlobalEventCount);
    match raw {
        None => 0,
        Some(val) => u64::try_from_val(env, &val)
            .or_else(|_| u32::try_from_val(env, &val).map(u64::from))
            .unwrap_or_else(|_| panic_with_error!(env, NavinError::CounterOverflow)),
    }
}

/// Increment and return the next contract-wide event counter. Panics on overflow.
pub fn increment_global_event_count(env: &Env) -> u64 {
    try_increment_global_event_count(env).unwrap_or_else(|err| panic_with_error!(env, err))
}

/// Checked variant of [`increment_global_event_count`].
pub fn try_increment_global_event_count(env: &Env) -> Result<u64, NavinError> {
    let next = get_global_event_count(env)
        .checked_add(1)
        .ok_or(NavinError::CounterOverflow)?;
    env.storage()
        .instance()
        .set(&DataKey::GlobalEventCount, &next);
    Ok(next)
}

// ── Idempotency window ────────────────────────────────────────────────────────

/// Returns true if the action hash is already within an active idempotency window.
pub fn has_idempotency_window(env: &Env, action_hash: &BytesN<32>) -> bool {
    env.storage()
        .temporary()
        .has(&DataKey::IdempotencyWindow(action_hash.clone()))
}

/// Record an action hash in temporary storage for `window_seconds`.
pub fn set_idempotency_window(env: &Env, action_hash: &BytesN<32>, window_seconds: u64) {
    let key = DataKey::IdempotencyWindow(action_hash.clone());
    let ledgers = window_seconds.div_ceil(5).max(1) as u32;
    env.storage().temporary().set(&key, &true);
    env.storage().temporary().extend_ttl(&key, 0, ledgers);
}

// ── Analytics counters ────────────────────────────────────────────────────────

/// Get the total escrow volume processed by the contract.
pub fn get_total_escrow_volume(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalEscrowVolume)
        .unwrap_or(0)
}

/// Add an amount to the total escrow volume.
pub fn add_total_escrow_volume(env: &Env, amount: i128) -> Result<(), NavinError> {
    if amount < 0 {
        return Err(NavinError::ArithmeticError);
    }
    let current = get_total_escrow_volume(env);
    let updated = current
        .checked_add(amount)
        .ok_or(NavinError::ArithmeticError)?;
    if updated < 0 {
        return Err(NavinError::ArithmeticError);
    }
    env.storage()
        .instance()
        .set(&DataKey::TotalEscrowVolume, &updated);
    Ok(())
}

/// Get the total number of disputes raised.
pub fn get_total_disputes(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::TotalDisputes)
        .unwrap_or(0)
}

/// Increment the total disputes counter by 1.
pub fn increment_total_disputes(env: &Env) {
    let current = get_total_disputes(env);
    env.storage()
        .instance()
        .set(&DataKey::TotalDisputes, &(current + 1));
}

/// Get the count of shipments with a specific status.
pub fn get_status_count(env: &Env, status: &ShipmentStatus) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::StatusCount(status.clone()))
        .unwrap_or(0)
}

/// Increment the count of shipments with a specific status.
pub fn increment_status_count(env: &Env, status: &ShipmentStatus) {
    let current = get_status_count(env, status);
    env.storage()
        .instance()
        .set(&DataKey::StatusCount(status.clone()), &(current + 1));
}

/// Decrement the count of shipments with a specific status. Saturates at 0.
pub fn decrement_status_count(env: &Env, status: &ShipmentStatus) {
    let current = get_status_count(env, status);
    if current > 0 {
        env.storage()
            .instance()
            .set(&DataKey::StatusCount(status.clone()), &(current - 1));
    }
}

// ── Shipment limit ────────────────────────────────────────────────────────────

/// Get the configurable limit on active shipments per company. Defaults to 100.
pub fn get_shipment_limit(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::ShipmentLimit)
        .unwrap_or(100)
}

/// Set the configurable limit on active shipments.
pub fn set_shipment_limit(env: &Env, limit: u32) {
    env.storage()
        .instance()
        .set(&DataKey::ShipmentLimit, &limit);
}

/// Get the company-specific shipment limit override, if set.
pub fn get_company_shipment_limit(env: &Env, company: &Address) -> Option<u32> {
    if env
        .storage()
        .persistent()
        .has(&DataKey::CompanyShipmentLimit(company.clone()))
    {
        extend_company_shipment_limit_ttl(env, company);
    }
    env.storage()
        .persistent()
        .get(&DataKey::CompanyShipmentLimit(company.clone()))
}

/// Set the company-specific shipment limit override.
pub fn set_company_shipment_limit(env: &Env, company: &Address, limit: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::CompanyShipmentLimit(company.clone()), &limit);
    extend_company_shipment_limit_ttl(env, company);
}

fn extend_company_shipment_limit_ttl(env: &Env, company: &Address) {
    let key = DataKey::CompanyShipmentLimit(company.clone());
    let config = crate::config::get_config(env);
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            config.shipment_ttl_threshold,
            config.shipment_ttl_extension,
        );
    }
}

/// Resolve effective shipment limit (company override first, then global).
pub fn get_effective_shipment_limit(env: &Env, company: &Address) -> u32 {
    get_company_shipment_limit(env, company).unwrap_or_else(|| get_shipment_limit(env))
}

/// Get the platform fee configuration.
pub fn get_fee_config(env: &Env) -> Option<FeeConfig> {
    env.storage().instance().get(&DataKey::FeeConfig)
}

/// Set the platform fee configuration.
pub fn set_fee_config(env: &Env, config: &FeeConfig) {
    env.storage().instance().set(&DataKey::FeeConfig, config);
}

// ── Active shipment count per company ─────────────────────────────────────────

/// Get the current active shipment count for a company.
pub fn get_active_shipment_count(env: &Env, company: &Address) -> u32 {
    if env
        .storage()
        .persistent()
        .has(&DataKey::ActiveShipmentCount(company.clone()))
    {
        extend_active_shipment_count_ttl(env, company);
    }
    env.storage()
        .persistent()
        .get(&DataKey::ActiveShipmentCount(company.clone()))
        .unwrap_or(0)
}

/// Increment the active shipment count for a company.
pub fn increment_active_shipment_count(env: &Env, company: &Address) {
    let current = get_active_shipment_count(env, company);
    env.storage().persistent().set(
        &DataKey::ActiveShipmentCount(company.clone()),
        &current.saturating_add(1),
    );
    extend_active_shipment_count_ttl(env, company);
}

/// Decrement the active shipment count for a company.
pub fn decrement_active_shipment_count(env: &Env, company: &Address) {
    let current = get_active_shipment_count(env, company);
    env.storage().persistent().set(
        &DataKey::ActiveShipmentCount(company.clone()),
        &current.saturating_sub(1),
    );
    extend_active_shipment_count_ttl(env, company);
}

fn extend_active_shipment_count_ttl(env: &Env, company: &Address) {
    let key = DataKey::ActiveShipmentCount(company.clone());
    let config = crate::config::get_config(env);
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            config.shipment_ttl_threshold,
            config.shipment_ttl_extension,
        );
    }
}

// ── Creation quota ────────────────────────────────────────────────────────────

/// Get the creation quota tracker for a company.
pub fn get_creation_quota(
    env: &Env,
    company: &Address,
) -> Option<crate::types::CreationQuotaTracker> {
    if env
        .storage()
        .persistent()
        .has(&DataKey::CompanyCreationQuota(company.clone()))
    {
        extend_creation_quota_ttl(env, company);
    }
    env.storage()
        .persistent()
        .get(&DataKey::CompanyCreationQuota(company.clone()))
}

/// Set the creation quota tracker for a company.
pub fn set_creation_quota(
    env: &Env,
    company: &Address,
    tracker: &crate::types::CreationQuotaTracker,
) {
    env.storage()
        .persistent()
        .set(&DataKey::CompanyCreationQuota(company.clone()), tracker);
    extend_creation_quota_ttl(env, company);
}

fn extend_creation_quota_ttl(env: &Env, company: &Address) {
    let key = DataKey::CompanyCreationQuota(company.clone());
    let config = crate::config::get_config(env);
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            config.shipment_ttl_threshold,
            config.shipment_ttl_extension,
        );
    }
}
