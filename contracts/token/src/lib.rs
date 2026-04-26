#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Symbol};

mod errors;
mod storage;
mod test;

#[cfg(test)]
mod test_utils;

pub use errors::*;

#[contract]
pub struct NavinToken;

#[contractimpl]
impl NavinToken {
    /// Initialize the token with admin, name, symbol, and total supply
    pub fn initialize(
        env: Env,
        admin: Address,
        name: String,
        symbol: String,
        total_supply: i128,
    ) -> Result<(), TokenError> {
        if storage::is_initialized(&env) {
            return Err(TokenError::AlreadyInitialized);
        }

        if total_supply <= 0 {
            return Err(TokenError::InvalidAmount);
        }

        storage::set_admin(&env, &admin);
        storage::set_name(&env, &name);
        storage::set_symbol(&env, &symbol);
        storage::set_total_supply(&env, total_supply);
        storage::set_balance(&env, &admin, total_supply);

        env.events()
            .publish((symbol_short!("init"),), (admin.clone(), total_supply));

        Ok(())
    }

    /// Get the token admin
    pub fn get_admin(env: Env) -> Result<Address, TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        Ok(storage::get_admin(&env))
    }

    /// Get token name
    pub fn name(env: Env) -> Result<String, TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        Ok(storage::get_name(&env))
    }

    /// Get token decimals
    pub fn decimals(_env: Env) -> Result<u32, TokenError> {
        Ok(7)
    }

    /// Get token symbol
    pub fn symbol(env: Env) -> Result<String, TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        Ok(storage::get_symbol(&env))
    }

    /// Get total supply
    pub fn total_supply(env: Env) -> Result<i128, TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        Ok(storage::get_total_supply(&env))
    }

    /// Get balance of an address
    pub fn balance(env: Env, address: Address) -> Result<i128, TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        Ok(storage::get_balance(&env, &address))
    }

    /// Transfer tokens from caller to recipient
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }

        from.require_auth();

        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }

        if from == to {
            return Err(TokenError::SameAccount);
        }

        let from_balance = storage::get_balance(&env, &from);
        if from_balance < amount {
            return Err(TokenError::InsufficientBalance);
        }

        // Update balances
        storage::set_balance(&env, &from, from_balance - amount);
        storage::set_balance(&env, &to, storage::get_balance(&env, &to) + amount);

        env.events()
            .publish((symbol_short!("transfer"),), (from, to, amount));

        Ok(())
    }

    /// Transfer tokens from one address to another with approval
    pub fn transfer_from(
        env: Env,
        spender: Address,
        from: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }

        spender.require_auth();

        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }

        if from == to {
            return Err(TokenError::SameAccount);
        }

        let allowance = storage::get_allowance(&env, &from, &spender);
        if allowance < amount {
            return Err(TokenError::InsufficientAllowance);
        }

        let from_balance = storage::get_balance(&env, &from);
        if from_balance < amount {
            return Err(TokenError::InsufficientBalance);
        }

        // Update balances and allowance
        storage::set_balance(&env, &from, from_balance - amount);
        storage::set_balance(&env, &to, storage::get_balance(&env, &to) + amount);
        storage::set_allowance(&env, &from, &spender, allowance - amount);

        env.events()
            .publish((symbol_short!("tr_from"),), (from, to, spender, amount));

        Ok(())
    }

    /// Approve an address to spend tokens on behalf of caller
    pub fn approve(
        env: Env,
        owner: Address,
        spender: Address,
        amount: i128,
    ) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }

        owner.require_auth();

        if amount < 0 {
            return Err(TokenError::InvalidAmount);
        }

        if owner == spender {
            return Err(TokenError::SameAccount);
        }

        storage::set_allowance(&env, &owner, &spender, amount);

        env.events()
            .publish((symbol_short!("approve"),), (owner, spender, amount));

        Ok(())
    }

    /// Get allowance of spender for owner's tokens
    pub fn allowance(env: Env, owner: Address, spender: Address) -> Result<i128, TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        Ok(storage::get_allowance(&env, &owner, &spender))
    }

    /// Mint new tokens (admin only)
    pub fn mint(env: Env, admin: Address, to: Address, amount: i128) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }

        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(TokenError::Unauthorized);
        }

        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }

        let current_supply = storage::get_total_supply(&env);
        storage::set_total_supply(&env, current_supply + amount);
        storage::set_balance(&env, &to, storage::get_balance(&env, &to) + amount);

        env.events().publish((symbol_short!("mint"),), (to, amount));

        Ok(())
    }

    /// Burn tokens (admin only)
    pub fn burn(env: Env, admin: Address, from: Address, amount: i128) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }

        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(TokenError::Unauthorized);
        }

        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }

        let from_balance = storage::get_balance(&env, &from);
        if from_balance < amount {
            return Err(TokenError::InsufficientBalance);
        }

        let current_supply = storage::get_total_supply(&env);
        storage::set_total_supply(&env, current_supply - amount);
        storage::set_balance(&env, &from, from_balance - amount);

        env.events()
            .publish((symbol_short!("burn"),), (from, amount));

        Ok(())
    }

    // ========================================================================
    // Metadata Allowlist Management (Admin Only)
    // ========================================================================

    /// Add a metadata key to the admin-registered allowlist.
    /// Only admin can register allowed keys.
    ///
    /// # Arguments
    /// * `admin` - Admin address authorizing the operation.
    /// * `key` - The metadata key to allow (e.g., "website", "twitter").
    ///
    /// # Errors
    /// * `MetadataError::NotInitialized` - If contract is not initialized.
    /// * `MetadataError::Unauthorized` - If caller is not admin.
    /// * `MetadataError::InvalidKey` - If key is empty.
    /// * `MetadataError::KeyAlreadyExists` - If key is already allowed.
    pub fn add_allowed_metadata_key(
        env: Env,
        admin: Address,
        key: Symbol,
    ) -> Result<(), MetadataError> {
        if !storage::is_initialized(&env) {
            return Err(MetadataError::NotInitialized);
        }

        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(MetadataError::Unauthorized);
        }

        // Validate key is not empty (compare with empty symbol)
        let empty_key = Symbol::new(&env, "");
        if key == empty_key {
            return Err(MetadataError::InvalidKey);
        }

        // Check if key is already allowed
        if storage::is_metadata_key_allowed(&env, &key) {
            return Err(MetadataError::KeyAlreadyExists);
        }

        storage::add_allowed_metadata_key(&env, &key);

        env.events()
            .publish((symbol_short!("meta_add"),), (admin, key));

        Ok(())
    }

    /// Remove a metadata key from the admin-registered allowlist.
    /// Only admin can remove allowed keys.
    ///
    /// # Arguments
    /// * `admin` - Admin address authorizing the operation.
    /// * `key` - The metadata key to remove from allowlist.
    ///
    /// # Errors
    /// * `MetadataError::NotInitialized` - If contract is not initialized.
    /// * `MetadataError::Unauthorized` - If caller is not admin.
    /// * `MetadataError::KeyNotFound` - If key is not in allowlist.
    pub fn remove_allowed_metadata_key(
        env: Env,
        admin: Address,
        key: Symbol,
    ) -> Result<(), MetadataError> {
        if !storage::is_initialized(&env) {
            return Err(MetadataError::NotInitialized);
        }

        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(MetadataError::Unauthorized);
        }

        // Check if key exists in allowlist
        if !storage::is_metadata_key_allowed(&env, &key) {
            return Err(MetadataError::KeyNotFound);
        }

        storage::remove_allowed_metadata_key(&env, &key);

        env.events()
            .publish((symbol_short!("meta_rm"),), (admin, key));

        Ok(())
    }

    /// Check if a metadata key is in the admin-registered allowlist.
    ///
    /// # Arguments
    /// * `key` - The metadata key to check.
    ///
    /// # Returns
    /// * `bool` - True if the key is allowed, false otherwise.
    pub fn is_metadata_key_allowed(env: Env, key: Symbol) -> Result<bool, MetadataError> {
        if !storage::is_initialized(&env) {
            return Err(MetadataError::NotInitialized);
        }

        Ok(storage::is_metadata_key_allowed(&env, &key))
    }

    // ========================================================================
    // Token Metadata Management (Admin Only)
    // ========================================================================

    /// Set a metadata key-value pair for the token.
    /// Only admin can set metadata, and only for allowed keys.
    ///
    /// # Arguments
    /// * `admin` - Admin address authorizing the operation.
    /// * `key` - The metadata key (must be in allowlist).
    /// * `value` - The metadata value.
    ///
    /// # Errors
    /// * `MetadataError::NotInitialized` - If contract is not initialized.
    /// * `MetadataError::Unauthorized` - If caller is not admin.
    /// * `MetadataError::KeyNotAllowed` - If key is not in allowlist.
    /// * `MetadataError::InvalidValue` - If value is empty.
    pub fn set_metadata(
        env: Env,
        admin: Address,
        key: Symbol,
        value: String,
    ) -> Result<(), MetadataError> {
        if !storage::is_initialized(&env) {
            return Err(MetadataError::NotInitialized);
        }

        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(MetadataError::Unauthorized);
        }

        // Validate key is in allowlist
        if !storage::is_metadata_key_allowed(&env, &key) {
            return Err(MetadataError::KeyNotAllowed);
        }

        // Validate value is not empty
        if value.is_empty() {
            return Err(MetadataError::InvalidValue);
        }

        storage::set_metadata(&env, &key, &value);

        env.events()
            .publish((symbol_short!("meta_set"),), (admin, key, value));

        Ok(())
    }

    /// Get a metadata value by key.
    ///
    /// # Arguments
    /// * `key` - The metadata key to retrieve.
    ///
    /// # Returns
    /// * `Option<String>` - The metadata value if exists, None otherwise.
    ///
    /// # Errors
    /// * `MetadataError::NotInitialized` - If contract is not initialized.
    pub fn get_metadata(env: Env, key: Symbol) -> Result<Option<String>, MetadataError> {
        if !storage::is_initialized(&env) {
            return Err(MetadataError::NotInitialized);
        }

        Ok(storage::get_metadata(&env, &key))
    }

    /// Remove a metadata key-value pair.
    /// Only admin can remove metadata.
    ///
    /// # Arguments
    /// * `admin` - Admin address authorizing the operation.
    /// * `key` - The metadata key to remove.
    ///
    /// # Errors
    /// * `MetadataError::NotInitialized` - If contract is not initialized.
    /// * `MetadataError::Unauthorized` - If caller is not admin.
    /// * `MetadataError::KeyNotFound` - If key does not exist.
    pub fn remove_metadata(env: Env, admin: Address, key: Symbol) -> Result<(), MetadataError> {
        if !storage::is_initialized(&env) {
            return Err(MetadataError::NotInitialized);
        }

        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(MetadataError::Unauthorized);
        }

        // Check if metadata exists
        if !storage::has_metadata(&env, &key) {
            return Err(MetadataError::KeyNotFound);
        }

        storage::remove_metadata(&env, &key);

        env.events()
            .publish((symbol_short!("meta_del"),), (admin, key));

        Ok(())
    }
}
