use crate::errors::NavinError;

/// Broad category a contract error belongs to.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ErrorCategory {
    /// Caller supplied bad input (wrong hash, invalid amount, etc.).
    InvalidInput,
    /// Caller lacks the required role or signature.
    Unauthorized,
    /// The requested resource does not exist.
    NotFound,
    /// The operation is not allowed in the current state.
    InvalidState,
    /// A resource limit or rate cap was hit.
    LimitExceeded,
    /// A transient infrastructure or arithmetic failure.
    Transient,
    /// Contract-level configuration or initialisation problem.
    Configuration,
}

/// Retry posture the caller should adopt after receiving this error.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RetryGuidance {
    /// Do not retry; fix the request before resubmitting.
    NoRetry,
    /// Retry after a short delay (network / rate-limit transient).
    RetryAfterDelay,
    /// Retry only after the on-chain state changes (e.g. wait for expiry).
    RetryAfterStateChange,
}

/// Structured metadata for a single `NavinError` variant.
#[derive(Copy, Clone, Debug)]
pub struct ContractErrorInfo {
    pub error: NavinError,
    /// Numeric discriminant as exposed on-chain.
    pub code: u32,
    pub category: ErrorCategory,
    pub retry: RetryGuidance,
    /// Short human-readable description suitable for operator logs / UI.
    pub message: &'static str,
}

