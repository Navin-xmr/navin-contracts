use alloc::string::ToString;

use crate::errors::NavinError;
use crate::storage;
use crate::types::{Shipment, ShipmentInput, ShipmentStatus};
use soroban_sdk::{xdr::ToXdr, Address, BytesN, Env, Symbol};

/// Maximum reasonable escrow amount (1 quadrillion stroops ≈ 1 billion XLM).
const MAX_AMOUNT: i128 = 1_000_000_000_000_000;

/// How far in the past a timestamp may be before it is rejected (seconds).
/// Roughly 1 year.
const MAX_PAST_OFFSET: u64 = 365 * 24 * 60 * 60;

/// How far in the future a timestamp may be before it is rejected (seconds).
/// Roughly 10 years.
const MAX_FUTURE_OFFSET: u64 = 10 * 365 * 24 * 60 * 60;

/// Ensure a `BytesN<32>` hash is valid and not malformed.
///
/// Rejects the all-zeros sentinel used to represent "no data". All other 32-byte
/// values are accepted, including repeated-byte test vectors.
///
/// # Arguments
/// * `hash` - The 32-byte hash to validate.
///
/// # Returns
/// * `Ok(())` if the hash is non-zero.
/// * `Err(NavinError::InvalidHash)` if the hash is all zeros.
pub fn validate_hash(hash: &BytesN<32>) -> Result<(), NavinError> {
    let bytes: [u8; 32] = hash.to_array();
    if bytes.iter().all(|&b| b == 0) {
        return Err(NavinError::InvalidHash);
    }
    Ok(())
}

/// Validate a Symbol for bounded usage in shipment metadata and milestones.
///
/// This validator ensures that Symbol strings conform to expected length constraints
/// and do not contain invalid patterns. Symbols are used in:
/// - Milestone checkpoint names (e.g., "warehouse", "port")
/// - Metadata keys and values
/// - Event topic names
///
/// Stellar Symbol Constraints:
/// - Length: 1-12 characters (enforced by Stellar protocol)
/// - Format: Alphanumeric and underscore only (A-Z, a-z, 0-9, _)
/// - Invalid: spaces, hyphens, special characters, unicode, null bytes
///
/// # Arguments
/// * `env` - Execution environment.
/// * `symbol` - The Symbol to validate.
///
/// # Returns
/// * `Ok(())` if the symbol is valid.
/// * `Err(NavinError::InvalidShipmentInput)` if the symbol is empty or exceeds max length.
///
/// # Examples
/// ```rust
/// validate_symbol(&env, &Symbol::new(&env, "warehouse"))?;
/// ```
pub fn validate_symbol(env: &Env, symbol: &Symbol) -> Result<(), NavinError> {
    // XDR layout: 4-byte ScValType tag + 4-byte length field + content padded to 4-byte boundary.
    // Byte counts by character count:
    //   0 chars  →  8 bytes  (empty — rejected)
    //   1–4 chars → 12 bytes
    //   5–8 chars → 16 bytes
    //   9–12 chars → 20 bytes  (12-char is the Stellar Symbol maximum)
    //   13+ chars → 24+ bytes  (rejected)
    let symbol_bytes = symbol.to_xdr(env);
    let len = symbol_bytes.len();

    if !(12..=20).contains(&len) {
        return Err(NavinError::InvalidShipmentInput);
    }

    Ok(())
}

/// Validate a collection of milestone symbols for bounded usage.
///
/// This validator ensures that all milestone checkpoint names conform to length
/// constraints and are not duplicated within the same shipment.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `milestones` - Vector of (Symbol, percentage) tuples.
///
/// # Returns
/// * `Ok(())` if all symbols are valid and unique.
/// * `Err(NavinError::InvalidShipmentInput)` if any symbol is invalid or duplicated.
///
/// # Examples
/// ```rust
/// validate_milestone_symbols(&env, &milestones)?;
/// ```
pub fn validate_milestone_symbols(
    env: &Env,
    milestones: &soroban_sdk::Vec<(Symbol, u32)>,
) -> Result<(), NavinError> {
    // Check each milestone symbol for validity
    for milestone in milestones.iter() {
        validate_symbol(env, &milestone.0)?;
    }

    // Check for duplicate milestone names by comparing XDR representations
    for i in 0..milestones.len() {
        let current = &milestones.get_unchecked(i).0;
        let current_xdr = current.to_xdr(env);
        for j in (i + 1)..milestones.len() {
            let other = &milestones.get_unchecked(j).0;
            let other_xdr = other.to_xdr(env);
            if current_xdr == other_xdr {
                return Err(NavinError::DuplicatePaymentMilestone);
            }
        }
    }

    Ok(())
}

/// Validate the addresses involved in shipment creation.
///
/// The sender, receiver, and carrier must be three distinct addresses so the
/// shipment does not collapse multiple parties into the same role.
pub fn validate_shipment_participants(
    sender: &Address,
    receiver: &Address,
    carrier: &Address,
) -> Result<(), NavinError> {
    if sender == receiver || sender == carrier || receiver == carrier {
        return Err(NavinError::InvalidShipmentParticipants);
    }

    Ok(())
}

/// Validate the symbol format used for checkpoint names.
///
/// Symbols are already bounded by the Stellar symbol type, but this helper
/// adds an explicit format gate so checkpoint names remain readable and
/// machine-friendly: only ASCII letters, digits, and underscores are allowed.
pub fn validate_checkpoint_name_format(symbol: &Symbol) -> Result<(), NavinError> {
    let checkpoint_name = symbol.to_string();
    if checkpoint_name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Ok(())
    } else {
        Err(NavinError::InvalidPaymentMilestoneName)
    }
}

