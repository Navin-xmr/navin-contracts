#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String};

mod errors;
mod storage;
mod test;

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
}
