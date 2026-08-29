use soroban_sdk::{contracttype, symbol_short, Symbol};

use crate::errors::NavinError;

/// Broad category a contract error belongs to.
#[contracttype]
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
#[contracttype]
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
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractErrorInfo {
    /// Numeric discriminant as exposed on-chain.
    pub code: u32,
    pub category: ErrorCategory,
    pub retry: RetryGuidance,
    /// Short human-readable description suitable for operator logs / UI.
    pub message: Symbol,
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

    let (code, category, retry, _) = match error {
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
        NavinError::DependenciesNotMet => (
            54,
            InvalidState,
            RetryAfterStateChange,
            "Shipment cannot transition to InTransit or Delivered because its prerequisite shipments are not yet completed.",
        ),
        NavinError::CircularDependency => (
            55,
            InvalidInput,
            NoRetry,
            "A circular dependency was detected in the shipment prerequisites.",
        ),
        NavinError::ProposalSaltReused => (
            56,
            InvalidInput,
            NoRetry,
            "Proposal salt was already used in a prior proposal; replay attack prevented.",
        ),
        NavinError::InvalidShipmentParticipants => (
            57,
            InvalidInput,
            NoRetry,
            "Shipment sender, receiver, and carrier must be three distinct addresses.",
        ),
        NavinError::InvalidShipmentDeadline => (
            58,
            InvalidInput,
            NoRetry,
            "Shipment deadline must be strictly in the future.",
        ),
        NavinError::InvalidPaymentMilestones => (
            59,
            InvalidInput,
            NoRetry,
            "Payment milestone structure is invalid; each percentage must be 1-100.",
        ),
        NavinError::DuplicatePaymentMilestone => (
            60,
            InvalidInput,
            NoRetry,
            "Payment milestone checkpoint names must be unique.",
        ),
        NavinError::InvalidTokenAddress => (
            61,
            InvalidInput,
            NoRetry,
            "Shipment token address is invalid for this shipment.",
        ),
        NavinError::InvalidPaymentMilestoneName => (
            62,
            InvalidInput,
            NoRetry,
            "Payment milestone checkpoint name has an invalid format.",
        ),
        NavinError::MetadataSymbolCollision => (
            63,
            InvalidInput,
            NoRetry,
            "Metadata keys and values cannot be identical.",
        ),
        NavinError::ExternalIntegrationFailed => (
            64,
            Transient,
            RetryAfterDelay,
            "External integration failed.",
        ),
        NavinError::InvalidSymbol => (
            65,
            InvalidInput,
            NoRetry,
            "Symbol is invalid.",
        ),
        NavinError::NoteNotFound => (
            66,
            NotFound,
            NoRetry,
            "Note not found at the given index.",
        ),
        NavinError::EvidenceNotFound => (
            67,
            NotFound,
            NoRetry,
            "Evidence not found or index out of bounds.",
        ),
        NavinError::RoleAlreadyAssigned => (
            68,
            InvalidInput,
            NoRetry,
            "Address already holds the requested role.",
        ),
        NavinError::CarrierAlreadyWhitelisted => (
            69,
            InvalidState,
            NoRetry,
            "Carrier is already on the company's whitelist; duplicate addition is not allowed.",
        ),
        NavinError::InvalidAddress => (
            70,
            InvalidInput,
            NoRetry,
            "Address is invalid (e.g., zero-address sentinel).",
        ),
        NavinError::RecoveryLimitExceeded => (
            71,
            LimitExceeded,
            NoRetry,
            "Maximum allowed recovery action entries for a shipment has been reached.",
        ),
    };

    ContractErrorInfo {
        code,
        category,
        retry,
        message: message_for(error),
    }
}

