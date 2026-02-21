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
<<<<<<< feat/shipment-status-transitions
    InvalidStatus = 7,
=======
    CarrierNotAuthorized = 7,
    InvalidStatus = 8,
>>>>>>> main
}
