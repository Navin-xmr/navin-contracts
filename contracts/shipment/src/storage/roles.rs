//! Persistent-storage helpers for RBAC roles, carrier/company whitelist and
//! suspension state.

use crate::types::{DataKey, Role};
use soroban_sdk::{Address, Env};

// ── Carrier whitelist ─────────────────────────────────────────────────────────

/// Add a carrier to a company's whitelist in persistent storage.
pub fn add_carrier_to_whitelist(env: &Env, company: &Address, carrier: &Address) {
    let key = DataKey::CarrierWhitelist(company.clone(), carrier.clone());
    env.storage().persistent().set(&key, &true);
    extend_carrier_whitelist_ttl(env, company, carrier);
}

/// Remove a carrier from a company's whitelist in persistent storage.
pub fn remove_carrier_from_whitelist(env: &Env, company: &Address, carrier: &Address) {
    let key = DataKey::CarrierWhitelist(company.clone(), carrier.clone());
    env.storage().persistent().remove(&key);
}

/// Check whether a carrier is whitelisted for a given company.
pub fn is_carrier_whitelisted(env: &Env, company: &Address, carrier: &Address) -> bool {
    let key = DataKey::CarrierWhitelist(company.clone(), carrier.clone());
    if env.storage().persistent().has(&key) {
        extend_carrier_whitelist_ttl(env, company, carrier);
    }
    env.storage().persistent().get(&key).unwrap_or(false)
}

/// Extend TTL for a carrier whitelist entry.
fn extend_carrier_whitelist_ttl(env: &Env, company: &Address, carrier: &Address) {
    let key = DataKey::CarrierWhitelist(company.clone(), carrier.clone());
    let config = crate::config::get_config(env);
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            config.shipment_ttl_threshold,
            config.shipment_ttl_extension,
        );
    }
}

// ── Generic roles ─────────────────────────────────────────────────────────────

/// Assign a role to an address in persistent storage.
pub fn set_role(env: &Env, address: &Address, role: &Role) {
    let key = DataKey::UserRole(address.clone(), role.clone());
    env.storage().persistent().set(&key, &true);
    extend_role_ttl(env, address, role);
}

/// Check if an address has a specific role in persistent storage.
pub fn has_role(env: &Env, address: &Address, role: &Role) -> bool {
    let key = DataKey::UserRole(address.clone(), role.clone());
    if env.storage().persistent().has(&key) {
        extend_role_ttl(env, address, role);
    }
    env.storage().persistent().get(&key).unwrap_or(false)
}

/// Extend TTL for a user role entry.
fn extend_role_ttl(env: &Env, address: &Address, role: &Role) {
    let key = DataKey::UserRole(address.clone(), role.clone());
    let config = crate::config::get_config(env);
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            config.shipment_ttl_threshold,
            config.shipment_ttl_extension,
        );
    }
}

/// Retrieve the primary role assigned to an address.
///
/// Returns the first match in the order `Company`, `Carrier`, `Guardian`,
/// `Operator`. Returns `None` if the address holds none of those roles.
pub fn get_role(env: &Env, address: &Address) -> Option<Role> {
    [Role::Company, Role::Carrier, Role::Guardian, Role::Operator]
        .into_iter()
        .find(|role| has_role(env, address, role))
}

/// Grant the `Company` role to an address.
pub fn set_company_role(env: &Env, company: &Address) {
    set_role(env, company, &Role::Company);
}

/// Grant the `Carrier` role to an address.
pub fn set_carrier_role(env: &Env, carrier: &Address) {
    set_role(env, carrier, &Role::Carrier);
}

/// Revoke a role from an address in persistent storage.
pub fn revoke_role(env: &Env, address: &Address, role: &Role) {
    let key = DataKey::UserRole(address.clone(), role.clone());
    env.storage().persistent().remove(&key);
}

/// Suspend a role temporarily.
pub fn suspend_role(env: &Env, address: &Address, role: &Role) {
    let suspend_key = DataKey::RoleSuspended(address.clone(), role.clone());
    env.storage().persistent().set(&suspend_key, &true);
    extend_role_suspended_ttl(env, address, role);
}

/// Reactivate a suspended role.
pub fn reactivate_role(env: &Env, address: &Address, role: &Role) {
    let suspend_key = DataKey::RoleSuspended(address.clone(), role.clone());
    env.storage().persistent().remove(&suspend_key);
}

/// Check if a role is suspended.
pub fn is_role_suspended(env: &Env, address: &Address, role: &Role) -> bool {
    let suspend_key = DataKey::RoleSuspended(address.clone(), role.clone());
    if env.storage().persistent().has(&suspend_key) {
        extend_role_suspended_ttl(env, address, role);
    }
    env.storage()
        .persistent()
        .get(&suspend_key)
        .unwrap_or(false)
}

/// Extend TTL for a role suspended entry.
fn extend_role_suspended_ttl(env: &Env, address: &Address, role: &Role) {
    let suspend_key = DataKey::RoleSuspended(address.clone(), role.clone());
    let config = crate::config::get_config(env);
    if env.storage().persistent().has(&suspend_key) {
        env.storage().persistent().extend_ttl(
            &suspend_key,
            config.shipment_ttl_threshold,
            config.shipment_ttl_extension,
        );
    }
}

/// Check whether an address has Company role (legacy compatibility).
pub fn has_company_role(env: &Env, address: &Address) -> bool {
    has_role(env, address, &Role::Company)
}

/// Check whether an address has the `Carrier` role.
pub fn has_carrier_role(env: &Env, address: &Address) -> bool {
    has_role(env, address, &Role::Carrier)
}

// ── Carrier suspension ────────────────────────────────────────────────────────

/// Mark a carrier as suspended.
pub fn suspend_carrier(env: &Env, carrier: &Address) {
    suspend_role(env, carrier, &Role::Carrier);
}

/// Remove a carrier suspension.
pub fn reactivate_carrier(env: &Env, carrier: &Address) {
    reactivate_role(env, carrier, &Role::Carrier);
}

/// Returns true when the carrier's `Carrier` role is suspended.
pub fn is_carrier_suspended(env: &Env, carrier: &Address) -> bool {
    is_role_suspended(env, carrier, &Role::Carrier)
}

// ── Company suspension ────────────────────────────────────────────────────────

/// Mark a company as suspended in persistent storage.
pub fn suspend_company(env: &Env, company: &Address) {
    env.storage()
        .persistent()
        .set(&DataKey::CompanySuspended(company.clone()), &true);
    extend_company_suspended_ttl(env, company);
}

/// Remove a company suspension flag from persistent storage.
pub fn reactivate_company(env: &Env, company: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::CompanySuspended(company.clone()));
}

/// Returns true when the company has an active suspension flag.
pub fn is_company_suspended(env: &Env, company: &Address) -> bool {
    if env
        .storage()
        .persistent()
        .has(&DataKey::CompanySuspended(company.clone()))
    {
        extend_company_suspended_ttl(env, company);
    }
    env.storage()
        .persistent()
        .get(&DataKey::CompanySuspended(company.clone()))
        .unwrap_or(false)
}

/// Extend TTL for a company suspended entry.
fn extend_company_suspended_ttl(env: &Env, company: &Address) {
    let key = DataKey::CompanySuspended(company.clone());
    let config = crate::config::get_config(env);
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            config.shipment_ttl_threshold,
            config.shipment_ttl_extension,
        );
    }
}
