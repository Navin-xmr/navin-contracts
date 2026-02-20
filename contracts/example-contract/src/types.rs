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
    /// Tracks shipments by ID
    Shipment(u64),
    /// Tracks insurance deposits by shipment ID
    Insurance(u64),
    /// Tracks next shipment ID
    NextShipmentId,
    /// Tracks carriers
    Carriers,
    /// Tracks actual shipment data from batch creation
    BatchShipment(u64),
}

/// Represents a lockup configuration for assets
#[contracttype]
#[derive(Clone)]
pub struct AssetLock {
    pub amount: i128,        // Locked amount
    pub release_time: u64,   // Timestamp when assets can be unlocked
    pub description: String, // Purpose of the lock
}

/// Represents input data for a new shipment
#[contracttype]
#[derive(Clone, Debug)]
pub struct ShipmentInput {
    pub receiver: Address,
    pub carrier: Address,
    pub data_hash: BytesN<32>,
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

/// Represents a stored batch shipment
#[contracttype]
#[derive(Clone, Debug)]
pub struct BatchShipment {
    pub id: u64,
    pub receiver: Address,
    pub carrier: Address,
    pub data_hash: BytesN<32>,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub enum ShipmentError {
    BatchTooLarge,
    InvalidShipment,
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
    InsuranceDeposit,
    InsuranceClaim,
}

/// Represents a shipment with insurance
#[contracttype]
#[derive(Clone)]
pub struct Shipment {
    pub id: u64,
    pub company: Address,
    pub receiver: Address,
    pub escrow_amount: i128,
    pub insurance_amount: i128,
    pub status: ShipmentStatus,
    pub data_hash: String,
    pub updated_at: u64,
}

/// Shipment status
#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum ShipmentStatus {
    Created,
    InTransit,
    Delivered,
    Active,
    Completed,
    Disputed,
    InsuranceClaimed,
}

/// Insurance deposit record
#[contracttype]
#[derive(Clone)]
pub struct InsuranceDeposit {
    pub shipment_id: u64,
    pub depositor: Address,
    pub amount: i128,
    pub claimed: bool,
}
