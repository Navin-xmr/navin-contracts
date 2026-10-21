#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use soroban_sdk::{contract, contractimpl, Address, Env, String};

mod errors;
mod storage;

pub use errors::NftError;

#[cfg(any(test, feature = "testutils"))]
pub mod test_utils;

#[contract]
pub struct NavinNft;

#[contractimpl]
impl NavinNft {
    pub fn initialize(env: Env, admin: Address) -> Result<(), NftError> {
        if storage::is_initialized(&env) {
            return Err(NftError::AlreadyInitialized);
        }

        storage::set_admin(&env, &admin);
        Ok(())
    }

    pub fn set_base_uri(env: Env, admin: Address, base_uri: String) -> Result<(), NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }
        if admin != storage::get_admin(&env) {
            return Err(NftError::Unauthorized);
        }

        admin.require_auth();
        storage::set_base_uri(&env, &base_uri);
        Ok(())
    }

    pub fn mint(
        env: Env,
        admin: Address,
        to: Address,
        token_id: u64,
        metadata_uri: Option<String>,
    ) -> Result<(), NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }
        if admin != storage::get_admin(&env) {
            return Err(NftError::Unauthorized);
        }
        if storage::get_owner(&env, token_id).is_some() {
            return Err(NftError::AlreadyMinted);
        }
        if metadata_uri.is_none() && storage::get_base_uri(&env).is_none() {
            return Err(NftError::BaseUriNotSet);
        }

        admin.require_auth();
        storage::set_owner(&env, token_id, &to);
        if let Some(uri) = metadata_uri {
            storage::set_metadata_uri(&env, token_id, &uri);
        }
        Ok(())
    }

    pub fn owner_of(env: Env, token_id: u64) -> Result<Address, NftError> {
        storage::get_owner(&env, token_id).ok_or(NftError::TokenNotFound)
    }

    pub fn get_token_uri(env: Env, token_id: u64) -> Result<String, NftError> {
        if storage::get_owner(&env, token_id).is_none() {
            return Err(NftError::TokenNotFound);
        }

        if let Some(uri) = storage::get_metadata_uri(&env, token_id) {
            return Ok(uri);
        }

        let base_uri = storage::get_base_uri(&env).ok_or(NftError::BaseUriNotSet)?;
        Ok(append_token_id(&env, &base_uri, token_id))
    }
}

fn append_token_id(env: &Env, base_uri: &String, token_id: u64) -> String {
    let mut digits = [0u8; 20];
    let mut value = token_id;
    let mut start = digits.len();

    loop {
        start -= 1;
        digits[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }

    let mut uri = Vec::with_capacity(base_uri.len() as usize + digits.len() - start);
    uri.resize(base_uri.len() as usize, 0);
    base_uri.copy_into_slice(&mut uri);
    uri.extend_from_slice(&digits[start..]);
    String::from_bytes(env, &uri)
}

#[cfg(test)]
mod test;
