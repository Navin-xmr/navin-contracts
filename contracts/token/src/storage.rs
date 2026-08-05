use soroban_sdk::{contracttype, Address, Env, String, Symbol, Vec};

/// Storage keys for token contract data
#[contracttype]
pub enum DataKey {
    Admin,
    Name,
    Symbol,
    TotalSupply,
    Balance(Address),
    Allowance(Address, Address),
    /// Allowed metadata keys (admin-registered allowlist)
    AllowedMetadataKey(Symbol),
    /// Token metadata key-value pairs
    Metadata(Symbol),
    /// Contract-wide pause flag (issue #657)
    Paused,
}

/// An allowance amount plus the ledger sequence it expires on (issue #659),
/// matching the standard Soroban token interface's `approve`/`allowance`
/// shape. `u32::MAX` is used as the "never expires" sentinel.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowanceValue {
    pub amount: i128,
    pub expiration_ledger: u32,
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

/// Get the balance of an address from persistent storage.
pub fn get_balance(env: &Env, address: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(address.clone()))
        .unwrap_or(0)
}

/// Set the balance of an address in persistent storage.
pub fn set_balance(env: &Env, address: &Address, balance: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::Balance(address.clone()), &balance);
}

/// Get the allowance of a spender for an owner's tokens. Returns 0 once the
/// current ledger sequence has passed the stored `expiration_ledger`
/// (issue #659) without requiring an explicit reset.
pub fn get_allowance(env: &Env, owner: &Address, spender: &Address) -> i128 {
    let stored: Option<AllowanceValue> = env
        .storage()
        .persistent()
        .get(&DataKey::Allowance(owner.clone(), spender.clone()));

    match stored {
        Some(value) if env.ledger().sequence() <= value.expiration_ledger => value.amount,
        _ => 0,
    }
}

/// Get the raw stored allowance entry (amount + expiration_ledger), not
/// zeroed out on expiry. Used where the caller needs to distinguish "no
/// allowance was ever set" from "the allowance expired".
pub fn get_allowance_raw(env: &Env, owner: &Address, spender: &Address) -> Option<AllowanceValue> {
    env.storage()
        .persistent()
        .get(&DataKey::Allowance(owner.clone(), spender.clone()))
}

/// Set the allowance of a spender for an owner's tokens, along with the
/// ledger sequence it expires on (issue #659).
pub fn set_allowance(
    env: &Env,
    owner: &Address,
    spender: &Address,
    amount: i128,
    expiration_ledger: u32,
) {
    env.storage().persistent().set(
        &DataKey::Allowance(owner.clone(), spender.clone()),
        &AllowanceValue {
            amount,
            expiration_ledger,
        },
    );
}

/// Extend TTL for a single allowance entry.
pub fn extend_allowance_ttl(
    env: &Env,
    owner: &Address,
    spender: &Address,
    threshold: u32,
    extend_to: u32,
) {
    let key = DataKey::Allowance(owner.clone(), spender.clone());
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, threshold, extend_to);
    }
}

/// Extend TTL for a single balance entry.
pub fn extend_balance_ttl(env: &Env, address: &Address, threshold: u32, extend_to: u32) {
    let key = DataKey::Balance(address.clone());
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, threshold, extend_to);
    }
}

/// Extend TTL for several balance entries at once.
pub fn extend_balance_ttl_for(env: &Env, addresses: &[Address], threshold: u32, extend_to: u32) {
    for address in addresses {
        extend_balance_ttl(env, address, threshold, extend_to);
    }
}

// ============================================================================
// Pause Storage Functions (issue #657)
// ============================================================================

/// Check whether the contract is currently paused. Defaults to false
/// (unpaused) before pause() has ever been called.
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

/// Set the contract's paused flag.
pub fn set_paused(env: &Env, paused: bool) {
    env.storage().instance().set(&DataKey::Paused, &paused);
}

// ============================================================================
// Metadata Allowlist Storage Functions
// ============================================================================

/// Check if a metadata key is in the allowed list
pub fn is_metadata_key_allowed(env: &Env, key: &Symbol) -> bool {
    env.storage()
        .instance()
        .has(&DataKey::AllowedMetadataKey(key.clone()))
}

/// Add a key to the allowed metadata keys list
pub fn add_allowed_metadata_key(env: &Env, key: &Symbol) {
    env.storage()
        .instance()
        .set(&DataKey::AllowedMetadataKey(key.clone()), &true);
}

/// Remove a key from the allowed metadata keys list
pub fn remove_allowed_metadata_key(env: &Env, key: &Symbol) {
    env.storage()
        .instance()
        .remove(&DataKey::AllowedMetadataKey(key.clone()));
}

/// Get all allowed metadata keys
#[allow(dead_code)]
pub fn get_allowed_metadata_keys(env: &Env) -> Vec<Symbol> {
    // Note: This is a simplified implementation. In production, you might want
    // to use a different approach for iterating over all allowed keys.
    // For now, we'll return an empty Vec as iteration over dynamic keys
    // requires a separate index.
    Vec::new(env)
}

// ============================================================================
// Token Metadata Storage Functions
// ============================================================================

/// Set a metadata key-value pair
pub fn set_metadata(env: &Env, key: &Symbol, value: &String) {
    env.storage()
        .instance()
        .set(&DataKey::Metadata(key.clone()), value);
}

/// Get a metadata value by key
pub fn get_metadata(env: &Env, key: &Symbol) -> Option<String> {
    env.storage()
        .instance()
        .get(&DataKey::Metadata(key.clone()))
}

/// Remove a metadata key-value pair
pub fn remove_metadata(env: &Env, key: &Symbol) {
    env.storage()
        .instance()
        .remove(&DataKey::Metadata(key.clone()));
}

/// Check if a metadata key exists
pub fn has_metadata(env: &Env, key: &Symbol) -> bool {
    env.storage()
        .instance()
        .has(&DataKey::Metadata(key.clone()))
}
