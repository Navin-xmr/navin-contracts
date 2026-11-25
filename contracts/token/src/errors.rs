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
    /// Empty token name or symbol supplied to initialize().
    InvalidMetadata = 11,
}

#[contracterror]
#[derive(Clone, Debug, Eq, PartialEq)]
// Disjoint from TokenError: Error(Contract, #N) must have exactly one meaning
// across every token entry point.
pub enum MetadataError {
    NotInitialized = 101,
    Unauthorized = 102,
    KeyNotAllowed = 103,
    KeyNotFound = 104,
    KeyAlreadyExists = 105,
    InvalidKey = 106,
    InvalidValue = 107,
}