fn message_for(error: NavinError) -> Symbol {
    match error {
        NavinError::AlreadyInitialized => symbol_short!("already"),
        NavinError::NotInitialized => symbol_short!("not_init"),
        NavinError::Unauthorized => symbol_short!("unauth"),
        NavinError::ShipmentNotFound => symbol_short!("shipment"),
        NavinError::InvalidStatus => symbol_short!("invalid"),
        NavinError::InvalidHash => symbol_short!("hash_err"),
        NavinError::EscrowLocked => symbol_short!("escrow"),
        NavinError::InsufficientFunds => symbol_short!("funds"),
        NavinError::ShipmentAlreadyCompleted => symbol_short!("complete"),
        NavinError::InvalidTimestamp => symbol_short!("timestamp"),
        NavinError::CounterOverflow => symbol_short!("overflow"),
        NavinError::InvalidAmount => symbol_short!("amount"),
        NavinError::ReentrancyDetected => symbol_short!("reentry"),
        NavinError::BatchTooLarge => symbol_short!("batch"),
        NavinError::InvalidShipmentInput => symbol_short!("input"),
        NavinError::MilestoneSumInvalid => symbol_short!("milestone"),
        NavinError::MilestoneAlreadyPaid => symbol_short!("paid"),
        NavinError::MetadataLimitExceeded => symbol_short!("metadata"),
        NavinError::RateLimitExceeded => symbol_short!("rate_lim"),
        NavinError::ProposalNotFound => symbol_short!("proposal"),
        NavinError::ProposalAlreadyExecuted => symbol_short!("executed"),
        NavinError::ProposalExpired => symbol_short!("expired"),
        NavinError::AlreadyApproved => symbol_short!("approved"),
        NavinError::InsufficientApprovals => symbol_short!("approvals"),
        NavinError::NotAnAdmin => symbol_short!("not_admin"),
        NavinError::InvalidMultiSigConfig => symbol_short!("multisig"),
        NavinError::NotExpired => symbol_short!("not_expire"),
        NavinError::ShipmentLimitReached => symbol_short!("ship_lim"),
        NavinError::InvalidConfig => symbol_short!("config"),
        NavinError::CannotSelfRevoke => symbol_short!("self_rev"),
        NavinError::CarrierSuspended => symbol_short!("carrier"),
        NavinError::ForceCancelReasonHashMissing => symbol_short!("cancel_h"),
        NavinError::ArithmeticError => symbol_short!("arithmetic"),
        NavinError::DisputeReasonHashMissing => symbol_short!("dispute_h"),
        NavinError::CompanySuspended => symbol_short!("company"),
        NavinError::ShipmentFinalized => symbol_short!("finalized"),
        NavinError::TokenTransferFailed => symbol_short!("transfer"),
        NavinError::TokenMintFailed => symbol_short!("mint"),
        NavinError::DuplicateAction => symbol_short!("duplicate"),
        NavinError::ShipmentUnavailable => symbol_short!("unavailable"),
        NavinError::ContractPaused => symbol_short!("paused"),
        NavinError::StatusHashNotFound => symbol_short!("status_h"),
        NavinError::DataHashMismatch => symbol_short!("data_hash"),
        NavinError::CircuitBreakerOpen => symbol_short!("circuit"),
        NavinError::InvalidMigrationEdge => symbol_short!("migration"),
        NavinError::MilestoneLimitExceeded => symbol_short!("mile_lim"),
        NavinError::NoteLimitExceeded => symbol_short!("note_lim"),
        NavinError::EvidenceLimitExceeded => symbol_short!("evidence"),
        NavinError::BreachLimitExceeded => symbol_short!("breach"),
        NavinError::InvalidTokenDecimals => symbol_short!("decimals"),
        NavinError::CreationQuotaExceeded => symbol_short!("quota"),
        NavinError::DependenciesNotMet => symbol_short!("depends"),
        NavinError::CircularDependency => symbol_short!("circular"),
        NavinError::ProposalSaltReused => symbol_short!("salt"),
        NavinError::InvalidShipmentParticipants => symbol_short!("parties"),
        NavinError::InvalidShipmentDeadline => symbol_short!("deadline"),
        NavinError::InvalidPaymentMilestones => symbol_short!("pay_mile"),
        NavinError::DuplicatePaymentMilestone => symbol_short!("dup_pay"),
        NavinError::InvalidTokenAddress => symbol_short!("token_ad"),
        NavinError::InvalidPaymentMilestoneName => symbol_short!("pay_name"),
        NavinError::MetadataSymbolCollision => symbol_short!("collision"),
        NavinError::ExternalIntegrationFailed => symbol_short!("external"),
        NavinError::InvalidSymbol => symbol_short!("symbol"),
        NavinError::NoteNotFound => symbol_short!("note"),
        NavinError::EvidenceNotFound => symbol_short!("evid_nf"),
        NavinError::RoleAlreadyAssigned => symbol_short!("role"),
        NavinError::CarrierAlreadyWhitelisted => symbol_short!("carrier"),
        NavinError::InvalidAddress => symbol_short!("address"),
        NavinError::RecoveryLimitExceeded => symbol_short!("recovery"),
    }
}

