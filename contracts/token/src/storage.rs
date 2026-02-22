use soroban_sdk::{contracttype, Address, Env, String};

/// Storage keys for token contract data
#[contracttype]
pub enum DataKey {
    Admin,
    Name,
    Symbol,
    TotalSupply,
    Balance(Address),
    Allowance(Address, Address),
}

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

/// Get the token name
pub fn get_name(env: &Env) -> String {
    env.storage().instance().get(&DataKey::Name).unwrap()
}

/// Set the token name
pub fn set_name(env: &Env, name: &String) {
    env.storage().instance().set(&DataKey::Name, name);
}

/// Get the token symbol
pub fn get_symbol(env: &Env) -> String {
    env.storage().instance().get(&DataKey::Symbol).unwrap()
}

/// Set the token symbol
pub fn set_symbol(env: &Env, symbol: &String) {
    env.storage().instance().set(&DataKey::Symbol, symbol);
}

/// Get the total supply
pub fn get_total_supply(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::TotalSupply).unwrap()
}

/// Set the total supply
pub fn set_total_supply(env: &Env, supply: i128) {
    env.storage().instance().set(&DataKey::TotalSupply, &supply);
}

/// Get the balance of an address
pub fn get_balance(env: &Env, address: &Address) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::Balance(address.clone()))
        .unwrap_or(0)
}

/// Set the balance of an address
pub fn set_balance(env: &Env, address: &Address, balance: i128) {
    env.storage()
        .instance()
        .set(&DataKey::Balance(address.clone()), &balance);
}

/// Get the allowance of a spender for an owner's tokens
pub fn get_allowance(env: &Env, owner: &Address, spender: &Address) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::Allowance(owner.clone(), spender.clone()))
        .unwrap_or(0)
}

/// Set the allowance of a spender for an owner's tokens
pub fn set_allowance(env: &Env, owner: &Address, spender: &Address, allowance: i128) {
    env.storage().instance().set(
        &DataKey::Allowance(owner.clone(), spender.clone()),
        &allowance,
    );
}
