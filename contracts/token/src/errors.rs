use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    InsufficientBalance = 5,
    InsufficientAllowance = 6,
    SameAccount = 7,
    /// approve() was called with a non-zero amount and an expiration_ledger
    /// that has already passed (issue #659).
    InvalidExpirationLedger = 8,
    /// A state-changing call was rejected because the contract is paused
    /// (issue #657).
    ContractPaused = 9,
    /// Arithmetic overflow/underflow (checked_add/checked_sub failed).
    Overflow = 10,
}

#[contracterror]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataError {
    NotInitialized = 1,
    Unauthorized = 2,
    KeyNotAllowed = 3,
    KeyNotFound = 4,
    KeyAlreadyExists = 5,
    InvalidKey = 6,
    InvalidValue = 7,
}
