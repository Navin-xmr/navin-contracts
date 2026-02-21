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

fn require_initialized(env: &Env) -> Result<(), SdkError> {
    if !storage::is_initialized(env) {
        return Err(SdkError::from_contract_error(2));
    }
    Ok(())
}

fn require_role(env: &Env, address: &Address, role: Role) -> Result<(), SdkError> {
    require_initialized(env)?;

    match role {
        Role::Company => {
            if storage::has_company_role(env, address) {
                Ok(())
            } else {
                Err(SdkError::from_contract_error(3))
            }
        }
        Role::Carrier => {
            if storage::has_carrier_role(env, address) {
                Ok(())
            } else {
                Err(SdkError::from_contract_error(7))
            }
        }
    }
}

fn is_valid_transition(from: &ShipmentStatus, to: &ShipmentStatus) -> bool {
    use ShipmentStatus::*;

    matches!(
        (from, to),
        (Created, InTransit)
            | (Created, Cancelled)
            | (InTransit, AtCheckpoint)
            | (InTransit, Delivered)
            | (InTransit, Disputed)
            | (AtCheckpoint, InTransit)
            | (AtCheckpoint, Delivered)
            | (AtCheckpoint, Disputed)
            | (Delivered, Disputed)
            | (Disputed, Delivered)
            | (Disputed, Cancelled)
    )
}

#[contract]
pub struct NavinShipment;

#[contractimpl]
impl NavinShipment {
    /// Initialize the contract with an admin address.
    /// Can only be called once. Sets the admin and shipment counter to 0.
    pub fn initialize(env: Env, admin: Address) -> Result<(), SdkError> {
        if storage::is_initialized(&env) {
            return Err(SdkError::from_contract_error(1));
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

    /// Add a carrier to a company's whitelist
    /// Only the company can add carriers to their own whitelist
    pub fn add_carrier_to_whitelist(
        env: Env,
        company: Address,
        carrier: Address,
    ) -> Result<(), SdkError> {
        require_initialized(&env)?;
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
    ) -> Result<(), SdkError> {
        require_initialized(&env)?;
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
    ) -> Result<bool, SdkError> {
        require_initialized(&env)?;

        Ok(storage::is_carrier_whitelisted(&env, &company, &carrier))
    }

    /// Allow admin to grant Company role.
    pub fn add_company(env: Env, admin: Address, company: Address) -> Result<(), SdkError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(SdkError::from_contract_error(3));
        }

        storage::set_company_role(&env, &company);
        Ok(())
    }

    /// Allow admin to grant Carrier role.
    pub fn add_carrier(env: Env, admin: Address, carrier: Address) -> Result<(), SdkError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(SdkError::from_contract_error(3));
        }

        storage::set_carrier_role(&env, &carrier);
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
            .ok_or(SdkError::from_contract_error(5))?;
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
            escrow_amount: 0,
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
        storage::get_shipment(&env, shipment_id).ok_or(SdkError::from_contract_error(6))
    }

    /// Update shipment status with transition validation.
    /// Only the carrier or admin can update the status.
    pub fn update_status(
        env: Env,
        caller: Address,
        shipment_id: u64,
        new_status: ShipmentStatus,
        data_hash: BytesN<32>,
    ) -> Result<(), SdkError> {
        require_initialized(&env)?;
        caller.require_auth();

        let admin = storage::get_admin(&env);
        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(SdkError::from_contract_error(6))?;

        if caller != shipment.carrier && caller != admin {
            return Err(SdkError::from_contract_error(3));
        }

        if !is_valid_transition(&shipment.status, &new_status) {
            return Err(SdkError::from_contract_error(8));
        }

        let old_status = shipment.status.clone();
        shipment.status = new_status.clone();
        shipment.data_hash = data_hash.clone();
        shipment.updated_at = env.ledger().timestamp();

        storage::set_shipment(&env, &shipment);

        env.events().publish(
            (Symbol::new(&env, "status_updated"),),
            (shipment_id, old_status, new_status, data_hash),
        );

        Ok(())
    }

    /// Returns the current escrowed amount for a specific shipment.
    /// Returns 0 if no escrow has been deposited.
    /// Returns ShipmentNotFound if the shipment does not exist.
    pub fn get_escrow_balance(env: Env, shipment_id: u64) -> Result<i128, SdkError> {
        require_initialized(&env)?;
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(SdkError::from_contract_error(6));
        }
        Ok(storage::get_escrow_balance(&env, shipment_id))
    }

    /// Returns the total number of shipments created on the platform.
    /// Returns 0 if the contract has not been initialized.
    pub fn get_shipment_count(env: Env) -> u64 {
        storage::get_shipment_counter(&env)
    }

    /// Report a geofence event for a shipment.
    /// Only registered carriers can report geofence events.
    pub fn report_geofence_event(
        env: Env,
        carrier: Address,
        shipment_id: u64,
        zone_type: GeofenceEvent,
        data_hash: BytesN<32>,
    ) -> Result<(), SdkError> {
        require_initialized(&env)?;
        carrier.require_auth();
        require_role(&env, &carrier, Role::Carrier)?;

        // Verify shipment exists
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(SdkError::from_contract_error(6));
        }

        let timestamp = env.ledger().timestamp();

        env.events().publish(
            (Symbol::new(&env, "geofence_event"),),
            (shipment_id, zone_type, data_hash, timestamp),
        );

        Ok(())
    }

    /// Record a milestone for a shipment.
    /// Only registered carriers can record milestones.
    pub fn record_milestone(
        env: Env,
        carrier: Address,
        shipment_id: u64,
        checkpoint: Symbol,
        data_hash: BytesN<32>,
    ) -> Result<(), SdkError> {
        require_initialized(&env)?;
        carrier.require_auth();
        require_role(&env, &carrier, Role::Carrier)?;

        // Verify shipment exists and status
        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(SdkError::from_contract_error(6))?;

        if shipment.status != ShipmentStatus::InTransit {
            return Err(SdkError::from_contract_error(8));
        }

        let timestamp = env.ledger().timestamp();

        let _milestone = Milestone {
            shipment_id,
            checkpoint: checkpoint.clone(),
            data_hash: data_hash.clone(),
            timestamp,
            reporter: carrier.clone(),
        };

        // Do NOT store the milestone on-chain
        // Emit the milestone_recorded event (Hash-and-Emit pattern)
        env.events().publish(
            (Symbol::new(&env, "milestone_recorded"),),
            (shipment_id, checkpoint, data_hash, carrier),
        );

        Ok(())
    }
}