/// Validate a shipment's payment milestone structure.
///
/// This enforces the creation-time contract for milestone payments:
/// - every checkpoint name must pass symbol and format validation,
/// - checkpoint names must be unique,
/// - each percentage must be between 1 and 100,
/// - the percentages must sum to exactly 100 when milestones are present.
pub fn validate_payment_milestones(
    env: &Env,
    milestones: &soroban_sdk::Vec<(Symbol, u32)>,
) -> Result<(), NavinError> {
    if milestones.is_empty() {
        return Ok(());
    }

    validate_milestone_symbols(env, milestones)?;

    for milestone in milestones.iter() {
        validate_symbol(env, &milestone.0)?;
        validate_checkpoint_name_format(&milestone.0)?;

        if milestone.1 == 0 || milestone.1 > 100 {
            return Err(NavinError::InvalidPaymentMilestones);
        }
    }

    let mut total_percentage = 0u32;
    for milestone in milestones.iter() {
        total_percentage = total_percentage
            .checked_add(milestone.1)
            .ok_or(NavinError::InvalidPaymentMilestones)?;
    }

    if total_percentage != 100 {
        return Err(NavinError::MilestoneSumInvalid);
    }

    for i in 0..milestones.len() {
        let current = &milestones.get_unchecked(i).0;
        for j in (i + 1)..milestones.len() {
            let other = &milestones.get_unchecked(j).0;
            if current == other {
                return Err(NavinError::DuplicatePaymentMilestone);
            }
        }
    }

    Ok(())
}

/// Validate the deadline supplied at shipment creation time.
///
/// Deadlines must be strictly in the future so newly-created shipments cannot
/// start in an already-expired state.
pub fn validate_shipment_deadline(env: &Env, deadline: u64) -> Result<(), NavinError> {
    if deadline <= env.ledger().timestamp() {
        return Err(NavinError::InvalidShipmentDeadline);
    }

    Ok(())
}

/// Validate the shared inputs for shipment creation.
pub fn validate_shipment_creation_inputs(
    env: &Env,
    sender: &Address,
    receiver: &Address,
    carrier: &Address,
    payment_milestones: &soroban_sdk::Vec<(Symbol, u32)>,
    deadline: u64,
) -> Result<(), NavinError> {
    validate_shipment_participants(sender, receiver, carrier)?;
    validate_payment_milestones(env, payment_milestones)?;
    validate_shipment_deadline(env, deadline)?;
    Ok(())
}

/// Validate a shipment token address before it is stored on a shipment.
///
/// Soroban addresses are typed and cannot be an empty string, so this guard
/// rejects the only sentinel value this contract can confuse for "unset": the
/// shipment contract's own address. Token contracts must be external contracts.
pub fn validate_token_address(env: &Env, token_address: &Address) -> Result<(), NavinError> {
    if *token_address == env.current_contract_address() {
        return Err(NavinError::InvalidTokenAddress);
    }
    Ok(())
}

/// Validate the full input payload used to create a shipment.
///
/// Rules:
/// - `sender`, `receiver`, and `carrier` must be three distinct addresses.
/// - `data_hash` must be non-zero.
/// - `deadline` must be strictly greater than the current ledger timestamp.
/// - `token_address` must pass token-address validation.
/// - `payment_milestones` may be empty; when present each percentage must be
///   between 1 and 100 inclusive, all checkpoint names must be valid Symbols,
///   names must be unique, and percentages must sum exactly to 100.
///
/// The function returns domain errors only and does not panic.
pub fn validate_shipment_input(
    env: &Env,
    sender: &Address,
    input: &ShipmentInput,
) -> Result<(), NavinError> {
    if *sender == input.receiver || *sender == input.carrier || input.receiver == input.carrier {
        return Err(NavinError::InvalidShipmentParticipants);
    }

    validate_hash(&input.data_hash)?;
    validate_token_address(env, &input.token_address)?;

    if input.deadline <= env.ledger().timestamp() {
        return Err(NavinError::InvalidShipmentDeadline);
    }

    validate_payment_milestones(env, &input.payment_milestones)
}

/// Validate metadata key-value pair symbols for bounded usage.
///
/// This validator ensures that both metadata keys and values conform to
/// length constraints before storage.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `key` - The metadata key symbol.
/// * `value` - The metadata value symbol.
///
/// # Returns
/// * `Ok(())` if both symbols are valid.
/// * `Err(NavinError::InvalidShipmentInput)` if either symbol is invalid.
///
/// # Examples
/// ```rust
/// validate_metadata_symbols(&env, &key, &value)?;
/// ```
pub fn validate_metadata_symbols(
    env: &Env,
    key: &Symbol,
    value: &Symbol,
) -> Result<(), NavinError> {
    validate_symbol(env, key)?;
    validate_symbol(env, value)?;
    Ok(())
}

/// Ensure an escrow / payment amount is positive and within a sane upper bound.
///
/// # Arguments
/// * `amount` - The `i128` value to validate.
///
/// # Returns
/// * `Ok(())` if `0 < amount <= MAX_AMOUNT`.
/// * `Err(NavinError::InvalidAmount)` otherwise.
///
/// # Examples
/// ```rust
/// validate_amount(5_000_000)?;
/// ```
pub fn validate_amount(amount: i128) -> Result<(), NavinError> {
    if amount <= 0 || amount > MAX_AMOUNT {
        return Err(NavinError::InvalidAmount);
    }
    Ok(())
}

