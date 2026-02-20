use soroban_sdk::{contracttype, Address, BytesN};

#[contracttype]
pub enum DataKey {
    /// The contract admin address
    Admin,
    /// Counter tracking total shipments created
    ShipmentCounter,
    /// Addresses with Company role
    Company(Address),
    /// Individual shipment data keyed by ID
    Shipment(u64),
    /// Carrier whitelist for a company - (company, carrier) -> bool
    CarrierWhitelist(Address, Address),
}

/// Supported user roles
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    Company,
}

/// Shipment status lifecycle
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ShipmentStatus {
    Created,
    InTransit,
    Delivered,
    Cancelled,
}

/// Core shipment data
#[contracttype]
#[derive(Clone)]
pub struct Shipment {
    pub id: u64,
    pub sender: Address,
    pub receiver: Address,
    pub carrier: Address,
    pub data_hash: BytesN<32>,
    pub status: ShipmentStatus,
    pub created_at: u64,
    pub updated_at: u64,
}
