use soroban_sdk::{contracttype, Address, Symbol};

/// Storage keys for token contract data
#[contracttype]
pub enum DataKey {
    Admin,
    PendingAdmin,
    Name,
    Symbol,
    TotalSupply,
    Balance(Address),
    Allowance(Address, Address),
    /// Allowed metadata keys (admin-registered allowlist)
    AllowedMetadataKey(Symbol),
    /// Ordered index of all allowed metadata keys
    AllowedMetadataKeys,
    /// Token metadata key-value pairs
    Metadata(Symbol),
    /// Contract-wide pause flag (issue #657)
    Paused,
}

/// An allowance amount plus the ledger sequence it expires on (issue #659),
/// matching the standard Soroban token interface's `approve`/`allowance`
/// shape. `u32::MAX` is used as the "never expires" sentinel.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowanceValue {
    pub amount: i128,
    pub expiration_ledger: u32,
}
