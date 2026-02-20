#![no_std]

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Error, String, Symbol, Vec};

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
    EscrowNotFound,
    EscrowAlreadyExists,
    InvalidEscrowState,
}

// Implement conversion for VaultError to Soroban Error
impl From<VaultError> for Error {
    fn from(err: VaultError) -> Self {
        match err {
            VaultError::InsufficientFunds => Error::from_contract_error(1),
            VaultError::Unauthorized => Error::from_contract_error(2),
            VaultError::InvalidAmount => Error::from_contract_error(3),
            VaultError::AssetLocked => Error::from_contract_error(4),
            VaultError::EscrowNotFound => Error::from_contract_error(5),
            VaultError::EscrowAlreadyExists => Error::from_contract_error(6),
            VaultError::InvalidEscrowState => Error::from_contract_error(7),
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

    /// Create delivery escrow with auto-release timeout.
    pub fn create_delivery(
        env: Env,
        shipment_id: BytesN<32>,
        sender: Address,
        carrier: Address,
        receiver: Address,
        amount: i128,
        auto_release_after: u64,
    ) -> Result<(), Error> {
        sender.require_auth();

        if amount <= 0 || auto_release_after <= env.ledger().timestamp() {
            return Err(VaultError::InvalidAmount.into());
        }

        if env.storage().instance().has(&DataKey::Escrow(shipment_id.clone())) {
            return Err(VaultError::EscrowAlreadyExists.into());
        }

        let sender_balance = storage::get_balance(&env, &sender);
        if sender_balance < amount {
            return Err(VaultError::InsufficientFunds.into());
        }

        storage::update_balance(&env, &sender, sender_balance - amount);
        let escrow = DeliveryEscrow {
            carrier,
            receiver,
            amount,
            auto_release_after,
            status: DeliveryStatus::Pending,
        };
        env.storage()
            .instance()
            .set(&DataKey::Escrow(shipment_id), &escrow);

        Ok(())
    }

    /// Confirm delivery and release escrow to carrier.
    pub fn confirm_delivery(
        env: Env,
        shipment_id: BytesN<32>,
        receiver: Address,
    ) -> Result<(), Error> {
        receiver.require_auth();

        let mut escrow: DeliveryEscrow = env
            .storage()
            .instance()
            .get(&DataKey::Escrow(shipment_id.clone()))
            .ok_or(VaultError::EscrowNotFound)?;

        if escrow.receiver != receiver {
            return Err(VaultError::Unauthorized.into());
        }
        if escrow.status != DeliveryStatus::Pending {
            return Err(VaultError::InvalidEscrowState.into());
        }

        let carrier_balance = storage::get_balance(&env, &escrow.carrier);
        storage::update_balance(&env, &escrow.carrier, carrier_balance + escrow.amount);
        escrow.status = DeliveryStatus::Confirmed;
        env.storage()
            .instance()
            .set(&DataKey::Escrow(shipment_id), &escrow);

        Ok(())
    }

    /// Dispute delivery and keep escrow locked.
    pub fn dispute_delivery(
        env: Env,
        shipment_id: BytesN<32>,
        receiver: Address,
    ) -> Result<(), Error> {
        receiver.require_auth();

        let mut escrow: DeliveryEscrow = env
            .storage()
            .instance()
            .get(&DataKey::Escrow(shipment_id.clone()))
            .ok_or(VaultError::EscrowNotFound)?;

        if escrow.receiver != receiver {
            return Err(VaultError::Unauthorized.into());
        }
        if escrow.status != DeliveryStatus::Pending {
            return Err(VaultError::InvalidEscrowState.into());
        }

        escrow.status = DeliveryStatus::Disputed;
        env.storage()
            .instance()
            .set(&DataKey::Escrow(shipment_id), &escrow);

        Ok(())
    }

    /// Check if escrow timer is expired and auto-release if eligible.
    /// Returns true when release happens, false otherwise.
    pub fn check_auto_release(env: Env, shipment_id: BytesN<32>) -> Result<bool, Error> {
        let mut escrow: DeliveryEscrow = env
            .storage()
            .instance()
            .get(&DataKey::Escrow(shipment_id.clone()))
            .ok_or(VaultError::EscrowNotFound)?;

        if escrow.status != DeliveryStatus::Pending {
            return Ok(false);
        }
        let now = env.ledger().timestamp();
        if now < escrow.auto_release_after {
            return Ok(false);
        }

        let carrier_balance = storage::get_balance(&env, &escrow.carrier);
        storage::update_balance(&env, &escrow.carrier, carrier_balance + escrow.amount);
        escrow.status = DeliveryStatus::AutoReleased;
        env.storage()
            .instance()
            .set(&DataKey::Escrow(shipment_id.clone()), &escrow);

        env.events().publish(
            (Symbol::new(&env, "escrow_auto_released"), shipment_id),
            (escrow.carrier, escrow.amount, now),
        );

        Ok(true)
    }

    /// Retrieve delivery escrow details.
    pub fn get_delivery(env: Env, shipment_id: BytesN<32>) -> Result<DeliveryEscrow, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Escrow(shipment_id))
            .ok_or(VaultError::EscrowNotFound.into())
    }
}