/// Returns the structured error metadata for the given numeric code.
///
/// Unknown or unsupported codes fall back to a generic `InvalidInput` /
/// `NoRetry` response instead of panicking so wallets and indexers can safely
/// query arbitrary contract error codes.
pub fn get_error_info(code: u32) -> ContractErrorInfo {
    match code {
        1 => error_info(NavinError::AlreadyInitialized),
        2 => error_info(NavinError::NotInitialized),
        3 => error_info(NavinError::Unauthorized),
        4 => error_info(NavinError::ShipmentNotFound),
        5 => error_info(NavinError::InvalidStatus),
        6 => error_info(NavinError::InvalidHash),
        7 => error_info(NavinError::EscrowLocked),
        8 => error_info(NavinError::InsufficientFunds),
        9 => error_info(NavinError::ShipmentAlreadyCompleted),
        10 => error_info(NavinError::InvalidTimestamp),
        11 => error_info(NavinError::CounterOverflow),
        14 => error_info(NavinError::InvalidAmount),
        15 => error_info(NavinError::ReentrancyDetected),
        16 => error_info(NavinError::BatchTooLarge),
        17 => error_info(NavinError::InvalidShipmentInput),
        18 => error_info(NavinError::MilestoneSumInvalid),
        19 => error_info(NavinError::MilestoneAlreadyPaid),
        20 => error_info(NavinError::MetadataLimitExceeded),
        21 => error_info(NavinError::RateLimitExceeded),
        22 => error_info(NavinError::ProposalNotFound),
        23 => error_info(NavinError::ProposalAlreadyExecuted),
        24 => error_info(NavinError::ProposalExpired),
        25 => error_info(NavinError::AlreadyApproved),
        26 => error_info(NavinError::InsufficientApprovals),
        27 => error_info(NavinError::NotAnAdmin),
        28 => error_info(NavinError::InvalidMultiSigConfig),
        29 => error_info(NavinError::NotExpired),
        30 => error_info(NavinError::ShipmentLimitReached),
        31 => error_info(NavinError::InvalidConfig),
        32 => error_info(NavinError::CannotSelfRevoke),
        33 => error_info(NavinError::CarrierSuspended),
        34 => error_info(NavinError::ForceCancelReasonHashMissing),
        35 => error_info(NavinError::ArithmeticError),
        36 => error_info(NavinError::DisputeReasonHashMissing),
        37 => error_info(NavinError::CompanySuspended),
        38 => error_info(NavinError::ShipmentFinalized),
        39 => error_info(NavinError::TokenTransferFailed),
        40 => error_info(NavinError::TokenMintFailed),
        41 => error_info(NavinError::DuplicateAction),
        42 => error_info(NavinError::ShipmentUnavailable),
        43 => error_info(NavinError::ContractPaused),
        44 => error_info(NavinError::StatusHashNotFound),
        45 => error_info(NavinError::DataHashMismatch),
        46 => error_info(NavinError::CircuitBreakerOpen),
        47 => error_info(NavinError::InvalidMigrationEdge),
        48 => error_info(NavinError::MilestoneLimitExceeded),
        49 => error_info(NavinError::NoteLimitExceeded),
        50 => error_info(NavinError::EvidenceLimitExceeded),
        51 => error_info(NavinError::BreachLimitExceeded),
        52 => error_info(NavinError::InvalidTokenDecimals),
        53 => error_info(NavinError::CreationQuotaExceeded),
        54 => error_info(NavinError::DependenciesNotMet),
        55 => error_info(NavinError::CircularDependency),
        56 => error_info(NavinError::ProposalSaltReused),
        57 => error_info(NavinError::InvalidShipmentParticipants),
        58 => error_info(NavinError::InvalidShipmentDeadline),
        59 => error_info(NavinError::InvalidPaymentMilestones),
        60 => error_info(NavinError::DuplicatePaymentMilestone),
        61 => error_info(NavinError::InvalidTokenAddress),
        62 => error_info(NavinError::InvalidPaymentMilestoneName),
        63 => error_info(NavinError::MetadataSymbolCollision),
        64 => error_info(NavinError::ExternalIntegrationFailed),
        65 => error_info(NavinError::InvalidSymbol),
        66 => error_info(NavinError::NoteNotFound),
        67 => error_info(NavinError::EvidenceNotFound),
        68 => error_info(NavinError::RoleAlreadyAssigned),
        69 => error_info(NavinError::CarrierAlreadyWhitelisted),
        70 => error_info(NavinError::InvalidAddress),
        71 => error_info(NavinError::RecoveryLimitExceeded),
        _ => ContractErrorInfo {
            code,
            category: ErrorCategory::InvalidInput,
            retry: RetryGuidance::NoRetry,
            message: symbol_short!("unknown"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::NavinError;

    #[test]
    fn test_get_error_info_known_code() {
        let info = get_error_info(39);
        assert_eq!(info.code, 39);
        assert_eq!(info.code, 39);
        assert_eq!(info.category, ErrorCategory::Transient);
        assert_eq!(info.retry, RetryGuidance::RetryAfterDelay);
        assert_eq!(info.message, symbol_short!("token"));
    }

    #[test]
    fn test_get_error_info_unknown_code_falls_back_gracefully() {
        let info = get_error_info(999_999);
        assert_eq!(info.code, 999_999);
        assert_eq!(info.category, ErrorCategory::InvalidInput);
        assert_eq!(info.retry, RetryGuidance::NoRetry);
        assert_eq!(info.message, symbol_short!("unknown"));
    }

    // ── Token transfer failure recovery — error mapping (issue #447) ─────────

    #[test]
    fn test_token_transfer_failed_info() {
        let info = error_info(NavinError::TokenTransferFailed);
        assert_eq!(info.code, 39);
        assert_eq!(info.category, ErrorCategory::Transient);
        assert_eq!(info.retry, RetryGuidance::RetryAfterDelay);
        assert_eq!(info.message, symbol_short!("token"));
    }

    #[test]
    fn test_circuit_breaker_open_info() {
        let info = error_info(NavinError::CircuitBreakerOpen);
        assert_eq!(info.code, 46);
        assert_eq!(info.category, ErrorCategory::Transient);
        assert_eq!(info.retry, RetryGuidance::RetryAfterDelay);
    }

    /// error_info must be deterministic — calling it twice on the same variant
    /// must return identical results.
    #[test]
    fn test_error_info_is_deterministic() {
        let a = error_info(NavinError::TokenTransferFailed);
        let b = error_info(NavinError::TokenTransferFailed);
        assert_eq!(a.code, b.code);
        assert_eq!(a.category, b.category);
        assert_eq!(a.retry, b.retry);
        assert_eq!(a.message, b.message);

        let c = error_info(NavinError::CircuitBreakerOpen);
        let d = error_info(NavinError::CircuitBreakerOpen);
        assert_eq!(c.code, d.code);
        assert_eq!(c.category, d.category);
        assert_eq!(c.retry, d.retry);
    }

    /// Token-related transient errors must use RetryAfterDelay, not NoRetry,
    /// so callers know they can retry after a backoff.
    #[test]
    fn test_token_and_circuit_breaker_errors_use_retry_after_delay() {
        let transient_errors = [
            NavinError::TokenTransferFailed,
            NavinError::TokenMintFailed,
            NavinError::CircuitBreakerOpen,
        ];
        for err in &transient_errors {
            let info = error_info(*err);
            assert_eq!(
                info.retry,
                RetryGuidance::RetryAfterDelay,
                "{:?} must have RetryAfterDelay guidance",
                err
            );
            assert_eq!(
                info.category,
                ErrorCategory::Transient,
                "{:?} must be categorised as Transient",
                err
            );
        }
    }

    /// Every error code in error_info must match its NavinError discriminant.
    #[test]
    fn test_error_codes_match_discriminants() {
        let cases: &[(NavinError, u32)] = &[
            (NavinError::TokenTransferFailed, 39),
            (NavinError::TokenMintFailed, 40),
            (NavinError::CircuitBreakerOpen, 46),
            (NavinError::ShipmentFinalized, 38),
            (NavinError::ShipmentNotFound, 4),
            (NavinError::Unauthorized, 3),
        ];
        for (err, expected_code) in cases {
            let info = error_info(*err);
            assert_eq!(
                info.code, *expected_code,
                "{:?} must map to code {}",
                err, expected_code
            );
        }
    }

    // ── #456: Auth mismatch error-mapping tests ──────────────────────────────

    /// `Unauthorized` is the primary domain error for callers with the wrong
    /// role.  It must map to `ErrorCategory::Unauthorized` with `NoRetry`
    /// guidance — the caller must fix their role before retrying.
    #[test]
    fn test_unauthorized_error_info() {
        let info = error_info(NavinError::Unauthorized);
        assert_eq!(info.code, 3);
        assert_eq!(info.category, ErrorCategory::Unauthorized);
        assert_eq!(info.retry, RetryGuidance::NoRetry);
        assert_eq!(info.message, symbol_short!("unauth"));
    }

    /// `NotAnAdmin` is returned by multi-sig entry points when the caller is
    /// not in the admin list.  It must map to `ErrorCategory::Unauthorized`
    /// with `NoRetry` — joining the admin list requires admin action, not a retry.
    #[test]
    fn test_not_an_admin_error_info() {
        let info = error_info(NavinError::NotAnAdmin);
        assert_eq!(info.code, 27);
        assert_eq!(info.category, ErrorCategory::Unauthorized);
        assert_eq!(info.retry, RetryGuidance::NoRetry);
        assert_eq!(info.message, symbol_short!("unknown"));
    }

    /// Auth-failure errors (`Unauthorized`, `NotAnAdmin`) must consistently
    /// map to `ErrorCategory::Unauthorized` so that error-handling middleware
    /// can classify them without switching on individual variants.
    #[test]
    fn test_auth_mismatch_errors_map_to_unauthorized_category() {
        let auth_errors = [NavinError::Unauthorized, NavinError::NotAnAdmin];
        for err in &auth_errors {
            let info = error_info(*err);
            assert_eq!(
                info.category,
                ErrorCategory::Unauthorized,
                "{:?} must be categorised as Unauthorized",
                err
            );
            assert_eq!(
                info.retry,
                RetryGuidance::NoRetry,
                "{:?} must have NoRetry guidance — wrong role cannot be fixed by retrying",
                err
            );
        }
    }

    /// `error_info` must be consistent: calling it twice on auth-related
    /// variants must return identical metadata.
    #[test]
    fn test_auth_error_info_is_deterministic() {
        let a = error_info(NavinError::Unauthorized);
        let b = error_info(NavinError::Unauthorized);
        assert_eq!(a.code, b.code);
        assert_eq!(a.category, b.category);
        assert_eq!(a.retry, b.retry);
        assert_eq!(a.message, b.message);

        let c = error_info(NavinError::NotAnAdmin);
        let d = error_info(NavinError::NotAnAdmin);
        assert_eq!(c.code, d.code);
        assert_eq!(c.category, d.category);
        assert_eq!(c.retry, d.retry);
        assert_eq!(c.message, d.message);
    }
}
