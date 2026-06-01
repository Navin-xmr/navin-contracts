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
#[contracterror(export = false)]
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
    //    /// Carrier is not listed in the company's whitelist.
    //    CarrierNotWhitelisted = 12,
    //    /// Carrier is not authorized to perform the action.
    //    CarrierNotAuthorized = 13,
    /// Amount provided is invalid (zero or negative).
    InvalidAmount = 14,
    /// Escrow operation attempted while the reentrancy lock is already active.
    ReentrancyDetected = 15,
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
    /// Admin cannot revoke their own role; use `transfer_admin` instead.
    CannotSelfRevoke = 32,
    /// Carrier account is suspended from carrier action handlers.
    CarrierSuspended = 33,
    /// Force-cancel requires a non-zero reason hash.
    ForceCancelReasonHashMissing = 34,
    /// Arithmetic overflow/underflow encountered during escrow math operations.
    ArithmeticError = 35,
    /// Dispute resolution requires a reason hash.
    DisputeReasonHashMissing = 36,
    /// Company account is suspended from creating or updating shipments.
    CompanySuspended = 37,
    /// Action rejected because the shipment is finalized and locked.
    ShipmentFinalized = 38,
    /// A cross-contract token transfer failed.
    TokenTransferFailed = 39,
    /// A cross-contract token mint failed.
    TokenMintFailed = 40,
    /// Action hash was already processed within the idempotency window.
    DuplicateAction = 41,
    /// Shipment state is unavailable due to archival or expiration.
    ShipmentUnavailable = 42,
    /// Contract is paused; state-changing operations are disabled.
    ContractPaused = 43,
    /// Status hash not found for the given shipment and status.
    StatusHashNotFound = 44,
    /// Data hash verification failed; provided hash does not match stored hash.
    DataHashMismatch = 45,
    /// Circuit breaker is open; token transfers are temporarily disabled.
    CircuitBreakerOpen = 46,
    /// Migration version transition is not allowed.
    InvalidMigrationEdge = 47,
    /// Maximum allowed milestone events for a shipment has been reached.
    MilestoneLimitExceeded = 48,
    /// Maximum allowed note events for a shipment has been reached.
    NoteLimitExceeded = 49,
    /// Maximum allowed evidence entries for a dispute has been reached.
    EvidenceLimitExceeded = 50,
    /// Maximum allowed condition breach events for a shipment has been reached.
    BreachLimitExceeded = 51,
    /// Token decimals do not match the expected value (7 for Stellar standard).
    /// Prevents mismatched amount interpretations across different token types.
    InvalidTokenDecimals = 52,
    /// Company has exceeded the shipment creation quota for the current time window.
    CreationQuotaExceeded = 53,
}
