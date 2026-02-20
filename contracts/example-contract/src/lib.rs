#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Env, Error, String, Vec};

mod storage;
mod test;
mod transactions;
mod types;

pub use storage::*;
pub use transactions::*;
pub use types::*;

/// Error types for the contract
#[derive(Clone, Debug)]
pub enum VaultError {
    InsufficientFunds,
    Unauthorized,
    InvalidAmount,
    AssetLocked,
    ShipmentNotFound,
    InsuranceAlreadyClaimed,
    InvalidShipmentStatus,
}

// Implement conversion for VaultError to Soroban Error
impl From<VaultError> for Error {
    fn from(err: VaultError) -> Self {
        match err {
            VaultError::InsufficientFunds => Error::from_contract_error(1),
            VaultError::Unauthorized => Error::from_contract_error(2),
            VaultError::InvalidAmount => Error::from_contract_error(3),
            VaultError::AssetLocked => Error::from_contract_error(4),
            VaultError::ShipmentNotFound => Error::from_contract_error(5),
            VaultError::InsuranceAlreadyClaimed => Error::from_contract_error(6),
            VaultError::InvalidShipmentStatus => Error::from_contract_error(7),
        }
    }
}

#[contract]
pub struct SecureAssetVault;

#[contractimpl]
impl SecureAssetVault {
    /// Initialize the vault with initial admin
    pub fn initialize(env: Env, initial_admin: Address) -> Result<(), Error> {
        // Prevent re-initialization
        if env.storage().instance().has(&DataKey::Admins) {
            return Err(VaultError::Unauthorized.into());
        }

        let mut admins = Vec::new(&env);
        admins.push_back(initial_admin);

        env.storage().instance().set(&DataKey::Admins, &admins);

        Ok(())
    }

    /// Deposit assets into the vault
    pub fn deposit(env: Env, from: Address, amount: i128) -> Result<(), Error> {
        from.require_auth();

        if amount <= 0 {
            return Err(VaultError::InvalidAmount.into());
        }

        let current_balance = storage::get_balance(&env, &from);
        storage::update_balance(&env, &from, current_balance + amount);

        transactions::log_transaction(&env, &from, &from, amount, TransactionType::Deposit);

        Ok(())
    }

