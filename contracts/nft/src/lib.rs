#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Symbol, Map, BytesN};

mod errors;
mod storage;
mod test;
mod test_cross_contract;

#[cfg(test)]
mod test_utils;

pub use errors::*;

#[contract]
pub struct NavinShipmentNft;

#[contractimpl]
impl NavinShipmentNft {
    /// Initialize the NFT contract with admin and collection metadata
    pub fn initialize(
        env: Env,
        admin: Address,
        name: String,
        symbol: String,
    ) -> Result<(), NftError> {
        if storage::is_initialized(&env) {
            return Err(NftError::AlreadyInitialized);
        }

        if name.is_empty() || symbol.is_empty() {
            return Err(NftError::InvalidInput);
        }

        storage::set_admin(&env, &admin);
        storage::set_name(&env, &name);
        storage::set_symbol(&env, &symbol);
        storage::set_next_token_id(&env, 1);

        env.events()
            .publish((symbol_short!("init"),), (admin.clone(), name, symbol));

        Ok(())
    }

    /// Mint a new NFT representing a shipment
    pub fn mint_shipment_nft(
        env: Env,
        to: Address,
        shipment_id: u64,
        data_hash: BytesN<32>,
        metadata: Map<Symbol, String>,
    ) -> Result<u64, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }

        // For now, anyone can mint. In future versions, this might be restricted to authorized contracts
        to.require_auth();

        let token_id = storage::get_next_token_id(&env);
        
        // Store NFT data
        storage::set_owner(&env, token_id, &to);
        storage::set_shipment_id(&env, token_id, shipment_id);
        storage::set_data_hash(&env, token_id, &data_hash);
        storage::set_metadata(&env, token_id, &metadata);
        
        // Update next token ID
        storage::set_next_token_id(&env, token_id + 1);

        env.events()
            .publish((symbol_short!("mint"),), (to, token_id, shipment_id));

        Ok(token_id)
    }

    /// Get the owner of a token
    pub fn owner_of(env: Env, token_id: u64) -> Result<Address, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }

        storage::get_owner(&env, token_id).ok_or(NftError::TokenNotFound)
    }

    /// Transfer a token from one address to another
    pub fn transfer(
        env: Env,
        from: Address,
        to: Address,
        token_id: u64,
    ) -> Result<(), NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }

        from.require_auth();

        let current_owner = storage::get_owner(&env, token_id).ok_or(NftError::TokenNotFound)?;
        
        if current_owner != from {
            return Err(NftError::Unauthorized);
        }

        storage::set_owner(&env, token_id, &to);

        env.events()
            .publish((symbol_short!("transfer"),), (from, to, token_id));

        Ok(())
    }

    /// Get shipment ID associated with a token
    pub fn get_shipment_id(env: Env, token_id: u64) -> Result<u64, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }

        storage::get_shipment_id(&env, token_id).ok_or(NftError::TokenNotFound)
    }

    /// Get data hash associated with a token
    pub fn get_data_hash(env: Env, token_id: u64) -> Result<BytesN<32>, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }

        storage::get_data_hash(&env, token_id).ok_or(NftError::TokenNotFound)
    }

    /// Get metadata for a token
    pub fn get_metadata(env: Env, token_id: u64) -> Result<Map<Symbol, String>, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }

        storage::get_metadata(&env, token_id).ok_or(NftError::TokenNotFound)
    }

    /// Get total supply of tokens
    pub fn total_supply(env: Env) -> Result<u64, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }

        Ok(storage::get_next_token_id(&env) - 1)
    }

    /// Get collection name
    pub fn name(env: Env) -> Result<String, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }
        Ok(storage::get_name(&env))
    }

    /// Get collection symbol
    pub fn symbol(env: Env) -> Result<String, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }
        Ok(storage::get_symbol(&env))
    }

    /// Get admin address
    pub fn get_admin(env: Env) -> Result<Address, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }
        Ok(storage::get_admin(&env))
    }

    /// Check if contract is initialized
    pub fn is_initialized(env: Env) -> bool {
        storage::is_initialized(&env)
    }
}