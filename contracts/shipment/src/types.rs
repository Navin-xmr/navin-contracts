use soroban_sdk::{contracttype, Address, String};

/// Storage keys for the shipment contract
#[contracttype]
pub enum DataKey {
    /// The contract admin address
    Admin,
    /// Counter tracking total shipments created
    ShipmentCounter,
    /// Individual shipment data keyed by ID
    Shipment(u64),
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
    pub description: String,
    pub status: ShipmentStatus,
    pub created_at: u64,
    pub updated_at: u64,
}
