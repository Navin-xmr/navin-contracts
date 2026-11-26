//! Multi-sig proposal helpers and execution logic (issue #1138).
//!
//! Extracts proposal lifecycle, digestion, and execution helpers out of
//! `lib.rs` while preserving the exact contract entrypoint signatures and ABI.

use crate::config;
use crate::events;
use crate::storage;
use crate::types::{AdminAction, Proposal, ProposalActionDigest, ShipmentStatus};
use crate::validation;
use crate::NavinError;
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Address, BytesN, Env};

/// Compute the deterministic SHA-256 digest for a proposal action.
///
/// Canonical serialization: `sha256(proposal_id_be_u64 || action_xdr)`, with
/// the 32 salt bytes appended for a salted proposal
/// (`propose_action_with_salt`). Unsalted digests are unchanged.
pub fn compute_action_digest(
    env: &Env,
    proposal_id: u64,
    action: &AdminAction,
    salt: Option<&BytesN<32>>,
) -> BytesN<32> {
    let mut payload = soroban_sdk::Bytes::new(env);
    payload.append(&soroban_sdk::Bytes::from_array(
        env,
        &proposal_id.to_be_bytes(),
    ));
    payload.append(&action.clone().to_xdr(env));
    if let Some(salt) = salt {
        payload.append(&soroban_sdk::Bytes::from_array(env, &salt.to_array()));
    }
    env.crypto().sha256(&payload).into()
}

/// Shared body of `propose_action` and `propose_action_with_salt`.
///
/// When `salt` is `Some`, it must not have been used before; it is recorded
/// against the new proposal and folded into the proposal's digest.
pub fn propose_action_inner(
    env: &Env,
    proposer: &Address,
    action: &AdminAction,
    salt: Option<BytesN<32>>,
) -> Result<u64, NavinError> {
    crate::require_initialized(env)?;
    crate::require_not_paused(env)?;
    proposer.require_auth();

    // Check if proposer is in admin list
    if !storage::is_admin(env, proposer) {
        return Err(NavinError::NotAnAdmin);
    }

    if let Some(salt) = &salt {
        if storage::is_proposal_salt_used(env, salt) {
            return Err(NavinError::ProposalSaltReused);
        }
    }

    // Validate action
    if let AdminAction::Upgrade(hash) = action {
        if hash.to_array() == [0u8; 32] {
            return Err(NavinError::InvalidHash);
        }
    }

    let proposal_id = storage::get_proposal_counter(env)
        .checked_add(1)
        .ok_or(NavinError::CounterOverflow)?;

    let now = env.ledger().timestamp();
    let config = config::get_config(env);
    if config.proposal_expiry_seconds == 0 {
        return Err(NavinError::InvalidConfig);
    }
    let expires_at = now + config.proposal_expiry_seconds;

    let mut approvals = soroban_sdk::Vec::new(env);
    approvals.push_back(proposer.clone());

    let proposal = Proposal {
        id: proposal_id,
        proposer: proposer.clone(),
        action: action.clone(),
        approvals,
        created_at: now,
        expires_at,
        executed: false,
    };

    storage::set_proposal(env, &proposal);
    storage::set_proposal_counter(env, proposal_id);
    if let Some(salt) = &salt {
        storage::set_proposal_salt_used(env, salt, proposal_id, expires_at);
    }

    // Compute and store the deterministic action digest (issue #297).
    let digest_hash = compute_action_digest(env, proposal_id, action, salt.as_ref());
    let digest_record = ProposalActionDigest {
        proposal_id,
        digest: digest_hash.clone(),
        computed_at: now,
    };
    storage::set_proposal_digest(env, proposal_id, &digest_record);
    storage::extend_proposal_ttl(env, proposal_id, expires_at);

    events::emit_proposal_digest(env, proposal_id, digest_hash, now);

    Ok(proposal_id)
}

/// Approve a pending proposal. Only admins in the admin list can approve.
pub fn approve_action_internal(
    env: &Env,
    approver: &Address,
    proposal_id: u64,
) -> Result<(), NavinError> {
    crate::require_initialized(env)?;
    crate::require_not_paused(env)?;
    approver.require_auth();

    // Check if approver is in admin list
    if !storage::is_admin(env, approver) {
        return Err(NavinError::NotAnAdmin);
    }

    let mut proposal =
        storage::get_proposal(env, proposal_id).ok_or(NavinError::ProposalNotFound)?;

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
        if existing_approver == *approver {
            return Err(NavinError::AlreadyApproved);
        }
    }

    // Add approval
    proposal.approvals.push_back(approver.clone());
    storage::set_proposal(env, &proposal);
    storage::extend_proposal_ttl(env, proposal_id, proposal.expires_at);

    events::emit_proposal_approved(env, proposal_id, approver, proposal.approvals.len());

    // Check if threshold is met and auto-execute
    let threshold = storage::get_multisig_threshold(env).unwrap_or(2);
    if proposal.approvals.len() >= threshold {
        execute_proposal_internal(env, proposal_id)?;
    }

    Ok(())
}

