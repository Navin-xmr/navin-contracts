use soroban_sdk::{contracttype, Address, BytesN, Map, Symbol, Vec};

/// Storage keys for contract data.
///
/// # Examples
/// ```rust
/// use crate::types::DataKey;
/// let key = DataKey::Admin;
/// ```
#[contracttype]
pub enum DataKey {
    /// The contract admin address.
    Admin,
    /// Contract version number, incremented on each upgrade.
    Version,
    /// Counter tracking total shipments created.
    ShipmentCount,
    /// Addresses with Company role.
    Company(Address),
    /// Addresses with Carrier role.
    Carrier(Address),
    /// Individual shipment data keyed by ID.
    Shipment(u64),
    /// Carrier whitelist for a company — (company, carrier) -> bool.
    CarrierWhitelist(Address, Address),
    /// Escrow balance for a shipment.
    Escrow(u64),
    /// Role assigned to an address.
    Role(Address),
    /// Hash of proof-of-delivery data for a shipment.
    ConfirmationHash(u64),
    /// Token contract address for payments.
    TokenContract,
    /// Timestamp of the last status update for a shipment (used for rate limiting).
    LastStatusUpdate(u64),
}

/// Supported user roles.
///
/// # Examples
/// ```rust
/// use crate::types::Role;
/// let role = Role::Company;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    /// A registered company that can create shipments.
    Company,
    /// A registered carrier that can transport shipments and report geofence events.
    Carrier,
    /// No role assigned.
    Unassigned,
}

/// Shipment status lifecycle.
///
/// # Examples
/// ```rust
/// use crate::types::ShipmentStatus;
/// let status = ShipmentStatus::Created;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ShipmentStatus {
    /// Shipment has been created but not yet picked up.
    Created,
    /// Shipment is in transit between checkpoints.
    InTransit,
    /// Shipment has arrived at an intermediate checkpoint.
    AtCheckpoint,
    /// Shipment has been delivered to the receiver.
    Delivered,
    /// Shipment is under dispute.
    Disputed,
    /// Shipment has been cancelled.
    Cancelled,
}

impl ShipmentStatus {
    /// Checks if a transition from the current status to a new status is valid.
    ///
    /// ### Status Transition Diagram
    /// ```text
    ///           +-----------+       +-----------+       +-----------+
    ///           |  Created  |------>| InTransit |<----->| AtCheckpt |
    ///           +-----------+       +-----------+       +-----------+
    ///                 |                   |                   |
    ///                 |           +-------+-------+-----------+
    ///                 |           |               |
    ///                 v           v               v
    ///           +-----------+-----------+   +-----------+
    ///           | Cancelled | Disputed  |<--| Delivered |
    ///           +-----------+-----------+   +-----------+
    ///                               |
    ///                               v
    ///                         (Terminal States)
    /// ```
    ///
    /// **Valid Transitions:**
    /// - `Created` -> `InTransit`, `Cancelled`
    /// - `InTransit` -> `AtCheckpoint`, `Delivered`, `Disputed`
    /// - `AtCheckpoint` -> `InTransit`, `Delivered`, `Disputed`
    /// - `Any` -> `Cancelled` (except `Delivered`)
    /// - `Any` -> `Disputed` (except `Cancelled`, `Delivered`)
    /// - `Disputed` -> `Cancelled`, `Delivered` (Special recovery cases if needed)
    ///
    /// # Arguments
    /// * `to` - The target status to transition to.
    ///
    /// # Returns
    /// * `bool` - `true` if the transition is valid, `false` otherwise.
    ///
    /// # Examples
    /// ```rust
    /// use crate::types::ShipmentStatus;
    /// let status = ShipmentStatus::Created;
    /// assert!(status.is_valid_transition(&ShipmentStatus::InTransit));
    /// ```
    pub fn is_valid_transition(&self, to: &ShipmentStatus) -> bool {
        match (self, to) {
            (Self::Created, Self::InTransit) => true,
            (Self::Created, Self::Cancelled) => true,
            (Self::Created, Self::Disputed) => true,
            (Self::InTransit, Self::AtCheckpoint) => true,
            (Self::InTransit, Self::Delivered) => true,
            (Self::InTransit, Self::Disputed) => true,
            (Self::InTransit, Self::Cancelled) => true,
            (Self::AtCheckpoint, Self::InTransit) => true,
            (Self::AtCheckpoint, Self::Delivered) => true,
            (Self::AtCheckpoint, Self::Disputed) => true,
            (Self::AtCheckpoint, Self::Cancelled) => true,
            (Self::Disputed, Self::Cancelled) => true,
            (Self::Disputed, Self::Delivered) => true,
            (_, Self::Cancelled) if self != &Self::Delivered => true,
            (_, Self::Disputed) if self != &Self::Cancelled && self != &Self::Delivered => true,
            _ => false,
        }
    }
}

