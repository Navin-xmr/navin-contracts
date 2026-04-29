#![no_std]

use soroban_sdk::{
    contract, contractimpl, symbol_short, xdr::ToXdr, Address, BytesN, Env, IntoVal, Map, Symbol,
    Vec,
};

mod audit;
mod circuit_breaker;
mod config;
pub mod consistency;
pub mod diagnostics;
mod e2e_test;
pub mod error_map;
mod errors;
mod event_topics;
mod events;
mod rate_limit;
mod recovery;
mod storage;
mod stress_test;
pub mod test;
#[cfg(test)]
mod test_batch_queries;
#[cfg(test)]
mod test_consistency;
#[cfg(test)]
mod test_cross_contract_integration;
#[cfg(test)]
mod test_mixed_token_shipments;
#[cfg(test)]
mod test_reentrancy_guard;
#[cfg(test)]
mod test_token_compatibility;

#[cfg(test)]
mod test_event_fixtures;
#[cfg(test)]
mod test_finalization;
#[cfg(test)]
mod test_hash_emit_vectors;
#[cfg(test)]
mod test_performance;
#[cfg(test)]
mod test_rollback;
mod types;
mod validation;

#[cfg(test)]
mod test_auth;
#[cfg(test)]
mod test_auto_dispute;
#[cfg(test)]
mod test_carrier_relationship;
#[cfg(test)]
mod test_creation_quota;
#[cfg(test)]
mod test_diagnostics;
#[cfg(test)]
mod test_hash_domain_separation;
#[cfg(test)]
mod test_iot_verification;
#[cfg(test)]
mod test_panic_free_invariants;
#[cfg(test)]
mod test_pause;
#[cfg(test)]
mod test_precondition_guards;
#[cfg(test)]
mod test_proposal_digest;
#[cfg(test)]
mod test_require_auth_for_args;
#[cfg(test)]
mod test_signature_argument_ordering;
#[cfg(test)]
mod test_suspension;
#[cfg(test)]
mod test_utils;
#[cfg(test)]
mod test_verification;

// ── Fuzz / property-based test harnesses ─────────────────────────────────────
#[cfg(test)]
mod fuzz_escrow_arithmetic;
#[cfg(test)]
mod fuzz_escrow_lifecycle;
#[cfg(test)]
mod fuzz_milestone_releases;
#[cfg(test)]
mod fuzz_rbac_authorization;
#[cfg(test)]
mod fuzz_role_assignment;
#[cfg(test)]
mod fuzz_storage_operations;
#[cfg(test)]
mod fuzz_ttl_management;
#[cfg(test)]
mod fuzz_wallet_auth_integration;

pub use config::*;
pub use consistency::*;
pub use diagnostics::*;
pub use errors::*;
pub use types::*;
pub use validation::*;

const MAX_BATCH_QUERY_SIZE: u32 = 50;

fn extend_shipment_ttl(env: &Env, shipment_id: u64) {
    let config = config::get_config(env);
    storage::extend_shipment_ttl(
        env,
        shipment_id,
        config.shipment_ttl_threshold,
        config.shipment_ttl_extension,
    );
}

/// Extend TTL using already-cached threshold/extension values, avoiding a
/// redundant `get_config` storage read when called inside a batch loop.
#[inline]
fn extend_shipment_ttl_cached(env: &Env, shipment_id: u64, threshold: u32, extension: u32) {
    storage::extend_shipment_ttl(env, shipment_id, threshold, extension);
}

fn validate_milestones(env: &Env, milestones: &Vec<(Symbol, u32)>) -> Result<(), NavinError> {
    if milestones.is_empty() {
        return Ok(());
    }

    // Validate all milestone symbols for bounded usage
    validation::validate_milestone_symbols(env, milestones)?;

    let mut total_percentage = 0;
    for milestone in milestones.iter() {
        total_percentage += milestone.1;
    }

    if total_percentage != 100 {
        return Err(NavinError::MilestoneSumInvalid);
    }

    Ok(())
}

fn persist_shipment(env: &Env, shipment: &Shipment) -> Result<(), NavinError> {
    validation::validate_shipment_invariants(shipment)?;
    storage::set_shipment(env, shipment);
    Ok(())
}

fn checked_add_i128(a: i128, b: i128) -> Result<i128, NavinError> {
    a.checked_add(b).ok_or(NavinError::ArithmeticError)
}

fn checked_sub_i128(a: i128, b: i128) -> Result<i128, NavinError> {
    a.checked_sub(b).ok_or(NavinError::ArithmeticError)
}

fn checked_mul_div_i128(value: i128, multiplier: i128, divisor: i128) -> Result<i128, NavinError> {
    if divisor == 0 {
        return Err(NavinError::ArithmeticError);
    }
    let product = value
        .checked_mul(multiplier)
        .ok_or(NavinError::ArithmeticError)?;
    Ok(product / divisor)
}

fn with_reentrancy_lock<T, F>(env: &Env, operation: F) -> Result<T, NavinError>
where
    F: FnOnce() -> Result<T, NavinError>,
{
    if storage::is_reentrancy_locked(env) {
        return Err(NavinError::ReentrancyDetected);
    }

    storage::set_reentrancy_lock(env, true);
    let result = operation();
    storage::set_reentrancy_lock(env, false);
    result
}

fn effective_batch_query_limit(env: &Env) -> u32 {
    let _ = env;
    MAX_BATCH_QUERY_SIZE
}

fn finalize_if_settled(_env: &Env, shipment: &mut Shipment) {
    if (shipment.status == ShipmentStatus::Delivered
        || shipment.status == ShipmentStatus::Cancelled)
        && shipment.escrow_amount == 0
    {
        shipment.finalized = true;
    }
}

/// Create a new settlement record and mark it as active for the shipment.
fn create_settlement(
    env: &Env,
    shipment_id: u64,
    operation: SettlementOperation,
    amount: i128,
    from: &Address,
    to: &Address,
) -> Result<u64, NavinError> {
    let settlement_id = storage::increment_settlement_counter(env);
    let settlement = SettlementRecord {
        settlement_id,
        shipment_id,
        operation,
        state: SettlementState::Pending,
        amount,
        from: from.clone(),
        to: to.clone(),
        initiated_at: env.ledger().timestamp(),
        completed_at: None,
        error_code: None,
    };
    storage::set_settlement(env, &settlement);
    storage::set_active_settlement(env, shipment_id, settlement_id);
    Ok(settlement_id)
}

/// Mark a settlement as completed.
fn complete_settlement(env: &Env, settlement_id: u64, shipment_id: u64) -> Result<(), NavinError> {
    let mut settlement =
        storage::get_settlement(env, settlement_id).ok_or(NavinError::ShipmentNotFound)?; // Reusing error for simplicity
    settlement.state = SettlementState::Completed;
    settlement.completed_at = Some(env.ledger().timestamp());
    storage::set_settlement(env, &settlement);
    storage::clear_active_settlement(env, shipment_id);
    Ok(())
}

/// Mark a settlement as failed with an error code.
fn fail_settlement(
    env: &Env,
    settlement_id: u64,
    shipment_id: u64,
    error_code: u32,
) -> Result<(), NavinError> {
    let mut settlement =
        storage::get_settlement(env, settlement_id).ok_or(NavinError::ShipmentNotFound)?; // Reusing error for simplicity
    settlement.state = SettlementState::Failed;
    settlement.completed_at = Some(env.ledger().timestamp());
    settlement.error_code = Some(error_code);
    storage::set_settlement(env, &settlement);
    storage::clear_active_settlement(env, shipment_id);
    Ok(())
}

fn require_not_finalized(shipment: &Shipment) -> Result<(), NavinError> {
    if shipment.finalized {
        return Err(NavinError::ShipmentFinalized);
    }
    Ok(())
}

/// Build a 32-byte action hash from arbitrary bytes and check/set the idempotency window.
/// Returns `DuplicateAction` if the hash is already present in temporary storage.
fn check_idempotency(env: &Env, payload: soroban_sdk::Bytes) -> Result<(), NavinError> {
    let action_hash: BytesN<32> = env.crypto().sha256(&payload).into();
    if storage::has_idempotency_window(env, &action_hash) {
        return Err(NavinError::DuplicateAction);
    }
    let window = config::get_config(env).idempotency_window_seconds;
    storage::set_idempotency_window(env, &action_hash, window);
    Ok(())
}

#[derive(Copy, Clone)]
enum TokenOperation {
    Transfer,
    #[cfg(test)]
    Mint,
}

impl TokenOperation {
    fn symbol(self) -> Symbol {
        match self {
            TokenOperation::Transfer => symbol_short!("transfer"),
            #[cfg(test)]
            TokenOperation::Mint => symbol_short!("mint"),
        }
    }

    fn error(self) -> NavinError {
        match self {
            TokenOperation::Transfer => NavinError::TokenTransferFailed,
            #[cfg(test)]
            TokenOperation::Mint => NavinError::TokenMintFailed,
        }
    }
}

/// Validates that the token contract reports the expected number of decimal places (7).
///
/// The Navin contract assumes all amounts are expressed in the Stellar standard
/// unit where 1 token = 10_000_000 stroops (7 decimal places). Tokens returning
/// a different value from `decimals()` would cause mismatched amount calculations
/// in escrow operations, so they are rejected early.
///
/// # Errors
/// Returns `NavinError::InvalidTokenDecimals` if the token returns ≠ 7 decimals,
/// or if the call to the token contract fails (treated as an incompatible token).
fn validate_token_decimals(env: &Env, token_contract: &Address) -> Result<(), NavinError> {
    let args: Vec<soroban_sdk::Val> = Vec::new(env);
    let result = env.try_invoke_contract::<u32, soroban_sdk::Error>(
        token_contract,
        &Symbol::new(env, "decimals"),
        args,
    );
    match result {
        Ok(Ok(decimals)) if decimals == crate::types::EXPECTED_TOKEN_DECIMALS => Ok(()),
        _ => Err(NavinError::InvalidTokenDecimals),
    }
}

fn invoke_token_operation(
    env: &Env,
    token_contract: &Address,
    operation: TokenOperation,
    args: Vec<soroban_sdk::Val>,
) -> Result<(), NavinError> {
    match env.try_invoke_contract::<(), soroban_sdk::Error>(
        token_contract,
        &operation.symbol(),
        args,
    ) {
        Ok(Ok(())) => Ok(()),
        _ => Err(operation.error()),
    }
}

fn invoke_token_transfer(
    env: &Env,
    token_contract: &Address,
    from: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), NavinError> {
    let cb_config = circuit_breaker::CircuitBreakerConfig::default();
    circuit_breaker::check_transfer_allowed(env, &cb_config)?;

    let mut args: soroban_sdk::Vec<soroban_sdk::Val> = Vec::new(env);
    args.push_back(from.clone().into_val(env));
    args.push_back(to.clone().into_val(env));
    args.push_back(amount.into_val(env));

    match invoke_token_operation(env, token_contract, TokenOperation::Transfer, args) {
        Ok(()) => {
            circuit_breaker::record_transfer_success(env);
            Ok(())
        }
        Err(e) => {
            circuit_breaker::record_transfer_failure(env, &cb_config);
            Err(e)
        }
    }
}

#[cfg(test)]
fn invoke_token_mint(
    env: &Env,
    token_contract: &Address,
    admin: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), NavinError> {
    let mut args: soroban_sdk::Vec<soroban_sdk::Val> = Vec::new(env);
    args.push_back(admin.clone().into_val(env));
    args.push_back(to.clone().into_val(env));
    args.push_back(amount.into_val(env));
    invoke_token_operation(env, token_contract, TokenOperation::Mint, args)
}

fn internal_release_escrow(
    env: &Env,
    shipment: &mut Shipment,
    amount: i128,
) -> Result<(), NavinError> {
    if amount <= 0 {
        return Ok(());
    }
    let actual_release = if amount > shipment.escrow_amount {
        shipment.escrow_amount
    } else {
        amount
    };

    if actual_release > 0 {
        // Get token contract address
        let token_contract = storage::get_token_contract(env).ok_or(NavinError::NotInitialized)?;
        let contract_address = env.current_contract_address();

        // Create settlement record in Pending state
        let settlement_id = create_settlement(
            env,
            shipment.id,
            SettlementOperation::Release,
            actual_release,
            &contract_address,
            &shipment.carrier,
        )?;

        // Transfer tokens from this contract to carrier
        let transfer_result = invoke_token_transfer(
            env,
            &token_contract,
            &contract_address,
            &shipment.carrier,
            actual_release,
        );

        match transfer_result {
            Ok(()) => {
                // Mark settlement as completed
                complete_settlement(env, settlement_id, shipment.id)?;

                shipment.escrow_amount = checked_sub_i128(shipment.escrow_amount, actual_release)?;
                shipment.updated_at = env.ledger().timestamp();
                shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);
                persist_shipment(env, shipment)?;
                storage::set_escrow(env, shipment.id, shipment.escrow_amount);

                events::emit_escrow_released(env, shipment.id, &shipment.carrier, actual_release);
            }
            Err(e) => {
                // Mark settlement as failed
                fail_settlement(env, settlement_id, shipment.id, e as u32)?;
                return Err(e);
            }
        }
    }

    Ok(())
}

fn require_initialized(env: &Env) -> Result<(), NavinError> {
    if !storage::is_initialized(env) {
        return Err(NavinError::NotInitialized);
    }
    Ok(())
}

fn require_not_paused(env: &Env) -> Result<(), NavinError> {
    if storage::is_paused(env) {
        return Err(NavinError::ContractPaused);
    }
    Ok(())
}

fn require_admin_or_guardian(env: &Env, address: &Address) -> Result<(), NavinError> {
    require_initialized(env)?;
    if storage::get_admin(env) == *address {
        return Ok(());
    }
    if storage::has_role(env, address, &Role::Guardian)
        && !storage::is_role_suspended(env, address, &Role::Guardian)
    {
        return Ok(());
    }
    Err(NavinError::Unauthorized)
}

fn require_admin_or_operator(env: &Env, address: &Address) -> Result<(), NavinError> {
    require_initialized(env)?;
    if storage::get_admin(env) == *address {
        return Ok(());
    }
    if storage::has_role(env, address, &Role::Operator)
        && !storage::is_role_suspended(env, address, &Role::Operator)
    {
        return Ok(());
    }
    Err(NavinError::Unauthorized)
}

fn require_role(env: &Env, address: &Address, role: Role) -> Result<(), NavinError> {
    require_initialized(env)?;

    match role {
        Role::Company => {
            if storage::has_company_role(env, address) {
                // Check if role is suspended via generic role suspension
                if storage::is_role_suspended(env, address, &Role::Company) {
                    return Err(NavinError::Unauthorized);
                }
                // Check if company specifically is suspended
                if storage::is_company_suspended(env, address) {
                    return Err(NavinError::CompanySuspended);
                }
                Ok(())
            } else {
                Err(NavinError::Unauthorized)
            }
        }
        Role::Carrier => {
            if storage::has_carrier_role(env, address) {
                // Check if role is suspended
                if storage::is_role_suspended(env, address, &Role::Carrier) {
                    return Err(NavinError::Unauthorized);
                }
                // Also check legacy carrier-specific suspension
                if storage::is_carrier_suspended(env, address) {
                    return Err(NavinError::CarrierSuspended);
                }
                Ok(())
            } else {
                Err(NavinError::Unauthorized)
            }
        }
        Role::Guardian => {
            if storage::has_role(env, address, &Role::Guardian) {
                if storage::is_role_suspended(env, address, &Role::Guardian) {
                    return Err(NavinError::Unauthorized);
                }
                Ok(())
            } else {
                Err(NavinError::Unauthorized)
            }
        }
        Role::Operator => {
            if storage::has_role(env, address, &Role::Operator) {
                if storage::is_role_suspended(env, address, &Role::Operator) {
                    return Err(NavinError::Unauthorized);
                }
                Ok(())
            } else {
                Err(NavinError::Unauthorized)
            }
        }
        Role::Unassigned => Err(NavinError::Unauthorized),
        Role::Observer => Err(NavinError::Unauthorized),
    }
}

fn require_active_company(env: &Env, company: &Address) -> Result<(), NavinError> {
    if storage::is_company_suspended(env, company) {
        return Err(NavinError::CompanySuspended);
    }
    // Also check generic role suspension for completeness
    if storage::is_role_suspended(env, company, &Role::Company) {
        return Err(NavinError::Unauthorized);
    }
    Ok(())
}

fn require_active_carrier(env: &Env, carrier: &Address) -> Result<(), NavinError> {
    if storage::is_carrier_suspended(env, carrier) {
        return Err(NavinError::CarrierSuspended);
    }
    // Also check generic role suspension
    if storage::is_role_suspended(env, carrier, &Role::Carrier) {
        return Err(NavinError::Unauthorized);
    }
    Ok(())
}

/// Require that `caller` is the contract admin. Centralizes the repeated
/// `if storage::get_admin(&env) != admin { return Err(Unauthorized) }` pattern.
fn require_admin(env: &Env, caller: &Address) -> Result<(), NavinError> {
    if storage::get_admin(env) != *caller {
        return Err(NavinError::Unauthorized);
    }
    Ok(())
}

/// Require that a shipment exists and return it. Centralizes the repeated
/// `storage::get_shipment(&env, id).ok_or(ShipmentNotFound)?` pattern.
fn require_shipment(env: &Env, shipment_id: u64) -> Result<Shipment, NavinError> {
    storage::get_shipment(env, shipment_id).ok_or(NavinError::ShipmentNotFound)
}

/// Require that `caller` is the assigned carrier for a shipment (or admin).
/// Returns the shipment on success.
fn require_carrier_for_shipment(
    env: &Env,
    caller: &Address,
    shipment_id: u64,
) -> Result<Shipment, NavinError> {
    let shipment = require_shipment(env, shipment_id)?;
    let admin = storage::get_admin(env);
    if *caller != shipment.carrier && *caller != admin {
        return Err(NavinError::Unauthorized);
    }
    Ok(shipment)
}

#[contract]
pub struct NavinShipment;