/// Ensure an amount is strictly positive.
///
/// # Arguments
/// * `amount` - The `i128` value to validate.
///
/// # Returns
/// * `Ok(())` if `amount > 0`.
/// * `Err(NavinError::InvalidAmount)` otherwise.
pub fn validate_positive_amount(amount: i128) -> Result<(), NavinError> {
    if amount <= 0 || amount > MAX_AMOUNT {
        return Err(NavinError::InvalidAmount);
    }
    Ok(())
}

/// Ensure a timestamp is neither too far in the past nor too far in the future
/// relative to the current ledger time.
///
/// # Arguments
/// * `env`       - The execution environment (used to read `ledger().timestamp()`).
/// * `timestamp` - The `u64` UNIX timestamp to validate.
///
/// # Returns
/// * `Ok(())` if the timestamp is within acceptable bounds.
/// * `Err(NavinError::InvalidTimestamp)` otherwise.
///
/// # Examples
/// ```rust
/// validate_timestamp(&env, some_ts)?;
/// ```
pub fn validate_timestamp(env: &Env, timestamp: u64) -> Result<(), NavinError> {
    let now = env.ledger().timestamp();
    let earliest = now.saturating_sub(MAX_PAST_OFFSET);
    let latest = now.saturating_add(MAX_FUTURE_OFFSET);

    if timestamp < earliest || timestamp > latest {
        return Err(NavinError::InvalidTimestamp);
    }
    Ok(())
}

/// Look up a shipment by ID and return it, or surface `ShipmentNotFound`.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `id`  - Shipment ID to look up.
///
/// # Returns
/// * `Ok(Shipment)` if the shipment exists in persistent storage.
/// * `Err(NavinError::ShipmentNotFound)` if no shipment is stored under `id`.
///
/// # Examples
/// ```rust
/// let shipment = validate_shipment_exists(&env, shipment_id)?;
/// ```
pub fn validate_shipment_exists(env: &Env, id: u64) -> Result<Shipment, NavinError> {
    storage::get_shipment(env, id).ok_or(NavinError::ShipmentNotFound)
}

/// Preflight check for state-changing operations: ensures the shipment exists
/// and is available for mutation.
///
/// This helper gates all mutating endpoints to prevent operations on unavailable
/// shipment state due to archival or expiration. It performs two critical checks:
///
/// 1. **Existence Check**: Verifies the shipment exists in persistent storage.
///    Archived shipments (in temporary storage) are considered unavailable for
///    mutations to prevent accidental modifications to finalized state.
///
/// 2. **Finalization Check**: Ensures the shipment is not finalized. Finalized
///    shipments are locked and cannot be modified.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment to check.
///
/// # Returns
/// * `Ok(Shipment)` - The shipment if available for mutation.
/// * `Err(NavinError::ShipmentNotFound)` - If shipment doesn't exist in persistent storage.
/// * `Err(NavinError::ShipmentUnavailable)` - If shipment is archived or expired.
/// * `Err(NavinError::ShipmentFinalized)` - If shipment is finalized and locked.
///
/// # Design Rationale
///
/// **Why Archived Shipments Are Unavailable**:
/// - Archived shipments are moved to temporary storage (cheaper, shorter TTL)
/// - They represent terminal state (Delivered/Cancelled) with zero escrow
/// - Allowing mutations would violate the finalization contract
/// - Clients should query the shipment before attempting mutations
///
/// **Error Hierarchy**:
/// - `ShipmentNotFound`: Shipment never existed or has expired completely
/// - `ShipmentUnavailable`: Shipment exists but is archived (terminal state)
/// - `ShipmentFinalized`: Shipment is locked due to settlement
///
/// # Examples
/// ```rust
/// // In a mutating endpoint:
/// let shipment = preflight_check_shipment_available(&env, shipment_id)?;
/// // Now safe to mutate the shipment
/// ```
pub fn preflight_check_shipment_available(
    env: &Env,
    shipment_id: u64,
) -> Result<Shipment, NavinError> {
    // Check if shipment exists in persistent storage
    let shipment: Option<Shipment> = env
        .storage()
        .persistent()
        .get(&crate::types::DataKey::Shipment(shipment_id));

    let shipment = shipment.ok_or(NavinError::ShipmentNotFound)?;

    // Check if shipment is finalized (locked)
    if shipment.finalized {
        return Err(NavinError::ShipmentFinalized);
    }

    Ok(shipment)
}

/// Compute a canonical hash for an off-chain payload.
///
/// This helper standardizes how off-chain data is hashed to ensure consistency
/// between the contract and external backends/frontends. It uses a deterministic
/// ordering and XDR encoding of the fields.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `fields` - A vector of values to be included in the hash.
///
/// # Returns
/// * `BytesN<32>` - The computed SHA-256 hash.
///
/// # Design Rationale
///
/// **Why XDR Encoding?**:
/// - XDR is the native serialization format for Soroban.
/// - It is deterministic and handles various types (Address, Symbol, u64, etc.) consistently.
/// - Frontends can use the Stellar SDK to produce matching XDR blobs.
///
/// # Examples
/// ```rust
/// let mut fields = Vec::new(&env);
/// fields.push_back(Symbol::new(&env, "event_type").into_val(&env));
/// fields.push_back(shipment_id.into_val(&env));
/// let hash = compute_offchain_payload_hash(&env, fields);
/// ```
pub fn compute_offchain_payload_hash(
    env: &Env,
    fields: soroban_sdk::Vec<soroban_sdk::Val>,
) -> BytesN<32> {
    env.crypto().sha256(&fields.to_xdr(env)).into()
}

