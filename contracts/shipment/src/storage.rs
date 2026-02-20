use crate::types::*;
use soroban_sdk::{Address, Env};

/// Check if the contract has been initialized
pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

/// Get the admin address
pub fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

/// Set the admin address
pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

/// Get the current shipment counter
pub fn get_shipment_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::ShipmentCounter)
        .unwrap_or(0)
}

/// Set the shipment counter
pub fn set_shipment_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&DataKey::ShipmentCounter, &counter);
}
/// Add a carrier to a company's whitelist
pub fn add_carrier_to_whitelist(env: &Env, company: &Address, carrier: &Address) {
    let key = DataKey::CarrierWhitelist(company.clone(), carrier.clone());
    env.storage().instance().set(&key, &true);
}

/// Remove a carrier from a company's whitelist
pub fn remove_carrier_from_whitelist(env: &Env, company: &Address, carrier: &Address) {
    let key = DataKey::CarrierWhitelist(company.clone(), carrier.clone());
    env.storage().instance().remove(&key);
}

/// Check if a carrier is whitelisted for a company
pub fn is_carrier_whitelisted(env: &Env, company: &Address, carrier: &Address) -> bool {
    let key = DataKey::CarrierWhitelist(company.clone(), carrier.clone());
    env.storage().instance().get(&key).unwrap_or(false)
}
