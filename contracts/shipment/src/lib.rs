#![no_std]

use soroban_sdk::Error as SdkError;
use soroban_sdk::{contract, contractimpl, symbol_short, Address, BytesN, Env, Symbol};

mod errors;
mod events;
mod storage;
mod test;
mod types;

pub use errors::*;
pub use types::*;

/// Error types for the shipment contract
#[derive(Clone, Debug)]
pub enum NavinError {
    AlreadyInitialized,
    NotInitialized,
    Unauthorized,
    CounterOverflow,
    ShipmentNotFound,
}

impl From<NavinError> for SdkError {
    fn from(err: NavinError) -> Self {
        match err {
            NavinError::AlreadyInitialized => SdkError::from_contract_error(1),
            NavinError::NotInitialized => SdkError::from_contract_error(2),
            NavinError::Unauthorized => SdkError::from_contract_error(3),
            NavinError::CounterOverflow => SdkError::from_contract_error(4),
            NavinError::ShipmentNotFound => SdkError::from_contract_error(5),
        }
    }
}

#[contract]
pub struct NavinShipment;

fn require_initialized(env: &Env) -> Result<(), SdkError> {
    if !storage::is_initialized(env) {
        return Err(NavinError::NotInitialized.into());
    }
    Ok(())
}

fn require_role(env: &Env, address: &Address, role: Role) -> Result<(), SdkError> {
    if !storage::is_initialized(env) {
        return Err(NavinError::NotInitialized.into());
    }

    match role {
        Role::Company => {
            if storage::has_company_role(env, address) {
                Ok(())
            } else {
                Err(NavinError::Unauthorized.into())
            }
        }
    }
}

#[contractimpl]
impl NavinShipment {
    /// Initialize the contract with an admin address.
    /// Can only be called once. Sets the admin and shipment counter to 0.
    pub fn initialize(env: Env, admin: Address) -> Result<(), SdkError> {
        if storage::is_initialized(&env) {
            return Err(NavinError::AlreadyInitialized.into());
        }

        storage::set_admin(&env, &admin);
        storage::set_shipment_counter(&env, 0);
        storage::set_company_role(&env, &admin);

        env.events()
            .publish((symbol_short!("init"),), admin.clone());

        Ok(())
    }

    /// Get the contract admin address
    pub fn get_admin(env: Env) -> Result<Address, SdkError> {
        require_initialized(&env)?;
        Ok(storage::get_admin(&env))
    }

    /// Get the current shipment counter
    pub fn get_shipment_counter(env: Env) -> Result<u64, SdkError> {
        require_initialized(&env)?;
        Ok(storage::get_shipment_counter(&env))
    }

    /// Allow admin to grant Company role.
    pub fn add_company(env: Env, admin: Address, company: Address) -> Result<(), SdkError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized.into());
        }

        storage::set_company_role(&env, &company);
        Ok(())
    }

    /// Create a shipment and emit the shipment_created event.
    pub fn create_shipment(
        env: Env,
        sender: Address,
        receiver: Address,
        carrier: Address,
        data_hash: BytesN<32>,
    ) -> Result<u64, SdkError> {
        require_initialized(&env)?;
        sender.require_auth();
        require_role(&env, &sender, Role::Company)?;

        let shipment_id = storage::get_shipment_counter(&env)
            .checked_add(1)
            .ok_or(NavinError::CounterOverflow)?;
        let now = env.ledger().timestamp();

        let shipment = Shipment {
            id: shipment_id,
            sender: sender.clone(),
            receiver: receiver.clone(),
            carrier,
            data_hash: data_hash.clone(),
            status: ShipmentStatus::Created,
            created_at: now,
            updated_at: now,
        };

        storage::set_shipment(&env, &shipment);
        storage::set_shipment_counter(&env, shipment_id);

        env.events().publish(
            (Symbol::new(&env, "shipment_created"),),
            (shipment_id, sender, receiver, data_hash),
        );

        Ok(shipment_id)
    }

    /// Retrieve shipment details by ID.
    pub fn get_shipment(env: Env, shipment_id: u64) -> Result<Shipment, SdkError> {
        require_initialized(&env)?;
        storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound.into())
    }
}