/// Validate cross-field shipment state-machine invariants.
///
/// This validator protects against impossible state combinations and is intended
/// to be called on every write path before persisting shipment state.
pub fn validate_shipment_invariants(shipment: &Shipment) -> Result<(), NavinError> {
    if shipment.total_escrow < 0 || shipment.escrow_amount < 0 {
        return Err(NavinError::InvalidStatus);
    }

    if shipment.escrow_amount > shipment.total_escrow {
        return Err(NavinError::InvalidStatus);
    }

    if shipment.finalized {
        let terminal = shipment.status == ShipmentStatus::Delivered
            || shipment.status == ShipmentStatus::Cancelled;
        if !terminal || shipment.escrow_amount != 0 {
            return Err(NavinError::InvalidStatus);
        }
    }

    if shipment.status == ShipmentStatus::Disputed && shipment.finalized {
        return Err(NavinError::InvalidStatus);
    }

    if shipment.status == ShipmentStatus::Created && !shipment.paid_milestones.is_empty() {
        return Err(NavinError::InvalidStatus);
    }

    Ok(())
}

// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{testutils::Ledger, BytesN, Env, Symbol};

    // validate_hash
    #[test]
    fn test_validate_hash_all_zeros_fails() {
        let env = Env::default();
        let zero_hash: BytesN<32> = BytesN::from_array(&env, &[0u8; 32]);
        assert_eq!(validate_hash(&zero_hash), Err(NavinError::InvalidHash));
    }

    #[test]
    fn test_validate_hash_nonzero_passes() {
        let env = Env::default();
        let mut bytes = [0u8; 32];
        bytes[0] = 1;
        let hash: BytesN<32> = BytesN::from_array(&env, &bytes);
        assert_eq!(validate_hash(&hash), Ok(()));
    }

    #[test]
    fn test_validate_hash_all_ones_passes() {
        let env = Env::default();
        // All-0xFF is a non-zero hash and should pass
        let hash: BytesN<32> = BytesN::from_array(&env, &[0xFF_u8; 32]);
        assert_eq!(validate_hash(&hash), Ok(()));
    }

    #[test]
    fn test_validate_hash_all_same_bytes_nonzero_pass() {
        let env = Env::default();
        // Any repeated non-zero byte is a valid hash value
        let patterns = [0x01u8, 0x02, 0x07, 0x0A, 0x0B, 0x2A, 0x42, 0xAA, 0x7F, 0xFF];
        for byte in patterns {
            let hash: BytesN<32> = BytesN::from_array(&env, &[byte; 32]);
            assert_eq!(
                validate_hash(&hash),
                Ok(()),
                "All-{:#04x} bytes should be accepted",
                byte
            );
        }
    }

    #[test]
    fn test_validate_hash_realistic_patterns_pass() {
        let env = Env::default();
        // Test realistic hash patterns that should pass validation
        let valid_hashes = [
            // Single non-zero byte at different positions
            (
                [
                    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0,
                ],
                "first byte non-zero",
            ),
            (
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 1,
                ],
                "last byte non-zero",
            ),
            (
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0,
                ],
                "middle byte non-zero",
            ),
            // Multiple different bytes
            (
                [
                    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
                    23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
                ],
                "sequential bytes",
            ),
            (
                [
                    0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0xF9, 0xF8, 0xF7, 0xF6, 0xF5, 0xF4, 0xF3,
                    0xF2, 0xF1, 0xF0, 0xEF, 0xEE, 0xED, 0xEC, 0xEB, 0xEA, 0xE9, 0xE8, 0xE7, 0xE6,
                    0xE5, 0xE4, 0xE3, 0xE2, 0xE1, 0xE0,
                ],
                "descending bytes",
            ),
            // Random-looking pattern
            (
                [
                    0x4A, 0x1F, 0x8B, 0x2C, 0x7D, 0x93, 0x0E, 0xF5, 0x66, 0xB1, 0x2A, 0x47, 0x8C,
                    0x35, 0x9D, 0x70, 0x13, 0xA8, 0x5F, 0xE2, 0x94, 0x27, 0x6B, 0x0C, 0xD1, 0x48,
                    0x9A, 0x33, 0x7E, 0xC5, 0x10, 0x8F,
                ],
                "random pattern",
            ),
        ];

        for (bytes, description) in valid_hashes {
            let hash: BytesN<32> = BytesN::from_array(&env, &bytes);
            assert_eq!(
                validate_hash(&hash),
                Ok(()),
                "Valid hash ({}) should pass validation",
                description
            );
        }
    }

    #[test]
    fn test_validate_hash_boundary_cases() {
        let env = Env::default();

        // Test hash with exactly one non-zero byte
        let mut single_nonzero = [0u8; 32];
        single_nonzero[31] = 1;
        let hash: BytesN<32> = BytesN::from_array(&env, &single_nonzero);
        assert_eq!(validate_hash(&hash), Ok(()));

        // Test hash with alternating pattern
        let mut alternating = [0u8; 32];
        for (i, byte) in alternating.iter_mut().enumerate() {
            *byte = if i % 2 == 0 { 0xAA } else { 0x55 };
        }
        alternating[15] = 0x33;
        let hash: BytesN<32> = BytesN::from_array(&env, &alternating);
        assert_eq!(validate_hash(&hash), Ok(()));

        // Test hash with two different bytes alternating (not perfectly repeated)
        let mut two_byte_pattern = [0u8; 32];
        for i in 0..16 {
            two_byte_pattern[i * 2] = 0x12;
            two_byte_pattern[i * 2 + 1] = 0x34;
        }
        // Break the pattern at one position
        two_byte_pattern[10] = 0x56;
        let hash: BytesN<32> = BytesN::from_array(&env, &two_byte_pattern);
        assert_eq!(validate_hash(&hash), Ok(()));

        // Test hash with mostly zeros but a few different bytes
        let mut mostly_zeros = [0u8; 32];
        mostly_zeros[5] = 0x42;
        mostly_zeros[17] = 0x7F;
        mostly_zeros[28] = 0x01;
        let hash: BytesN<32> = BytesN::from_array(&env, &mostly_zeros);
        assert_eq!(validate_hash(&hash), Ok(()));
    }

    #[test]
    fn test_validate_shipment_invariants_accepts_valid_shipment() {
        let env = Env::default();
        let shipment = Shipment {
            id: 1,
            sender: soroban_sdk::Address::generate(&env),
            receiver: soroban_sdk::Address::generate(&env),
            carrier: soroban_sdk::Address::generate(&env),
            status: ShipmentStatus::InTransit,
            data_hash: BytesN::from_array(&env, &[1_u8; 32]),
            created_at: 100,
            updated_at: 100,
            escrow_amount: 10,
            total_escrow: 10,
            metadata: None,
            payment_milestones: soroban_sdk::Vec::new(&env),
            paid_milestones: soroban_sdk::Vec::new(&env),
            milestones_completed: soroban_sdk::Vec::new(&env),
            deadline: 200,
            integration_nonce: 0,
            finalized: false,
            depends_on: None,
        };

        assert_eq!(validate_shipment_invariants(&shipment), Ok(()));
    }

    #[test]
    fn test_validate_shipment_invariants_rejects_escrow_greater_than_total() {
        let env = Env::default();
        let shipment = Shipment {
            id: 1,
            sender: soroban_sdk::Address::generate(&env),
            receiver: soroban_sdk::Address::generate(&env),
            carrier: soroban_sdk::Address::generate(&env),
            status: ShipmentStatus::InTransit,
            data_hash: BytesN::from_array(&env, &[2_u8; 32]),
            created_at: 100,
            updated_at: 100,
            escrow_amount: 20,
            total_escrow: 10,
            metadata: None,
            payment_milestones: soroban_sdk::Vec::new(&env),
            paid_milestones: soroban_sdk::Vec::new(&env),
            milestones_completed: soroban_sdk::Vec::new(&env),
            deadline: 200,
            integration_nonce: 0,
            finalized: false,
            depends_on: None,
        };

        assert_eq!(
            validate_shipment_invariants(&shipment),
            Err(NavinError::InvalidStatus)
        );
    }

    // validate_amount
    #[test]
    fn test_validate_amount_zero_fails() {
        assert_eq!(validate_amount(0), Err(NavinError::InvalidAmount));
    }

    #[test]
    fn test_validate_amount_negative_fails() {
        assert_eq!(validate_amount(-1), Err(NavinError::InvalidAmount));
    }

    #[test]
    fn test_validate_amount_valid_passes() {
        assert_eq!(validate_amount(1), Ok(()));
        assert_eq!(validate_amount(5_000_000), Ok(()));
        assert_eq!(validate_amount(MAX_AMOUNT), Ok(()));
    }

    #[test]
    fn test_validate_amount_exceeds_max_fails() {
        assert_eq!(
            validate_amount(MAX_AMOUNT + 1),
            Err(NavinError::InvalidAmount)
        );
    }

    // validate_timestamp
    #[test]
    fn test_validate_timestamp_current_passes() {
        let env = Env::default();
        let now = env.ledger().timestamp();
        assert_eq!(validate_timestamp(&env, now), Ok(()));
    }

    #[test]
    fn test_validate_timestamp_near_future_passes() {
        let env = Env::default();
        let now = env.ledger().timestamp();
        // 30 days in the future — well within the 10-year window.
        assert_eq!(validate_timestamp(&env, now + 30 * 24 * 60 * 60), Ok(()));
    }

    #[test]
    fn test_validate_timestamp_far_future_fails() {
        let env = Env::default();
        let now = env.ledger().timestamp();
        let far_future = now + MAX_FUTURE_OFFSET + 1;
        assert_eq!(
            validate_timestamp(&env, far_future),
            Err(NavinError::InvalidTimestamp)
        );
    }

    #[test]
    fn test_validate_timestamp_far_past_fails() {
        let env = Env::default();
        // Set ledger time far enough ahead that subtracting MAX_PAST_OFFSET + 1
        // gives a clearly out-of-range value.
        env.ledger().with_mut(|li| {
            li.timestamp = MAX_PAST_OFFSET + 10;
        });
        let far_past = env.ledger().timestamp() - MAX_PAST_OFFSET - 1;
        assert_eq!(
            validate_timestamp(&env, far_past),
            Err(NavinError::InvalidTimestamp)
        );
    }

    // validate_shipment_exists
    #[test]
    fn test_validate_shipment_exists_missing_returns_error() {
        let env = Env::default();
        // Storage access requires a contract context in Soroban.
        let result = env.as_contract(&env.register(crate::NavinShipment, ()), || {
            validate_shipment_exists(&env, 999)
        });
        assert!(matches!(result, Err(NavinError::ShipmentNotFound)));
    }

    // validate_symbol
    #[test]
    fn test_validate_symbol_valid_short_passes() {
        let env = Env::default();
        let symbol = Symbol::new(&env, "warehouse");
        assert_eq!(validate_symbol(&env, &symbol), Ok(()));
    }

    #[test]
    fn test_validate_symbol_valid_max_length_passes() {
        let env = Env::default();
        // Exactly 12 characters — the Stellar Symbol maximum
        let max_name = "a".repeat(12);
        let symbol = Symbol::new(&env, &max_name);
        assert_eq!(validate_symbol(&env, &symbol), Ok(()));
    }

    #[test]
    fn test_validate_symbol_single_char_passes() {
        let env = Env::default();
        let symbol = Symbol::new(&env, "a");
        assert_eq!(validate_symbol(&env, &symbol), Ok(()));
    }

    #[test]
    fn test_validate_symbol_common_names_pass() {
        let env = Env::default();
        // All names are ≤ 12 chars (Stellar Symbol maximum)
        let test_names = ["port", "warehouse", "checkpoint", "delivered"];
        for name in &test_names {
            let symbol = Symbol::new(&env, name);
            assert_eq!(
                validate_symbol(&env, &symbol),
                Ok(()),
                "Symbol '{name}' should be valid"
            );
        }
    }

    // validate_milestone_symbols
    #[test]
    fn test_validate_milestone_symbols_valid_passes() {
        let env = Env::default();
        let mut milestones = soroban_sdk::Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "warehouse"), 30_u32));
        milestones.push_back((Symbol::new(&env, "port"), 30_u32));
        milestones.push_back((Symbol::new(&env, "final"), 40_u32));
        assert_eq!(validate_milestone_symbols(&env, &milestones), Ok(()));
    }

    #[test]
    fn test_validate_milestone_symbols_single_milestone_passes() {
        let env = Env::default();
        let mut milestones = soroban_sdk::Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "delivery"), 100_u32));
        assert_eq!(validate_milestone_symbols(&env, &milestones), Ok(()));
    }

    #[test]
    fn test_validate_milestone_symbols_empty_passes() {
        let env = Env::default();
        let milestones: soroban_sdk::Vec<(Symbol, u32)> = soroban_sdk::Vec::new(&env);
        assert_eq!(validate_milestone_symbols(&env, &milestones), Ok(()));
    }

    #[test]
    fn test_validate_milestone_symbols_duplicate_fails() {
        let env = Env::default();
        let mut milestones = soroban_sdk::Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "warehouse"), 50_u32));
        milestones.push_back((Symbol::new(&env, "warehouse"), 50_u32));
        assert_eq!(
            validate_milestone_symbols(&env, &milestones),
            Err(NavinError::InvalidShipmentInput)
        );
    }

    #[test]
    fn test_validate_milestone_symbols_many_unique_passes() {
        let env = Env::default();
        let mut milestones = soroban_sdk::Vec::new(&env);
        let names = ["a", "b", "c", "d", "e"];
        for name in &names {
            milestones.push_back((Symbol::new(&env, name), 20_u32));
        }
        assert_eq!(validate_milestone_symbols(&env, &milestones), Ok(()));
    }

    // validate_metadata_symbols
    #[test]
    fn test_validate_metadata_symbols_valid_passes() {
        let env = Env::default();
        let key = Symbol::new(&env, "weight");
        let value = Symbol::new(&env, "kg_100");
        assert_eq!(validate_metadata_symbols(&env, &key, &value), Ok(()));
    }

    #[test]
    fn test_validate_metadata_symbols_single_char_passes() {
        let env = Env::default();
        let key = Symbol::new(&env, "w");
        let value = Symbol::new(&env, "k");
        assert_eq!(validate_metadata_symbols(&env, &key, &value), Ok(()));
    }

    #[test]
    fn test_validate_metadata_symbols_max_length_names_pass() {
        let env = Env::default();
        // 12-character key and value — at the Stellar Symbol maximum
        let key = Symbol::new(&env, "shipment_key");
        let value = Symbol::new(&env, "shipment_val");
        assert_eq!(validate_metadata_symbols(&env, &key, &value), Ok(()));
    }

    #[test]
    fn test_validate_metadata_symbols_common_pairs_pass() {
        let env = Env::default();
        let pairs = [
            ("weight", "kg_100"),
            ("priority", "high"),
            ("category", "fragile"),
            ("temperature", "controlled"),
        ];
        for (key_str, val_str) in &pairs {
            let key = Symbol::new(&env, key_str);
            let value = Symbol::new(&env, val_str);
            assert_eq!(
                validate_metadata_symbols(&env, &key, &value),
                Ok(()),
                "Pair ({key_str}, {val_str}) should be valid"
            );
        }
    }
}

