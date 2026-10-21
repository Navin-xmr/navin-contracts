use soroban_sdk::{contracttype, Address, Env, String};

#[contracttype]
pub enum DataKey {
    Admin,
    BaseUri,
    Owner(u64),
    MetadataUri(u64),
}

pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

pub fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn get_owner(env: &Env, token_id: u64) -> Option<Address> {
    env.storage().persistent().get(&DataKey::Owner(token_id))
}

pub fn set_owner(env: &Env, token_id: u64, owner: &Address) {
    env.storage()
        .persistent()
        .set(&DataKey::Owner(token_id), owner);
}

pub fn get_metadata_uri(env: &Env, token_id: u64) -> Option<String> {
    env.storage()
        .persistent()
        .get(&DataKey::MetadataUri(token_id))
}

pub fn set_metadata_uri(env: &Env, token_id: u64, uri: &String) {
    env.storage()
        .persistent()
        .set(&DataKey::MetadataUri(token_id), uri);
}

pub fn get_base_uri(env: &Env) -> Option<String> {
    env.storage().instance().get(&DataKey::BaseUri)
}

pub fn set_base_uri(env: &Env, base_uri: &String) {
    env.storage().instance().set(&DataKey::BaseUri, base_uri);
}