/// Internal function to execute a proposal.
pub fn execute_proposal_internal(env: &Env, proposal_id: u64) -> Result<(), NavinError> {
    let mut proposal =
        storage::get_proposal(env, proposal_id).ok_or(NavinError::ProposalNotFound)?;

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
    let threshold = storage::get_multisig_threshold(env).unwrap_or(2);
    if proposal.approvals.len() < threshold {
        return Err(NavinError::InsufficientApprovals);
    }

    // Mark as executed
    proposal.executed = true;
    storage::set_proposal(env, &proposal);

    // Execute the action (clone action before matching to avoid move issues)
    let action = proposal.action.clone();
    match action {
        AdminAction::Upgrade(wasm_hash) => {
            let new_version = storage::get_version(env)
                .checked_add(1)
                .ok_or(NavinError::CounterOverflow)?;

            storage::set_version(env, new_version);
            events::emit_contract_upgraded(env, &proposal.proposer, &wasm_hash, new_version);
            env.deployer().update_current_contract_wasm(wasm_hash);
        }
        AdminAction::TransferAdmin(new_admin) => {
            let old_admin = storage::get_admin(env);
            storage::set_admin(env, &new_admin);
            storage::set_company_role(env, &new_admin);
            events::emit_admin_transferred(env, &old_admin, &new_admin);
        }
        AdminAction::ForceRelease(shipment_id, reason_hash) => {
            // Reason hash is mandatory and must be non-zero for audit trail.
            validation::validate_hash(&reason_hash)?;

            let mut shipment =
                storage::get_shipment(env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

            // Terminal shipments have already had their status/counters settled.
            if shipment.status == ShipmentStatus::Delivered
                || shipment.status == ShipmentStatus::Cancelled
            {
                return Err(NavinError::ShipmentAlreadyCompleted);
            }

            let escrow_amount = shipment.escrow_amount;
            if escrow_amount > 0 {
                // Get token contract address
                if let Some(token_contract) = storage::get_token_contract(env) {
                    // Transfer tokens from this contract to carrier
                    let contract_address = env.current_contract_address();
                    crate::invoke_token_transfer(
                        env,
                        &token_contract,
                        &contract_address,
                        &shipment.carrier,
                        escrow_amount,
                    )?;
                }

                shipment.escrow_amount = 0;
                events::emit_escrow_released(env, shipment_id, &shipment.carrier, escrow_amount);
            }

            let old_status = shipment.status.clone();
            shipment.status = ShipmentStatus::Delivered;
            shipment.updated_at = env.ledger().timestamp();
            shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);

            storage::decrement_status_count(env, &old_status);
            storage::increment_status_count(env, &shipment.status);
            storage::decrement_active_shipment_count(env, &shipment.sender);

            crate::finalize_if_settled(env, &mut shipment);
            crate::persist_shipment(env, &shipment)?;

            // Emit the dedicated force-release event with reason hash for audit trail
            events::emit_force_released(
                env,
                shipment_id,
                &proposal.proposer,
                &reason_hash,
                escrow_amount,
            );
        }
        AdminAction::ForceRefund(shipment_id, reason_hash) => {
            // Reason hash is mandatory and must be non-zero for audit trail.
            validation::validate_hash(&reason_hash)?;

            let mut shipment =
                storage::get_shipment(env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

            // Terminal shipments have already had their status/counters settled.
            if shipment.status == ShipmentStatus::Delivered
                || shipment.status == ShipmentStatus::Cancelled
            {
                return Err(NavinError::ShipmentAlreadyCompleted);
            }

            let escrow_amount = shipment.escrow_amount;
            if escrow_amount > 0 {
                // Get token contract address
                if let Some(token_contract) = storage::get_token_contract(env) {
                    // Transfer tokens from this contract to company
                    let contract_address = env.current_contract_address();
                    crate::invoke_token_transfer(
                        env,
                        &token_contract,
                        &contract_address,
                        &shipment.sender,
                        escrow_amount,
                    )?;
                }

                shipment.escrow_amount = 0;
                events::emit_escrow_refunded(env, shipment_id, &shipment.sender, escrow_amount);
            }

            let old_status = shipment.status.clone();
            shipment.status = ShipmentStatus::Cancelled;
            shipment.updated_at = env.ledger().timestamp();
            shipment.integration_nonce = shipment.integration_nonce.saturating_add(1);

            storage::decrement_status_count(env, &old_status);
            storage::increment_status_count(env, &shipment.status);
            storage::decrement_active_shipment_count(env, &shipment.sender);

            crate::finalize_if_settled(env, &mut shipment);
            crate::persist_shipment(env, &shipment)?;

            // Emit the dedicated force-refund event with reason hash for audit trail
            events::emit_force_refunded(
                env,
                shipment_id,
                &proposal.proposer,
                &reason_hash,
                escrow_amount,
            );
        }
    }

    events::emit_proposal_executed(env, proposal_id, &proposal.action);

    Ok(())
}
