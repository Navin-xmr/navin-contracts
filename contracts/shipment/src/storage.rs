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

/// Grant Company role to an address
pub fn set_company_role(env: &Env, company: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::Company(company.clone()), &true);
}

/// Check whether an address has Company role
pub fn has_company_role(env: &Env, address: &Address) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::Company(address.clone()))
        .unwrap_or(false)
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
