use soroban_sdk::{Address, BytesN, Env, Map, String, Symbol, contracttype};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    IsInitialized,
    Admin,
    Name,
    Symbol,
    NextTokenId,
    Owner(u64),
    ShipmentId(u64),
    DataHash(u64),
    Metadata(u64),
}

// Initialization
pub fn is_initialized(env: &Env) -> bool {
    env.storage().persistent().has(&DataKey::IsInitialized)
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().persistent().set(&DataKey::Admin, admin);
    env.storage().persistent().set(&DataKey::IsInitialized, &true);
}

pub fn get_admin(env: &Env) -> Address {
    env.storage().persistent().get(&DataKey::Admin).unwrap()
}

// Collection metadata
pub fn set_name(env: &Env, name: &String) {
    env.storage().persistent().set(&DataKey::Name, name);
}

pub fn get_name(env: &Env) -> String {
    env.storage().persistent().get(&DataKey::Name).unwrap()
}

pub fn set_symbol(env: &Env, symbol: &String) {
    env.storage().persistent().set(&DataKey::Symbol, symbol);
}

pub fn get_symbol(env: &Env) -> String {
    env.storage().persistent().get(&DataKey::Symbol).unwrap()
}

// Token ID management
pub fn set_next_token_id(env: &Env, id: u64) {
    env.storage().persistent().set(&DataKey::NextTokenId, &id);
}

pub fn get_next_token_id(env: &Env) -> u64 {
    env.storage().persistent().get(&DataKey::NextTokenId).unwrap_or(1)
}

// Token ownership
pub fn set_owner(env: &Env, token_id: u64, owner: &Address) {
    env.storage().persistent().set(&DataKey::Owner(token_id), owner);
}

pub fn get_owner(env: &Env, token_id: u64) -> Option<Address> {
    env.storage().persistent().get(&DataKey::Owner(token_id))
}

// Shipment association
pub fn set_shipment_id(env: &Env, token_id: u64, shipment_id: u64) {
    env.storage().persistent().set(&DataKey::ShipmentId(token_id), &shipment_id);
}

pub fn get_shipment_id(env: &Env, token_id: u64) -> Option<u64> {
    env.storage().persistent().get(&DataKey::ShipmentId(token_id))
}

// Data hash
pub fn set_data_hash(env: &Env, token_id: u64, data_hash: &BytesN<32>) {
    env.storage().persistent().set(&DataKey::DataHash(token_id), data_hash);
}

pub fn get_data_hash(env: &Env, token_id: u64) -> Option<BytesN<32>> {
    env.storage().persistent().get(&DataKey::DataHash(token_id))
}

// Metadata
pub fn set_metadata(env: &Env, token_id: u64, metadata: &Map<Symbol, String>) {
    env.storage().persistent().set(&DataKey::Metadata(token_id), metadata);
}

pub fn get_metadata(env: &Env, token_id: u64) -> Option<Map<Symbol, String>> {
    env.storage().persistent().get(&DataKey::Metadata(token_id))
}

// TTL management for persistent storage
pub fn extend_token_ttl(env: &Env, token_id: u64, threshold_ledgers: u32, extend_to_ledgers: u32) {
    let keys = [
        DataKey::Owner(token_id),
        DataKey::ShipmentId(token_id),
        DataKey::DataHash(token_id),
        DataKey::Metadata(token_id),
    ];
    
    for key in keys.iter() {
        env.storage()
            .persistent()
            .extend_ttl(key, threshold_ledgers, extend_to_ledgers);
    }
}