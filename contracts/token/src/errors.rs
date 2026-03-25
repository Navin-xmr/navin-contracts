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
