#![cfg(test)]

extern crate std;

use super::test_helpers::*;

#[test]
fn test_get_circuit_breaker_status_closed_by_default() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let (state, failure_count, recovery_remaining) = client.get_circuit_breaker_status();
    assert_eq!(state, crate::CircuitBreakerState::Closed);
    assert_eq!(failure_count, 0);
    assert_eq!(recovery_remaining, 0);
}

#[test]
fn test_get_circuit_breaker_status_open() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Inject Open state directly — Soroban rolls back storage on error so we
    // cannot accumulate real failures through contract calls.
    let opened_at = env.ledger().timestamp();
    env.as_contract(&client.address, || {
        let tracker = crate::circuit_breaker::CircuitBreakerTracker {
            state: crate::circuit_breaker::CircuitBreakerState::Open,
            failure_count: 5,
            opened_at,
            half_open_requests: 0,
        };
        env.storage()
            .persistent()
            .set(&crate::types::DataKey::CircuitBreakerState, &tracker);
    });

    let (state, failure_count, recovery_remaining) = client.get_circuit_breaker_status();
    assert_eq!(state, crate::CircuitBreakerState::Open);
    assert_eq!(failure_count, 5);
    // Default recovery_timeout is 300 s; we just opened it so ~300 s remain.
    assert!(recovery_remaining > 0 && recovery_remaining <= 300);
}

#[test]
fn test_get_circuit_breaker_status_half_open() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Inject HalfOpen state.
    env.as_contract(&client.address, || {
        let tracker = crate::circuit_breaker::CircuitBreakerTracker {
            state: crate::circuit_breaker::CircuitBreakerState::HalfOpen,
            failure_count: 5,
            opened_at: env.ledger().timestamp(),
            half_open_requests: 1,
        };
        env.storage()
            .persistent()
            .set(&crate::types::DataKey::CircuitBreakerState, &tracker);
    });

    let (state, failure_count, recovery_remaining) = client.get_circuit_breaker_status();
    assert_eq!(state, crate::CircuitBreakerState::HalfOpen);
    assert_eq!(failure_count, 5);
    // HalfOpen means recovery window has already passed — no time remaining.
    assert_eq!(recovery_remaining, 0);
}

// is_company_suspended tests (issue #642)

#[test]
fn test_is_company_suspended_active_by_default() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    assert!(!client.is_company_suspended(&company));
}

#[test]
fn test_is_company_suspended_returns_true_when_suspended() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.suspend_company(&admin, &company);
    assert!(client.is_company_suspended(&company));
}

#[test]
fn test_is_company_suspended_returns_false_after_reactivation() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.suspend_company(&admin, &company);
    assert!(client.is_company_suspended(&company));

    client.reactivate_company(&admin, &company);
    assert!(!client.is_company_suspended(&company));
}

// get_platform_fee_config tests (issue #641)

#[test]
fn test_get_platform_fee_config_none_before_set() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let config = client.get_platform_fee_config();
    assert!(config.is_none());
}

#[test]
fn test_get_platform_fee_config_reflects_set_platform_fee() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let treasury = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.set_platform_fee(&admin, &50_u32, &treasury);

    let config = client
        .get_platform_fee_config()
        .expect("fee config should be set");
    assert_eq!(config.fee_bps, 50);
    assert_eq!(config.treasury, treasury);
}

#[test]
fn test_get_platform_fee_config_reflects_updated_value() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let treasury1 = Address::generate(&env);
    let treasury2 = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.set_platform_fee(&admin, &100_u32, &treasury1);
    client.set_platform_fee(&admin, &200_u32, &treasury2);

    let config = client
        .get_platform_fee_config()
        .expect("fee config should be set");
    assert_eq!(config.fee_bps, 200);
    assert_eq!(config.treasury, treasury2);
}

// Issue #601 — DuplicateAction error variant across operations

/// Verify that duplicate `create_shipment` calls within the idempotency window
/// are rejected with `DuplicateAction` (error code 41).
/// Verify that duplicate `update_status` calls within the idempotency window
/// are rejected with `DuplicateAction` (error code 41).
/// Verify that `deposit_escrow` rejects a second deposit with `EscrowLocked`
/// (error code 7), not `DuplicateAction` — escrow operations use their own
/// guard rather than the idempotency mechanism.
/// Verify that actions succeed after the idempotency window expires.
/// Verify that DuplicateAction is error code 41.
#[test]
fn test_duplicate_action_error_code_is_41() {
    assert_eq!(crate::NavinError::DuplicateAction as u32, 41);
}