/// Returns the `ContractErrorInfo` for the given `NavinError`.
///
/// Consumers (backends, frontends, indexers) call this to translate a raw
/// contract error code into a category and retry decision without hard-coding
/// the mapping themselves.
///
/// # Example
/// ```rust
/// use shipment::error_map::{error_info, RetryGuidance};
/// use shipment::errors::NavinError;
///
/// let info = error_info(NavinError::RateLimitExceeded);
/// assert_eq!(info.retry, RetryGuidance::RetryAfterDelay);
/// ```
pub fn error_info(error: NavinError) -> ContractErrorInfo {
    use ErrorCategory::*;
    use RetryGuidance::*;

    let (code, category, retry, message) = match error {
        NavinError::AlreadyInitialized => (
            1,
            Configuration,
            NoRetry,
            "Contract is already initialised; call init only once.",
        ),
        NavinError::NotInitialized => (
            2,
            Configuration,
            NoRetry,
            "Contract has not been initialised; call init first.",
        ),
        NavinError::Unauthorized => (
            3,
            Unauthorized,
            NoRetry,
            "Caller does not hold the required role or signature.",
        ),
        NavinError::ShipmentNotFound => (4, NotFound, NoRetry, "Shipment ID does not exist."),
        NavinError::InvalidStatus => (
            5,
            InvalidState,
            RetryAfterStateChange,
            "State transition is not allowed from the current shipment status.",
        ),
        NavinError::InvalidHash => (
            6,
            InvalidInput,
            NoRetry,
            "Provided data hash does not match the stored value.",
        ),
        NavinError::EscrowLocked => (
            7,
            InvalidState,
            RetryAfterStateChange,
            "Escrow is locked; wait for the shipment to reach a terminal state.",
        ),
        NavinError::InsufficientFunds => (
            8,
            InvalidInput,
            NoRetry,
            "Caller balance is too low to cover the escrow deposit.",
        ),
        NavinError::ShipmentAlreadyCompleted => (
            9,
            InvalidState,
            NoRetry,
            "Shipment is already in a terminal state (Delivered or Disputed).",
        ),
        NavinError::InvalidTimestamp => (
            10,
            InvalidInput,
            NoRetry,
            "Timestamp is invalid (e.g. ETA is in the past).",
        ),
        NavinError::CounterOverflow => (
            11,
            Transient,
            NoRetry,
            "Internal counter overflowed; contact the contract operator.",
        ),
        NavinError::InvalidAmount => (
            14,
            InvalidInput,
            NoRetry,
            "Amount must be a positive non-zero value.",
        ),
        NavinError::ReentrancyDetected => (
            15,
            InvalidState,
            RetryAfterDelay,
            "Reentrancy lock is active; retry once the current escrow operation completes.",
        ),
        NavinError::BatchTooLarge => (
            16,
            LimitExceeded,
            NoRetry,
            "Batch exceeds the maximum allowed item count; split into smaller batches.",
        ),
        NavinError::InvalidShipmentInput => (
            17,
            InvalidInput,
            NoRetry,
            "Shipment parameters are invalid (e.g. receiver equals carrier).",
        ),
        NavinError::MilestoneSumInvalid => (
            18,
            InvalidInput,
            NoRetry,
            "Milestone percentages must sum to exactly 100.",
        ),
        NavinError::MilestoneAlreadyPaid => (
            19,
            InvalidState,
            NoRetry,
            "This milestone has already been paid.",
        ),
        NavinError::MetadataLimitExceeded => (
            20,
            LimitExceeded,
            NoRetry,
            "Maximum of 5 metadata entries per shipment reached.",
        ),
        NavinError::RateLimitExceeded => (
            21,
            LimitExceeded,
            RetryAfterDelay,
            "Minimum interval between status updates has not elapsed; retry later.",
        ),
        NavinError::ProposalNotFound => (
            22,
            NotFound,
            NoRetry,
            "Multi-sig proposal ID does not exist.",
        ),
        NavinError::ProposalAlreadyExecuted => (
            23,
            InvalidState,
            NoRetry,
            "Proposal has already been executed.",
        ),
        NavinError::ProposalExpired => (
            24,
            InvalidState,
            NoRetry,
            "Proposal has expired; create a new proposal.",
        ),
        NavinError::AlreadyApproved => (
            25,
            InvalidState,
            NoRetry,
            "This admin has already approved the proposal.",
        ),
        NavinError::InsufficientApprovals => (
            26,
            InvalidState,
            RetryAfterStateChange,
            "Not enough admin approvals; wait for additional signers.",
        ),
        NavinError::NotAnAdmin => (
            27,
            Unauthorized,
            NoRetry,
            "Caller is not in the admin list.",
        ),
        NavinError::InvalidMultiSigConfig => (
            28,
            InvalidInput,
            NoRetry,
            "Multi-sig config is invalid (e.g. threshold exceeds admin count).",
        ),
        NavinError::NotExpired => (
            29,
            InvalidState,
            RetryAfterStateChange,
            "Shipment deadline has not yet passed; wait for expiry.",
        ),
        NavinError::ShipmentLimitReached => (
            30,
            LimitExceeded,
            RetryAfterStateChange,
            "Company has reached its active shipment cap; close existing shipments first.",
        ),
        NavinError::InvalidConfig => (
            31,
            InvalidInput,
            NoRetry,
            "Configuration parameters are invalid.",
        ),
        NavinError::CannotSelfRevoke => (
            32,
            InvalidInput,
            NoRetry,
            "An admin cannot revoke their own role; use transfer_admin instead.",
        ),
        NavinError::CarrierSuspended => (
            33,
            Unauthorized,
            RetryAfterStateChange,
            "Carrier account is suspended; contact the contract operator.",
        ),
        NavinError::ForceCancelReasonHashMissing => (
            34,
            InvalidInput,
            NoRetry,
            "Force-cancel requires a non-zero reason hash.",
        ),
        NavinError::ArithmeticError => (
            35,
            Transient,
            NoRetry,
            "Arithmetic overflow/underflow in escrow calculation; check amounts.",
        ),
        NavinError::DisputeReasonHashMissing => (
            36,
            InvalidInput,
            NoRetry,
            "Dispute resolution requires a non-zero reason hash.",
        ),
        NavinError::CompanySuspended => (
            37,
            Unauthorized,
            RetryAfterStateChange,
            "Company account is suspended; contact the contract operator.",
        ),
        NavinError::ShipmentFinalized => (
            38,
            InvalidState,
            NoRetry,
            "Shipment is finalised and locked; no further mutations are allowed.",
        ),
        NavinError::TokenTransferFailed => (
            39,
            Transient,
            RetryAfterDelay,
            "Cross-contract token transfer failed; retry after verifying token contract state.",
        ),
        NavinError::TokenMintFailed => (
            40,
            Transient,
            RetryAfterDelay,
            "Cross-contract token mint failed; retry after verifying token contract state.",
        ),
        NavinError::DuplicateAction => (
            41,
            InvalidInput,
            NoRetry,
            "Action hash was already processed within the idempotency window.",
        ),
        NavinError::ShipmentUnavailable => (
            42,
            InvalidState,
            RetryAfterStateChange,
            "Shipment state is unavailable (archived or expired); restore before retrying.",
        ),
        NavinError::ContractPaused => (
            43,
            InvalidState,
            RetryAfterStateChange,
            "Contract is paused; wait for the operator to resume operations.",
        ),
        NavinError::StatusHashNotFound => (
            44,
            NotFound,
            NoRetry,
            "No status hash found for the given shipment and status.",
        ),
        NavinError::DataHashMismatch => (
            45,
            InvalidInput,
            NoRetry,
            "Provided hash does not match the stored hash; recompute and resubmit.",
        ),
        NavinError::CircuitBreakerOpen => (
            46,
            Transient,
            RetryAfterDelay,
            "Circuit breaker is open; token transfers are temporarily disabled.",
        ),
        NavinError::InvalidMigrationEdge => (
            47,
            InvalidInput,
            NoRetry,
            "Migration version transition is not permitted.",
        ),
        NavinError::MilestoneLimitExceeded => (
            48,
            LimitExceeded,
            NoRetry,
            "Maximum milestone events per shipment reached.",
        ),
        NavinError::NoteLimitExceeded => (
            49,
            LimitExceeded,
            NoRetry,
            "Maximum note events per shipment reached.",
        ),
        NavinError::EvidenceLimitExceeded => (
            50,
            LimitExceeded,
            NoRetry,
            "Maximum evidence entries per dispute reached.",
        ),
        NavinError::BreachLimitExceeded => (
            51,
            LimitExceeded,
            NoRetry,
            "Maximum condition breach events per shipment reached.",
        ),
        NavinError::InvalidTokenDecimals => (
            52,
            InvalidInput,
            NoRetry,
            "Token decimals do not match the expected value (7); use a Stellar-standard token.",
        ),
        NavinError::CreationQuotaExceeded => (
            53,
            LimitExceeded,
            RetryAfterStateChange,
            "Company has exceeded the shipment creation quota for the current time window.",
        ),
    };

    ContractErrorInfo {
        error,
        code,
        category,
        retry,
        message,
    }
}