/// Core shipment data stored on-chain.
/// Raw payload is off-chain; only the hash is stored.
///
/// # Examples
/// ```rust
/// // Struct represents the full shipment payload tracked on-chain.
/// ```
#[contracttype]
#[derive(Clone)]
pub struct Shipment {
    /// Unique shipment identifier.
    pub id: u64,
    /// Address that created the shipment.
    pub sender: Address,
    /// Intended recipient of the shipment.
    pub receiver: Address,
    /// Carrier responsible for transport.
    pub carrier: Address,
    /// Current status in the shipment lifecycle.
    pub status: ShipmentStatus,
    /// SHA-256 hash of the off-chain shipment data.
    pub data_hash: BytesN<32>,
    /// Ledger timestamp when the shipment was created.
    pub created_at: u64,
    /// Ledger timestamp of the last status update.
    pub updated_at: u64,
    /// Amount held in escrow for this shipment.
    pub escrow_amount: i128,
    /// Total amount deposited in escrow.
    pub total_escrow: i128,
    /// Optional metadata for storing small key-value pairs (e.g., weight category, priority).
    pub metadata: Option<Map<Symbol, Symbol>>,
    /// Milestone-based payment schedule: (checkpoint name, percentage).
    pub payment_milestones: Vec<(Symbol, u32)>,
    /// List of symbols for milestones that have already been paid.
    pub paid_milestones: Vec<Symbol>,
}

/// A checkpoint milestone recorded during shipment transit.
/// Only the data hash is stored; full details live off-chain.
///
/// # Examples
/// ```rust
/// // Struct represents a milestone reached by a shipment.
/// ```
#[contracttype]
#[derive(Clone)]
pub struct Milestone {
    /// ID of the shipment this milestone belongs to.
    pub shipment_id: u64,
    /// Symbolic name of the checkpoint (e.g. "warehouse", "port").
    pub checkpoint: Symbol,
    /// SHA-256 hash of the off-chain milestone data.
    pub data_hash: BytesN<32>,
    /// Ledger timestamp when the milestone was recorded.
    pub timestamp: u64,
    /// Address that reported this milestone.
    pub reporter: Address,
}

/// Condition breach types reported by carriers for out-of-range sensor readings.
///
/// # Examples
/// ```rust
/// use crate::types::BreachType;
/// let breach = BreachType::TemperatureHigh;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum BreachType {
    /// Temperature exceeded the upper acceptable limit.
    TemperatureHigh,
    /// Temperature dropped below the lower acceptable limit.
    TemperatureLow,
    /// Humidity exceeded the upper acceptable limit.
    HumidityHigh,
    /// Physical impact detected (shock/drop event).
    Impact,
    /// Tamper detection triggered.
    TamperDetected,
}

/// Geofence event types for tracking shipment location events.
///
/// # Examples
/// ```rust
/// use crate::types::GeofenceEvent;
/// let event = GeofenceEvent::ZoneEntry;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum GeofenceEvent {
    /// Shipment entered a predefined geographical zone.
    ZoneEntry,
    /// Shipment exited a predefined geographical zone.
    ZoneExit,
    /// Shipment deviated from the expected route.
    RouteDeviation,
}

/// Input data for creating a shipment in a batch.
///
/// # Examples
/// ```rust
/// // Struct represents batch creation parameters for a shipment.
/// ```
#[contracttype]
#[derive(Clone, Debug)]
pub struct ShipmentInput {
    pub receiver: Address,
    pub carrier: Address,
    pub data_hash: BytesN<32>,
    pub payment_milestones: Vec<(Symbol, u32)>,
}

/// On-chain introspection snapshot of the contract state.
///
/// # Examples
/// ```rust
/// // Struct holds metadata about the contract state itself.
/// ```
#[contracttype]
#[derive(Clone)]
pub struct ContractMetadata {
    /// Current contract version (starts at 1, incremented on each upgrade).
    pub version: u32,
    /// Address of the contract administrator.
    pub admin: Address,
    /// Total number of shipments created since initialization.
    pub shipment_count: u64,
    /// Whether the contract has been initialized.
    pub initialized: bool,
}

/// Dispute resolution options for admin.
///
/// # Examples
/// ```rust
/// use crate::types::DisputeResolution;
/// let resolution = DisputeResolution::RefundToCompany;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum DisputeResolution {
    /// Release escrowed funds to the carrier.
    ReleaseToCarrier,
    /// Refund escrowed funds to the company.
    RefundToCompany,
}
