#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Error};

mod storage;
mod test;
mod types;

pub use storage::*;
pub use types::*;

/// Error types for the shipment contract
#[derive(Clone, Debug)]
pub enum NavinError {
    AlreadyInitialized,
    NotInitialized,
    Unauthorized,
    CarrierNotWhitelisted,
}

impl From<NavinError> for Error {
    fn from(err: NavinError) -> Self {
        match err {
            NavinError::AlreadyInitialized => Error::from_contract_error(1),
            NavinError::NotInitialized => Error::from_contract_error(2),
            NavinError::Unauthorized => Error::from_contract_error(3),
            NavinError::CarrierNotWhitelisted => Error::from_contract_error(4),
        }
    }
}

#[contract]
pub struct ShipmentContract;

#[contractimpl]
impl ShipmentContract {
    /// Initialize the contract with an admin address.
    /// Can only be called once. Sets the admin and shipment counter to 0.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if storage::is_initialized(&env) {
            return Err(NavinError::AlreadyInitialized.into());
        }

        storage::set_admin(&env, &admin);
        storage::set_shipment_counter(&env, 0);

        env.events()
            .publish((symbol_short!("init"),), admin.clone());

        Ok(())
    }

    /// Get the contract admin address
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        if !storage::is_initialized(&env) {
            return Err(NavinError::NotInitialized.into());
        }
        Ok(storage::get_admin(&env))
    }

    /// Get the current shipment counter
    pub fn get_shipment_counter(env: Env) -> Result<u64, Error> {
        if !storage::is_initialized(&env) {
            return Err(NavinError::NotInitialized.into());
        }
        Ok(storage::get_shipment_counter(&env))
    }

    /// Add a carrier to a company's whitelist
    /// Only the company can add carriers to their own whitelist
    pub fn add_carrier_to_whitelist(
        env: Env,
        company: Address,
        carrier: Address,
    ) -> Result<(), Error> {
        if !storage::is_initialized(&env) {
            return Err(NavinError::NotInitialized.into());
        }

        // Verify that the caller is the company
        company.require_auth();

        storage::add_carrier_to_whitelist(&env, &company, &carrier);

        env.events().publish(
            (symbol_short!("add_wl"),),
            (company.clone(), carrier.clone()),
        );

        Ok(())
    }

    /// Remove a carrier from a company's whitelist
    /// Only the company can remove carriers from their own whitelist
    pub fn remove_carrier_from_whitelist(
        env: Env,
        company: Address,
        carrier: Address,
    ) -> Result<(), Error> {
        if !storage::is_initialized(&env) {
            return Err(NavinError::NotInitialized.into());
        }

        // Verify that the caller is the company
        company.require_auth();

        storage::remove_carrier_from_whitelist(&env, &company, &carrier);

        env.events().publish(
            (symbol_short!("rm_wl"),),
            (company.clone(), carrier.clone()),
        );

        Ok(())
    }

    /// Check if a carrier is whitelisted for a company
    pub fn is_carrier_whitelisted(
        env: Env,
        company: Address,
        carrier: Address,
    ) -> Result<bool, Error> {
        if !storage::is_initialized(&env) {
            return Err(NavinError::NotInitialized.into());
        }

        Ok(storage::is_carrier_whitelisted(&env, &company, &carrier))
    }
}