    /// Withdraw assets from the vault
    pub fn withdraw(env: Env, from: Address, to: Address, amount: i128) -> Result<(), Error> {
        from.require_auth();

        let current_balance = storage::get_balance(&env, &from);

        if amount <= 0 {
            return Err(VaultError::InvalidAmount.into());
        }

        if current_balance < amount {
            return Err(VaultError::InsufficientFunds.into());
        }

        // Check for any locks
        let locks: Vec<AssetLock> = env
            .storage()
            .instance()
            .get(&DataKey::LockedAssets(from.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        let current_time = env.ledger().timestamp();
        let locked_amount: i128 = locks
            .iter()
            .filter(|lock| lock.release_time > current_time)
            .map(|lock| lock.amount)
            .sum();

        if current_balance - amount < locked_amount {
            return Err(VaultError::AssetLocked.into());
        }

        storage::update_balance(&env, &from, current_balance - amount);

        transactions::log_transaction(&env, &from, &to, amount, TransactionType::Withdrawal);

        Ok(())
    }

    /// Add a new admin (only callable by existing admins)
    pub fn add_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), Error> {
        storage::add_admin(&env, &caller, &new_admin);
        Ok(())
    }

    /// Lock assets for a specific duration
    pub fn lock_assets(
        env: Env,
        from: Address,
        amount: i128,
        release_time: u64,
        description: String,
    ) -> Result<(), Error> {
        from.require_auth();

        let current_balance = storage::get_balance(&env, &from);

        if amount <= 0 || amount > current_balance {
            return Err(VaultError::InvalidAmount.into());
        }

        let mut locks: Vec<AssetLock> = env
            .storage()
            .instance()
            .get(&DataKey::LockedAssets(from.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        let new_lock = AssetLock {
            amount,
            release_time,
            description,
        };

        locks.push_back(new_lock);

        env.storage()
            .instance()
            .set(&DataKey::LockedAssets(from.clone()), &locks);

        transactions::log_transaction(&env, &from, &from, amount, TransactionType::Lock);

        Ok(())
    }

    /// Retrieve current balance
    pub fn get_balance(env: Env, address: Address) -> i128 {
        storage::get_balance(&env, &address)
    }

    /// Create a new shipment with escrow
    pub fn create_shipment(
        env: Env,
        company: Address,
        receiver: Address,
        escrow_amount: i128,
    ) -> Result<u64, Error> {
        company.require_auth();

        if escrow_amount <= 0 {
            return Err(VaultError::InvalidAmount.into());
        }

        let shipment_id = env
            .storage()
            .instance()
            .get(&DataKey::NextShipmentId)
            .unwrap_or(1u64);

        let shipment = Shipment {
            id: shipment_id,
            company: company.clone(),
            receiver,
            escrow_amount,
            insurance_amount: 0,
            status: ShipmentStatus::Active,
        };

        env.storage()
            .instance()
            .set(&DataKey::Shipment(shipment_id), &shipment);
        env.storage()
            .instance()
            .set(&DataKey::NextShipmentId, &(shipment_id + 1));

        Ok(shipment_id)
    }

    /// Deposit insurance for a shipment
    pub fn deposit_insurance(
        env: Env,
        company: Address,
        shipment_id: u64,
        amount: i128,
    ) -> Result<(), Error> {
        company.require_auth();

        if amount <= 0 {
            return Err(VaultError::InvalidAmount.into());
        }

        let mut shipment: Shipment = env
            .storage()
            .instance()
            .get(&DataKey::Shipment(shipment_id))
            .ok_or(VaultError::ShipmentNotFound)?;

        if shipment.company != company {
            return Err(VaultError::Unauthorized.into());
        }

        shipment.insurance_amount += amount;
        env.storage()
            .instance()
            .set(&DataKey::Shipment(shipment_id), &shipment);

        let insurance = InsuranceDeposit {
            shipment_id,
            depositor: company.clone(),
            amount,
            claimed: false,
        };

        env.storage()
            .instance()
            .set(&DataKey::Insurance(shipment_id), &insurance);

        transactions::log_transaction(
            &env,
            &company,
            &company,
            amount,
            TransactionType::InsuranceDeposit,
        );

        env.events().publish(
            (String::from_str(&env, "insurance_deposited"),),
            (shipment_id, amount),
        );

        Ok(())
    }

    /// Claim insurance after dispute resolution (admin only)
    pub fn claim_insurance(
        env: Env,
        admin: Address,
        shipment_id: u64,
        claimant: Address,
    ) -> Result<(), Error> {
        admin.require_auth();

        if !storage::is_admin(&env, &admin) {
            return Err(VaultError::Unauthorized.into());
        }

        let mut insurance: InsuranceDeposit = env
            .storage()
            .instance()
            .get(&DataKey::Insurance(shipment_id))
            .ok_or(VaultError::ShipmentNotFound)?;

        if insurance.claimed {
            return Err(VaultError::InsuranceAlreadyClaimed.into());
        }

        let mut shipment: Shipment = env
            .storage()
            .instance()
            .get(&DataKey::Shipment(shipment_id))
            .ok_or(VaultError::ShipmentNotFound)?;

        if shipment.status != ShipmentStatus::Disputed {
            return Err(VaultError::InvalidShipmentStatus.into());
        }

        insurance.claimed = true;
        shipment.status = ShipmentStatus::InsuranceClaimed;

        env.storage()
            .instance()
            .set(&DataKey::Insurance(shipment_id), &insurance);
        env.storage()
            .instance()
            .set(&DataKey::Shipment(shipment_id), &shipment);

        transactions::log_transaction(
            &env,
            &shipment.company,
            &claimant,
            insurance.amount,
            TransactionType::InsuranceClaim,
        );

        env.events().publish(
            (String::from_str(&env, "insurance_claimed"),),
            (shipment_id, claimant, insurance.amount),
        );

        Ok(())
    }

    /// Mark shipment as disputed (for testing)
    pub fn mark_disputed(env: Env, admin: Address, shipment_id: u64) -> Result<(), Error> {
        admin.require_auth();

        if !storage::is_admin(&env, &admin) {
            return Err(VaultError::Unauthorized.into());
        }

        let mut shipment: Shipment = env
            .storage()
            .instance()
            .get(&DataKey::Shipment(shipment_id))
            .ok_or(VaultError::ShipmentNotFound)?;

        shipment.status = ShipmentStatus::Disputed;
        env.storage()
            .instance()
            .set(&DataKey::Shipment(shipment_id), &shipment);

        Ok(())
    }

    /// Get shipment details
    pub fn get_shipment(env: Env, shipment_id: u64) -> Result<Shipment, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Shipment(shipment_id))
            .ok_or(VaultError::ShipmentNotFound.into())
    }
}
