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
    /// Proposal ID doesn't exist.
    ProposalNotFound = 22,
    /// Proposal has already been executed.
    ProposalAlreadyExecuted = 23,
    /// Proposal has expired and can no longer be approved or executed.
    ProposalExpired = 24,
    /// Admin has already approved this proposal.
    AlreadyApproved = 25,
    /// Not enough approvals to execute the proposal.
    InsufficientApprovals = 26,
    /// Caller is not in the admin list.
    NotAnAdmin = 27,
    /// Invalid multi-sig configuration (e.g., threshold > admin count).
    InvalidMultiSigConfig = 28,
    /// Shipment deadline has not yet expired.
    NotExpired = 29,
    /// The company has reached its active shipment limit.
    ShipmentLimitReached = 30,
    /// Invalid configuration parameters provided.
    InvalidConfig = 31,
    /// Proposer's token balance (or delegated balance) is below min_proposal_tokens.
    InsufficientProposalTokens = 32,
    /// Approver cannot vote because their tokens are locked from a prior approval.
    VoteLockActive = 33,
    /// Approver had no voting power at the proposal's snapshot.
    NoVotingPowerAtSnapshot = 34,
}
