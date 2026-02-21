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
        .get(&DataKey::ShipmentCount)
        .unwrap_or(0)
}

/// Set the shipment counter
pub fn set_shipment_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&DataKey::ShipmentCount, &counter);
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

/// Assign a role to an address
pub fn set_role(env: &Env, address: &Address, role: &Role) {
    let key = DataKey::UserRole(address.clone(), role.clone());
    env.storage().instance().set(&key, &true);
}

/// Check if an address has a specific role
pub fn has_role(env: &Env, address: &Address, role: &Role) -> bool {
    let key = DataKey::UserRole(address.clone(), role.clone());
    env.storage().instance().get(&key).unwrap_or(false)
}

/// Grant Company role to an address (legacy compatibility)
pub fn set_company_role(env: &Env, company: &Address) {
    set_role(env, company, &Role::Company);
}

/// Check whether an address has Company role (legacy compatibility)
#[allow(dead_code)]
pub fn has_company_role(env: &Env, address: &Address) -> bool {
    has_role(env, address, &Role::Company)
}


/// Get shipment by ID
pub fn get_shipment(env: &Env, shipment_id: u64) -> Option<Shipment> {
    env.storage()
        .instance()
        .get(&DataKey::Shipment(shipment_id))
}

/// Persist shipment by ID
pub fn set_shipment(env: &Env, shipment: &Shipment) {
    env.storage()
        .instance()
        .set(&DataKey::Shipment(shipment.id), shipment);
}