#[contractimpl]
impl NavinShipment {
    /// Set metadata key-value pair for a shipment. Only Company (sender) or Admin can set.
    /// Max 5 metadata entries allowed.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `caller` - The address attempting to set the metadata.
    /// * `shipment_id` - ID of the shipment.
    /// * `key` - The metadata key (max 32 chars).
    /// * `value` - The metadata value (max 32 chars).
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully set.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If the shipment doesn't exist.
    /// * `NavinError::Unauthorized` - If the caller is not the sender or admin.
    /// * `NavinError::MetadataLimitExceeded` - If adding would exceed the 5 key limit.
    ///
    /// # Examples
    /// ```rust
    /// // contract.set_shipment_metadata(&env, &caller, 1, &Symbol::new(&env, "weight"), &Symbol::new(&env, "kg_100"));
    /// ```
    pub fn set_shipment_metadata(
        env: Env,
        caller: Address,
        shipment_id: u64,
        key: Symbol,
        value: Symbol,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        caller.require_auth();

        // Validate metadata symbols for bounded usage before storage
        validation::validate_metadata_symbols(&env, &key, &value)?;

        let admin = storage::get_admin(&env);
        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;
        require_not_finalized(&shipment)?;
        // Only sender or admin can set
        if caller != shipment.sender && caller != admin {
            return Err(NavinError::Unauthorized);
        }
        // If caller is the company (sender), check for suspension
        if caller == shipment.sender {
            require_active_company(&env, &caller)?;
        }
        // Initialize metadata map if not present
        let mut metadata = shipment.metadata.unwrap_or(Map::new(&env));
        // Enforce max metadata entries from config
        let config = config::get_config(&env);
        if !metadata.contains_key(key.clone()) && metadata.len() >= config.max_metadata_entries {
            return Err(NavinError::MetadataLimitExceeded);
        }
        metadata.set(key.clone(), value.clone());
        shipment.metadata = Some(metadata);
        shipment.updated_at = env.ledger().timestamp();
        shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);
        persist_shipment(&env, &shipment)?;
        Ok(())
    }

    /// Append a hash-only note to a shipment for commentary.
    /// Only the sender, receiver, assigned carrier, or admin can append notes.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `reporter` - The address appending the note.
    /// * `shipment_id` - ID of the shipment.
    /// * `note_hash` - SHA-256 hash of the off-chain note text.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully appended.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If the shipment doesn't exist.
    /// * `NavinError::Unauthorized` - If the caller is not involved in the shipment or admin.
    pub fn append_note_hash(
        env: Env,
        reporter: Address,
        shipment_id: u64,
        note_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        reporter.require_auth();

        // Validate hash before storage
        validation::validate_hash(&note_hash)?;

        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;
        require_not_finalized(&shipment)?;
        let admin = storage::get_admin(&env);

        // Authorization: Sender, Receiver, Carrier, or Admin
        if reporter != shipment.sender
            && reporter != shipment.receiver
            && reporter != shipment.carrier
            && reporter != admin
        {
            return Err(NavinError::Unauthorized);
        }

        // If reporter is the company (sender), check for suspension
        if reporter == shipment.sender {
            require_active_company(&env, &reporter)?;
        }

        // Check note event payload size guard
        let config = config::get_config(&env);
        let current_note_count = storage::get_note_count(&env, shipment_id);
        if current_note_count >= config.max_notes_per_shipment {
            return Err(NavinError::NoteLimitExceeded);
        }

        // notes are append-only; we just increment the counter and store at the next index.
        let index = storage::increment_note_count(&env, shipment_id);
        storage::set_note_hash(&env, shipment_id, index, &note_hash);

        // Emit the event following the Hash-and-Emit pattern.
        events::emit_note_appended(&env, shipment_id, index, &note_hash, &reporter);

        Ok(())
    }

    /// Add an evidence hash to an active shipment dispute.
    /// Only in Disputed state. Authorization: Sender, Receiver, Carrier, or Admin.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `reporter` - The address adding the evidence.
    /// * `shipment_id` - ID of the shipment.
    /// * `evidence_hash` - SHA-256 hash of the off-chain evidence.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully added.
    pub fn add_dispute_evidence_hash(
        env: Env,
        reporter: Address,
        shipment_id: u64,
        evidence_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        reporter.require_auth();

        // Validate hash before storage
        validation::validate_hash(&evidence_hash)?;

        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;
        require_not_finalized(&shipment)?;
        let admin = storage::get_admin(&env);

        // State check: Only in Disputed state
        if shipment.status != ShipmentStatus::Disputed {
            return Err(NavinError::InvalidStatus);
        }

        // Authorization: Sender, Receiver, Carrier, or Admin
        if reporter != shipment.sender
            && reporter != shipment.receiver
            && reporter != shipment.carrier
            && reporter != admin
        {
            return Err(NavinError::Unauthorized);
        }

        // If reporter is the company (sender), check for suspension
        if reporter == shipment.sender {
            require_active_company(&env, &reporter)?;
        }

        // Check evidence count payload size guard
        let config = config::get_config(&env);
        let current_evidence_count = storage::get_evidence_count(&env, shipment_id);
        if current_evidence_count >= config.max_evidence_per_dispute {
            return Err(NavinError::EvidenceLimitExceeded);
        }

        // Increment counter and store hash
        let index = storage::increment_evidence_count(&env, shipment_id);
        storage::set_evidence_hash(&env, shipment_id, index, &evidence_hash);

        // Increment integration nonce
        let mut shipment_mut = shipment;
        shipment_mut.integration_nonce = shipment_mut.integration_nonce.saturating_add(1);
        storage::set_shipment(&env, &shipment_mut);

        // Emit event
        events::emit_evidence_added(&env, shipment_id, index, &evidence_hash, &reporter);

        Ok(())
    }

    /// Get the total number of evidence hashes for a shipment dispute.
    pub fn get_dispute_evidence_count(env: Env, shipment_id: u64) -> Result<u32, NavinError> {
        require_initialized(&env)?;
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }
        Ok(storage::get_evidence_count(&env, shipment_id))
    }

    /// Get a specific evidence hash for a shipment dispute by its sequence index.
    pub fn get_dispute_evidence_hash(
        env: Env,
        shipment_id: u64,
        index: u32,
    ) -> Result<Option<BytesN<32>>, NavinError> {
        require_initialized(&env)?;
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }
        Ok(storage::get_evidence_hash(&env, shipment_id, index))
    }

    /// Get the current integration nonce for a shipment.
    /// Nonce increments on critical transitions like status changes and escrow movements.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment.
    ///
    /// # Returns
    /// * `Result<u32, NavinError>` - The current nonce.
    pub fn get_integration_nonce(env: Env, shipment_id: u64) -> Result<u32, NavinError> {
        require_initialized(&env)?;
        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;
        Ok(shipment.integration_nonce)
    }

    /// Get the total number of notes appended to a shipment.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment.
    ///
    /// # Returns
    /// * `Result<u32, NavinError>` - Number of notes for the shipment.
    pub fn get_note_count(env: Env, shipment_id: u64) -> Result<u32, NavinError> {
        require_initialized(&env)?;
        // Verify existence or check archived
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }
        Ok(storage::get_note_count(&env, shipment_id))
    }

    /// Get a specific note hash for a shipment by its sequence index.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment.
    /// * `index` - The 0-based index of the note.
    ///
    /// # Returns
    /// * `Result<Option<BytesN<32>>, NavinError>` - The note hash if found.
    pub fn get_note_hash(
        env: Env,
        shipment_id: u64,
        index: u32,
    ) -> Result<Option<BytesN<32>>, NavinError> {
        require_initialized(&env)?;
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }
        Ok(storage::get_note_hash(&env, shipment_id, index))
    }
    /// Initialize the contract with an admin address and token contract address.
    /// Can only be called once. Sets the admin and shipment counter to 0.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - The address designated as the administrator.
    /// * `token_contract` - The address of the token contract used for escrow.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if initialized.
    ///
    /// # Errors
    /// * `NavinError::AlreadyInitialized` - If called when already initialized.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use soroban_sdk::{Env, Address};
    /// # use soroban_sdk::testutils::Address as _;
    /// # use shipment::{NavinShipment, NavinShipmentClient};
    /// # let env = Env::default();
    /// # env.mock_all_auths();
    /// # let contract_id = env.register(NavinShipment, ());
    /// # let client = NavinShipmentClient::new(&env, &contract_id);
    /// let admin = Address::generate(&env);
    /// let token_contract = Address::generate(&env); // replace with deployed token address
    ///
    /// client.initialize(&admin, &token_contract);
    /// ```
    pub fn initialize(env: Env, admin: Address, token_contract: Address) -> Result<(), NavinError> {
        if storage::is_initialized(&env) {
            return Err(NavinError::AlreadyInitialized);
        }

        storage::set_admin(&env, &admin);
        storage::set_token_contract(&env, &token_contract);
        storage::set_shipment_counter(&env, 0);
        storage::set_version(&env, 1);
        storage::set_company_role(&env, &admin);

        // Initialize with default configuration
        let default_config = ContractConfig::default();
        config::set_config(&env, &default_config);
        storage::set_shipment_limit(&env, default_config.default_shipment_limit);

        env.events().publish(
            (symbol_short!("init"),),
            (admin.clone(), token_contract.clone()),
        );

        Ok(())
    }

    /// Set the configurable limit on the number of active shipments a company can have.
    /// Only the admin can call this.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin address.
    /// * `limit` - The new active shipment limit.
    pub fn set_shipment_limit(env: Env, admin: Address, limit: u32) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        storage::set_shipment_limit(&env, limit);

        env.events()
            .publish((Symbol::new(&env, "set_limit"),), (admin, limit));

        Ok(())
    }

    /// Get the current shipment limit.
    pub fn get_shipment_limit(env: Env) -> Result<u32, NavinError> {
        require_initialized(&env)?;
        Ok(storage::get_shipment_limit(&env))
    }

    /// Set a company-specific active shipment limit override.
    pub fn set_company_shipment_limit(
        env: Env,
        admin: Address,
        company: Address,
        limit: u32,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        storage::set_company_shipment_limit(&env, &company, limit);
        env.events().publish(
            (Symbol::new(&env, "set_cmp_limit"),),
            (admin, company, limit),
        );
        Ok(())
    }

    /// Get effective shipment limit for a company (override or global fallback).
    pub fn get_effective_shipment_limit(env: Env, company: Address) -> Result<u32, NavinError> {
        require_initialized(&env)?;
        Ok(storage::get_effective_shipment_limit(&env, &company))
    }

    /// Get the current active shipment count for a company.
    pub fn get_active_shipment_count(env: Env, company: Address) -> Result<u32, NavinError> {
        require_initialized(&env)?;
        Ok(storage::get_active_shipment_count(&env, &company))
    }

    /// Get the contract admin address.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<Address, NavinError>` - The current admin address.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let admin = contract.get_admin(&env);
    /// ```
    pub fn get_admin(env: Env) -> Result<Address, NavinError> {
        require_initialized(&env)?;
        Ok(storage::get_admin(&env))
    }

    /// Get the contract version number.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<u32, NavinError>` - The version number of the contract.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let version = contract.get_version(&env);
    /// ```
    pub fn get_version(env: Env) -> Result<u32, NavinError> {
        require_initialized(&env)?;
        Ok(storage::get_version(&env))
    }

    /// Get the current hash algorithm version used for data verification.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<u32, NavinError>` - The hash algorithm version constant.
    pub fn get_hash_algo_version(env: Env) -> Result<u32, NavinError> {
        require_initialized(&env)?;
        Ok(DEFAULT_HASH_ALGO)
    }

    /// Get the token decimals policy expected by escrow math normalization.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<u32, NavinError>` - Expected token decimals (7).
    pub fn get_expected_token_decimals(env: Env) -> Result<u32, NavinError> {
        require_initialized(&env)?;
        Ok(crate::types::EXPECTED_TOKEN_DECIMALS)
    }

    /// Get on-chain metadata for this contract.
    /// Returns version, admin, shipment count, and initialization status.
    /// Read-only — no authentication required.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<ContractMetadata, NavinError>` - Snapshot of contract metadata.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let metadata = contract.get_contract_metadata(&env);
    /// ```
    pub fn get_contract_metadata(env: Env) -> Result<ContractMetadata, NavinError> {
        require_initialized(&env)?;
        Ok(ContractMetadata {
            version: storage::get_version(&env),
            admin: storage::get_admin(&env),
            shipment_count: storage::get_shipment_counter(&env),
            initialized: true,
            hash_algo_version: DEFAULT_HASH_ALGO,
        })
    }

    /// Get the current shipment counter.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<u64, NavinError>` - The total number of shipments created.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let count = contract.get_shipment_counter(&env);
    /// ```
    pub fn get_shipment_counter(env: Env) -> Result<u64, NavinError> {
        require_initialized(&env)?;
        Ok(storage::get_shipment_counter(&env))
    }

    /// Get aggregated analytics for the contract.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<Analytics, NavinError>` - Aggregated analytics data.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    pub fn get_analytics(env: Env) -> Result<Analytics, NavinError> {
        require_initialized(&env)?;

        Ok(Analytics {
            total_shipments: storage::get_shipment_counter(&env),
            total_escrow_volume: storage::get_total_escrow_volume(&env),
            total_disputes: storage::get_total_disputes(&env),
            created_count: storage::get_status_count(&env, &ShipmentStatus::Created),
            in_transit_count: storage::get_status_count(&env, &ShipmentStatus::InTransit),
            at_checkpoint_count: storage::get_status_count(&env, &ShipmentStatus::AtCheckpoint),
            delivered_count: storage::get_status_count(&env, &ShipmentStatus::Delivered),
            disputed_count: storage::get_status_count(&env, &ShipmentStatus::Disputed),
            cancelled_count: storage::get_status_count(&env, &ShipmentStatus::Cancelled),
        })
    }

    /// Retrieve a compact summary of shipment counts aggregated by status.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<ShipmentStatusSummary, NavinError>` - Summary of counts for all statuses.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    pub fn get_status_summary(env: Env) -> Result<ShipmentStatusSummary, NavinError> {
        require_initialized(&env)?;
        Ok(ShipmentStatusSummary {
            created: storage::get_status_count(&env, &ShipmentStatus::Created),
            in_transit: storage::get_status_count(&env, &ShipmentStatus::InTransit),
            at_checkpoint: storage::get_status_count(&env, &ShipmentStatus::AtCheckpoint),
            partially_delivered: storage::get_status_count(
                &env,
                &ShipmentStatus::PartiallyDelivered,
            ),
            delivered: storage::get_status_count(&env, &ShipmentStatus::Delivered),
            disputed: storage::get_status_count(&env, &ShipmentStatus::Disputed),
            cancelled: storage::get_status_count(&env, &ShipmentStatus::Cancelled),
        })
    }

    /// Retrieve the total number of non-terminal shipments currently tracked.
    ///
    /// Non-terminal shipments are those in one of the following states:
    /// 'Created', 'InTransit', 'AtCheckpoint', or 'PartiallyDelivered'.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<u64, NavinError>` - Total count of active (non-terminal) shipments.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    pub fn get_non_terminal_count(env: Env) -> Result<u64, NavinError> {
        require_initialized(&env)?;
        let count = storage::get_status_count(&env, &ShipmentStatus::Created)
            + storage::get_status_count(&env, &ShipmentStatus::InTransit)
            + storage::get_status_count(&env, &ShipmentStatus::AtCheckpoint)
            + storage::get_status_count(&env, &ShipmentStatus::PartiallyDelivered);
        Ok(count)
    }

    /// Get the deterministic SHA-256 checksum of critical config fields.
    ///
    /// This query exposes the config checksum to help indexers and operators
    /// detect unintended configuration drift. The checksum is computed from
    /// all config fields serialized in a fixed order and is automatically
    /// updated whenever the config changes.
    ///
    /// # Serialization Order
    /// Fields are serialized in declaration order:
    /// 1. shipment_ttl_threshold (u32)
    /// 2. shipment_ttl_extension (u32)
    /// 3. min_status_update_interval (u64)
    /// 4. batch_operation_limit (u32)
    /// 5. max_metadata_entries (u32)
    /// 6. default_shipment_limit (u32)
    /// 7. multisig_min_admins (u32)
    /// 8. multisig_max_admins (u32)
    /// 9. proposal_expiry_seconds (u64)
    /// 10. deadline_grace_seconds (u64)
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<BytesN<32>, NavinError>` - The SHA-256 checksum of the config.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let checksum = contract.get_config_checksum(&env)?;
    /// // Indexer can verify: checksum == sha256(serialized_config)
    /// ```
    pub fn get_config_checksum(env: Env) -> Result<BytesN<32>, NavinError> {
        require_initialized(&env)?;

        // Retrieve stored checksum, or compute it if not yet stored
        match config::get_config_checksum(&env) {
            Some(checksum) => Ok(checksum),
            None => {
                // Fallback: compute checksum from current config
                let current_config = config::get_config(&env);
                Ok(config::compute_config_checksum(&current_config, &env))
            }
        }
    }

    /// Compute the idempotency key for a shipment event.
    ///
    /// This helper enables off-chain indexers to recompute the same idempotency
    /// key that the contract emits in events. The key is used to deduplicate
    /// events during indexing and to protect against duplicate submissions of
    /// high-impact operations (e.g., dispute resolution).
    ///
    /// Canonical serialization order:
    /// 1. `shipment_id` as big-endian u64 (8 bytes)
    /// 2. `event_type` as XDR-encoded Symbol (variable-length)
    /// 3. `event_counter` as big-endian u32 (4 bytes)
    ///
    /// The concatenated byte vector is hashed with SHA-256 to produce a
    /// 32-byte idempotency key.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - The shipment identifier.
    /// * `event_type` - The event type symbol (e.g., "shipment_created").
    /// * `event_counter` - The per-shipment event counter value.
    ///
    /// # Returns
    /// * `BytesN<32>` - The idempotency key.
    ///
    /// # Examples
    /// ```rust
    /// let key = contract.compute_idempotency_key(&env, 1, Symbol::new(&env, "shipment_created"), 1);
    /// ```
    pub fn compute_idempotency_key(
        env: Env,
        shipment_id: u64,
        event_type: Symbol,
        event_counter: u32,
    ) -> BytesN<32> {
        use soroban_sdk::Bytes;

        let mut payload = Bytes::new(&env);

        // Domain prefix (HASH_DOMAIN_SHIPMENT = 0x01)
        let domain = crate::event_topics::HASH_DOMAIN_SHIPMENT;
        let domain_bytes = domain.to_be_bytes();
        let domain_len = (domain_bytes.len() as u32).to_be_bytes();
        payload.append(&Bytes::from_array(&env, &domain_len));
        payload.append(&Bytes::from_slice(&env, &domain_bytes));

        // Shipment ID (raw bytes)
        payload.append(&Bytes::from_array(&env, &shipment_id.to_be_bytes()));

        // Event type: use the same XDR encoding as generate_idempotency_key
        // (as_bytes() of the &str produces the same result as XDR for symbol strings)
        payload.append(&event_type.clone().to_xdr(&env));

        // Event counter (raw bytes)
        payload.append(&Bytes::from_array(&env, &event_counter.to_be_bytes()));

        env.crypto().sha256(&payload).into()
    }

    /// Add a carrier to a company's whitelist.
    /// Only the company can add carriers to their own whitelist.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `company` - The company's address acting as caller.
    /// * `carrier` - The carrier address to whitelist.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully registered.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // contract.add_carrier_to_whitelist(&env, &company, &carrier);
    /// ```
    pub fn add_carrier_to_whitelist(
        env: Env,
        company: Address,
        carrier: Address,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        company.require_auth();
        require_role(&env, &company, Role::Company)?;

        storage::add_carrier_to_whitelist(&env, &company, &carrier);

        env.events().publish(
            (symbol_short!("add_wl"),),
            (company.clone(), carrier.clone()),
        );

        Ok(())
    }

    /// Remove a carrier from a company's whitelist.
    /// Only the company can remove carriers from their own whitelist.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `company` - The company address removing the carrier.
    /// * `carrier` - The carrier address to be removed.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully removed.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // contract.remove_carrier_from_whitelist(&env, &company, &carrier);
    /// ```
    pub fn remove_carrier_from_whitelist(
        env: Env,
        company: Address,
        carrier: Address,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        company.require_auth();
        require_role(&env, &company, Role::Company)?;

        storage::remove_carrier_from_whitelist(&env, &company, &carrier);

        env.events().publish(
            (symbol_short!("rm_wl"),),
            (company.clone(), carrier.clone()),
        );

        Ok(())
    }

    /// Check if a carrier is whitelisted for a company.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `company` - The company address.
    /// * `carrier` - The carrier address in question.
    ///
    /// # Returns
    /// * `Result<bool, NavinError>` - True if the carrier is whitelisted.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let is_whitelisted = contract.is_carrier_whitelisted(&env, &company, &carrier);
    /// ```
    pub fn is_carrier_whitelisted(
        env: Env,
        company: Address,
        carrier: Address,
    ) -> Result<bool, NavinError> {
        require_initialized(&env)?;

        Ok(storage::is_carrier_whitelisted(&env, &company, &carrier))
    }

    /// Returns the role assigned to a given address.
    /// Returns Role::Unassigned if no role is assigned.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `address` - The address to check.
    ///
    /// # Returns
    /// * `Result<Role, NavinError>` - The role assigned to the address.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let role = contract.get_role(&env, &address);
    /// ```
    pub fn get_role(env: Env, address: Address) -> Result<Role, NavinError> {
        require_initialized(&env)?;
        Ok(storage::get_role(&env, &address).unwrap_or(Role::Unassigned))
    }

    /// Allow admin to grant Company role.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin executing the role grant.
    /// * `company` - The address receiving the company role.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful role assignment.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If called by a non-admin.
    ///
    /// # Examples
    /// ```rust
    /// // contract.add_company(&env, &admin, &new_company_addr);
    /// ```
    pub fn add_company(env: Env, admin: Address, company: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        admin.require_auth();

        require_admin_or_operator(&env, &admin)?;

        storage::set_company_role(&env, &company);

        // Emit role history event
        events::emit_role_changed(
            &env,
            &RoleChangeAction::Assigned,
            &admin,
            &company,
            &Role::Company,
        );

        Ok(())
    }

    /// Allow admin to grant Carrier role.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin executing the role grant.
    /// * `carrier` - The address receiving the carrier role.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful role assignment.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If called by a non-admin.
    ///
    /// # Examples
    /// ```rust
    /// // contract.add_carrier(&env, &admin, &new_carrier_addr);
    /// ```
    pub fn add_carrier(env: Env, admin: Address, carrier: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        admin.require_auth();

        require_admin_or_operator(&env, &admin)?;

        storage::set_carrier_role(&env, &carrier);

        // Emit role history event
        events::emit_role_changed(
            &env,
            &RoleChangeAction::Assigned,
            &admin,
            &carrier,
            &Role::Carrier,
        );

        Ok(())
    }

    /// Allow admin to grant Guardian role.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin executing the role grant.
    /// * `guardian` - The address receiving the guardian role.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful role assignment.
    pub fn add_guardian(env: Env, admin: Address, guardian: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        storage::set_role(&env, &guardian, &Role::Guardian);

        events::emit_role_changed(
            &env,
            &RoleChangeAction::Assigned,
            &admin,
            &guardian,
            &Role::Guardian,
        );

        Ok(())
    }

    /// Allow admin to grant Operator role.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin executing the role grant.
    /// * `operator` - The address receiving the operator role.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful role assignment.
    pub fn add_operator(env: Env, admin: Address, operator: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        storage::set_role(&env, &operator, &Role::Operator);

        events::emit_role_changed(
            &env,
            &RoleChangeAction::Assigned,
            &admin,
            &operator,
            &Role::Operator,
        );

        Ok(())
    }

    /// Suspend a carrier from carrier-only operations.
    ///
    /// Only the admin can call this function.
    pub fn suspend_carrier(env: Env, admin: Address, carrier: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        admin.require_auth();

        require_admin_or_operator(&env, &admin)?;

        storage::suspend_carrier(&env, &carrier);
        env.events()
            .publish((Symbol::new(&env, "carrier_suspended"),), (admin, carrier));
        Ok(())
    }

    /// Reactivate a previously suspended carrier.
    ///
    /// Only the admin can call this function.
    pub fn reactivate_carrier(
        env: Env,
        admin: Address,
        carrier: Address,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        admin.require_auth();

        require_admin_or_operator(&env, &admin)?;

        storage::reactivate_carrier(&env, &carrier);
        env.events().publish(
            (Symbol::new(&env, "carrier_reactivated"),),
            (admin, carrier),
        );
        Ok(())
    }

    /// Return whether a carrier is currently suspended.
    pub fn is_carrier_suspended(env: Env, carrier: Address) -> Result<bool, NavinError> {
        require_initialized(&env)?;
        Ok(storage::is_carrier_suspended(&env, &carrier))
    }

    /// Revoke a previously assigned role from an address.
    ///
    /// Only the admin can revoke roles. The admin cannot revoke their own role;
    /// use `transfer_admin` instead.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin executing the revocation.
    /// * `target` - The address whose role is being revoked.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful role revocation.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If called by a non-admin.
    /// * `NavinError::CannotSelfRevoke` - If admin tries to revoke their own role.
    ///
    /// # Examples
    /// ```rust
    /// // contract.revoke_role(&env, &admin, &target_addr);
    /// ```
    pub fn revoke_role(env: Env, admin: Address, target: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        if admin == target {
            return Err(NavinError::CannotSelfRevoke);
        }

        let current_role = storage::get_role(&env, &target).unwrap_or(Role::Unassigned);

        match current_role {
            Role::Company => storage::revoke_role(&env, &target, &Role::Company),
            Role::Carrier => storage::revoke_role(&env, &target, &Role::Carrier),
            Role::Guardian => storage::revoke_role(&env, &target, &Role::Guardian),
            Role::Operator => storage::revoke_role(&env, &target, &Role::Operator),
            Role::Unassigned => {}
            Role::Observer => {}
        }

        events::emit_role_revoked(&env, &admin, &target, &current_role);

        // Emit role history event for audit trail
        events::emit_role_changed(
            &env,
            &RoleChangeAction::Revoked,
            &admin,
            &target,
            &current_role,
        );

        Ok(())
    }

    /// Suspend a role temporarily (e.g., for investigation or compliance review).
    ///
    /// Only the admin can suspend roles. Suspended addresses retain their role
    /// assignment but cannot perform role-gated actions until reactivated.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin executing the suspension.
    /// * `target` - The address whose role is being suspended.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful suspension.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If called by a non-admin.
    /// * `NavinError::CannotSelfRevoke` - If admin tries to suspend their own role.
    ///
    /// # Examples
    /// ```rust
    /// // contract.suspend_role(&env, &admin, &target_addr);
    /// ```
    pub fn suspend_role(env: Env, admin: Address, target: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        if admin == target {
            return Err(NavinError::CannotSelfRevoke);
        }

        let current_role = storage::get_role(&env, &target).unwrap_or(Role::Unassigned);

        if current_role == Role::Unassigned {
            return Err(NavinError::Unauthorized);
        }

        // Mark as suspended in storage
        storage::suspend_role(&env, &target, &current_role);

        // Emit role history event
        events::emit_role_changed(
            &env,
            &RoleChangeAction::Suspended,
            &admin,
            &target,
            &current_role,
        );

        Ok(())
    }

    /// Reactivate a previously suspended role.
    ///
    /// Only the admin can reactivate roles. This restores the address's
    /// ability to perform role-gated actions.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin executing the reactivation.
    /// * `target` - The address whose role is being reactivated.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful reactivation.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If called by a non-admin or target not suspended.
    ///
    /// # Examples
    /// ```rust
    /// // contract.reactivate_role(&env, &admin, &target_addr);
    /// ```
    pub fn reactivate_role(env: Env, admin: Address, target: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        let current_role = storage::get_role(&env, &target).unwrap_or(Role::Unassigned);

        if current_role == Role::Unassigned {
            return Err(NavinError::Unauthorized);
        }

        // Reactivate the role
        storage::reactivate_role(&env, &target, &current_role);

        // Emit role history event
        events::emit_role_changed(
            &env,
            &RoleChangeAction::Reactivated,
            &admin,
            &target,
            &current_role,
        );

        Ok(())
    }

    /// Suspend a company from creating or updating shipments.
    pub fn suspend_company(env: Env, admin: Address, company: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        admin.require_auth();

        require_admin_or_operator(&env, &admin)?;

        if !storage::has_company_role(&env, &company) {
            return Err(NavinError::Unauthorized);
        }

        storage::suspend_company(&env, &company);

        // Emit role history event (reusing Reactive/Suspended for audit)
        events::emit_role_changed(
            &env,
            &RoleChangeAction::Suspended,
            &admin,
            &company,
            &Role::Company,
        );

        Ok(())
    }

    /// Reactivate a suspended company.
    pub fn reactivate_company(
        env: Env,
        admin: Address,
        company: Address,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        admin.require_auth();

        require_admin_or_operator(&env, &admin)?;

        storage::reactivate_company(&env, &company);

        // Emit role history event
        events::emit_role_changed(
            &env,
            &RoleChangeAction::Reactivated,
            &admin,
            &company,
            &Role::Company,
        );

        Ok(())
    }

    /// Create a shipment and emit the shipment_created event.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `sender` - Company address creating the shipment.
    /// * `receiver` - Destination address for the shipment.
    /// * `carrier` - Carrier address assigned to the shipment.
    /// * `data_hash` - Off-chain data hash of shipment details.
    /// * `payment_milestones` - Schedule for escrow releases based on checkpoints.
    /// * `deadline` - Timestamp after which shipment is considered expired and can be auto-cancelled.
    ///
    /// # Returns
    /// * `Result<u64, NavinError>` - Newly created shipment ID.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller isn't a Company.
    /// * `NavinError::InvalidHash` - If data_hash is all zeros.
    /// * `NavinError::MilestoneSumInvalid` - If milestone percentages do not equal 100%.
    /// * `NavinError::CounterOverflow` - If total shipment count overflows max u64.
    /// * `NavinError::InvalidTimestamp` - If the deadline is not strictly in the future.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use soroban_sdk::{Env, Address, BytesN, Vec, Symbol};
    /// # use soroban_sdk::testutils::Address as _;
    /// # use shipment::{NavinShipment, NavinShipmentClient};
    /// # let env = Env::default();
    /// # env.mock_all_auths();
    /// # let contract_id = env.register(NavinShipment, ());
    /// # let client = NavinShipmentClient::new(&env, &contract_id);
    /// # let admin = Address::generate(&env);
    /// # let token = Address::generate(&env);
    /// # client.initialize(&admin, &token);
    /// # client.add_company(&admin, &admin);
    /// let sender = admin.clone();
    /// let receiver = Address::generate(&env);
    /// let carrier = Address::generate(&env);
    /// let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    /// let milestones: Vec<(Symbol, u32)> = Vec::new(&env); // no milestone splits
    /// let deadline = env.ledger().timestamp() + 86_400; // 1 day from now
    ///
    /// let shipment_id = client.create_shipment(
    ///     &sender, &receiver, &carrier, &data_hash, &milestones, &deadline,
    /// , &None);
    /// assert_eq!(shipment_id, 1);
    /// ```
    pub fn create_shipment(
        env: Env,
        sender: Address,
        receiver: Address,
        carrier: Address,
        data_hash: BytesN<32>,
        payment_milestones: Vec<(Symbol, u32)>,
        deadline: u64,
        depends_on: Option<Vec<u64>>,
    ) -> Result<u64, NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        sender.require_auth();
        require_role(&env, &sender, Role::Company)?;
        validate_milestones(&env, &payment_milestones)?;
        validate_hash(&data_hash)?;

        // Idempotency: reject duplicate (sender, data_hash) within the window.
        let mut payload = soroban_sdk::Bytes::new(&env);
        payload.append(&sender.clone().to_xdr(&env));
        payload.append(&data_hash.clone().into());
        check_idempotency(&env, payload)?;

        let now = env.ledger().timestamp();
        if deadline <= now {
            return Err(NavinError::InvalidTimestamp);
        }

        // Check company active shipment limit
        let current_active = storage::get_active_shipment_count(&env, &sender);
        let limit = storage::get_effective_shipment_limit(&env, &sender);
        if current_active >= limit {
            return Err(NavinError::ShipmentLimitReached);
        }

        // Check per-company creation quota window (issue #296).
        check_and_update_creation_quota(&env, &sender)?;

        let shipment_id = storage::get_shipment_counter(&env)
            .checked_add(1)
            .ok_or(NavinError::CounterOverflow)?;

        validation::validate_dependencies(&env, shipment_id, &depends_on)?;

        let shipment = Shipment {
            id: shipment_id,
            sender: sender.clone(),
            receiver: receiver.clone(),
            carrier,
            data_hash: data_hash.clone(),
            status: ShipmentStatus::Created,
            created_at: now,
            updated_at: now,
            escrow_amount: 0,
            total_escrow: 0,
            payment_milestones,
            paid_milestones: Vec::new(&env),
            milestones_completed: Vec::new(&env),
            metadata: None,
            deadline,
            integration_nonce: 0,
            finalized: false,
            depends_on: depends_on.clone(),
        };

        persist_shipment(&env, &shipment)?;
        if let Some(dependencies) = depends_on {
            storage::set_dependencies(&env, shipment_id, &dependencies);
            for dep_id in dependencies.iter() {
                storage::add_dependent(&env, dep_id, shipment_id);
            }
        }
        storage::set_shipment_counter(&env, shipment_id);
        storage::increment_status_count(&env, &ShipmentStatus::Created);
        storage::increment_active_shipment_count(&env, &sender);
        extend_shipment_ttl(&env, shipment_id);

        events::emit_shipment_created(&env, shipment_id, &sender, &receiver, &data_hash);
        events::emit_notification(
            &env,
            &receiver,
            NotificationType::ShipmentCreated,
            shipment_id,
            &data_hash,
        );
        events::emit_notification(
            &env,
            &shipment.carrier,
            NotificationType::ShipmentCreated,
            shipment_id,
            &data_hash,
        );

        Ok(shipment_id)
    }

    /// Create multiple shipments in a single atomic transaction.
    /// Limit: 10 shipments per batch.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `sender` - Company address creating shipments.
    /// * `shipments` - Vector of shipment inputs.
    ///
    /// # Returns
    /// * `Result<Vec<u64>, NavinError>` - Vector of newly created shipment IDs.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller isn't a Company.
    /// * `NavinError::BatchTooLarge` - If more than 10 shipments are submitted.
    /// * `NavinError::InvalidShipmentInput` - If receiver matches carrier for any shipment.
    /// * `NavinError::InvalidHash` - If any data_hash is all zeros.
    /// * `NavinError::MilestoneSumInvalid` - If payment milestones are invalid per item.
    /// * `NavinError::InvalidTimestamp` - If the deadline is not strictly in the future.
    ///
    /// # Examples
    /// ```rust
    /// // let ids = contract.create_shipments_batch(&env, &sender, inputs_vec);
    /// ```
    pub fn create_shipments_batch(
        env: Env,
        sender: Address,
        shipments: Vec<ShipmentInput>,
    ) -> Result<Vec<u64>, NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        sender.require_auth();
        require_role(&env, &sender, Role::Company)?;

        let config = config::get_config(&env);
        if shipments.len() > config.batch_operation_limit {
            return Err(NavinError::BatchTooLarge);
        }

        let mut ids = Vec::new(&env);
        let now = env.ledger().timestamp();

        // Check batch size against limit
        let current_active = storage::get_active_shipment_count(&env, &sender);
        let limit = storage::get_effective_shipment_limit(&env, &sender);
        if current_active.saturating_add(shipments.len()) > limit {
            return Err(NavinError::ShipmentLimitReached);
        }

        // Check per-company creation quota window for the entire batch (issue #296).
        // We check once per item to correctly track the rolling count.
        {
            let cfg = config::get_config(&env);
            if cfg.creation_quota_max > 0 {
                let now_ts = env.ledger().timestamp();
                let mut tracker = storage::get_creation_quota(&env, &sender).unwrap_or(
                    crate::types::CreationQuotaTracker {
                        count: 0,
                        window_start: now_ts,
                    },
                );
                if now_ts >= tracker.window_start + cfg.creation_quota_window_seconds {
                    tracker.window_start = now_ts;
                    tracker.count = 0;
                }
                let batch_len = shipments.len();
                let new_count = tracker
                    .count
                    .checked_add(batch_len)
                    .ok_or(NavinError::CounterOverflow)?;
                if new_count > cfg.creation_quota_max {
                    return Err(NavinError::CreationQuotaExceeded);
                }
                tracker.count = new_count;
                storage::set_creation_quota(&env, &sender, &tracker);
            }
        }

        for shipment_input in shipments.iter() {
            if shipment_input.receiver == shipment_input.carrier {
                return Err(NavinError::InvalidShipmentInput);
            }
            validate_milestones(&env, &shipment_input.payment_milestones)?;
            validate_hash(&shipment_input.data_hash)?;

            if shipment_input.deadline <= now {
                return Err(NavinError::InvalidTimestamp);
            }

            let shipment_id = storage::get_shipment_counter(&env)
                .checked_add(1)
                .ok_or(NavinError::CounterOverflow)?;

            validation::validate_dependencies(
                &env,
                shipment_id,
                &shipment_input.depends_on,
            )?;

            let shipment = Shipment {
                id: shipment_id,
                sender: sender.clone(),
                receiver: shipment_input.receiver.clone(),
                carrier: shipment_input.carrier.clone(),
                data_hash: shipment_input.data_hash.clone(),
                status: ShipmentStatus::Created,
                created_at: now,
                updated_at: now,
                escrow_amount: 0,
                total_escrow: 0,
                payment_milestones: shipment_input.payment_milestones,
                paid_milestones: Vec::new(&env),
                milestones_completed: Vec::new(&env),
                metadata: None,
                deadline: shipment_input.deadline,
                integration_nonce: 0,
                finalized: false,
                depends_on: shipment_input.depends_on.clone(),
            };

            persist_shipment(&env, &shipment)?;
            storage::set_shipment_counter(&env, shipment_id);
            storage::increment_status_count(&env, &ShipmentStatus::Created);
            storage::increment_active_shipment_count(&env, &sender);
            // Use the cached-config variant to avoid re-reading config from storage per item.
            extend_shipment_ttl_cached(
                &env,
                shipment_id,
                config.shipment_ttl_threshold,
                config.shipment_ttl_extension,
            );

            events::emit_shipment_created(
                &env,
                shipment_id,
                &sender,
                &shipment_input.receiver,
                &shipment_input.data_hash,
            );
            events::emit_notification(
                &env,
                &shipment_input.receiver,
                NotificationType::ShipmentCreated,
                shipment_id,
                &shipment_input.data_hash,
            );
            events::emit_notification(
                &env,
                &shipment_input.carrier,
                NotificationType::ShipmentCreated,
                shipment_id,
                &shipment_input.data_hash,
            );
            ids.push_back(shipment_id);
        }

        Ok(ids)
    }

    /// Retrieve shipment details by ID.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment to fetch.
    ///
    /// # Returns
    /// * `Result<Shipment, NavinError>` - Reconstructed shipment struct.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    ///
    /// # Examples
    /// ```rust
    /// // let shipment = contract.get_shipment(&env, 1);
    /// ```
    pub fn get_shipment(env: Env, shipment_id: u64) -> Result<Shipment, NavinError> {
        require_initialized(&env)?;
        storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)
    }

    /// Retrieve the immutable creator identity for a shipment.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment.
    ///
    /// # Returns
    /// * `Result<Address, NavinError>` - Address that originally created the shipment.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    pub fn get_shipment_creator(env: Env, shipment_id: u64) -> Result<Address, NavinError> {
        require_initialized(&env)?;
        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;
        Ok(shipment.sender)
    }

    /// Retrieve the immutable receiver identity for a shipment.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment.
    ///
    /// # Returns
    /// * `Result<Address, NavinError>` - Address designated as shipment receiver at creation.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    pub fn get_shipment_receiver(env: Env, shipment_id: u64) -> Result<Address, NavinError> {
        require_initialized(&env)?;
        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;
        Ok(shipment.receiver)
    }

    /// Retrieve the creation timestamp for a shipment.
    /// Retrieve the immutable sender (creator) identity for a shipment.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment.
    ///
    /// # Returns
    /// * `Result<u64, NavinError>` - Ledger timestamp of creation.
    pub fn get_shipment_created_at(env: Env, shipment_id: u64) -> Result<u64, NavinError> {
        require_initialized(&env)?;
        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;
        Ok(shipment.created_at)
    }

    /// Retrieve the last update timestamp for a shipment.
    /// * `Result<Address, NavinError>` - Address that originally created the shipment.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    pub fn get_shipment_sender(env: Env, shipment_id: u64) -> Result<Address, NavinError> {
        require_initialized(&env)?;
        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;
        Ok(shipment.sender)
    }

    /// Retrieve the immutable carrier identity for a shipment.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment.
    ///
    /// # Returns
    /// * `Result<u64, NavinError>` - Ledger timestamp of the last update.
    pub fn get_shipment_updated_at(env: Env, shipment_id: u64) -> Result<u64, NavinError> {
        require_initialized(&env)?;
        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;
        Ok(shipment.updated_at)
    }

    /// * `Result<Address, NavinError>` - Address designated as shipment carrier at creation.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    pub fn get_shipment_carrier(env: Env, shipment_id: u64) -> Result<Address, NavinError> {
        require_initialized(&env)?;
        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;
        Ok(shipment.carrier)
    }

    /// Return read-only diagnostics that help operators triage restore requirements.
    ///
    /// This query does not mutate state. It classifies the shipment ID as active,
    /// archived-expected, missing, or inconsistent (both active and archived present).
    pub fn get_restore_diagnostics(
        env: Env,
        shipment_id: u64,
    ) -> Result<PersistentRestoreDiagnostics, NavinError> {
        require_initialized(&env)?;

        let persistent_shipment_present = storage::has_persistent_shipment(&env, shipment_id);
        let archived_shipment_present = storage::is_shipment_archived(&env, shipment_id);

        let state = if persistent_shipment_present && archived_shipment_present {
            StoragePresenceState::InconsistentDualPresence
        } else if persistent_shipment_present {
            StoragePresenceState::ActivePersistent
        } else if archived_shipment_present {
            StoragePresenceState::ArchivedExpected
        } else {
            StoragePresenceState::Missing
        };

        Ok(PersistentRestoreDiagnostics {
            shipment_id,
            state,
            persistent_shipment_present,
            archived_shipment_present,
            escrow_present: storage::has_escrow_entry(&env, shipment_id),
            confirmation_hash_present: storage::has_confirmation_hash_entry(&env, shipment_id),
            last_status_update_present: storage::has_last_status_update_entry(&env, shipment_id),
            event_count_present: storage::has_event_count_entry(&env, shipment_id),
        })
    }

    /// Deposit escrow funds for a shipment.
    /// Only a Company can deposit, and the shipment must be in Created status.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `from` - Company address providing escrow.
    /// * `shipment_id` - Target shipment.
    /// * `amount` - Balance of tokens deposited into escrow.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful deposit.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller isn't a Company.
    /// * `NavinError::InvalidAmount` - If amount is zero, negative, or exceeds the maximum.
    /// * `NavinError::ShipmentNotFound` - If shipment is untracked.
    /// * `NavinError::InvalidStatus` - If shipment is not in `Created` status.
    /// * `NavinError::EscrowLocked` - If escrow is already deposited for shipment.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use soroban_sdk::{Env, Address, BytesN, Vec, Symbol};
    /// # use soroban_sdk::testutils::Address as _;
    /// # use shipment::{NavinShipment, NavinShipmentClient};
    /// # let env = Env::default();
    /// # env.mock_all_auths();
    /// # let contract_id = env.register(NavinShipment, ());
    /// # let client = NavinShipmentClient::new(&env, &contract_id);
    /// # let admin = Address::generate(&env);
    /// # let token = Address::generate(&env);
    /// # client.initialize(&admin, &token);
    /// # client.add_company(&admin, &admin);
    /// # let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    /// # let milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    /// # let deadline = env.ledger().timestamp() + 86_400;
    /// # let receiver = Address::generate(&env);
    /// # let carrier = Address::generate(&env);
    /// # let shipment_id = client.create_shipment(&admin, &receiver, &carrier, &data_hash, &milestones, &deadline, &None);
    /// // Deposit 5_000_000 stroops (0.5 tokens) into escrow for the shipment.
    /// // The company must have pre-approved the token transfer allowance.
    /// client.deposit_escrow(&admin, &shipment_id, &5_000_000_i128);
    /// ```
    pub fn deposit_escrow(
        env: Env,
        from: Address,
        shipment_id: u64,
        amount: i128,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        from.require_auth();
        require_role(&env, &from, Role::Company)?;

        with_reentrancy_lock(&env, || {
            validation::validate_positive_amount(amount)?;

            let mut shipment =
                storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

            require_not_finalized(&shipment)?;

            if shipment.status != ShipmentStatus::Created {
                return Err(NavinError::InvalidStatus);
            }

            if shipment.escrow_amount > 0 {
                return Err(NavinError::EscrowLocked);
            }

            // Get token contract address
            let token_contract =
                storage::get_token_contract(&env).ok_or(NavinError::NotInitialized)?;

            // Validate that the token uses 7 decimal places (Stellar standard).
            // This prevents silent amount mismatches for non-standard tokens.
            validate_token_decimals(&env, &token_contract)?;

            // Create settlement record in Pending state
            let contract_address = env.current_contract_address();
            let settlement_id = create_settlement(
                &env,
                shipment_id,
                SettlementOperation::Deposit,
                amount,
                &from,
                &contract_address,
            )?;

            // Transfer tokens from user to this contract
            let transfer_result =
                invoke_token_transfer(&env, &token_contract, &from, &contract_address, amount);

            match transfer_result {
                Ok(()) => {
                    complete_settlement(&env, settlement_id, shipment_id)?;

                    let mut net_amount = amount;
                    if let Some(fee_config) = storage::get_fee_config(&env) {
                        if fee_config.fee_bps > 0 {
                            let fee_amount =
                                checked_mul_div_i128(amount, fee_config.fee_bps as i128, 10000)?;
                            if fee_amount > 0 {
                                // Transfer fee from this contract to treasury
                                invoke_token_transfer(
                                    &env,
                                    &token_contract,
                                    &contract_address,
                                    &fee_config.treasury,
                                    fee_amount,
                                )?;
                                net_amount = checked_sub_i128(amount, fee_amount)?;
                                events::emit_platform_fee_collected(
                                    &env,
                                    shipment_id,
                                    &fee_config.treasury,
                                    fee_amount,
                                );
                            }
                        }
                    }

                    shipment.escrow_amount = net_amount;
                    shipment.total_escrow = net_amount;
                    shipment.updated_at = env.ledger().timestamp();
                    shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);
                    persist_shipment(&env, &shipment)?;
                    storage::set_escrow(&env, shipment_id, net_amount);
                    storage::add_total_escrow_volume(&env, amount)?;
                    extend_shipment_ttl(&env, shipment_id);

                    events::emit_escrow_deposited(&env, shipment_id, &from, net_amount);
                }
                Err(e) => {
                    fail_settlement(&env, settlement_id, shipment_id, e as u32)?;
                    return Err(e);
                }
            }

            Ok(())
        })
    }

    /// Update shipment status with transition validation.
    /// Only the carrier or admin can update the status.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `caller` - Carrier or admin address making the update.
    /// * `shipment_id` - Current shipment identifier.
    /// * `new_status` - The destination transitional status.
    /// * `data_hash` - The off-chain data hash tracking context for update.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on valid transition.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If shipment doesn't exist.
    /// * `NavinError::Unauthorized` - If caller is neither the carrier nor admin.
    /// * `NavinError::InvalidHash` - If data_hash is all zeros.
    /// * `NavinError::CarrierSuspended` - If the assigned carrier is suspended.
    /// * `NavinError::RateLimitExceeded` - If status was updated too recently (unless Admin).
    /// * `NavinError::InvalidStatus` - If transitioning to an improperly sequenced state.
    ///
    /// # Examples
    /// ```rust
    /// ```rust,no_run
    /// # use soroban_sdk::{Env, Address, BytesN, Vec, Symbol};
    /// # use soroban_sdk::testutils::Address as _;
    /// # use shipment::{NavinShipment, NavinShipmentClient, ShipmentStatus};
    /// # let env = Env::default();
    /// # env.mock_all_auths();
    /// # let contract_id = env.register(NavinShipment, ());
    /// # let client = NavinShipmentClient::new(&env, &contract_id);
    /// # let admin = Address::generate(&env);
    /// # let token = Address::generate(&env);
    /// # client.initialize(&admin, &token);
    /// # client.add_company(&admin, &admin);
    /// # let carrier = Address::generate(&env);
    /// # client.add_carrier(&admin, &carrier);
    /// # let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    /// # let milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    /// # let deadline = env.ledger().timestamp() + 86_400;
    /// # let receiver = Address::generate(&env);
    /// # let shipment_id = client.create_shipment(&admin, &receiver, &carrier, &data_hash, &milestones, &deadline, &None);
    /// let transit_hash = BytesN::from_array(&env, &[2u8; 32]);
    ///
    /// // Carrier moves shipment from Created -> InTransit.
    /// client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &transit_hash);
    /// ```
    pub fn update_status(
        env: Env,
        caller: Address,
        shipment_id: u64,
        new_status: ShipmentStatus,
        data_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        caller.require_auth();

        // Validate hash before storage
        validation::validate_hash(&data_hash)?;

        let admin = storage::get_admin(&env);
        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        if caller != shipment.carrier && caller != admin {
            return Err(NavinError::Unauthorized);
        }
        require_not_finalized(&shipment)?;
        if caller == shipment.carrier {
            require_active_carrier(&env, &caller)?;
        }

        // Idempotency: reject duplicate (shipment_id, new_status, data_hash) within the window.
        let mut payload = soroban_sdk::Bytes::new(&env);
        payload.append(&soroban_sdk::Bytes::from_array(
            &env,
            &shipment_id.to_be_bytes(),
        ));
        payload.append(&new_status.clone().to_xdr(&env));
        payload.append(&data_hash.clone().into());
        check_idempotency(&env, payload)?;

        // Rate-limit check: admin bypasses; all other callers must wait the minimum interval.
        if caller != admin {
            if let Some(last) = storage::get_last_status_update(&env, shipment_id) {
                let now = env.ledger().timestamp();
                let config = config::get_config(&env);
                if now.saturating_sub(last) < config.min_status_update_interval {
                    return Err(NavinError::RateLimitExceeded);
                }
            }
        }

        if !shipment.status.is_valid_transition(&new_status) {
            return Err(NavinError::InvalidStatus);
        }

        if matches!(new_status, ShipmentStatus::InTransit | ShipmentStatus::Delivered) {
            let unmet = validation::collect_unmet_dependencies(&env, shipment_id)?;
            if !unmet.is_empty() {
                events::emit_shipment_blocked(&env, shipment_id, &caller, unmet);
                return Err(NavinError::DependenciesNotMet);
            }
        }

        let old_status = shipment.status.clone();
        shipment.status = new_status.clone();
        shipment.data_hash = data_hash.clone();
        shipment.updated_at = env.ledger().timestamp();
        shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);

        storage::decrement_status_count(&env, &old_status);
        storage::increment_status_count(&env, &shipment.status);

        finalize_if_settled(&env, &mut shipment);
        persist_shipment(&env, &shipment)?;

        if shipment.status == ShipmentStatus::Disputed {
            storage::increment_total_disputes(&env);
        }

        storage::set_last_status_update(&env, shipment_id, env.ledger().timestamp());
        extend_shipment_ttl(&env, shipment_id);

        // Store the data hash for this status transition (IoT verification)
        storage::set_status_hash(&env, shipment_id, &new_status, &data_hash);

        events::emit_status_updated(&env, shipment_id, &old_status, &new_status, &data_hash);
        events::emit_notification(
            &env,
            &shipment.sender,
            NotificationType::StatusChanged,
            shipment_id,
            &data_hash,
        );
        events::emit_notification(
            &env,
            &shipment.receiver,
            NotificationType::StatusChanged,
            shipment_id,
            &data_hash,
        );

        Ok(())
    }

    /// Returns the current escrowed amount for a specific shipment.
    /// Returns 0 if no escrow has been deposited.
    /// Returns ShipmentNotFound if the shipment does not exist.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment.
    ///
    /// # Returns
    /// * `Result<i128, NavinError>` - Amount stored in escrow.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    ///
    /// # Examples
    /// ```rust
    /// // let balance = contract.get_escrow_balance(&env, 1);
    /// ```
    pub fn get_escrow_balance(env: Env, shipment_id: u64) -> Result<i128, NavinError> {
        require_initialized(&env)?;
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }
        Ok(storage::get_escrow_balance(&env, shipment_id))
    }

    /// Return the dependency list for a shipment.
    pub fn get_dependencies(env: Env, shipment_id: u64) -> Result<Vec<u64>, NavinError> {
        require_initialized(&env)?;
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }
        Ok(storage::get_dependencies(&env, shipment_id).unwrap_or_else(|| Vec::new(&env)))
    }

    /// Return shipments that are blocked on a prerequisite shipment.
    pub fn get_blocked_shipments(env: Env, shipment_id: u64) -> Result<Vec<u64>, NavinError> {
        require_initialized(&env)?;
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }
        Ok(storage::get_dependents(&env, shipment_id).unwrap_or_else(|| Vec::new(&env)))
    }

    /// Get the latest structured escrow freeze reason for a shipment, if present.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment.
    ///
    /// # Returns
    /// * `Result<Option<EscrowFreezeReason>, NavinError>` - Latest freeze reason code.
    pub fn get_escrow_freeze_reason(
        env: Env,
        shipment_id: u64,
    ) -> Result<Option<EscrowFreezeReason>, NavinError> {
        require_initialized(&env)?;
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }
        Ok(storage::get_escrow_freeze_reason(&env, shipment_id))
    }

    /// Get a settlement record by ID.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `settlement_id` - The ID of the settlement.
    ///
    /// # Returns
    /// * `Result<SettlementRecord, NavinError>` - The settlement record.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If settlement doesn't exist (reusing error).
    ///
    /// # Examples
    /// ```rust
    /// // let settlement = contract.get_settlement(&env, 1);
    /// ```
    pub fn get_settlement(env: Env, settlement_id: u64) -> Result<SettlementRecord, NavinError> {
        require_initialized(&env)?;
        storage::get_settlement(&env, settlement_id).ok_or(NavinError::ShipmentNotFound)
    }

    /// Get the active settlement ID for a shipment.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - The ID of the shipment.
    ///
    /// # Returns
    /// * `Result<Option<u64>, NavinError>` - The active settlement ID if one exists.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let active_id = contract.get_active_settlement(&env, 1);
    /// ```
    pub fn get_active_settlement(env: Env, shipment_id: u64) -> Result<Option<u64>, NavinError> {
        require_initialized(&env)?;
        Ok(storage::get_active_settlement(&env, shipment_id))
    }

    /// Get the total number of settlements created.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `u64` - The total settlement count.
    ///
    /// # Examples
    /// ```rust
    /// // let count = contract.get_settlement_count(&env);
    /// ```
    pub fn get_settlement_count(env: Env) -> u64 {
        storage::get_settlement_counter(&env)
    }

    /// Returns the total number of shipments created on the platform.
    /// Returns 0 if the contract has not been initialized.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `u64` - Overall total shipments registered.
    ///
    /// # Examples
    /// ```rust
    /// // let total = contract.get_shipment_count(&env);
    /// ```
    pub fn get_shipment_count(env: Env) -> u64 {
        storage::get_shipment_counter(&env)
    }

    /// Fetch multiple shipments in one call while preserving input order.
    ///
    /// Returns `None` for unknown IDs instead of failing the entire request.
    pub fn get_shipments_batch(
        env: Env,
        shipment_ids: Vec<u64>,
    ) -> Result<Vec<Option<Shipment>>, NavinError> {
        require_initialized(&env)?;

        let max_batch = effective_batch_query_limit(&env);
        if shipment_ids.len() > max_batch {
            return Err(NavinError::BatchTooLarge);
        }

        let mut results = Vec::new(&env);
        for shipment_id in shipment_ids.iter() {
            results.push_back(storage::get_shipment(&env, shipment_id));
        }

        Ok(results)
    }

    /// Filter shipments by sender with optional offset pagination.
    pub fn get_shipments_by_sender(
        env: Env,
        sender: Address,
        limit: u32,
    ) -> Result<Vec<Shipment>, NavinError> {
        Self::get_shipments_by_sender_page(env, sender, 0, limit)
    }

    /// Filter shipments by sender with offset pagination.
    pub fn get_shipments_by_sender_page(
        env: Env,
        sender: Address,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Shipment>, NavinError> {
        require_initialized(&env)?;
        let max_batch = effective_batch_query_limit(&env);
        if limit == 0 || limit > max_batch {
            return Err(NavinError::InvalidConfig);
        }

        let mut matched = Vec::new(&env);
        let mut skipped = 0_u32;
        let mut collected = 0_u32;
        let total_shipments = storage::get_shipment_counter(&env);

        for shipment_id in 1..=total_shipments {
            if let Some(shipment) = storage::get_shipment(&env, shipment_id) {
                if shipment.sender != sender {
                    continue;
                }
                if skipped < offset {
                    skipped = skipped.saturating_add(1);
                    continue;
                }
                matched.push_back(shipment);
                collected = collected.saturating_add(1);
                if collected >= limit {
                    break;
                }
            }
        }

        Ok(matched)
    }

    /// Filter shipments by carrier with optional offset pagination.
    pub fn get_shipments_by_carrier(
        env: Env,
        carrier: Address,
        limit: u32,
    ) -> Result<Vec<Shipment>, NavinError> {
        Self::get_shipments_by_carrier_page(env, carrier, 0, limit)
    }

    /// Filter shipments by carrier with offset pagination.
    pub fn get_shipments_by_carrier_page(
        env: Env,
        carrier: Address,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Shipment>, NavinError> {
        require_initialized(&env)?;
        let max_batch = effective_batch_query_limit(&env);
        if limit == 0 || limit > max_batch {
            return Err(NavinError::InvalidConfig);
        }

        let mut matched = Vec::new(&env);
        let mut skipped = 0_u32;
        let mut collected = 0_u32;
        let total_shipments = storage::get_shipment_counter(&env);

        for shipment_id in 1..=total_shipments {
            if let Some(shipment) = storage::get_shipment(&env, shipment_id) {
                if shipment.carrier != carrier {
                    continue;
                }
                if skipped < offset {
                    skipped = skipped.saturating_add(1);
                    continue;
                }
                matched.push_back(shipment);
                collected = collected.saturating_add(1);
                if collected >= limit {
                    break;
                }
            }
        }

        Ok(matched)
    }

    /// Filter shipments by status with optional offset pagination.
    pub fn get_shipments_by_status(
        env: Env,
        status: ShipmentStatus,
        limit: u32,
    ) -> Result<Vec<Shipment>, NavinError> {
        Self::get_shipments_by_status_page(env, status, 0, limit)
    }

    /// Filter shipments by status with offset pagination.
    pub fn get_shipments_by_status_page(
        env: Env,
        status: ShipmentStatus,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Shipment>, NavinError> {
        require_initialized(&env)?;
        let max_batch = effective_batch_query_limit(&env);
        if limit == 0 || limit > max_batch {
            return Err(NavinError::InvalidConfig);
        }

        let mut matched = Vec::new(&env);
        let mut skipped = 0_u32;
        let mut collected = 0_u32;
        let total_shipments = storage::get_shipment_counter(&env);

        for shipment_id in 1..=total_shipments {
            if let Some(shipment) = storage::get_shipment(&env, shipment_id) {
                if shipment.status != status {
                    continue;
                }
                if skipped < offset {
                    skipped = skipped.saturating_add(1);
                    continue;
                }
                matched.push_back(shipment);
                collected = collected.saturating_add(1);
                if collected >= limit {
                    break;
                }
            }
        }

        Ok(matched)
    }

    /// Cursor-based search for shipment IDs by status.
    ///
    /// Results are returned in ascending shipment ID order for deterministic pagination.
    /// `cursor` is the last seen shipment ID from a previous page.
    pub fn search_shipments_by_status(
        env: Env,
        status: ShipmentStatus,
        cursor: Option<u64>,
        page_size: u32,
    ) -> Result<ShipmentStatusCursorPage, NavinError> {
        require_initialized(&env)?;

        let config = config::get_config(&env);
        if page_size == 0 || page_size > config.batch_operation_limit {
            return Err(NavinError::InvalidConfig);
        }

        let mut shipment_ids = Vec::new(&env);
        let mut current_id = cursor.unwrap_or(0);
        let total_shipments = storage::get_shipment_counter(&env);
        let mut next_cursor = None;

        while current_id < total_shipments {
            current_id = current_id.saturating_add(1);

            if let Some(shipment) = storage::get_shipment(&env, current_id) {
                if shipment.status == status {
                    shipment_ids.push_back(current_id);
                    if shipment_ids.len() == page_size {
                        if current_id < total_shipments {
                            next_cursor = Some(current_id);
                        }
                        break;
                    }
                }
            }
        }

        Ok(ShipmentStatusCursorPage {
            shipment_ids,
            next_cursor,
        })
    }

    /// Get the event count for a shipment.
    /// Returns the number of events emitted for this shipment.
    /// Returns 0 for brand-new shipments or shipments with no events yet.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment.
    ///
    /// # Returns
    /// * `Result<u32, NavinError>` - The number of events emitted for this shipment.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    ///
    /// # Examples
    /// ```rust
    /// // let event_count = contract.get_event_count(&env, 1);
    /// ```
    pub fn get_event_count(env: Env, shipment_id: u64) -> Result<u32, NavinError> {
        require_initialized(&env)?;
        // Verify shipment exists
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }
        Ok(storage::get_event_count(&env, shipment_id))
    }

    /// Archive a shipment by moving it from persistent to temporary storage.
    /// This reduces state rent costs for completed shipments.
    /// Only admin can archive, and shipment must be in a terminal state (Delivered or Cancelled).
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Admin address performing the archival.
    /// * `shipment_id` - ID of the shipment to archive.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully archived.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller is not the admin.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    /// * `NavinError::InvalidStatus` - If shipment is not in a terminal state (Delivered or Cancelled).
    ///
    /// # Examples
    /// ```rust
    /// // contract.archive_shipment(&env, &admin, 1);
    /// ```
    pub fn archive_shipment(env: Env, admin: Address, shipment_id: u64) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        // Only allow archiving terminal state shipments
        if shipment.status != ShipmentStatus::Delivered
            && shipment.status != ShipmentStatus::Cancelled
        {
            return Err(NavinError::InvalidStatus);
        }

        // Archive the shipment (move from persistent to temporary storage)
        storage::archive_shipment(&env, shipment_id, &shipment);

        let timestamp = env.ledger().timestamp();
        events::emit_shipment_archived(&env, shipment_id, timestamp);

        Ok(())
    }

    /// Confirm delivery of a shipment.
    /// Only the designated receiver can call this function.
    /// Shipment must be in InTransit or AtCheckpoint status.
    /// Stores the confirmation_hash (hash of proof-of-delivery data) and
    /// transitions the shipment status to Delivered.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `receiver` - Receiver address confirming the delivery.
    /// * `shipment_id` - Identifier of delivered shipment.
    /// * `confirmation_hash` - The proof-of-delivery hash.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful confirmation.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    /// * `NavinError::Unauthorized` - If called by an address other than the shipment receiver.
    /// * `NavinError::InvalidHash` - If confirmation_hash is all zeros.
    /// * `NavinError::InvalidStatus` - If shipment is not in a transitable status to Delivered.
    ///
    /// # Examples
    /// ```rust
    /// ```rust,no_run
    /// # use soroban_sdk::{Env, Address, BytesN, Vec, Symbol};
    /// # use soroban_sdk::testutils::Address as _;
    /// # use shipment::{NavinShipment, NavinShipmentClient, ShipmentStatus};
    /// # let env = Env::default();
    /// # env.mock_all_auths();
    /// # let contract_id = env.register(NavinShipment, ());
    /// # let client = NavinShipmentClient::new(&env, &contract_id);
    /// # let admin = Address::generate(&env);
    /// # let token = Address::generate(&env);
    /// # client.initialize(&admin, &token);
    /// # client.add_company(&admin, &admin);
    /// # let carrier = Address::generate(&env);
    /// # client.add_carrier(&admin, &carrier);
    /// # let receiver = Address::generate(&env);
    /// # let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    /// # let milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    /// # let deadline = env.ledger().timestamp() + 86_400;
    /// # let shipment_id = client.create_shipment(&admin, &receiver, &carrier, &data_hash, &milestones, &deadline, &None);
    /// # client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &BytesN::from_array(&env, &[2u8; 32]));
    /// let pod_hash = BytesN::from_array(&env, &[3u8; 32]); // SHA-256 of proof-of-delivery doc
    ///
    /// // Receiver confirms delivery; escrow is automatically released to the carrier.
    /// client.confirm_delivery(&receiver, &shipment_id, &pod_hash);
    /// ```
    pub fn confirm_delivery(
        env: Env,
        receiver: Address,
        shipment_id: u64,
        confirmation_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        receiver.require_auth();

        // Validate hash before storage
        validation::validate_hash(&confirmation_hash)?;

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        // Only the designated receiver can confirm delivery
        if shipment.receiver != receiver {
            return Err(NavinError::Unauthorized);
        }
        require_not_finalized(&shipment)?;

        // Validate transition to Delivered
        if !shipment
            .status
            .is_valid_transition(&ShipmentStatus::Delivered)
        {
            return Err(NavinError::InvalidStatus);
        }

        let now = env.ledger().timestamp();
        let old_status = shipment.status.clone();
        shipment.status = ShipmentStatus::Delivered;
        shipment.updated_at = now;

        storage::decrement_status_count(&env, &old_status);
        storage::increment_status_count(&env, &ShipmentStatus::Delivered);
        storage::set_confirmation_hash(&env, shipment_id, &confirmation_hash);
        storage::decrement_active_shipment_count(&env, &shipment.sender);
        extend_shipment_ttl(&env, shipment_id);

        let remaining_escrow = shipment.escrow_amount;
        internal_release_escrow(&env, &mut shipment, remaining_escrow)?;

        finalize_if_settled(&env, &mut shipment);
        persist_shipment(&env, &shipment)?;

        env.events().publish(
            (Symbol::new(&env, "delivery_confirmed"),),
            (shipment_id, receiver, confirmation_hash.clone()),
        );

        // Reputation: record successful delivery for the carrier
        events::emit_delivery_success(&env, &shipment.carrier, shipment_id, now);

        let total_milestones = shipment.payment_milestones.len();
        let milestones_hit = shipment.paid_milestones.len();
        events::emit_carrier_milestone_rate(
            &env,
            &shipment.carrier,
            shipment_id,
            milestones_hit,
            total_milestones,
        );

        if now > shipment.deadline {
            events::emit_carrier_late_delivery(
                &env,
                &shipment.carrier,
                shipment_id,
                shipment.deadline,
                now,
            );
        } else {
            events::emit_carrier_on_time_delivery(&env, &shipment.carrier, shipment_id);
        }

        events::emit_notification(
            &env,
            &shipment.sender,
            NotificationType::DeliveryConfirmed,
            shipment_id,
            &confirmation_hash,
        );
        events::emit_notification(
            &env,
            &shipment.carrier,
            NotificationType::DeliveryConfirmed,
            shipment_id,
            &confirmation_hash,
        );

        Ok(())
    }

    /// Confirm a partial delivery and release a bounded escrow percentage.
    ///
    /// The receiver can repeatedly confirm partial delivery slices while the
    /// shipment is in transit/checkpoint/partial states. Each call releases
    /// `release_percent` of `total_escrow`, and cumulative releases are bounded
    /// so they never exceed the escrow initially deposited.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `receiver` - Receiver address confirming the partial delivery.
    /// * `shipment_id` - Identifier of the shipment.
    /// * `confirmation_hash` - The proof-of-delivery hash for this partial confirmation.
    /// * `release_percent` - Percentage of total escrow to release (1-100).
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful partial confirmation.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::InvalidHash` - If confirmation_hash is all zeros.
    /// * `NavinError::InvalidAmount` - If release_percent is 0 or > 100.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    /// * `NavinError::Unauthorized` - If called by an address other than the shipment receiver.
    /// * `NavinError::InvalidStatus` - If shipment is not in a valid state for partial delivery.
    ///
    /// # Examples
    /// ```rust
    /// // contract.confirm_partial_delivery(&env, &receiver, 1, &hash, 50);
    /// ```
    pub fn confirm_partial_delivery(
        env: Env,
        receiver: Address,
        shipment_id: u64,
        confirmation_hash: BytesN<32>,
        release_percent: u32,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        receiver.require_auth();

        // Validate hash before storage
        validation::validate_hash(&confirmation_hash)?;

        if release_percent == 0 || release_percent > 100 {
            return Err(NavinError::InvalidAmount);
        }

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;
        if shipment.receiver != receiver {
            return Err(NavinError::Unauthorized);
        }
        require_not_finalized(&shipment)?;

        if shipment.status != ShipmentStatus::InTransit
            && shipment.status != ShipmentStatus::AtCheckpoint
            && shipment.status != ShipmentStatus::PartiallyDelivered
        {
            return Err(NavinError::InvalidStatus);
        }

        let release_amount =
            checked_mul_div_i128(shipment.total_escrow, release_percent as i128, 100)?;
        if release_amount <= 0 {
            return Err(NavinError::InvalidAmount);
        }

        let released_so_far = checked_sub_i128(shipment.total_escrow, shipment.escrow_amount)?;
        let new_total_released = checked_add_i128(released_so_far, release_amount)?;
        if new_total_released > shipment.total_escrow {
            return Err(NavinError::InvalidAmount);
        }

        let old_status = shipment.status.clone();
        shipment.status = if new_total_released == shipment.total_escrow {
            ShipmentStatus::Delivered
        } else {
            ShipmentStatus::PartiallyDelivered
        };
        shipment.updated_at = env.ledger().timestamp();

        storage::decrement_status_count(&env, &old_status);
        storage::increment_status_count(&env, &shipment.status);
        storage::set_confirmation_hash(&env, shipment_id, &confirmation_hash);
        if shipment.status == ShipmentStatus::Delivered {
            storage::decrement_active_shipment_count(&env, &shipment.sender);
        }

        internal_release_escrow(&env, &mut shipment, release_amount)?;
        finalize_if_settled(&env, &mut shipment);
        persist_shipment(&env, &shipment)?;
        extend_shipment_ttl(&env, shipment_id);

        events::emit_status_updated(
            &env,
            shipment_id,
            &old_status,
            &shipment.status,
            &confirmation_hash,
        );

        Ok(())
    }

    /// Report a geofence event for a shipment.
    /// Only registered carriers can report geofence events.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `carrier` - Carrier address reporting the event.
    /// * `shipment_id` - ID of the tracked shipment.
    /// * `zone_type` - Type of geofence event crossed.
    /// * `data_hash` - Encrypted off-chain location data representation.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful report tracking.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller isn't a Carrier role.
    /// * `NavinError::InvalidHash` - If data_hash is all zeros.
    /// * `NavinError::ShipmentNotFound` - If tracking context specifies an invalid shipment.
    ///
    /// # Examples
    /// ```rust
    /// // contract.report_geofence_event(&env, &carrier, 1, GeofenceEvent::ZoneEntry, &hash);
    /// ```
    pub fn report_geofence_event(
        env: Env,
        carrier: Address,
        shipment_id: u64,
        zone_type: GeofenceEvent,
        data_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        carrier.require_auth();
        require_role(&env, &carrier, Role::Carrier)?;

        // Verify shipment exists and carrier is assigned
        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        require_not_finalized(&shipment)?;

        // Validate hash before storage
        validation::validate_hash(&data_hash)?;

        if shipment.carrier != carrier {
            return Err(NavinError::Unauthorized);
        }

        let timestamp = env.ledger().timestamp();

        env.events().publish(
            (Symbol::new(&env, "geofence_event"),),
            (shipment_id, zone_type, data_hash, timestamp),
        );

        Ok(())
    }

    /// Update ETA for a shipment.
    /// Only the designated registered carrier can update ETA.
    /// ETA must be strictly in the future.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `carrier` - Active assigned carrier modifying ETA.
    /// * `shipment_id` - Identifiable tracker mapping to shipment.
    /// * `eta_timestamp` - The estimated timestamp prediction in the future.
    /// * `data_hash` - The mapped hash associated with the update.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful ETA registry.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller isn't the assigned carrier.
    /// * `NavinError::InvalidHash` - If data_hash is all zeros.
    /// * `NavinError::ShipmentNotFound` - If shipment instance targets missing entry.
    /// * `NavinError::InvalidTimestamp` - If provided ETA is strictly in the past or present.
    ///
    /// # Examples
    /// ```rust
    /// // contract.update_eta(&env, &carrier, 1, new_eta, &hash);
    /// ```
    pub fn update_eta(
        env: Env,
        carrier: Address,
        shipment_id: u64,
        eta_timestamp: u64,
        data_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        carrier.require_auth();
        require_role(&env, &carrier, Role::Carrier)?;

        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        require_not_finalized(&shipment)?;

        // Validate hash before storage
        validation::validate_hash(&data_hash)?;

        if shipment.carrier != carrier {
            return Err(NavinError::Unauthorized);
        }

        if eta_timestamp <= env.ledger().timestamp() {
            return Err(NavinError::InvalidTimestamp);
        }

        env.events().publish(
            (Symbol::new(&env, "eta_updated"),),
            (shipment_id, eta_timestamp, data_hash),
        );

        Ok(())
    }

    /// Record a milestone for a shipment.
    /// Only registered carriers can record milestones.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `carrier` - Assigned carrier address triggering the recording.
    /// * `shipment_id` - ID of the tracked shipment.
    /// * `checkpoint` - Representation of progress milestone achieved.
    /// * `data_hash` - Integrity hash associated with offchain progress indicators.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful tracking record update.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If called by unassigned identity.
    /// * `NavinError::InvalidHash` - If data_hash is all zeros.
    /// * `NavinError::CarrierSuspended` - If the carrier is suspended.
    /// * `NavinError::ShipmentNotFound` - If shipment instance targets missing entry.
    /// * `NavinError::InvalidStatus` - If tracked instance is not `InTransit`.
    ///
    /// # Examples
    /// ```rust
    /// // contract.record_milestone(&env, &carrier, 1, Symbol::new(&env, "warehouse"), &hash);
    /// ```
    pub fn record_milestone(
        env: Env,
        carrier: Address,
        shipment_id: u64,
        checkpoint: Symbol,
        data_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        carrier.require_auth();
        require_role(&env, &carrier, Role::Carrier)?;
        require_active_carrier(&env, &carrier)?;

        // Verify shipment exists, carrier is assigned, and status
        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        require_not_finalized(&shipment)?;

        // Validate hash before storage
        validation::validate_hash(&data_hash)?;

        if shipment.carrier != carrier {
            return Err(NavinError::Unauthorized);
        }

        if shipment.status != ShipmentStatus::InTransit {
            return Err(NavinError::InvalidStatus);
        }

        // Enforce milestone event payload size guard
        let config = config::get_config(&env);
        let current_milestone_count = storage::get_milestone_event_count(&env, shipment_id);
        if current_milestone_count >= config.max_milestones_per_shipment {
            return Err(NavinError::MilestoneLimitExceeded);
        }

        let timestamp = env.ledger().timestamp();

        let _milestone = Milestone {
            shipment_id,
            checkpoint: checkpoint.clone(),
            data_hash: data_hash.clone(),
            timestamp,
            reporter: carrier.clone(),
        };

        // Do NOT store the milestone on-chain
        // Emit the milestone_recorded event (Hash-and-Emit pattern)
        events::emit_milestone_recorded(&env, shipment_id, &checkpoint, &data_hash, &carrier);

        // Check for milestone-based payments
        let mut mut_shipment = shipment;
        let mut found_index = None;
        for (i, milestone) in mut_shipment.payment_milestones.iter().enumerate() {
            if milestone.0 == checkpoint {
                found_index = Some(i);
                break;
            }
        }

        if let Some(idx) = found_index {
            let mut already_paid = false;
            for paid_symbol in mut_shipment.paid_milestones.iter() {
                if paid_symbol == checkpoint {
                    already_paid = true;
                    break;
                }
            }

            if already_paid {
                return Err(NavinError::MilestoneAlreadyPaid);
            }

            let milestone = mut_shipment.payment_milestones.get(idx as u32).unwrap();
            let release_amount =
                checked_mul_div_i128(mut_shipment.total_escrow, milestone.1 as i128, 100)?;

            mut_shipment
                .milestones_completed
                .push_back(checkpoint.clone());
            if !mut_shipment.paid_milestones.iter().any(|m| m == checkpoint) {
                mut_shipment.paid_milestones.push_back(checkpoint.clone());
            }

            events::emit_milestone_payment_released(
                &env,
                shipment_id,
                &checkpoint,
                release_amount,
                &mut_shipment.carrier,
            );
            internal_release_escrow(&env, &mut mut_shipment, release_amount)?;
        }

        finalize_if_settled(&env, &mut mut_shipment);
        storage::set_shipment(&env, &mut_shipment);

        Ok(())
    }

    /// Record multiple milestones for a shipment in a single atomic transaction.
    /// Allows a carrier to record multiple checkpoints at once, reducing gas costs.
    /// Limit: 10 milestones per batch.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `carrier` - Assigned carrier address triggering the recording.
    /// * `shipment_id` - ID of the tracked shipment.
    /// * `milestones` - Vector of (checkpoint, data_hash) tuples.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful batch recording.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If called by unassigned identity.
    /// * `NavinError::InvalidHash` - If any data_hash is all zeros.
    /// * `NavinError::CarrierSuspended` - If the carrier is suspended.
    /// * `NavinError::ShipmentNotFound` - If shipment instance targets missing entry.
    /// * `NavinError::InvalidStatus` - If tracked instance is not `InTransit`.
    /// * `NavinError::BatchTooLarge` - If more than 10 milestones are submitted.
    ///
    /// # Examples
    /// ```rust
    /// // let milestones = vec![
    /// //     (Symbol::new(&env, "warehouse"), hash1),
    /// //     (Symbol::new(&env, "port"), hash2),
    /// // ];
    /// // contract.record_milestones_batch(&env, &carrier, 1, milestones);
    /// ```
    pub fn record_milestones_batch(
        env: Env,
        carrier: Address,
        shipment_id: u64,
        milestones: Vec<(Symbol, BytesN<32>)>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        carrier.require_auth();
        require_role(&env, &carrier, Role::Carrier)?;
        require_active_carrier(&env, &carrier)?;

        // Validate batch size
        let config = config::get_config(&env);
        if milestones.len() > config.batch_operation_limit {
            return Err(NavinError::BatchTooLarge);
        }

        // Validate all hashes in milestones
        for (_, hash) in &milestones {
            validation::validate_hash(&hash)?;
        }

        // Verify shipment exists, carrier is assigned, and status
        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        require_not_finalized(&shipment)?;

        if shipment.carrier != carrier {
            return Err(NavinError::Unauthorized);
        }

        if shipment.status != ShipmentStatus::InTransit {
            return Err(NavinError::InvalidStatus);
        }

        // Validate all milestones before committing any (atomic operation)
        // This ensures that if any milestone is invalid, none are committed
        for milestone_tuple in milestones.iter() {
            let data_hash = &milestone_tuple.1;

            // Validate hash
            validation::validate_hash(data_hash)?;
        }

        // Enforce milestone event payload size guard
        let config = config::get_config(&env);
        let current_milestone_count = storage::get_milestone_event_count(&env, shipment_id);
        let new_milestones = milestones.len();
        if current_milestone_count
            .checked_add(new_milestones)
            .ok_or(NavinError::ArithmeticError)?
            > config.max_milestones_per_shipment
        {
            return Err(NavinError::MilestoneLimitExceeded);
        }

        // All validations passed, now process each milestone
        let timestamp = env.ledger().timestamp();
        let mut mut_shipment = shipment;

        for milestone_tuple in milestones.iter() {
            let checkpoint = milestone_tuple.0.clone();
            let data_hash = milestone_tuple.1.clone();

            let _milestone = Milestone {
                shipment_id,
                checkpoint: checkpoint.clone(),
                data_hash: data_hash.clone(),
                timestamp,
                reporter: carrier.clone(),
            };

            // Emit one event per milestone (Hash-and-Emit pattern)
            events::emit_milestone_recorded(&env, shipment_id, &checkpoint, &data_hash, &carrier);

            // Check for milestone-based payments
            let mut found_index = None;
            for (i, payment_milestone) in mut_shipment.payment_milestones.iter().enumerate() {
                if payment_milestone.0 == checkpoint {
                    found_index = Some(i);
                    break;
                }
            }

            if let Some(idx) = found_index {
                let mut already_paid = false;
                for paid_symbol in mut_shipment.paid_milestones.iter() {
                    if paid_symbol == checkpoint {
                        already_paid = true;
                        break;
                    }
                }

                if !already_paid {
                    let payment_milestone =
                        mut_shipment.payment_milestones.get(idx as u32).unwrap();
                    let release_amount = checked_mul_div_i128(
                        mut_shipment.total_escrow,
                        payment_milestone.1 as i128,
                        100,
                    )?;

                    mut_shipment
                        .milestones_completed
                        .push_back(checkpoint.clone());
                    if !mut_shipment.paid_milestones.iter().any(|m| m == checkpoint) {
                        mut_shipment.paid_milestones.push_back(checkpoint.clone());
                    }

                    events::emit_milestone_payment_released(
                        &env,
                        shipment_id,
                        &checkpoint,
                        release_amount,
                        &mut_shipment.carrier,
                    );
                    internal_release_escrow(&env, &mut mut_shipment, release_amount)?;
                }
            }
        }

        finalize_if_settled(&env, &mut mut_shipment);
        storage::set_shipment(&env, &mut_shipment);

        Ok(())
    }

    /// Explicitly release a partial escrow payment for a specific milestone.
    /// Only Carrier or Admin can call this.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `caller` - Identity triggering the release (Carrier or Admin).
    /// * `shipment_id` - ID of the target shipment.
    /// * `milestone_name` - symbolic name of the milestone to pay.
    pub fn release_milestone_payment(
        env: Env,
        caller: Address,
        shipment_id: u64,
        milestone_name: Symbol,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        caller.require_auth();

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;
        require_not_finalized(&shipment)?;

        let admin = storage::get_admin(&env);
        if caller != shipment.carrier && caller != admin {
            return Err(NavinError::Unauthorized);
        }

        if caller == shipment.carrier {
            require_active_carrier(&env, &caller)?;
        }

        // Check if milestone exists in payment_milestones
        let mut milestone_idx = None;
        for (i, ms) in shipment.payment_milestones.iter().enumerate() {
            if ms.0 == milestone_name {
                milestone_idx = Some(i);
                break;
            }
        }

        let idx = milestone_idx.ok_or(NavinError::InvalidShipmentInput)?;

        // Check if already in milestones_completed
        let mut already_completed = false;
        for ms in shipment.milestones_completed.iter() {
            if ms == milestone_name {
                already_completed = true;
                break;
            }
        }

        if already_completed {
            return Err(NavinError::MilestoneAlreadyPaid);
        }

        let ms_config = shipment.payment_milestones.get(idx as u32).unwrap();
        let release_amount = checked_mul_div_i128(shipment.total_escrow, ms_config.1 as i128, 100)?;

        if release_amount > 0 {
            shipment
                .milestones_completed
                .push_back(milestone_name.clone());
            // Keep paid_milestones in sync for backward compatibility
            let mut in_paid = false;
            for ms in shipment.paid_milestones.iter() {
                if ms == milestone_name {
                    in_paid = true;
                    break;
                }
            }
            if !in_paid {
                shipment.paid_milestones.push_back(milestone_name.clone());
            }

            events::emit_milestone_payment_released(
                &env,
                shipment_id,
                &milestone_name,
                release_amount,
                &shipment.carrier,
            );
            internal_release_escrow(&env, &mut shipment, release_amount)?;
        }

        finalize_if_settled(&env, &mut shipment);
        storage::set_shipment(&env, &shipment);

        Ok(())
    }

    /// Extend the TTL of a shipment's persistent storage entries.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - Shipment ID to renew TTL.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on success.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // contract.extend_shipment_ttl(env, 1);
    /// ```
    pub fn extend_shipment_ttl(env: Env, shipment_id: u64) -> Result<(), NavinError> {
        require_initialized(&env)?;
        extend_shipment_ttl(&env, shipment_id);
        Ok(())
    }

    /// Cancel a shipment before it is delivered.
    /// Only the Company (sender) or Admin can cancel.
    /// Shipment must not be Delivered or Disputed.
    /// If escrow exists, triggers automatic refund to the Company.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `caller` - Executing Company or Admin address.
    /// * `shipment_id` - ID specifying cancelled shipment instance.
    /// * `reason_hash` - The mapped hash associated to the cancellation context.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on cancellation.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::InvalidHash` - If reason_hash is all zeros.
    /// * `NavinError::ShipmentNotFound` - If tracking context is invalid list element.
    /// * `NavinError::Unauthorized` - If called by unauthorized accounts.
    /// * `NavinError::ShipmentAlreadyCompleted` - If tracking context specified reached terminal states.
    ///
    /// # Examples
    /// ```rust
    /// // contract.cancel_shipment(&env, &admin, 1, &hash);
    /// ```
    pub fn cancel_shipment(
        env: Env,
        caller: Address,
        shipment_id: u64,
        reason_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        caller.require_auth();

        // Validate hash before storage
        validation::validate_hash(&reason_hash)?;

        let admin = storage::get_admin(&env);
        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        require_not_finalized(&shipment)?;

        if caller != shipment.sender && caller != admin {
            return Err(NavinError::Unauthorized);
        }

        // Check for suspension if caller is the sender (company)
        if caller == shipment.sender {
            require_active_company(&env, &caller)?;
        }

        match shipment.status {
            ShipmentStatus::Delivered | ShipmentStatus::Disputed => {
                return Err(NavinError::ShipmentAlreadyCompleted);
            }
            _ => {}
        }

        let escrow_amount = shipment.escrow_amount;
        let old_status = shipment.status.clone();
        shipment.status = ShipmentStatus::Cancelled;
        shipment.escrow_amount = 0;
        shipment.updated_at = env.ledger().timestamp();
        shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);

        persist_shipment(&env, &shipment)?;
        storage::decrement_status_count(&env, &old_status);
        storage::increment_status_count(&env, &ShipmentStatus::Cancelled);

        // Decrement active shipment count if it was not already cancelled
        if old_status != ShipmentStatus::Cancelled {
            storage::decrement_active_shipment_count(&env, &shipment.sender);
        }

        if escrow_amount > 0 {
            storage::remove_escrow_balance(&env, shipment_id);
            events::emit_escrow_released(&env, shipment_id, &shipment.sender, escrow_amount);
        }
        finalize_if_settled(&env, &mut shipment);
        persist_shipment(&env, &shipment)?;
        storage::remove_escrow_balance(&env, shipment_id);
        extend_shipment_ttl(&env, shipment_id);

        events::emit_shipment_cancelled(&env, shipment_id, &caller, &reason_hash);

        Ok(())
    }

    /// Emergency admin-only force-cancel for a shipment.
    ///
    /// This is a privileged override that bypasses the normal cancellation rules.
    /// It can cancel a shipment in **any non-terminal state** (including Disputed),
    /// and it requires a mandatory, non-zero `reason_hash` to ensure an immutable
    /// audit trail is always present.
    ///
    /// Escrow behaviour is deterministic:
    /// - If escrow is held, the full remaining balance is refunded to the company
    ///   via the token contract before the shipment is marked Cancelled.
    /// - If no escrow is held, the shipment is cancelled with no token transfer.
    ///
    /// Only the single admin or a multi-sig admin (via `propose_action` /
    /// `approve_action`) may call this function directly. Regular companies and
    /// carriers are rejected with `Unauthorized`.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Admin address executing the force-cancel.
    /// * `shipment_id` - ID of the shipment to force-cancel.
    /// * `reason_hash` - Mandatory SHA-256 hash of the off-chain reason document.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on success.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - Contract not initialized.
    /// * `NavinError::Unauthorized` - Caller is not the admin.
    /// * `NavinError::ShipmentNotFound` - Shipment does not exist.
    /// * `NavinError::InvalidHash` - `reason_hash` is all zeros.
    /// * `NavinError::ShipmentAlreadyCompleted` - Shipment is already Delivered or Cancelled.
    ///
    /// # Examples
    /// ```rust
    /// // contract.force_cancel_shipment(&env, &admin, 1, &reason_hash);
    /// ```
    pub fn force_cancel_shipment(
        env: Env,
        admin: Address,
        shipment_id: u64,
        reason_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        admin.require_auth();

        // Strict admin-only gate — no company/carrier bypass.
        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        // Reason hash is mandatory and must be non-zero.
        validation::validate_hash(&reason_hash)?;

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        require_not_finalized(&shipment)?;

        // Terminal states cannot be force-cancelled.
        match shipment.status {
            ShipmentStatus::Delivered | ShipmentStatus::Cancelled => {
                return Err(NavinError::ShipmentAlreadyCompleted);
            }
            _ => {}
        }

        let old_status = shipment.status.clone();
        let escrow_amount = shipment.escrow_amount;

        // Deterministic escrow refund: always refund to company if escrow is held.
        if escrow_amount > 0 {
            let token_contract =
                storage::get_token_contract(&env).ok_or(NavinError::NotInitialized)?;
            let contract_address = env.current_contract_address();
            invoke_token_transfer(
                &env,
                &token_contract,
                &contract_address,
                &shipment.sender,
                escrow_amount,
            )?;

            shipment.escrow_amount = 0;
            events::emit_escrow_refunded(&env, shipment_id, &shipment.sender, escrow_amount);
        }

        shipment.status = ShipmentStatus::Cancelled;
        shipment.updated_at = env.ledger().timestamp();
        shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);

        storage::decrement_status_count(&env, &old_status);
        storage::increment_status_count(&env, &ShipmentStatus::Cancelled);

        // Decrement active count only if the shipment was not already in a
        // non-active state (Cancelled is the only non-active non-terminal state
        // that can't reach here, so this is always safe).
        storage::decrement_active_shipment_count(&env, &shipment.sender);

        finalize_if_settled(&env, &mut shipment);
        persist_shipment(&env, &shipment)?;

        extend_shipment_ttl(&env, shipment_id);

        // Emit the dedicated force-cancel event — distinct from shipment_cancelled.
        events::emit_force_cancelled(&env, shipment_id, &admin, &reason_hash, escrow_amount);

        Ok(())
    }

    /// Upgrade the contract to a new WASM implementation.
    /// Only the admin can trigger upgrades. State is preserved.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin executing the upgrade.
    /// * `new_wasm_hash` - Hash pointer to the new WASM instance loaded on network.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful deployment upgrade instance.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller isn't contract admin instance.
    /// * `NavinError::InvalidHash` - If new_wasm_hash is all zeros.
    /// * `NavinError::CounterOverflow` - If total tracking version identifier pointer triggers overflow.
    ///
    /// # Examples
    /// ```rust
    /// // contract.upgrade(env, admin, new_wasm_hash);
    /// ```
    pub fn upgrade(
        env: Env,
        admin: Address,
        new_wasm_hash: BytesN<32>,
        target_version: u32,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        // Validate hash before storage
        validation::validate_hash(&new_wasm_hash)?;

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        let current_version = storage::get_version(&env);

        // Enforce one-way migration guardrails and allowed edges
        if !is_allowed_migration(current_version, target_version) {
            return Err(NavinError::InvalidMigrationEdge);
        }

        let shipment_count = storage::get_shipment_counter(&env);

        let report = MigrationReport {
            current_version,
            target_version,
            affected_shipments: shipment_count,
        };

        storage::set_version(&env, target_version);
        events::emit_contract_upgraded(&env, &admin, &new_wasm_hash, target_version);
        events::emit_migration_report(&env, &report);

        env.deployer().update_current_contract_wasm(new_wasm_hash);

        Ok(())
    }

    /// Read-only dry-run for a proposed migration to estimate impact and validate edges.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `target_version` - The version to simulate migrating to.
    ///
    /// # Returns
    /// * `Result<MigrationReport, NavinError>` - Summary of the migration impact.
    pub fn dry_run_migration(env: Env, target_version: u32) -> Result<MigrationReport, NavinError> {
        require_initialized(&env)?;

        let current_version = storage::get_version(&env);

        if !is_allowed_migration(current_version, target_version) {
            return Err(NavinError::InvalidMigrationEdge);
        }

        let shipment_count = storage::get_shipment_counter(&env);

        Ok(MigrationReport {
            current_version,
            target_version,
            affected_shipments: shipment_count,
        })
    }

    /// Release escrowed funds to the carrier after delivery confirmation.
    /// Only the receiver or admin can trigger release.
    /// Shipment must be in Delivered status.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `caller` - Originating user triggering escrow delivery (receiver/admin).
    /// * `shipment_id` - Tracking assignment associated with delivery payload instances.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful asset delivery.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If tracking context specifies an invalid shipment.
    /// * `NavinError::Unauthorized` - If caller isn't receiver or admin.
    /// * `NavinError::InvalidStatus` - If contract expects specific lifecycle constraint and differs.
    /// * `NavinError::InsufficientFunds` - If payload is fully released and balances are zeroed out.
    ///
    /// # Examples
    /// ```rust
    /// ```rust,no_run
    /// # use soroban_sdk::{Env, Address, BytesN, Vec, Symbol};
    /// # use soroban_sdk::testutils::Address as _;
    /// # use shipment::{NavinShipment, NavinShipmentClient, ShipmentStatus};
    /// # let env = Env::default();
    /// # env.mock_all_auths();
    /// # let contract_id = env.register(NavinShipment, ());
    /// # let client = NavinShipmentClient::new(&env, &contract_id);
    /// # let admin = Address::generate(&env);
    /// # let token = Address::generate(&env);
    /// # client.initialize(&admin, &token);
    /// # client.add_company(&admin, &admin);
    /// # let carrier = Address::generate(&env);
    /// # client.add_carrier(&admin, &carrier);
    /// # let receiver = Address::generate(&env);
    /// # let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    /// # let milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    /// # let deadline = env.ledger().timestamp() + 86_400;
    /// # let shipment_id = client.create_shipment(&admin, &receiver, &carrier, &data_hash, &milestones, &deadline, &None);
    /// # client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &BytesN::from_array(&env, &[2u8; 32]));
    /// # client.confirm_delivery(&receiver, &shipment_id, &BytesN::from_array(&env, &[3u8; 32]));
    /// // Manually release any remaining escrow to the carrier after delivery is confirmed.
    /// client.release_escrow(&receiver, &shipment_id);
    /// ```
    pub fn release_escrow(env: Env, caller: Address, shipment_id: u64) -> Result<(), NavinError> {
        require_initialized(&env)?;
        caller.require_auth();

        with_reentrancy_lock(&env, || {
            let admin = storage::get_admin(&env);
            let mut shipment =
                storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

            require_not_finalized(&shipment)?;

            if caller != shipment.receiver && caller != admin {
                return Err(NavinError::Unauthorized);
            }

            if shipment.status != ShipmentStatus::Delivered {
                return Err(NavinError::InvalidStatus);
            }

            let escrow_amount = shipment.escrow_amount;
            if escrow_amount == 0 {
                return Err(NavinError::InsufficientFunds);
            }

            internal_release_escrow(&env, &mut shipment, escrow_amount)?;
            finalize_if_settled(&env, &mut shipment);
            persist_shipment(&env, &shipment)?;
            events::emit_notification(
                &env,
                &shipment.sender,
                NotificationType::EscrowReleased,
                shipment_id,
                &BytesN::from_array(&env, &[0u8; 32]),
            );
            events::emit_notification(
                &env,
                &shipment.carrier,
                NotificationType::EscrowReleased,
                shipment_id,
                &BytesN::from_array(&env, &[0u8; 32]),
            );

            Ok(())
        })
    }

    /// Refund escrowed funds to the company if shipment is cancelled.
    /// Only the sender (Company) or admin can trigger refund.
    /// Shipment must be in Created or Cancelled status.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `caller` - Reference mapping handler execution triggers for scope access control checks.
    /// * `shipment_id` - Identification marker mapping.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful refund sequence generation.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If valid identifiers track undefined mappings instances.
    /// * `NavinError::Unauthorized` - If execution identity doesn't resolve matching configurations contexts mappings.
    /// * `NavinError::InvalidStatus` - If mapping resolves illegal flow mappings configuration combinations triggers.
    /// * `NavinError::InsufficientFunds` - If token escrow state points map uninitialized quantities values scope checks.
    ///
    /// # Examples
    /// ```rust
    /// ```rust,no_run
    /// # use soroban_sdk::{Env, Address, BytesN, Vec, Symbol};
    /// # use soroban_sdk::testutils::Address as _;
    /// # use shipment::{NavinShipment, NavinShipmentClient};
    /// # let env = Env::default();
    /// # env.mock_all_auths();
    /// # let contract_id = env.register(NavinShipment, ());
    /// # let client = NavinShipmentClient::new(&env, &contract_id);
    /// # let admin = Address::generate(&env);
    /// # let token = Address::generate(&env);
    /// # client.initialize(&admin, &token);
    /// # client.add_company(&admin, &admin);
    /// # let carrier = Address::generate(&env);
    /// # client.add_carrier(&admin, &carrier);
    /// # let receiver = Address::generate(&env);
    /// # let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    /// # let milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    /// # let deadline = env.ledger().timestamp() + 86_400;
    /// # let shipment_id = client.create_shipment(&admin, &receiver, &carrier, &data_hash, &milestones, &deadline, &None);
    /// // Refund escrow back to the company when the shipment is in Created or Cancelled state.
    /// client.refund_escrow(&admin, &shipment_id);
    /// ```
    pub fn refund_escrow(env: Env, caller: Address, shipment_id: u64) -> Result<(), NavinError> {
        require_initialized(&env)?;
        caller.require_auth();

        with_reentrancy_lock(&env, || {
            let admin = storage::get_admin(&env);
            let mut shipment =
                storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

            require_not_finalized(&shipment)?;

            if caller != shipment.sender && caller != admin {
                return Err(NavinError::Unauthorized);
            }

            // Check for suspension if caller is the sender (company)
            if caller == shipment.sender {
                require_active_company(&env, &caller)?;
            }

            if shipment.status != ShipmentStatus::Created
                && shipment.status != ShipmentStatus::Cancelled
            {
                return Err(NavinError::InvalidStatus);
            }

            let escrow_amount = shipment.escrow_amount;
            if escrow_amount == 0 {
                return Err(NavinError::InsufficientFunds);
            }

            // Get token contract address
            let token_contract =
                storage::get_token_contract(&env).ok_or(NavinError::NotInitialized)?;

            // Transfer tokens from this contract to company
            let contract_address = env.current_contract_address();
            invoke_token_transfer(
                &env,
                &token_contract,
                &contract_address,
                &shipment.sender,
                escrow_amount,
            )?;

            shipment.escrow_amount = 0;
            let old_status = shipment.status.clone();
            shipment.status = ShipmentStatus::Cancelled;
            shipment.updated_at = env.ledger().timestamp();
            shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);

            finalize_if_settled(&env, &mut shipment);
            persist_shipment(&env, &shipment)?;
            storage::decrement_status_count(&env, &old_status);
            storage::increment_status_count(&env, &ShipmentStatus::Cancelled);

            // Decrement active shipment count if it was not already cancelled
            if old_status != ShipmentStatus::Cancelled {
                storage::decrement_active_shipment_count(&env, &shipment.sender);
            }

            extend_shipment_ttl(&env, shipment_id);
            extend_shipment_ttl(&env, shipment_id);

            events::emit_escrow_refunded(&env, shipment_id, &shipment.sender, escrow_amount);

            Ok(())
        })
    }

    /// Assign an observer role to an address for a specific shipment.
    /// Observers can read shipment data but cannot modify any state.
    /// Only admin can assign observers.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Admin address (must be the contract admin).
    /// * `observer` - Address to assign observer role to.
    /// * `shipment_id` - ID of the shipment.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful assignment.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller is not admin.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    pub fn assign_observer_role(
        env: Env,
        admin: Address,
        observer: Address,
        shipment_id: u64,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }

        storage::set_shipment_observer(&env, shipment_id, &observer);
        events::emit_observer_assigned(&env, shipment_id, &observer, &admin);

        Ok(())
    }

    /// Revoke observer role from an address for a specific shipment.
    /// Only admin can revoke observers.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Admin address (must be the contract admin).
    /// * `observer` - Address to revoke observer role from.
    /// * `shipment_id` - ID of the shipment.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful revocation.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller is not admin.
    pub fn revoke_observer_role(
        env: Env,
        admin: Address,
        observer: Address,
        shipment_id: u64,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        storage::remove_shipment_observer(&env, shipment_id, &observer);
        events::emit_observer_revoked(&env, shipment_id, &observer, &admin);

        Ok(())
    }

    /// Get the list of observers for a specific shipment.
    /// This is a read-only query that returns the count of observers.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment.
    ///
    /// # Returns
    /// * `Result<u32, NavinError>` - Number of observers for the shipment.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    pub fn get_observer_count(env: Env, shipment_id: u64) -> Result<u32, NavinError> {
        require_initialized(&env)?;
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }
        Ok(storage::get_observer_count(&env, shipment_id))
    }

    /// Execute a partial refund of escrow funds.
    /// Refunds a percentage of the escrow to the sender and pays the remainder to the carrier.
    /// Only admin can execute partial refunds (e.g., after dispute resolution).
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Admin address (must be the contract admin).
    /// * `shipment_id` - ID of the shipment.
    /// * `refund_percentage` - Percentage (1-100) of escrow to refund to sender.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful partial refund.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller is not admin.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    /// * `NavinError::InvalidAmount` - If refund_percentage is not 1-100.
    /// * `NavinError::InvalidStatus` - If shipment is already in a terminal state.
    /// * `NavinError::InsufficientFunds` - If no escrow to refund.
    pub fn partial_refund_escrow(
        env: Env,
        admin: Address,
        shipment_id: u64,
        refund_percentage: u32,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        if refund_percentage == 0 || refund_percentage > 100 {
            return Err(NavinError::InvalidAmount);
        }

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        require_not_finalized(&shipment)?;

        if shipment.status == ShipmentStatus::Delivered
            || shipment.status == ShipmentStatus::Cancelled
            || shipment.status == ShipmentStatus::PartiallyRefunded
        {
            return Err(NavinError::InvalidStatus);
        }

        let escrow_balance = shipment.escrow_amount;
        if escrow_balance == 0 {
            return Err(NavinError::InsufficientFunds);
        }

        if storage::get_partial_refund_record(&env, shipment_id).is_some() {
            return Err(NavinError::InvalidStatus);
        }

        let sender_refund = checked_mul_div_i128(escrow_balance, refund_percentage as i128, 100)?;
        let carrier_compensation = escrow_balance - sender_refund;

        let token_contract = storage::get_token_contract(&env).ok_or(NavinError::NotInitialized)?;
        let contract_address = env.current_contract_address();

        invoke_token_transfer(
            &env,
            &token_contract,
            &contract_address,
            &shipment.sender,
            sender_refund,
        )?;

        invoke_token_transfer(
            &env,
            &token_contract,
            &contract_address,
            &shipment.carrier,
            carrier_compensation,
        )?;

        shipment.escrow_amount = 0;
        let old_status = shipment.status.clone();
        shipment.status = ShipmentStatus::PartiallyRefunded;
        shipment.updated_at = env.ledger().timestamp();
        shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);

        finalize_if_settled(&env, &mut shipment);
        persist_shipment(&env, &shipment)?;
        storage::decrement_status_count(&env, &old_status);
        storage::increment_status_count(&env, &ShipmentStatus::PartiallyRefunded);

        let record = PartialRefundRecord {
            shipment_id,
            sender_refund,
            carrier_compensation,
            refund_percentage,
            executed_at: env.ledger().timestamp(),
        };
        storage::set_partial_refund_record(&env, &record);

        extend_shipment_ttl(&env, shipment_id);

        events::emit_escrow_partially_refunded(
            &env,
            shipment_id,
            sender_refund,
            carrier_compensation,
            refund_percentage,
        );

        Ok(())
    }

    /// Raise a dispute for a shipment.
    /// Only the sender, receiver, or carrier can raise a dispute.
    /// Shipment must not be Cancelled or already Disputed.
    ///
    /// # Arguments
    /// * `env` - Execution environment tracking context.
    /// * `caller` - Identity specifying resolution event raising instances configuration contexts.
    /// * `shipment_id` - Object tracker index identifying execution scope handlers.
    /// * `reason_hash` - Encoded offchain metadata representation parameter validation identifier limits strings pointers.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful dispute registry logging.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::InvalidHash` - If reason_hash is all zeros.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    /// * `NavinError::Unauthorized` - If caller is not involved in the shipment.
    /// * `NavinError::ShipmentAlreadyCompleted` - If shipment is already completed.
    ///
    /// # Examples
    /// ```rust
    /// ```rust,no_run
    /// # use soroban_sdk::{Env, Address, BytesN, Vec, Symbol};
    /// # use soroban_sdk::testutils::Address as _;
    /// # use shipment::{NavinShipment, NavinShipmentClient, ShipmentStatus};
    /// # let env = Env::default();
    /// # env.mock_all_auths();
    /// # let contract_id = env.register(NavinShipment, ());
    /// # let client = NavinShipmentClient::new(&env, &contract_id);
    /// # let admin = Address::generate(&env);
    /// # let token = Address::generate(&env);
    /// # client.initialize(&admin, &token);
    /// # client.add_company(&admin, &admin);
    /// # let carrier = Address::generate(&env);
    /// # client.add_carrier(&admin, &carrier);
    /// # let receiver = Address::generate(&env);
    /// # let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    /// # let milestones: Vec<(Symbol, u32)> = Vec::new(&env);
    /// # let deadline = env.ledger().timestamp() + 86_400;
    /// # let shipment_id = client.create_shipment(&admin, &receiver, &carrier, &data_hash, &milestones, &deadline, &None);
    /// # client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &BytesN::from_array(&env, &[2u8; 32]));
    /// let reason_hash = BytesN::from_array(&env, &[4u8; 32]); // SHA-256 of dispute reason doc
    ///
    /// // Receiver raises a dispute; escrow is frozen until admin resolves it.
    /// client.raise_dispute(&receiver, &shipment_id, &reason_hash);
    /// ```
    pub fn raise_dispute(
        env: Env,
        caller: Address,
        shipment_id: u64,
        reason_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        caller.require_auth();

        // Validate hash before storage
        validation::validate_hash(&reason_hash)?;

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        require_not_finalized(&shipment)?;

        if caller != shipment.sender && caller != shipment.receiver && caller != shipment.carrier {
            return Err(NavinError::Unauthorized);
        }

        // Check for suspension if caller is the sender (company)
        if caller == shipment.sender {
            require_active_company(&env, &caller)?;
        }

        if shipment.status == ShipmentStatus::Cancelled
            || shipment.status == ShipmentStatus::Disputed
        {
            return Err(NavinError::ShipmentAlreadyCompleted);
        }

        let old_status = shipment.status.clone();
        shipment.status = ShipmentStatus::Disputed;
        shipment.updated_at = env.ledger().timestamp();
        shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);

        persist_shipment(&env, &shipment)?;
        storage::decrement_status_count(&env, &old_status);
        storage::increment_status_count(&env, &ShipmentStatus::Disputed);
        storage::increment_total_disputes(&env);
        storage::set_escrow_freeze_reason(
            &env,
            shipment_id,
            &crate::types::EscrowFreezeReason::DisputeRaised,
        );

        extend_shipment_ttl(&env, shipment_id);

        events::emit_dispute_raised(&env, shipment_id, &caller, &reason_hash);
        // Emit a structured freeze reason so indexers can classify the escrow block.
        events::emit_escrow_frozen(
            &env,
            shipment_id,
            crate::types::EscrowFreezeReason::DisputeRaised,
            &caller,
        );
        events::emit_notification(
            &env,
            &shipment.sender,
            NotificationType::DisputeRaised,
            shipment_id,
            &reason_hash,
        );
        events::emit_notification(
            &env,
            &shipment.receiver,
            NotificationType::DisputeRaised,
            shipment_id,
            &reason_hash,
        );
        events::emit_notification(
            &env,
            &shipment.carrier,
            NotificationType::DisputeRaised,
            shipment_id,
            &reason_hash,
        );

        Ok(())
    }

    /// Resolve a shipment dispute. Only the admin can call this.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin address.
    /// * `shipment_id` - ID of the shipment.
    /// * `resolution` - Target resolution (Release to Carrier or Refund to Company).
    /// * `reason_hash` - SHA-256 hash of the off-chain justification document.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully resolved.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If the shipment doesn't exist.
    /// * `NavinError::Unauthorized` - If called by a non-admin.
    /// * `NavinError::InvalidHash` - If reason_hash is all zeros.
    pub fn resolve_dispute(
        env: Env,
        admin: Address,
        shipment_id: u64,
        resolution: DisputeResolution,
        reason_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        admin.require_auth();

        require_admin_or_guardian(&env, &admin)?;

        // Reason hash is mandatory; use a specific error rather than the generic InvalidHash.
        if reason_hash.to_array().iter().all(|&b| b == 0) {
            return Err(NavinError::DisputeReasonHashMissing);
        }

        // Idempotency: reject duplicate (shipment_id, resolution, reason_hash) within the window.
        let mut payload = soroban_sdk::Bytes::new(&env);
        payload.append(&soroban_sdk::Bytes::from_array(
            &env,
            &shipment_id.to_be_bytes(),
        ));
        payload.append(&resolution.clone().to_xdr(&env));
        payload.append(&reason_hash.clone().into());
        check_idempotency(&env, payload)?;

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        require_not_finalized(&shipment)?;

        if shipment.status != ShipmentStatus::Disputed {
            return Err(NavinError::InvalidStatus);
        }

        let escrow_amount = shipment.escrow_amount;
        if escrow_amount == 0 {
            return Err(NavinError::InsufficientFunds);
        }

        shipment.escrow_amount = 0;
        shipment.updated_at = env.ledger().timestamp();
        shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);

        let recipient = match resolution {
            DisputeResolution::ReleaseToCarrier => {
                shipment.status = ShipmentStatus::Delivered;
                shipment.carrier.clone()
            }
            DisputeResolution::RefundToCompany => {
                shipment.status = ShipmentStatus::Cancelled;
                shipment.sender.clone()
            }
        };

        storage::decrement_status_count(&env, &ShipmentStatus::Disputed);
        storage::increment_status_count(&env, &shipment.status);
        storage::decrement_active_shipment_count(&env, &shipment.sender);

        finalize_if_settled(&env, &mut shipment);
        persist_shipment(&env, &shipment)?;
        storage::remove_escrow_balance(&env, shipment_id);
        extend_shipment_ttl(&env, shipment_id);

        match resolution {
            DisputeResolution::ReleaseToCarrier => {
                events::emit_escrow_released(&env, shipment_id, &recipient, escrow_amount);
            }
            DisputeResolution::RefundToCompany => {
                events::emit_escrow_refunded(&env, shipment_id, &recipient, escrow_amount);
                // Reputation: carrier lost this dispute
                events::emit_carrier_dispute_loss(&env, &shipment.carrier, shipment_id);
            }
        }

        // Emit specialized resolution event with context
        events::emit_dispute_resolved(&env, shipment_id, &resolution, &reason_hash, &admin);

        events::emit_notification(
            &env,
            &shipment.sender,
            NotificationType::DisputeResolved,
            shipment_id,
            &reason_hash,
        );
        events::emit_notification(
            &env,
            &shipment.receiver,
            NotificationType::DisputeResolved,
            shipment_id,
            &reason_hash,
        );
        events::emit_notification(
            &env,
            &shipment.carrier,
            NotificationType::DisputeResolved,
            shipment_id,
            &reason_hash,
        );

        Ok(())
    }

    /// Handoff a shipment from current carrier to a new carrier.
    /// Only the current assigned carrier can initiate the handoff.
    /// New carrier must have Carrier role.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `current_carrier` - Current assigned carrier address.
    /// * `new_carrier` - New carrier address to assign.
    /// * `shipment_id` - ID of the shipment.
    /// * `handoff_hash` - Hash of the handoff documentation.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful handoff.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::InvalidHash` - If handoff_hash is all zeros.
    /// * `NavinError::Unauthorized` - If current_carrier is not the assigned carrier.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    /// * `NavinError::ShipmentAlreadyCompleted` - If shipment is already completed.
    ///
    /// # Examples
    /// ```rust
    /// // contract.handoff_shipment(env, old, new_carrier, 1, hash);
    /// ```
    pub fn handoff_shipment(
        env: Env,
        current_carrier: Address,
        new_carrier: Address,
        shipment_id: u64,
        handoff_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        current_carrier.require_auth();
        require_role(&env, &current_carrier, Role::Carrier)?;
        require_role(&env, &new_carrier, Role::Carrier)?;

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        require_not_finalized(&shipment)?;

        // Validate hash before storage
        validation::validate_hash(&handoff_hash)?;

        // Verify current carrier is the assigned carrier
        if shipment.carrier != current_carrier {
            return Err(NavinError::Unauthorized);
        }

        // Prevent handoff from completed shipments
        match shipment.status {
            ShipmentStatus::Delivered | ShipmentStatus::Cancelled => {
                return Err(NavinError::ShipmentAlreadyCompleted);
            }
            _ => {}
        }

        // Update carrier address on the shipment
        let old_carrier = shipment.carrier.clone();
        shipment.carrier = new_carrier.clone();
        shipment.updated_at = env.ledger().timestamp();
        shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);

        persist_shipment(&env, &shipment)?;
        extend_shipment_ttl(&env, shipment_id);

        // Emit carrier_handoff event
        events::emit_carrier_handoff(&env, shipment_id, &old_carrier, &new_carrier, &handoff_hash);

        // Emit carrier_handoff_completed event
        events::emit_carrier_handoff_completed(&env, &old_carrier, &new_carrier, shipment_id);

        // Record a milestone for the handoff
        events::emit_milestone_recorded(
            &env,
            shipment_id,
            &symbol_short!("handoff"),
            &handoff_hash,
            &current_carrier,
        );

        Ok(())
    }

    /// Report a condition breach for a shipment (temperature, humidity, impact, tamper).
    ///
    /// Only the assigned carrier can report a breach. This is purely informational:
    /// shipment status is **not** changed. The full sensor payload stays off-chain;
    /// only its `data_hash` is emitted on-chain following the Hash-and-Emit pattern.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `carrier` - Carrier address reporting the breach.
    /// * `shipment_id` - ID of the shipment.
    /// * `breach_type` - Type of condition breach.
    /// * `severity` - Severity level of the breach.
    /// * `data_hash` - Hash of the breach data.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful breach report.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::InvalidHash` - If data_hash is all zeros.
    /// * `NavinError::Unauthorized` - If caller is not the assigned carrier.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    ///
    /// # Examples
    /// ```rust
    /// // contract.report_condition_breach(&env, &carrier, 1, BreachType::TemperatureHigh, Severity::High, &hash);
    /// ```
    pub fn report_condition_breach(
        env: Env,
        carrier: Address,
        shipment_id: u64,
        breach_type: BreachType,
        severity: Severity,
        data_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        carrier.require_auth();
        require_role(&env, &carrier, Role::Carrier)?;

        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        require_not_finalized(&shipment)?;

        // Validate hash before storage
        validation::validate_hash(&data_hash)?;

        // Only the assigned carrier for this shipment may report
        if shipment.carrier != carrier {
            return Err(NavinError::Unauthorized);
        }

        // Enforce breach payload size guard
        let config = config::get_config(&env);
        let current_breach_count = storage::get_breach_event_count(&env, shipment_id);
        if current_breach_count >= config.max_breaches_per_shipment {
            return Err(NavinError::BreachLimitExceeded);
        }

        events::emit_condition_breach(
            &env,
            shipment_id,
            &carrier,
            &breach_type,
            &severity,
            &data_hash,
        );

        // Reputation: record breach against carrier
        events::emit_carrier_breach(&env, &carrier, shipment_id, &breach_type, &severity);

        // Increment breach event count
        storage::increment_breach_event_count(&env, shipment_id);

        // Auto-open dispute on Critical breaches when the config toggle is enabled.
        // Skips silently if the shipment is already Disputed or Cancelled.
        let cfg = config::get_config(&env);
        if cfg.auto_dispute_breach
            && severity == Severity::Critical
            && shipment.status != ShipmentStatus::Cancelled
            && shipment.status != ShipmentStatus::Disputed
        {
            let old_status = shipment.status.clone();
            let mut s = shipment;
            s.status = ShipmentStatus::Disputed;
            s.updated_at = env.ledger().timestamp();
            s.integration_nonce = s.integration_nonce.saturating_add(1);
            let sender = s.sender.clone();
            let receiver = s.receiver.clone();
            storage::set_shipment(&env, &s);
            storage::decrement_status_count(&env, &old_status);
            storage::increment_status_count(&env, &ShipmentStatus::Disputed);
            storage::increment_total_disputes(&env);
            extend_shipment_ttl(&env, shipment_id);
            // Use the breach data hash as the dispute reason so indexers can correlate
            events::emit_dispute_raised(&env, shipment_id, &carrier, &data_hash);
            events::emit_notification(
                &env,
                &sender,
                NotificationType::DisputeRaised,
                shipment_id,
                &data_hash,
            );
            events::emit_notification(
                &env,
                &receiver,
                NotificationType::DisputeRaised,
                shipment_id,
                &data_hash,
            );
            events::emit_notification(
                &env,
                &carrier,
                NotificationType::DisputeRaised,
                shipment_id,
                &data_hash,
            );
        }

        Ok(())
    }

    /// Verify a proof-of-delivery hash against the stored confirmation hash.
    ///
    /// Returns `true` if `proof_hash` matches the hash stored during delivery confirmation,
    /// `false` if delivered but hashes differ, and errors if the shipment does not exist.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment.
    /// * `proof_hash` - Hash to verify against stored confirmation hash.
    ///
    /// # Returns
    /// * `Result<bool, NavinError>` - True if hashes match, false otherwise.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::InvalidHash` - If proof_hash is all zeros.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    ///
    /// # Examples
    /// ```rust
    /// // let is_valid = contract.verify_delivery_proof(&env, 1, hash);
    /// ```
    pub fn verify_delivery_proof(
        env: Env,
        shipment_id: u64,
        proof_hash: BytesN<32>,
    ) -> Result<bool, NavinError> {
        require_initialized(&env)?;

        // Validate hash
        validation::validate_hash(&proof_hash)?;

        // Ensure the shipment exists
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }

        let stored = storage::get_confirmation_hash(&env, shipment_id);
        Ok(stored == Some(proof_hash))
    }

    /// Propose a new admin for the contract. Only the current admin can call this.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Current administrator address.
    /// * `new_admin` - Address proposed as the new administrator.
    pub fn transfer_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        storage::set_proposed_admin(&env, &new_admin);
        events::emit_admin_proposed(&env, &admin, &new_admin);

        Ok(())
    }

    /// Accept the admin role transfer. Only the proposed admin can call this.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `new_admin` - The proposed administrator address accepting the role.
    pub fn accept_admin_transfer(env: Env, new_admin: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        new_admin.require_auth();

        let proposed = storage::get_proposed_admin(&env).ok_or(NavinError::Unauthorized)?;

        if proposed != new_admin {
            return Err(NavinError::Unauthorized);
        }

        let old_admin = storage::get_admin(&env);

        storage::set_admin(&env, &new_admin);
        storage::clear_proposed_admin(&env);

        // Also update the role for the new admin if it's not already set
        storage::set_company_role(&env, &new_admin);

        events::emit_admin_transferred(&env, &old_admin, &new_admin);

        Ok(())
    }

    /// Initialize multi-signature configuration for critical admin actions.
    /// Only the current admin can call this. Must be called after contract initialization.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Current administrator address.
    /// * `admins` - List of admin addresses for multi-sig (2-10 addresses).
    /// * `threshold` - Number of approvals required (must be <= admin count).
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if multi-sig is configured.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller is not the admin.
    /// * `NavinError::InvalidMultiSigConfig` - If config is invalid.
    ///
    /// # Examples
    /// ```rust
    /// // let admins = vec![&env, admin1, admin2, admin3];
    /// // contract.init_multisig(&env, &admin, &admins, 2);
    /// ```
    pub fn init_multisig(
        env: Env,
        admin: Address,
        admins: soroban_sdk::Vec<Address>,
        threshold: u32,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        // Validate configuration
        let config = config::get_config(&env);
        let admin_count = admins.len();
        if admin_count < config.multisig_min_admins || admin_count > config.multisig_max_admins {
            return Err(NavinError::InvalidMultiSigConfig);
        }

        if threshold == 0 || threshold > admin_count {
            return Err(NavinError::InvalidMultiSigConfig);
        }

        storage::set_admin_list(&env, &admins);
        storage::set_multisig_threshold(&env, threshold);
        storage::set_proposal_counter(&env, 0);

        env.events()
            .publish((symbol_short!("ms_init"),), (admin_count, threshold));

        Ok(())
    }

    /// Propose a critical admin action that requires multi-sig approval.
    /// Only admins in the admin list can propose actions.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `proposer` - Admin address creating the proposal.
    /// * `action` - The action to be executed after approval.
    ///
    /// # Returns
    /// * `Result<u64, NavinError>` - The proposal ID.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::NotAnAdmin` - If caller is not in the admin list.
    ///
    /// # Examples
    /// ```rust
    /// // let action = AdminAction::Upgrade(new_wasm_hash);
    /// // let proposal_id = contract.propose_action(&env, &admin, &action);
    /// ```
    pub fn propose_action(
        env: Env,
        proposer: Address,
        action: crate::types::AdminAction,
    ) -> Result<u64, NavinError> {
        require_initialized(&env)?;
        proposer.require_auth();

        // Check if proposer is in admin list
        if !storage::is_admin(&env, &proposer) {
            return Err(NavinError::NotAnAdmin);
        }

        let proposal_id = storage::get_proposal_counter(&env)
            .checked_add(1)
            .ok_or(NavinError::CounterOverflow)?;

        let now = env.ledger().timestamp();
        let config = config::get_config(&env);
        let expires_at = now + config.proposal_expiry_seconds;

        let mut approvals = soroban_sdk::Vec::new(&env);
        approvals.push_back(proposer.clone());

        let proposal = crate::types::Proposal {
            id: proposal_id,
            proposer: proposer.clone(),
            action: action.clone(),
            approvals,
            created_at: now,
            expires_at,
            executed: false,
        };

        storage::set_proposal(&env, &proposal);
        storage::set_proposal_counter(&env, proposal_id);

        // Compute and store the deterministic action digest (issue #297).
        let digest_hash = compute_action_digest(&env, proposal_id, &action);
        let digest_record = crate::types::ProposalActionDigest {
            proposal_id,
            digest: digest_hash.clone(),
            computed_at: now,
        };
        storage::set_proposal_digest(&env, proposal_id, &digest_record);

        env.events()
            .publish((symbol_short!("propose"),), (proposal_id, proposer, action));
        // Emit digest event so off-chain indexers can capture it without a query.
        env.events().publish(
            (Symbol::new(&env, "proposal_digest"),),
            (proposal_id, digest_hash),
        );

        Ok(proposal_id)
    }

    /// Approve a pending proposal. Only admins in the admin list can approve.
    /// Same admin cannot approve twice.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `approver` - Admin address approving the proposal.
    /// * `proposal_id` - ID of the proposal to approve.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if approved successfully.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::NotAnAdmin` - If caller is not in the admin list.
    /// * `NavinError::ProposalNotFound` - If proposal doesn't exist.
    /// * `NavinError::ProposalExpired` - If proposal has expired.
    /// * `NavinError::ProposalAlreadyExecuted` - If proposal was already executed.
    /// * `NavinError::AlreadyApproved` - If admin already approved this proposal.
    ///
    /// # Examples
    /// ```rust
    /// // contract.approve_action(&env, &admin2, 1);
    /// ```
    pub fn approve_action(env: Env, approver: Address, proposal_id: u64) -> Result<(), NavinError> {
        require_initialized(&env)?;
        approver.require_auth();

        // Check if approver is in admin list
        if !storage::is_admin(&env, &approver) {
            return Err(NavinError::NotAnAdmin);
        }

        let mut proposal =
            storage::get_proposal(&env, proposal_id).ok_or(NavinError::ProposalNotFound)?;

        // Check if proposal has expired
        let now = env.ledger().timestamp();
        if now > proposal.expires_at {
            return Err(NavinError::ProposalExpired);
        }

        // Check if already executed
        if proposal.executed {
            return Err(NavinError::ProposalAlreadyExecuted);
        }

        // Check if already approved by this admin
        for existing_approver in proposal.approvals.iter() {
            if existing_approver == approver {
                return Err(NavinError::AlreadyApproved);
            }
        }

        // Add approval
        proposal.approvals.push_back(approver.clone());
        storage::set_proposal(&env, &proposal);

        env.events().publish(
            (symbol_short!("approve"),),
            (proposal_id, approver, proposal.approvals.len()),
        );

        // Check if threshold is met and auto-execute
        let threshold = storage::get_multisig_threshold(&env).unwrap_or(2);
        if proposal.approvals.len() >= threshold {
            Self::execute_proposal_internal(env.clone(), proposal_id)?;
        }

        Ok(())
    }

    /// Execute a proposal that has met the approval threshold.
    /// Can be called by anyone once threshold is met.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `proposal_id` - ID of the proposal to execute.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if executed successfully.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ProposalNotFound` - If proposal doesn't exist.
    /// * `NavinError::ProposalExpired` - If proposal has expired.
    /// * `NavinError::ProposalAlreadyExecuted` - If proposal was already executed.
    /// * `NavinError::InsufficientApprovals` - If not enough approvals.
    ///
    /// # Examples
    /// ```rust
    /// // contract.execute_proposal(&env, 1);
    /// ```
    pub fn execute_proposal(env: Env, proposal_id: u64) -> Result<(), NavinError> {
        require_initialized(&env)?;
        Self::execute_proposal_internal(env, proposal_id)
    }

    /// Internal function to execute a proposal.
    fn execute_proposal_internal(env: Env, proposal_id: u64) -> Result<(), NavinError> {
        let mut proposal =
            storage::get_proposal(&env, proposal_id).ok_or(NavinError::ProposalNotFound)?;

        // Check if proposal has expired
        let now = env.ledger().timestamp();
        if now > proposal.expires_at {
            return Err(NavinError::ProposalExpired);
        }

        // Check if already executed
        if proposal.executed {
            return Err(NavinError::ProposalAlreadyExecuted);
        }

        // Check if threshold is met
        let threshold = storage::get_multisig_threshold(&env).unwrap_or(2);
        if proposal.approvals.len() < threshold {
            return Err(NavinError::InsufficientApprovals);
        }

        // Mark as executed
        proposal.executed = true;
        storage::set_proposal(&env, &proposal);

        // Execute the action (clone action before matching to avoid move issues)
        let action = proposal.action.clone();
        match action {
            crate::types::AdminAction::Upgrade(wasm_hash) => {
                let new_version = storage::get_version(&env)
                    .checked_add(1)
                    .ok_or(NavinError::CounterOverflow)?;

                storage::set_version(&env, new_version);
                events::emit_contract_upgraded(&env, &proposal.proposer, &wasm_hash, new_version);
                env.deployer().update_current_contract_wasm(wasm_hash);
            }
            crate::types::AdminAction::TransferAdmin(new_admin) => {
                let old_admin = storage::get_admin(&env);
                storage::set_admin(&env, &new_admin);
                storage::set_company_role(&env, &new_admin);
                events::emit_admin_transferred(&env, &old_admin, &new_admin);
            }
            crate::types::AdminAction::ForceRelease(shipment_id) => {
                let mut shipment =
                    storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

                let escrow_amount = shipment.escrow_amount;
                if escrow_amount > 0 {
                    // Get token contract address
                    if let Some(token_contract) = storage::get_token_contract(&env) {
                        // Transfer tokens from this contract to carrier
                        let contract_address = env.current_contract_address();
                        invoke_token_transfer(
                            &env,
                            &token_contract,
                            &contract_address,
                            &shipment.carrier,
                            escrow_amount,
                        )?;
                    }

                    shipment.escrow_amount = 0;
                    shipment.updated_at = env.ledger().timestamp();
                    shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);
                    persist_shipment(&env, &shipment)?;

                    events::emit_escrow_released(
                        &env,
                        shipment_id,
                        &shipment.carrier,
                        escrow_amount,
                    );
                }
            }
            crate::types::AdminAction::ForceRefund(shipment_id) => {
                let mut shipment =
                    storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

                let escrow_amount = shipment.escrow_amount;
                if escrow_amount > 0 {
                    // Get token contract address
                    if let Some(token_contract) = storage::get_token_contract(&env) {
                        // Transfer tokens from this contract to company
                        let contract_address = env.current_contract_address();
                        invoke_token_transfer(
                            &env,
                            &token_contract,
                            &contract_address,
                            &shipment.sender,
                            escrow_amount,
                        )?;
                    }

                    shipment.escrow_amount = 0;
                    shipment.updated_at = env.ledger().timestamp();
                    shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);
                    persist_shipment(&env, &shipment)?;

                    events::emit_escrow_refunded(
                        &env,
                        shipment_id,
                        &shipment.sender,
                        escrow_amount,
                    );
                }
            }
        }

        env.events()
            .publish((symbol_short!("executed"),), (proposal_id, proposal.action));

        Ok(())
    }

    /// Get a proposal by ID.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `proposal_id` - ID of the proposal.
    ///
    /// # Returns
    /// * `Result<Proposal, NavinError>` - The proposal data.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ProposalNotFound` - If proposal doesn't exist.
    ///
    /// # Examples
    /// ```rust
    /// // let proposal = contract.get_proposal(&env, 1);
    /// ```
    pub fn get_proposal(env: Env, proposal_id: u64) -> Result<crate::types::Proposal, NavinError> {
        require_initialized(&env)?;
        storage::get_proposal(&env, proposal_id).ok_or(NavinError::ProposalNotFound)
    }

    /// Get the multi-sig configuration.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<(Vec<Address>, u32), NavinError>` - Tuple of (admin list, threshold).
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let (admins, threshold) = contract.get_multisig_config(&env);
    /// ```
    pub fn get_multisig_config(env: Env) -> Result<(soroban_sdk::Vec<Address>, u32), NavinError> {
        require_initialized(&env)?;
        let admins = storage::get_admin_list(&env).unwrap_or(soroban_sdk::Vec::new(&env));
        let threshold = storage::get_multisig_threshold(&env).unwrap_or(0);
        Ok((admins, threshold))
    }

    /// Update the contract configuration.
    /// Only the admin can update the configuration.
    /// Emits a `config_updated` event on success.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin address.
    /// * `new_config` - The new configuration to apply.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully updated.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller is not the admin.
    /// * `NavinError::InvalidConfig` - If the configuration is invalid.
    ///
    /// # Examples
    /// ```rust
    /// // let mut config = ContractConfig::default();
    /// // config.batch_operation_limit = 20;
    /// // contract.update_config(&env, &admin, config);
    /// ```
    pub fn update_config(
        env: Env,
        admin: Address,
        new_config: ContractConfig,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        // Validate the new configuration
        config::validate_config(&new_config).map_err(|_| NavinError::InvalidConfig)?;

        // Store the new configuration
        config::set_config(&env, &new_config);

        // Emit config_updated event
        env.events()
            .publish((Symbol::new(&env, "config_updated"),), (admin, new_config));

        Ok(())
    }

    /// Update the platform fee configuration. Only Admin can execute.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin executing the configuration.
    /// * `fee_bps` - Fee in basis points (capped at 1000).
    /// * `treasury` - Address where fees will be collected.
    pub fn set_platform_fee(
        env: Env,
        admin: Address,
        fee_bps: u32,
        treasury: Address,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        require_not_paused(&env)?;
        admin.require_auth();
        require_admin(&env, &admin)?;

        if fee_bps > 1000 {
            return Err(NavinError::InvalidAmount);
        }

        let config = FeeConfig {
            fee_bps,
            treasury: treasury.clone(),
        };

        storage::set_fee_config(&env, &config);
        storage::set_treasury(&env, &treasury);

        events::emit_fee_config_updated(&env, &admin, fee_bps, &treasury);

        Ok(())
    }

    /// Add a new carrier to the contract.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<ContractConfig, NavinError>` - The current configuration.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let config = contract.get_config(&env);
    /// ```
    pub fn get_contract_config(env: Env) -> Result<ContractConfig, NavinError> {
        require_initialized(&env)?;
        Ok(config::get_config(&env))
    }

    /// Cancel a shipment and auto-refund escrow if its delivery deadline has passed.
    /// Permissionless design — can be triggered by any caller (e.g., automated cron/crank).
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the target shipment.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully cancelled and escrow refunded.
    ///
    /// # Errors
    /// * `NavinError::NotExpired` - If the current ledger time hasn't passed the deadline.
    /// * `NavinError::ShipmentAlreadyCompleted` - If the shipment is already in a terminal state.
    pub fn check_deadline(env: Env, shipment_id: u64) -> Result<(), NavinError> {
        require_initialized(&env)?;

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        let config = config::get_config(&env);
        let expiry_threshold = shipment
            .deadline
            .saturating_add(config.deadline_grace_seconds);

        if env.ledger().timestamp() < expiry_threshold {
            return Err(NavinError::NotExpired);
        }

        match shipment.status {
            ShipmentStatus::Delivered | ShipmentStatus::Disputed | ShipmentStatus::Cancelled => {
                return Err(NavinError::ShipmentAlreadyCompleted);
            }
            _ => {}
        }

        let escrow_amount = shipment.escrow_amount;
        let old_status = shipment.status.clone();
        shipment.status = ShipmentStatus::Cancelled;
        shipment.escrow_amount = 0;
        shipment.updated_at = env.ledger().timestamp();
        shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);

        persist_shipment(&env, &shipment)?;
        storage::decrement_status_count(&env, &old_status);
        storage::increment_status_count(&env, &ShipmentStatus::Cancelled);
        storage::decrement_active_shipment_count(&env, &shipment.sender);

        if escrow_amount > 0 {
            storage::remove_escrow_balance(&env, shipment_id);

            let token_contract =
                storage::get_token_contract(&env).ok_or(NavinError::NotInitialized)?;
            let contract_address = env.current_contract_address();
            invoke_token_transfer(
                &env,
                &token_contract,
                &contract_address,
                &shipment.sender,
                escrow_amount,
            )?;
            events::emit_escrow_refunded(&env, shipment_id, &shipment.sender, escrow_amount);
        }

        extend_shipment_ttl(&env, shipment_id);
        events::emit_shipment_expired(&env, shipment_id);

        Ok(())
    }

    /// Generate a deterministic shipment reference string for cross-system interoperability.
    /// The reference is derived from: SHA-256(NetworkIdentifier | ContractAddress | ShipmentID).
    pub fn get_shipment_reference(
        env: Env,
        shipment_id: u64,
    ) -> Result<soroban_sdk::String, NavinError> {
        require_initialized(&env)?;
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }

        let network_id = env.ledger().network_id();
        let contract_address = env.current_contract_address();

        let mut payload = soroban_sdk::Bytes::new(&env);
        payload.append(&network_id.into());
        payload.append(&contract_address.to_xdr(&env));
        payload.append(&soroban_sdk::Bytes::from_array(
            &env,
            &shipment_id.to_be_bytes(),
        ));

        let hash_array = env.crypto().sha256(&payload).to_array();
        let mut hex_chars = [0u8; 64];
        let alphabet = b"0123456789abcdef";
        for i in 0..32 {
            hex_chars[i * 2] = alphabet[(hash_array[i] >> 4) as usize];
            hex_chars[i * 2 + 1] = alphabet[(hash_array[i] & 0x0f) as usize];
        }

        Ok(soroban_sdk::String::from_str(&env, unsafe {
            core::str::from_utf8_unchecked(&hex_chars)
        }))
    }

    /// Pause the contract, disabling all state-changing operations.
    /// Only the admin can pause the contract. Read-only queries still work.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - The admin address pausing the contract.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully paused.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller is not the admin.
    ///
    /// # Examples
    /// ```rust
    /// // contract.pause(&env, &admin);
    /// ```
    pub fn pause(env: Env, admin: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        require_admin_or_guardian(&env, &admin)?;

        storage::set_paused(&env, true);
        events::emit_contract_paused(&env, &admin);

        Ok(())
    }

    /// Unpause the contract, re-enabling state-changing operations.
    /// Only the admin can unpause the contract.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - The admin address unpausing the contract.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully unpaused.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller is not the admin.
    ///
    /// # Examples
    /// ```rust
    /// // contract.unpause(&env, &admin);
    /// ```
    pub fn unpause(env: Env, admin: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        require_admin_or_guardian(&env, &admin)?;

        storage::set_paused(&env, false);
        events::emit_contract_unpaused(&env, &admin);

        Ok(())
    }

    /// Check if the contract is currently paused.
    /// Read-only function, no authentication required.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<bool, NavinError>` - True if paused, false otherwise.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let paused = contract.is_paused(&env)?;
    /// ```
    pub fn is_paused(env: Env) -> Result<bool, NavinError> {
        require_initialized(&env)?;
        Ok(storage::is_paused(&env))
    }

    /// Get the status hash for a shipment at a specific status point.
    /// Read-only function, no authentication required.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - The ID of the shipment.
    /// * `status` - The status to retrieve the hash for.
    ///
    /// # Returns
    /// * `Result<BytesN<32>, NavinError>` - The data hash recorded at that status.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If the shipment doesn't exist.
    /// * `NavinError::StatusHashNotFound` - If no hash was recorded for that status.
    ///
    /// # Examples
    /// ```rust
    /// // let hash = contract.get_status_hash(&env, 1, &ShipmentStatus::InTransit)?;
    /// ```
    pub fn get_status_hash(
        env: Env,
        shipment_id: u64,
        status: ShipmentStatus,
    ) -> Result<BytesN<32>, NavinError> {
        require_initialized(&env)?;

        // Verify shipment exists
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }

        storage::get_status_hash(&env, shipment_id, &status).ok_or(NavinError::StatusHashNotFound)
    }

    /// Verify that a given data hash matches what was recorded on-chain for a
    /// shipment at a specific status point.
    /// Read-only function, no authentication required.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - The ID of the shipment.
    /// * `status` - The status to verify against.
    /// * `expected_hash` - The hash to verify.
    ///
    /// # Returns
    /// * `Result<bool, NavinError>` - True if the hash matches, false otherwise.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::InvalidHash` - If expected_hash is all zeros.
    /// * `NavinError::ShipmentNotFound` - If the shipment doesn't exist.
    /// * `NavinError::StatusHashNotFound` - If no hash was recorded for that status.
    ///
    /// # Examples
    /// ```rust
    /// // let verified = contract.verify_data_hash(&env, 1, &ShipmentStatus::InTransit, &hash)?;
    /// ```
    pub fn verify_data_hash(
        env: Env,
        shipment_id: u64,
        status: ShipmentStatus,
        expected_hash: BytesN<32>,
    ) -> Result<bool, NavinError> {
        require_initialized(&env)?;

        // Validate hash
        validation::validate_hash(&expected_hash)?;

        // Verify shipment exists
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }

        let stored_hash = storage::get_status_hash(&env, shipment_id, &status)
            .ok_or(NavinError::StatusHashNotFound)?;

        Ok(stored_hash == expected_hash)
    }

    /// Check the health of the contract data.
    pub fn check_contract_health(
        env: Env,
        admin: Address,
    ) -> Result<SystemHealthStatus, NavinError> {
        require_initialized(&env)?;
        admin.require_auth();
        require_admin_or_operator(&env, &admin)?;

        Ok(diagnostics::run_system_health_check(&env))
    }

    /// Manually reset the circuit breaker after resolving a token contract issue.
    ///
    /// Only callable by the admin. Use after confirming the token contract is healthy
    /// following a run of consecutive transfer failures.
    pub fn reset_circuit_breaker(env: Env, admin: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        circuit_breaker::manual_reset(&env, &admin)
    }

    /// Scan all tracked shipments and return every consistency violation found.
    ///
    /// Checks per-shipment invariants across the full ledger:
    /// - Escrow amounts match storage
    /// - Finalized flag is only set on terminal shipments with zero escrow
    /// - Paid milestones are a subset of the payment schedule
    /// - Timestamps are non-decreasing
    /// - Deadlines are strictly after creation time
    ///
    /// Intended for periodic admin audits; not safe to call on hot paths
    /// due to the O(n) scan over all shipments.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Admin or operator address (auth required).
    ///
    /// # Returns
    /// * `Result<Vec<ConsistencyViolation>, NavinError>` - List of detected violations.
    ///   An empty vec means all invariants hold.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller is not admin or operator.
    pub fn check_consistency_violations(
        env: Env,
        admin: Address,
    ) -> Result<soroban_sdk::Vec<ConsistencyViolation>, NavinError> {
        require_initialized(&env)?;
        admin.require_auth();
        require_admin_or_operator(&env, &admin)?;
        Ok(consistency::check_all_consistency(&env))
    }

    // =========================================================================
    // Issue #295 — Company/Carrier Relationship Query APIs
    // =========================================================================

    /// Check whether a carrier is allowed (whitelisted and not suspended) for a company.
    ///
    /// Semantic alias for `is_carrier_whitelisted` that also checks suspension
    /// state, making it the single authoritative query for frontend consumers.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `company` - The company address.
    /// * `carrier` - The carrier address to check.
    ///
    /// # Returns
    /// * `Result<bool, NavinError>` - `true` if whitelisted and neither party suspended.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    pub fn is_company_carrier_allowed(
        env: Env,
        company: Address,
        carrier: Address,
    ) -> Result<bool, NavinError> {
        require_initialized(&env)?;
        if !storage::is_carrier_whitelisted(&env, &company, &carrier) {
            return Ok(false);
        }
        if storage::is_company_suspended(&env, &company) {
            return Ok(false);
        }
        if storage::is_carrier_suspended(&env, &carrier) {
            return Ok(false);
        }
        Ok(true)
    }

    /// Paginated listing of carriers whitelisted by a company.
    ///
    /// Iterates over a caller-supplied `candidates` list and returns only those
    /// that are whitelisted. The caller provides the candidate set (e.g. from an
    /// off-chain index); this keeps the query deterministic and bounded.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `company` - The company whose whitelist is queried.
    /// * `candidates` - Ordered list of carrier addresses to check.
    /// * `cursor` - Index into `candidates` to start from (0 for first page).
    /// * `page_size` - Maximum whitelisted carriers to return (1–50).
    ///
    /// # Returns
    /// * `Result<CarrierRelationshipPage, NavinError>` - Page of whitelisted carriers.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::InvalidConfig` - If `page_size` is 0 or > 50.
    pub fn list_company_carriers(
        env: Env,
        company: Address,
        candidates: Vec<Address>,
        cursor: u32,
        page_size: u32,
    ) -> Result<CarrierRelationshipPage, NavinError> {
        require_initialized(&env)?;

        if page_size == 0 || page_size > 50 {
            return Err(NavinError::InvalidConfig);
        }

        let total = candidates.len();
        let mut result = Vec::new(&env);
        let mut scanned: u32 = 0;
        let mut next_cursor: Option<u32> = None;
        let mut idx = cursor;

        while idx < total {
            let candidate = candidates.get(idx).unwrap();
            scanned = scanned.saturating_add(1);

            if storage::is_carrier_whitelisted(&env, &company, &candidate) {
                result.push_back(candidate);
                if result.len() == page_size {
                    let next = idx.saturating_add(1);
                    if next < total {
                        next_cursor = Some(next);
                    }
                    break;
                }
            }
            idx = idx.saturating_add(1);
        }

        Ok(CarrierRelationshipPage {
            carriers: result,
            next_cursor,
            total_scanned: scanned,
        })
    }

    // =========================================================================
    // Issue #296 — Shipment Creation Quota Window
    // =========================================================================

    /// Configure the per-company shipment creation quota.
    ///
    /// Sets `creation_quota_max` (max shipments per window) and
    /// `creation_quota_window_seconds` (window duration). Set `max` to 0 to
    /// disable the quota entirely (default).
    ///
    /// Only the admin can call this.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin address.
    /// * `max_per_window` - Max shipments a company may create per window (0 = disabled).
    /// * `window_seconds` - Duration of the quota window in seconds.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on success.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller is not the admin.
    /// * `NavinError::InvalidConfig` - If `window_seconds` is 0 when `max > 0`.
    pub fn set_creation_quota(
        env: Env,
        admin: Address,
        max_per_window: u32,
        window_seconds: u64,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();
        require_admin(&env, &admin)?;

        if max_per_window > 0 && window_seconds == 0 {
            return Err(NavinError::InvalidConfig);
        }

        let mut cfg = config::get_config(&env);
        cfg.creation_quota_max = max_per_window;
        cfg.creation_quota_window_seconds = window_seconds;
        config::set_config(&env, &cfg);

        env.events().publish(
            (Symbol::new(&env, "quota_set"),),
            (admin, max_per_window, window_seconds),
        );

        Ok(())
    }

    /// Query the current creation quota status for a company.
    ///
    /// Returns `(used, remaining)` for the current window. When the quota is
    /// disabled (`max == 0`), returns `(0, u32::MAX)`.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `company` - The company address to query.
    ///
    /// # Returns
    /// * `Result<(u32, u32), NavinError>` - `(used, remaining)` in the current window.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    pub fn get_creation_quota_status(env: Env, company: Address) -> Result<(u32, u32), NavinError> {
        require_initialized(&env)?;
        let cfg = config::get_config(&env);

        if cfg.creation_quota_max == 0 {
            return Ok((0, u32::MAX));
        }

        let now = env.ledger().timestamp();
        match storage::get_creation_quota(&env, &company) {
            None => Ok((0, cfg.creation_quota_max)),
            Some(t) => {
                if now >= t.window_start + cfg.creation_quota_window_seconds {
                    Ok((0, cfg.creation_quota_max))
                } else {
                    let used = t.count;
                    let remaining = cfg.creation_quota_max.saturating_sub(used);
                    Ok((used, remaining))
                }
            }
        }
    }

    // =========================================================================
    // Issue #297 — Multi-sig Proposal Action Hash and Digest Query
    // =========================================================================

    /// Retrieve the stored action digest for a proposal.
    ///
    /// The digest is computed and stored when `propose_action` is called.
    /// Off-chain signers can recompute `sha256(proposal_id_be_u64 || action_xdr)`
    /// to verify the exact payload before approving.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `proposal_id` - The proposal whose digest to retrieve.
    ///
    /// # Returns
    /// * `Result<ProposalActionDigest, NavinError>` - The stored digest record.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ProposalNotFound` - If the proposal or its digest does not exist.
    pub fn get_proposal_action_digest(
        env: Env,
        proposal_id: u64,
    ) -> Result<ProposalActionDigest, NavinError> {
        require_initialized(&env)?;
        storage::get_proposal_digest(&env, proposal_id).ok_or(NavinError::ProposalNotFound)
    }

    /// Compute the action digest for an `AdminAction` without storing it.
    ///
    /// Pure helper for off-chain tooling: returns the same digest that
    /// `propose_action` would store, so callers can verify before submitting.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `proposal_id` - The proposal ID to bind into the digest.
    /// * `action` - The action to hash.
    ///
    /// # Returns
    /// * `BytesN<32>` - The SHA-256 digest.
    pub fn compute_proposal_digest(
        env: Env,
        proposal_id: u64,
        action: crate::types::AdminAction,
    ) -> BytesN<32> {
        compute_action_digest(&env, proposal_id, &action)
    }

    /// Compute a deterministic SHA-256 hash of an ordered list of values.
    ///
    /// Each element is XDR-serialized in order and the concatenated bytes are
    /// hashed. Useful for off-chain verification of on-chain data payloads.
    pub fn get_canonical_hash(env: Env, fields: soroban_sdk::Vec<soroban_sdk::Val>) -> BytesN<32> {
        use soroban_sdk::xdr::ToXdr;
        let xdr_bytes = fields.to_xdr(&env);
        env.crypto().sha256(&xdr_bytes).into()
    }
}

