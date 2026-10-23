use soroban_sdk::contracterror;

/// Errors returned by the Navin proof-of-delivery NFT contract.
///
/// The numeric discriminants are part of the contract interface: they are what
/// an off-chain caller sees as `Error(Contract, #N)`. The values `1..=13` are
/// the original ones and must not be renumbered; variants added later are
/// appended with the next free value.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NftError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAdmin = 3,
    TokenDoesNotExist = 4,
    NotOwner = 5,
    TokenAlreadyMinted = 6,
    MintToContract = 7,
    ContractPaused = 8,
    InvalidNameOrSymbol = 9,
    BurnNotApproved = 10,
    ApprovalNotActive = 11,
    Overflow = 12,
    InvalidBatchLength = 13,
    /// The recipient contract refused the token (no `can_receive_nft`, or it
    /// answered `false`).
    TransferNotAllowed = 14,
    /// No admin transfer is pending.
    NoProposedAdmin = 15,
    /// Neither a per-token metadata URI nor a base URI is configured.
    BaseUriNotSet = 16,
}