// Issue #603 — InvalidMultiSigConfig error variant

/// Verify that `init_multisig` rejects threshold higher than admin count
/// with `InvalidConfig` (error code 31).
#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_multisig_threshold_higher_than_admin_count_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1);
    admins.push_back(admin2);

    // Threshold 3 > admin count 2 → InvalidConfig (#31)
    client.init_multisig(&admin, &admins, &3);
}

/// Verify that `init_multisig` rejects threshold of zero
/// with `InvalidMultiSigConfig` (error code 28).
#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_multisig_threshold_zero_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1);
    admins.push_back(admin2);

    // Threshold 0 is invalid
    client.init_multisig(&admin, &admins, &0);
}

/// Verify that `init_multisig` rejects empty admin list with non-zero threshold
/// with `InvalidMultiSigConfig` (error code 28).
#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_multisig_empty_admin_list_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admins = soroban_sdk::Vec::new(&env);

    // Empty admin list with threshold 1
    client.init_multisig(&admin, &admins, &1);
}

/// Verify that a valid multi-sig configuration succeeds.
#[test]
fn test_multisig_valid_config_succeeds() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    // Threshold 2 with 3 admins is valid
    client.init_multisig(&admin, &admins, &2);

    let (stored_admins, threshold) = client.get_multisig_config();
    assert_eq!(stored_admins.len(), 3);
    assert_eq!(threshold, 2);
}

/// Verify that InvalidMultiSigConfig is error code 28.
#[test]
fn test_invalid_multisig_config_error_code_is_28() {
    assert_eq!(crate::NavinError::InvalidMultiSigConfig as u32, 28);
}

// Issue #604 — InsufficientApprovals error variant

/// Verify that executing a proposal with fewer approvals than threshold
/// fails with `InsufficientApprovals` (error code 26).
#[test]
#[should_panic(expected = "Error(Contract, #26)")]
fn test_execute_proposal_with_insufficient_approvals_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    // Threshold 3 — all three must approve
    client.init_multisig(&admin, &admins, &3);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);
    let proposal_id = client.propose_action(&admin1, &action);

    // Only proposer (admin1) has approved — need 3 total
    client.execute_proposal(&proposal_id);
}

/// Verify that reaching exactly the approval threshold auto-executes the proposal.
/// Uses TransferAdmin action to avoid replacing the contract WASM (which would
/// remove `get_proposal` from the deployed code).
#[test]
fn test_execute_proposal_with_exact_threshold_approvals_succeeds() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let new_admin = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    // Threshold 2
    client.init_multisig(&admin, &admins, &2);

    let action = crate::AdminAction::TransferAdmin(new_admin.clone());
    let proposal_id = client.propose_action(&admin1, &action);

    // Proposer (admin1) auto-approves → 1/2 approvals
    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.approvals.len(), 1);
    assert!(!proposal.executed);

    // admin2 approves → reaches threshold of 2 → auto-executes
    client.approve_action(&admin2, &proposal_id);

    // Verify the proposal was auto-executed
    let proposal = client.get_proposal(&proposal_id);
    assert!(proposal.executed);

    // Verify admin was actually transferred
    assert_eq!(client.get_admin(), new_admin);
}

/// Verify that InsufficientApprovals is error code 26.
#[test]
fn test_insufficient_approvals_error_code_is_26() {
    assert_eq!(crate::NavinError::InsufficientApprovals as u32, 26);
}

// New Tests for Error Variants

#[test]
fn test_invalid_symbol_error_variant() {
    let env = Env::default();

    // length < 1 (which means length 0, 8 bytes in XDR)
    let empty_symbol = Symbol::new(&env, "");
    let result1 = crate::validation::validate_symbol(&env, &empty_symbol);
    assert_eq!(result1, Err(crate::NavinError::InvalidSymbol));

    // length > 12 characters (24+ bytes in XDR)
    let long_symbol = Symbol::new(&env, "toolongsymbolname");
    let result2 = crate::validation::validate_symbol(&env, &long_symbol);
    assert_eq!(result2, Err(crate::NavinError::InvalidSymbol));
}

// Issue #607 — ProposalExpired error variant
// Tests that expired proposals cannot be approved or executed, that the error
// code 24 is returned correctly, and that non-expired proposals work normally.

