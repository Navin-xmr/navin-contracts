// Defines core types and enums for the Secure Asset Vault contract

use soroban_sdk::{contracttype, Address, BytesN, String};

/// Enumeration of possible storage keys
#[contracttype]
pub enum DataKey {
    /// Tracks authorized administrators
    Admins,
    /// Tracks asset balances for each address
    AssetBalance(Address),
    /// Tracks total vault balance
    TotalVaultBalance,
    /// Tracks locked assets for specific addresses
    LockedAssets(Address),
    /// Tracks withdrawal limits
    WithdrawalLimits(Address),
    /// Escrowed delivery state by shipment id
    Escrow(BytesN<32>),
}

/// Delivery escrow status used by timeout release flow
#[contracttype]
#[derive(PartialEq, Clone, Debug)]
pub enum DeliveryStatus {
    Pending,
    Confirmed,
    Disputed,
    AutoReleased,
}

/// Delivery escrow record keyed by shipment id
#[contracttype]
#[derive(Clone)]
pub struct DeliveryEscrow {
    pub carrier: Address,
    pub receiver: Address,
    pub amount: i128,
    pub auto_release_after: u64,
    pub status: DeliveryStatus,
}

/// Represents a lockup configuration for assets
#[contracttype]
#[derive(Clone)]
pub struct AssetLock {
    pub amount: i128,        // Locked amount
    pub release_time: u64,   // Timestamp when assets can be unlocked
    pub description: String, // Purpose of the lock
}

/// Tracks permission levels for different roles
#[contracttype]
#[derive(PartialEq, Clone)]
pub enum PermissionLevel {
    None,     // No permissions
    Viewer,   // Can view but not modify
    Operator, // Can perform limited actions
    Admin,    // Full control
}

/// Represents a transaction log entry
#[contracttype]
#[derive(Clone)]
pub struct TransactionLog {
    pub from: Address,
    pub to: Address,
    pub amount: i128,
    pub timestamp: u64,
    pub transaction_type: TransactionType,
}

/// Types of transactions for logging
#[contracttype]
#[derive(Clone)]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    Lock,
    Unlock,
    Transfer,
}
