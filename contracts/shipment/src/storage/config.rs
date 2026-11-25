//! Instance-storage helpers for contract-level configuration fields:
//! initialization flag, admin address, version, token contract, pause flag
//! and reentrancy lock.

use crate::types::DataKey;
use soroban_sdk::{Address, Env};

/// Check if the contract has been initialized (admin set).
pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

/// Returns the stored admin address. Panics if not set.
pub fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

/// Store the admin address in instance storage (config scope).
pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

/// Returns the proposed admin address from instance storage, if set.
pub fn get_proposed_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::ProposedAdmin)
}

/// Store the proposed admin address in instance storage.
pub fn set_proposed_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::ProposedAdmin, admin);
}

/// Clear the proposed admin address from instance storage.
pub fn clear_proposed_admin(env: &Env) {
    env.storage().instance().remove(&DataKey::ProposedAdmin);
}

/// Get the contract version number.
pub fn get_version(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::Version).unwrap_or(1)
}

/// Set the contract version number.
pub fn set_version(env: &Env, version: u32) {
    env.storage().instance().set(&DataKey::Version, &version);
}

/// Get the token contract address.
pub fn get_token_contract(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::TokenContract)
}

/// Set the token contract address.
pub fn set_token_contract(env: &Env, token_contract: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::TokenContract, token_contract);
}

/// Check if the contract is paused.
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::IsPaused)
        .unwrap_or(false)
}

/// Set the contract pause state.
pub fn set_paused(env: &Env, paused: bool) {
    env.storage().instance().set(&DataKey::IsPaused, &paused);
}

/// Returns whether the reentrancy lock is currently active.
pub fn is_reentrancy_locked(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::ReentrancyLock)
        .unwrap_or(false)
}

/// Sets the reentrancy lock flag.
pub fn set_reentrancy_lock(env: &Env, locked: bool) {
    env.storage()
        .instance()
        .set(&DataKey::ReentrancyLock, &locked);
}