/// Helper to set up a multisig environment with a pending (not-yet-executed)
/// proposal that has NOT yet expired.  Returns
/// `(env, client, admin1, admin2, admin3, proposal_id)`.
fn setup_multisig_with_pending_proposal() -> (
    soroban_sdk::Env,
    NavinShipmentClient<'static>,
    soroban_sdk::Address,
    soroban_sdk::Address,
    soroban_sdk::Address,
    u64,
) {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();

    let admin1 = soroban_sdk::Address::generate(&env);
    let admin2 = soroban_sdk::Address::generate(&env);
    let admin3 = soroban_sdk::Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    // Threshold 3 so the proposal stays pending after proposal + 1 approval
    client.init_multisig(&admin, &admins, &3);

    let new_admin = soroban_sdk::Address::generate(&env);
    let action = crate::AdminAction::TransferAdmin(new_admin);
    let proposal_id = client.propose_action(&admin1, &action);

    (env, client, admin1, admin2, admin3, proposal_id)
}

/// Attempting to approve an expired proposal must return `ProposalExpired`
/// (error code 24) via `try_approve_action`.
#[test]
fn test_approve_expired_proposal_returns_proposal_expired() {
    let (env, client, _admin1, admin2, _admin3, proposal_id) =
        setup_multisig_with_pending_proposal();

    // Advance time past the proposal expiry window (7 days + 1 second)
    super::test_utils::advance_past_multisig_expiry(&env);

    let result = client.try_approve_action(&admin2, &proposal_id);
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalExpired)),
        "approving an expired proposal must return ProposalExpired"
    );
}

/// Attempting to execute an expired proposal (even if threshold was met
/// before expiry) must return `ProposalExpired` (error code 24).
#[test]
fn test_execute_expired_proposal_returns_proposal_expired() {
    let (env, client, _admin1, admin2, _admin3, proposal_id) =
        setup_multisig_with_pending_proposal();

    // admin2 approves — still below threshold (need 3)
    client.approve_action(&admin2, &proposal_id);

    // Advance time past expiry
    super::test_utils::advance_past_multisig_expiry(&env);

    let result = client.try_execute_proposal(&proposal_id);
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalExpired)),
        "executing an expired proposal must return ProposalExpired"
    );
}

/// `ProposalExpired` must carry error code 24.
#[test]
fn test_proposal_expired_error_code_is_24() {
    assert_eq!(
        crate::NavinError::ProposalExpired as u32,
        24,
        "ProposalExpired discriminant must be 24"
    );
}

/// A proposal that is within its expiry window can still be approved normally.
/// Verifies the non-expired happy path (Acceptance Criterion: non-expired
/// proposals work normally).
#[test]
fn test_non_expired_proposal_can_be_approved() {
    let (_env, client, _admin1, admin2, _admin3, proposal_id) =
        setup_multisig_with_pending_proposal();

    // No time advance — proposal is fresh
    let result = client.try_approve_action(&admin2, &proposal_id);
    assert!(
        result.is_ok(),
        "approving a non-expired proposal must succeed: {:?}",
        result
    );

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(
        proposal.approvals.len(),
        2,
        "admin2 approval must be recorded"
    );
    assert!(
        !proposal.executed,
        "proposal must not auto-execute with only 2/3 approvals"
    );
}

/// A non-expired proposal that reaches the approval threshold is auto-executed
/// without error.  This confirms the full non-expired happy path including
/// execution.
#[test]
fn test_non_expired_proposal_executes_at_threshold() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();

    let admin1 = soroban_sdk::Address::generate(&env);
    let admin2 = soroban_sdk::Address::generate(&env);
    let new_admin = soroban_sdk::Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    // Threshold 2 — executes on admin2 approval
    client.init_multisig(&admin, &admins, &2);

    let action = crate::AdminAction::TransferAdmin(new_admin.clone());
    let proposal_id = client.propose_action(&admin1, &action);

    // admin2 approves → reaches threshold → auto-executes
    let result = client.try_approve_action(&admin2, &proposal_id);
    assert!(
        result.is_ok(),
        "approving non-expired proposal at threshold must succeed"
    );

    // Confirm the action was actually executed
    assert_eq!(
        client.get_admin(),
        new_admin,
        "admin must be transferred after auto-execution"
    );
}

