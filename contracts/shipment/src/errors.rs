use soroban_sdk::contracterror;

/// Domain-specific error type for the Navin shipment contract.
///
/// Each variant is assigned a unique `u32` discriminant starting from 1
/// so that the Soroban host can surface the code to clients without ambiguity.
///
/// # Examples
/// ```rust
/// use crate::errors::NavinError;
/// let error = NavinError::ShipmentNotFound;
/// ```
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum NavinError {
    /// Contract is already initialized.
    AlreadyInitialized = 1,
    /// Contract has not been initialized.
    NotInitialized = 2,
    /// Caller does not have the required permissions.
    Unauthorized = 3,
    /// Shipment ID doesn't exist.
    ShipmentNotFound = 4,
    /// Invalid state transition for the shipment.
    InvalidStatus = 5,
    /// Provided data hash does not match expectation.
    InvalidHash = 6,
    /// Escrow is locked and cannot be removed/modified.
    EscrowLocked = 7,
    /// Caller doesn't have sufficient funds for escrow deposit.
    InsufficientFunds = 8,
    /// Action cannot be performed on completed shipment (Delivered/Disputed).
    ShipmentAlreadyCompleted = 9,
    /// Invalid timestamp provided (e.g., ETA is in the past).
    InvalidTimestamp = 10,
    /// Counter value overflowed the maximum capacity.
    CounterOverflow = 11,
    /// Carrier is not listed in the company's whitelist.
    CarrierNotWhitelisted = 12,
    /// Carrier is not authorized to perform the action.
    CarrierNotAuthorized = 13,
    /// Amount provided is invalid (zero or negative).
    InvalidAmount = 14,
    /// Escrow for shipment has already been deposited.
    EscrowAlreadyDeposited = 15,
    /// Batch creation array exceeds maximum allowed item limit.
    BatchTooLarge = 16,
    /// Shipment input contained invalid parameters (e.g., receiver equals carrier).
    InvalidShipmentInput = 17,
    /// Milestone percentages do not sum to 100%.
    MilestoneSumInvalid = 18,
    /// Attempting to pay a milestone that was already paid.
    MilestoneAlreadyPaid = 19,
    /// Attempted to store more than the allowed maximum metadata entries (5).
    MetadataLimitExceeded = 20,
    /// Status update rejected because the minimum time interval has not elapsed.
    RateLimitExceeded = 21,
}
