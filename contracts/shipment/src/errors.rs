use soroban_sdk::contracterror;

/// Domain-specific error type for the Navin shipment contract.
///
/// Each variant is assigned a unique `u32` discriminant starting from 1
/// so that the Soroban host can surface the code to clients without ambiguity.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum NavinError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ShipmentNotFound = 4,
    InvalidStatus = 5,
    InvalidHash = 6,
    EscrowLocked = 7,
    InsufficientFunds = 8,
    ShipmentAlreadyCompleted = 9,
    InvalidTimestamp = 10,
    CounterOverflow = 11,
}