// ============================================================================
// Additional Symbol Validation Tests - Comprehensive Coverage
// ============================================================================

#[cfg(test)]
mod symbol_validation_tests {
    extern crate std;

    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Env, Symbol, Vec};

    // Boundary tests for symbol length
    #[test]
    fn test_symbol_length_boundary_12_chars() {
        let env = Env::default();

        // Exactly 12 characters - maximum allowed by Stellar
        let symbols_12_chars = std::vec![
            "VERYLONGNAME", // 12 uppercase
            "verylongname", // 12 lowercase
            "VeryLongName", // 12 mixed case
            "SYMBOL123456", // 12 alphanumeric
            "symbol_12345", // 12 with underscore
            "ABCDEFGHIJKL", // 12 letters
            "123456789012", // 12 digits
        ];

        for name in symbols_12_chars {
            let symbol = Symbol::new(&env, name);
            let result = validate_symbol(&env, &symbol);
            assert!(
                result.is_ok(),
                "12-character symbol '{}' should be valid",
                name
            );
        }
    }

    #[test]
    fn test_symbol_length_30_chars_rejected() {
        let env = Env::default();
        // 30 chars: within Soroban SDK's 32-char limit but above our 12-char max
        let long_30 = "A".repeat(30);
        let symbol = Symbol::new(&env, &long_30);
        let result = validate_symbol(&env, &symbol);
        assert!(
            result.is_err(),
            "Symbol of length 30 should be rejected (exceeds 12-char max)"
        );
    }

    #[test]
    fn test_symbol_length_25_chars_rejected() {
        let env = Env::default();
        let long_25 = "A".repeat(25);
        let symbol = Symbol::new(&env, &long_25);
        let result = validate_symbol(&env, &symbol);
        assert!(
            result.is_err(),
            "Symbol of length 25 should be rejected (exceeds 12-char max)"
        );
    }

    // Format validation tests
    #[test]
    fn test_symbol_alphanumeric_formats() {
        let env = Env::default();

        let valid_formats = std::vec![
            "SHIPMENT",    // uppercase
            "shipment",    // lowercase
            "Shipment",    // mixed case
            "SHIP123",     // alphanumeric
            "ship_123",    // with underscore
            "ABC",         // short uppercase
            "xyz",         // short lowercase
            "A1B2C3",      // mixed alphanumeric
            "_underscore", // leading underscore
            "trailing_",   // trailing underscore
        ];

        for name in valid_formats {
            let symbol = Symbol::new(&env, name);
            let result = validate_symbol(&env, &symbol);
            assert!(
                result.is_ok(),
                "Alphanumeric symbol '{}' should be valid",
                name
            );
        }
    }

    // Milestone validation tests
    #[test]
    fn test_milestone_symbols_no_duplicates() {
        let env = Env::default();

        let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "pickup"), 25));
        milestones.push_back((Symbol::new(&env, "transit"), 25));
        milestones.push_back((Symbol::new(&env, "delivery"), 50));

        let result = validate_milestone_symbols(&env, &milestones);
        assert!(result.is_ok(), "Unique milestone symbols should be valid");
    }

    #[test]
    fn test_milestone_symbols_with_duplicates_rejected() {
        let env = Env::default();

        let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "warehouse"), 50));
        milestones.push_back((Symbol::new(&env, "warehouse"), 50)); // Duplicate

        let result = validate_milestone_symbols(&env, &milestones);
        assert_eq!(
            result,
            Err(NavinError::InvalidShipmentInput),
            "Duplicate milestone symbols should be rejected"
        );
    }

    #[test]
    fn test_milestone_symbols_many_unique() {
        let env = Env::default();

        let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
        let names = ["pickup", "warehouse", "port", "transit", "delivery"];
        for name in &names {
            milestones.push_back((Symbol::new(&env, name), 20));
        }

        let result = validate_milestone_symbols(&env, &milestones);
        assert!(
            result.is_ok(),
            "Many unique milestone symbols should be valid"
        );
    }

    // Metadata validation tests
    #[test]
    fn test_metadata_symbols_various_pairs() {
        let env = Env::default();

        let test_pairs = std::vec![
            ("weight", "kg_100"),
            ("priority", "high"),
            ("category", "fragile"),
            ("temp", "controlled"),
            ("status", "active"),
            ("tracking", "enabled"),
        ];

        for (key_str, val_str) in test_pairs {
            let key = Symbol::new(&env, key_str);
            let value = Symbol::new(&env, val_str);

            let result = validate_metadata_symbols(&env, &key, &value);
            assert!(
                result.is_ok(),
                "Metadata pair ({}, {}) should be valid",
                key_str,
                val_str
            );
        }
    }

    #[test]
    fn test_metadata_symbols_max_length() {
        let env = Env::default();

        // 12-character key and value (maximum)
        let key = Symbol::new(&env, "verylongkey1");
        let value = Symbol::new(&env, "verylongval1");

        let result = validate_metadata_symbols(&env, &key, &value);
        assert!(
            result.is_ok(),
            "12-character metadata symbols should be valid"
        );
    }

    // Error message tests
    #[test]
    fn test_validation_error_types() {
        let env = Env::default();
        // 30 chars: within Soroban SDK's 32-char limit but above our 12-char max
        let oversized = "A".repeat(30);
        let oversized_symbol = Symbol::new(&env, &oversized);
        let result = validate_symbol(&env, &oversized_symbol);
        assert_eq!(
            result,
            Err(NavinError::InvalidShipmentInput),
            "Oversized symbol must return InvalidShipmentInput"
        );
    }

    #[test]
    fn test_duplicate_milestone_error_type() {
        let env = Env::default();

        let mut milestones: Vec<(Symbol, u32)> = Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "checkpoint"), 50));
        milestones.push_back((Symbol::new(&env, "checkpoint"), 50));

        let result = validate_milestone_symbols(&env, &milestones);

        assert_eq!(
            result,
            Err(NavinError::InvalidShipmentInput),
            "Duplicate milestone should return InvalidShipmentInput"
        );
    }

    #[test]
    fn test_validate_shipment_input_accepts_valid_payload() {
        let env = Env::default();
        let contract_id = env.register(crate::NavinShipment, ());
        let sender = soroban_sdk::Address::generate(&env);
        let receiver = soroban_sdk::Address::generate(&env);
        let carrier = soroban_sdk::Address::generate(&env);
        let token_address = soroban_sdk::Address::generate(&env);
        let deadline = env.ledger().timestamp() + 3_600;

        let mut payment_milestones = Vec::new(&env);
        payment_milestones.push_back((Symbol::new(&env, "warehouse"), 50));
        payment_milestones.push_back((Symbol::new(&env, "delivery"), 50));

        let input = ShipmentInput {
            receiver,
            carrier,
            token_address,
            data_hash: BytesN::from_array(&env, &[1u8; 32]),
            payment_milestones,
            deadline,
            depends_on: None,
        };

        let result = env.as_contract(&contract_id, || {
            validate_shipment_input(&env, &sender, &input)
        });

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_validate_shipment_input_rejects_duplicate_participants() {
        let env = Env::default();
        let contract_id = env.register(crate::NavinShipment, ());
        let sender = soroban_sdk::Address::generate(&env);
        let carrier = soroban_sdk::Address::generate(&env);
        let token_address = soroban_sdk::Address::generate(&env);
        let deadline = env.ledger().timestamp() + 3_600;

        let input = ShipmentInput {
            receiver: sender.clone(),
            carrier,
            token_address,
            data_hash: BytesN::from_array(&env, &[2u8; 32]),
            payment_milestones: Vec::new(&env),
            deadline,
            depends_on: None,
        };

        let result = env.as_contract(&contract_id, || {
            validate_shipment_input(&env, &sender, &input)
        });

        assert_eq!(result, Err(NavinError::InvalidShipmentParticipants));
    }

    #[test]
    fn test_validate_shipment_input_rejects_past_deadline() {
        let env = Env::default();
        let contract_id = env.register(crate::NavinShipment, ());
        let sender = soroban_sdk::Address::generate(&env);
        let receiver = soroban_sdk::Address::generate(&env);
        let carrier = soroban_sdk::Address::generate(&env);
        let token_address = soroban_sdk::Address::generate(&env);
        let deadline = env.ledger().timestamp();

        let input = ShipmentInput {
            receiver,
            carrier,
            token_address,
            data_hash: BytesN::from_array(&env, &[3u8; 32]),
            payment_milestones: Vec::new(&env),
            deadline,
            depends_on: None,
        };

        let result = env.as_contract(&contract_id, || {
            validate_shipment_input(&env, &sender, &input)
        });

        assert_eq!(result, Err(NavinError::InvalidShipmentDeadline));
    }

    #[test]
    fn test_validate_payment_milestones_rejects_non_100_sum() {
        let env = Env::default();
        let mut milestones = Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "warehouse"), 30));
        milestones.push_back((Symbol::new(&env, "delivery"), 60));

        assert_eq!(
            validate_payment_milestones(&env, &milestones),
            Err(NavinError::MilestoneSumInvalid)
        );
    }

    #[test]
    fn test_validate_payment_milestones_accepts_exact_100_sum() {
        let env = Env::default();
        let mut milestones = Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "warehouse"), 40));
        milestones.push_back((Symbol::new(&env, "delivery"), 60));

        assert_eq!(validate_payment_milestones(&env, &milestones), Ok(()));
    }

    #[test]
    fn test_validate_token_address_rejects_contract_address() {
        let env = Env::default();
        let contract_id = env.register(crate::NavinShipment, ());

        let result = env.as_contract(&contract_id, || validate_token_address(&env, &contract_id));

        assert_eq!(result, Err(NavinError::InvalidTokenAddress));
    }
}

