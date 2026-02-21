use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    CarrierNotWhitelisted = 4,
    CounterOverflow = 5,
    ShipmentNotFound = 6,
    CarrierNotAuthorized = 7,
    InvalidAmount = 8,
    InvalidShipmentStatus = 9,
    InvalidStatusTransition = 10,
    CannotSelfRevoke = 11,
    RoleNotAssigned = 12,
}