/// Validates whether a version transition is permitted.
///
/// Standard upgrades are always allowed (current + 1).
/// Backward migrations or jump migrations must be explicitly defined.
fn is_allowed_migration(current: u32, target: u32) -> bool {
    // Forward progression is the standard case
    if target == current + 1 {
        return true;
    }

    // Explicitly allowed edges (e.g. for emergency rollback or skip-version migrations)
    // Format: &[(from_version, to_version)]
    let allowed_edges: &[(u32, u32)] = &[];

    for &(from, to) in allowed_edges {
        if from == current && to == target {
            return true;
        }
    }

    false
}

/// Compute the deterministic SHA-256 digest for a proposal action (issue #297).
///
/// Canonical serialization: `sha256(proposal_id_be_u64 || action_xdr)`
fn compute_action_digest(
    env: &Env,
    proposal_id: u64,
    action: &crate::types::AdminAction,
) -> BytesN<32> {
    let mut payload = soroban_sdk::Bytes::new(env);
    payload.append(&soroban_sdk::Bytes::from_array(
        env,
        &proposal_id.to_be_bytes(),
    ));
    payload.append(&action.clone().to_xdr(env));
    env.crypto().sha256(&payload).into()
}

/// Enforce the per-company creation quota window (issue #296).
///
/// Returns `CreationQuotaExceeded` if the company has exhausted their quota
/// for the current window. Rolls the window forward automatically when expired.
/// No-ops when `creation_quota_max == 0` (quota disabled).
fn check_and_update_creation_quota(env: &Env, company: &Address) -> Result<(), NavinError> {
    let cfg = config::get_config(env);
    if cfg.creation_quota_max == 0 {
        return Ok(());
    }

    let now = env.ledger().timestamp();
    let mut tracker =
        storage::get_creation_quota(env, company).unwrap_or(crate::types::CreationQuotaTracker {
            count: 0,
            window_start: now,
        });

    // Roll window if expired.
    if now >= tracker.window_start + cfg.creation_quota_window_seconds {
        tracker.window_start = now;
        tracker.count = 0;
    }

    if tracker.count >= cfg.creation_quota_max {
        return Err(NavinError::CreationQuotaExceeded);
    }

    tracker.count = tracker.count.saturating_add(1);
    storage::set_creation_quota(env, company, &tracker);
    Ok(())
}
