use crate::types::*;
use soroban_sdk::{Address, BytesN, Env};

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

/// Get the contract version number.
pub fn get_version(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::Version).unwrap_or(1)
}

/// Set the contract version number.
pub fn set_version(env: &Env, version: u32) {
    env.storage().instance().set(&DataKey::Version, &version);
}

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

/// Increment the shipment counter by 1 and return the new value.
#[allow(dead_code)]
pub fn increment_shipment_counter(env: &Env) -> u64 {
    let cur = get_shipment_counter(env);
    let next = cur.checked_add(1).unwrap_or(cur);
    set_shipment_counter(env, next);
    next
}

/// Alternate name requested: returns the shipment count (wrapper).
#[allow(dead_code)]
pub fn get_shipment_count(env: &Env) -> u64 {
    get_shipment_counter(env)
}

/// Alternate name requested: increment shipment count and return new value.
#[allow(dead_code)]
pub fn increment_shipment_count(env: &Env) -> u64 {
    increment_shipment_counter(env)
}

/// Add a carrier to a company's whitelist in instance storage.
pub fn add_carrier_to_whitelist(env: &Env, company: &Address, carrier: &Address) {
    let key = DataKey::CarrierWhitelist(company.clone(), carrier.clone());
    env.storage().instance().set(&key, &true);
}

/// Remove a carrier from a company's whitelist in instance storage.
pub fn remove_carrier_from_whitelist(env: &Env, company: &Address, carrier: &Address) {
    let key = DataKey::CarrierWhitelist(company.clone(), carrier.clone());
    env.storage().instance().remove(&key);
}

/// Check whether a carrier is whitelisted for a given company.
pub fn is_carrier_whitelisted(env: &Env, company: &Address, carrier: &Address) -> bool {
    let key = DataKey::CarrierWhitelist(company.clone(), carrier.clone());
    env.storage().instance().get(&key).unwrap_or(false)
}

/// Store a role for an address in instance storage.
pub fn set_role(env: &Env, address: &Address, role: &Role) {
    env.storage()
        .instance()
        .set(&DataKey::Role(address.clone()), role);
}

/// Retrieve the role assigned to an address. Returns None if not set.
pub fn get_role(env: &Env, address: &Address) -> Option<Role> {
    env.storage()
        .instance()
        .get(&DataKey::Role(address.clone()))
}

/// Backwards-compatible: grant Company role to an address.
pub fn set_company_role(env: &Env, company: &Address) {
    set_role(env, company, &Role::Company);
}

/// Backwards-compatible: grant Carrier role to an address.
pub fn set_carrier_role(env: &Env, carrier: &Address) {
    set_role(env, carrier, &Role::Carrier);
}

/// Backwards-compatible: check whether an address has Company role.
pub fn has_company_role(env: &Env, address: &Address) -> bool {
    matches!(get_role(env, address), Some(Role::Company))
}

/// Backwards-compatible: check whether an address has Carrier role.
pub fn has_carrier_role(env: &Env, address: &Address) -> bool {
    matches!(get_role(env, address), Some(Role::Carrier))
}

/// Retrieve a shipment from persistent storage. Returns None if not found.
pub fn get_shipment(env: &Env, shipment_id: u64) -> Option<Shipment> {
    env.storage()
        .persistent()
        .get(&DataKey::Shipment(shipment_id))
}

/// Persist a shipment to persistent storage (survives TTL extension).
pub fn set_shipment(env: &Env, shipment: &Shipment) {
    env.storage()
        .persistent()
        .set(&DataKey::Shipment(shipment.id), shipment);
}

/// Get escrow amount for a shipment from persistent storage. Returns 0 if unset.
pub fn get_escrow(env: &Env, shipment_id: u64) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Escrow(shipment_id))
        .unwrap_or(0)
}

/// Set escrow amount for a shipment in persistent storage.
#[allow(dead_code)]
pub fn set_escrow(env: &Env, shipment_id: u64, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::Escrow(shipment_id), &amount);
}

/// Remove escrow for a shipment from persistent storage.
#[allow(dead_code)]
pub fn remove_escrow(env: &Env, shipment_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::Escrow(shipment_id));
}

/// Backwards-compatible name used by tests: set escrow balance.
#[allow(dead_code)]
pub fn set_escrow_balance(env: &Env, shipment_id: u64, amount: i128) {
    set_escrow(env, shipment_id, amount);
}

/// Backwards-compatible name used by tests: remove escrow balance.
#[allow(dead_code)]
pub fn remove_escrow_balance(env: &Env, shipment_id: u64) {
    remove_escrow(env, shipment_id);
}

/// Store confirmation hash for a shipment in persistent storage.
pub fn set_confirmation_hash(env: &Env, shipment_id: u64, hash: &BytesN<32>) {
    let key = DataKey::ConfirmationHash(shipment_id);
    env.storage().persistent().set(&key, hash);
    env.storage().persistent().set(&key, hash);
}

/// Retrieve confirmation hash for a shipment from persistent storage.
#[allow(dead_code)]
pub fn get_confirmation_hash(env: &Env, shipment_id: u64) -> Option<BytesN<32>> {
    let key = DataKey::ConfirmationHash(shipment_id);
    env.storage().persistent().get(&key)
}

/// Extend TTL for shipment data
pub fn extend_shipment_ttl(env: &Env, shipment_id: u64, threshold: u32, extend_to: u32) {
    let key = DataKey::Shipment(shipment_id);
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, threshold, extend_to);
    }

    let escrow_key = DataKey::Escrow(shipment_id);
    if env.storage().persistent().has(&escrow_key) {
        env.storage()
            .persistent()
            .extend_ttl(&escrow_key, threshold, extend_to);
    }

    let hash_key = DataKey::ConfirmationHash(shipment_id);
    if env.storage().persistent().has(&hash_key) {
        env.storage()
            .persistent()
            .extend_ttl(&hash_key, threshold, extend_to);
    }
}

/// Backwards-compatible wrapper used by existing contract code/tests.
pub fn get_escrow_balance(env: &Env, shipment_id: u64) -> i128 {
    get_escrow(env, shipment_id)
}

/// Get the token contract address
pub fn get_token_contract(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::TokenContract)
}

/// Set the token contract address
pub fn set_token_contract(env: &Env, token_contract: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::TokenContract, token_contract);
}

/// Retrieve the timestamp of the last status update for a shipment.
/// Returns None if no status update has been recorded yet.
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