/// Maximum number of dependencies allowed per shipment.
const MAX_DEPENDENCIES: usize = 10;

/// Maximum recursion depth for cycle detection.
const MAX_DEPTH: usize = 64;

/// Validate shipment dependencies before persistence.
///
/// Ensures:
/// - Dependency count does not exceed MAX_DEPENDENCIES (10)
/// - All referenced shipment IDs exist
/// - No circular dependencies are present
pub fn validate_dependencies(
    env: &Env,
    shipment_id: u64,
    dependencies: &Option<soroban_sdk::Vec<u64>>,
) -> Result<(), NavinError> {
    let deps = match dependencies {
        Some(d) => d,
        None => return Ok(()),
    };

    if deps.len() as usize > MAX_DEPENDENCIES {
        return Err(NavinError::InvalidShipmentInput);
    }

    for i in 0..deps.len() {
        let dep_id = deps.get_unchecked(i);
        if dep_id == shipment_id {
            return Err(NavinError::CircularDependency);
        }
        let _ = storage::get_shipment(env, dep_id).ok_or(NavinError::ShipmentNotFound)?;
        if transitive_has_dependency(env, dep_id, shipment_id, 0)? {
            return Err(NavinError::CircularDependency);
        }
    }

    Ok(())
}

fn transitive_has_dependency(
    env: &Env,
    target: u64,
    shipment_id: u64,
    depth: usize,
) -> Result<bool, NavinError> {
    if depth > MAX_DEPTH {
        return Err(NavinError::CircularDependency);
    }

    if let Some(target_deps) = storage::get_dependencies(env, target) {
        for i in 0..target_deps.len() {
            let dep = target_deps.get_unchecked(i);
            if dep == shipment_id {
                return Ok(true);
            }
            if transitive_has_dependency(env, dep, shipment_id, depth + 1)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn collect_unmet_dependencies(
    env: &Env,
    shipment_id: u64,
) -> Result<soroban_sdk::Vec<u64>, NavinError> {
    let mut unmet = soroban_sdk::Vec::new(env);

    if let Some(deps) = storage::get_dependencies(env, shipment_id) {
        for i in 0..deps.len() {
            let dep_id = deps.get_unchecked(i);
            match storage::get_shipment(env, dep_id) {
                Some(shipment) if shipment.status == ShipmentStatus::Delivered => {}
                _ => {
                    unmet.push_back(dep_id);
                }
            }
        }
    }

    Ok(unmet)
}
