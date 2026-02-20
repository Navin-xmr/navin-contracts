use crate::types::*;
use soroban_sdk::{Address, Env, Vec};

/// Checks if an address has admin privileges
pub fn is_admin(env: &Env, address: &Address) -> bool {
    let admins: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Admins)
        .unwrap_or_else(|| Vec::new(env));

    admins.contains(address)
}

/// Add a new admin address
pub fn add_admin(env: &Env, admin: &Address, new_admin: &Address) {
    // Only existing admins can add new admins
    if !is_admin(env, admin) {
        panic!("Unauthorized: Only admins can add new admins");
    }

    let mut admins: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Admins)
        .unwrap_or_else(|| Vec::new(env));

    if !admins.contains(new_admin) {
        admins.push_back(new_admin.clone());
        env.storage().instance().set(&DataKey::Admins, &admins);
    }
}

/// Get current balance for an address
pub fn get_balance(env: &Env, address: &Address) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::AssetBalance(address.clone()))
        .unwrap_or(0)
}

/// Update balance for an address
pub fn update_balance(env: &Env, address: &Address, amount: i128) {
    env.storage()
        .instance()
        .set(&DataKey::AssetBalance(address.clone()), &amount);
}

/// Checks if an address is a carrier
pub fn is_carrier(env: &Env, address: &Address) -> bool {
    let carriers: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Carriers)
        .unwrap_or_else(|| Vec::new(env));

    carriers.contains(address)
}

/// Get next shipment ID and increment
pub fn get_next_shipment_id(env: &Env) -> u64 {
    let id = env
        .storage()
        .instance()
        .get(&DataKey::NextShipmentId)
        .unwrap_or(1u64);

    env.storage()
        .instance()
        .set(&DataKey::NextShipmentId, &(id + 1));
    id
}

/// Save batch shipment data
pub fn save_batch_shipment(env: &Env, shipment: &BatchShipment) {
    env.storage()
        .instance()
        .set(&DataKey::BatchShipment(shipment.id), shipment);
}