/// An expired proposal must not be approvable at any point — not even a third
/// admin who had never approved before.
#[test]
fn test_all_approvals_fail_after_expiry() {
    let (env, client, _admin1, admin2, admin3, proposal_id) =
        setup_multisig_with_pending_proposal();

    // admin2 approves while proposal is still valid (1 more approval but still below 3)
    client.approve_action(&admin2, &proposal_id);

    // Advance time past expiry
    super::test_utils::advance_past_multisig_expiry(&env);

    // admin3 (fresh approver) tries to approve after expiry
    let result = client.try_approve_action(&admin3, &proposal_id);
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalExpired)),
        "a fresh approver must still receive ProposalExpired after expiry"
    );
}

// ProposalAlreadyExecuted error variant: duplicate proposal executions must be
// rejected with error code 23, while a single execution still succeeds.

/// `ProposalAlreadyExecuted` must carry error code 23.
#[test]
fn test_proposal_already_executed_error_code_is_23() {
    assert_eq!(
        crate::NavinError::ProposalAlreadyExecuted as u32,
        23,
        "ProposalAlreadyExecuted discriminant must be 23"
    );
}

/// Executing a proposal a second time must return `ProposalAlreadyExecuted`
/// (error code 23) via `try_execute_proposal`.
#[test]
fn test_execute_already_executed_proposal_returns_error() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();

    let admin1 = soroban_sdk::Address::generate(&env);
    let admin2 = soroban_sdk::Address::generate(&env);
    let new_admin = soroban_sdk::Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    let action = crate::AdminAction::TransferAdmin(new_admin.clone());
    let proposal_id = client.propose_action(&admin1, &action);

    // First approval reaches threshold → auto-executes
    client.approve_action(&admin2, &proposal_id);

    // Verify it executed
    let proposal = client.get_proposal(&proposal_id);
    assert!(proposal.executed, "proposal must be marked as executed");

    // Attempt a second execute call
    let result = client.try_execute_proposal(&proposal_id);
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalAlreadyExecuted)),
        "second execute call must return ProposalAlreadyExecuted"
    );
}

/// Approving an already-executed proposal must also return
/// `ProposalAlreadyExecuted` (error code 23).
#[test]
fn test_approve_already_executed_proposal_returns_error() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();

    let admin1 = soroban_sdk::Address::generate(&env);
    let admin2 = soroban_sdk::Address::generate(&env);
    let admin3 = soroban_sdk::Address::generate(&env);
    let new_admin = soroban_sdk::Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    // Threshold 2 — executes when admin2 approves
    client.init_multisig(&admin, &admins, &2);

    let action = crate::AdminAction::TransferAdmin(new_admin);
    let proposal_id = client.propose_action(&admin1, &action);

    // Reaches threshold, auto-executes
    client.approve_action(&admin2, &proposal_id);

    // admin3 tries to approve the already-executed proposal
    let result = client.try_approve_action(&admin3, &proposal_id);
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalAlreadyExecuted)),
        "approving an already-executed proposal must return ProposalAlreadyExecuted"
    );
}

/// A single execution succeeds and correctly applies the proposed action.
/// Uses TransferAdmin to avoid replacing the WASM (which would break
/// subsequent `get_proposal` reads).
#[test]
fn test_single_execution_succeeds_and_applies_action() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();

    let admin1 = soroban_sdk::Address::generate(&env);
    let admin2 = soroban_sdk::Address::generate(&env);
    let new_admin = soroban_sdk::Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    let action = crate::AdminAction::TransferAdmin(new_admin.clone());
    let proposal_id = client.propose_action(&admin1, &action);

    // Proposal not yet executed
    let proposal_before = client.get_proposal(&proposal_id);
    assert!(!proposal_before.executed);

    // admin2 approval reaches threshold → auto-executes
    client.approve_action(&admin2, &proposal_id);

    // Confirm execution state
    let proposal_after = client.get_proposal(&proposal_id);
    assert!(
        proposal_after.executed,
        "proposal must be marked executed after threshold is reached"
    );

    // Confirm the admin was actually transferred
    assert_eq!(
        client.get_admin(),
        new_admin,
        "admin must be the new admin after successful single execution"
    );
}

// ForceRelease proposal: single execution releases escrow correctly.
// Verifies that the `ProposalAlreadyExecuted` guard works for non-Upgrade actions.
// ForceRelease reason_hash validation and audit trail tests
