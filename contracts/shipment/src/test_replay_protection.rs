//! # Replay Protection Tests — Issue #246
//!
//! Validates that privileged proposals enforce one-time execution semantics via
//! anti-replay salts and expiry windows.
//!
//! ## Acceptance criteria covered
//! - Replayed privileged proposals (same salt) are rejected with
//!   `NavinError::ProposalSaltReused (#48)`.
//! - Expired proposals cannot be approved or executed; both return
//!   `NavinError::ProposalExpired (#24)`.

#![cfg(test)]

extern crate std;

use crate::{NavinError, NavinShipment, NavinShipmentClient};
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Vec};

// ── Minimal no-op token ───────────────────────────────────────────────────────

#[contract]
struct ReplayMockToken;

#[contractimpl]
impl ReplayMockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

// ── Setup helpers ─────────────────────────────────────────────────────────────

/// Returns (env, client, admin) with mock_all_auths active.
fn setup() -> (Env, NavinShipmentClient<'static>, Address) {
    let (env, admin) = crate::test_utils::setup_env();
    let token = env.register(ReplayMockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token);
    (env, client, admin)
}

/// Initialise a 2-of-1 multisig: two admins registered, threshold=1 so the
/// proposer auto-executes (meets the min-admins=2 config constraint).
fn setup_multisig(env: &Env, client: &NavinShipmentClient, admin: &Address) {
    let mut admins: Vec<Address> = Vec::new(env);
    admins.push_back(admin.clone());
    admins.push_back(Address::generate(env));
    client.init_multisig(admin, &admins, &1);
}

fn salt(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn dummy_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[1u8; 32])
}

// ── Salt replay tests ─────────────────────────────────────────────────────────

/// test_auth_replay_salt_reuse_rejected — proposing with the same salt a second
/// time must return ProposalSaltReused even though the first proposal is still
/// pending.
#[test]
fn test_auth_replay_salt_reuse_rejected() {
    let (env, client, admin) = setup();
    setup_multisig(&env, &client, &admin);

    let s = salt(&env, 0xAA);
    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));

    // First proposal succeeds.
    client.propose_action(&admin, &action, &s);

    // Same salt on a second proposal must be rejected.
    let result = client.try_propose_action(&admin, &action, &s);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, NavinError::ProposalSaltReused);
}

/// test_auth_replay_distinct_salts_both_allowed — two proposals with distinct
/// salts must both succeed.
#[test]
fn test_auth_replay_distinct_salts_both_allowed() {
    let (env, client, admin) = setup();
    setup_multisig(&env, &client, &admin);

    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));

    let id1 = client.propose_action(&admin, &action, &salt(&env, 0x01));
    let id2 = client.propose_action(&admin, &action, &salt(&env, 0x02));

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
}

/// test_auth_replay_salt_reused_after_proposal_expires — a salt used in an
/// expired (but not executed) proposal is still permanently recorded; a new
/// proposal with the same salt must be rejected.
#[test]
fn test_auth_replay_salt_reused_after_proposal_expires() {
    let (env, client, admin) = setup();
    setup_multisig(&env, &client, &admin);

    let s = salt(&env, 0xBB);
    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));

    client.propose_action(&admin, &action, &s);

    // Advance past expiry.
    crate::test_utils::advance_past_multisig_expiry(&env);

    // Trying to reuse the salt must still return ProposalSaltReused, not succeed.
    let result = client.try_propose_action(&admin, &action, &s);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, NavinError::ProposalSaltReused);
}

/// test_auth_replay_salt_reused_after_execution — after a proposal is executed
/// the salt remains consumed; reuse must be rejected.
#[test]
fn test_auth_replay_salt_reused_after_execution() {
    let (env, client, admin) = setup();
    setup_multisig(&env, &client, &admin);

    let new_admin = Address::generate(&env);
    let action = crate::types::AdminAction::TransferAdmin(new_admin.clone());
    let s = salt(&env, 0xCC);

    // Propose (threshold = 1, so it auto-executes on propose/approve).
    let proposal_id = client.propose_action(&admin, &action, &s);
    // With threshold=1 and proposer auto-approved, execute manually.
    client.execute_proposal(&proposal_id);

    // Reuse the same salt — must be rejected even after execution.
    // (Proposal transferred admin, so new_admin is now admin; but salt is still locked.)
    let result = client.try_propose_action(&admin, &action, &s);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, NavinError::ProposalSaltReused);
}

/// test_auth_replay_salt_stored_in_proposal_record — the salt is persisted in
/// the Proposal struct and retrievable via get_proposal.
#[test]
fn test_auth_replay_salt_stored_in_proposal_record() {
    let (env, client, admin) = setup();
    setup_multisig(&env, &client, &admin);

    let s = salt(&env, 0xDD);
    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));

    let proposal_id = client.propose_action(&admin, &action, &s);
    let proposal = client.get_proposal(&proposal_id);

    assert_eq!(proposal.salt, s);
}

// ── Expiry tests ──────────────────────────────────────────────────────────────

/// test_auth_expiry_approve_after_deadline_rejected — approving a proposal
/// after its expiry timestamp must return ProposalExpired.
#[test]
fn test_auth_expiry_approve_after_deadline_rejected() {
    let (env, client, admin) = setup();

    // Two-admin multisig so the proposal needs a second approval.
    let admin2 = Address::generate(&env);
    let mut admins: Vec<Address> = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &2);

    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
    let proposal_id = client.propose_action(&admin, &action, &salt(&env, 0x01));

    // Advance past the 7-day proposal expiry.
    crate::test_utils::advance_past_multisig_expiry(&env);

    // Second admin tries to approve — must be rejected.
    let result = client.try_approve_action(&admin2, &proposal_id);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, NavinError::ProposalExpired);
}

/// test_auth_expiry_execute_after_deadline_rejected — calling execute_proposal
/// after expiry must return ProposalExpired.
#[test]
fn test_auth_expiry_execute_after_deadline_rejected() {
    let (env, client, admin) = setup();
    setup_multisig(&env, &client, &admin);

    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
    let proposal_id = client.propose_action(&admin, &action, &salt(&env, 0x01));

    // Advance past expiry before executing.
    crate::test_utils::advance_past_multisig_expiry(&env);

    let result = client.try_execute_proposal(&proposal_id);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, NavinError::ProposalExpired);
}

/// test_auth_expiry_execute_within_window_succeeds — executing within the
/// expiry window must succeed.
#[test]
fn test_auth_expiry_execute_within_window_succeeds() {
    let (env, client, admin) = setup();
    setup_multisig(&env, &client, &admin);

    let new_admin = Address::generate(&env);
    let action = crate::types::AdminAction::TransferAdmin(new_admin.clone());
    let proposal_id = client.propose_action(&admin, &action, &salt(&env, 0x01));

    // Advance only 1 hour — well within the 7-day window.
    crate::test_utils::advance_ledger_time(&env, 3_600);

    let result = client.try_execute_proposal(&proposal_id);
    assert!(result.is_ok());
}

/// test_auth_expiry_approve_within_window_succeeds — a second admin approving
/// within the window must succeed and auto-execute.
#[test]
fn test_auth_expiry_approve_within_window_succeeds() {
    let (env, client, admin) = setup();

    let admin2 = Address::generate(&env);
    let mut admins: Vec<Address> = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &2);

    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
    let proposal_id = client.propose_action(&admin, &action, &salt(&env, 0x01));

    // Advance 1 day — still within 7-day window.
    crate::test_utils::advance_ledger_time(&env, 86_400);

    let result = client.try_approve_action(&admin2, &proposal_id);
    assert!(result.is_ok());
}

/// test_auth_expiry_already_executed_cannot_replay — attempting to execute a
/// proposal a second time returns ProposalAlreadyExecuted.
#[test]
fn test_auth_expiry_already_executed_cannot_replay() {
    let (env, client, admin) = setup();
    setup_multisig(&env, &client, &admin);

    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
    let proposal_id = client.propose_action(&admin, &action, &salt(&env, 0x01));
    client.execute_proposal(&proposal_id);

    // Second execute call must be rejected.
    let result = client.try_execute_proposal(&proposal_id);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, NavinError::ProposalAlreadyExecuted);
}

/// test_auth_replay_force_release_salt_reuse_rejected — the salt constraint
/// applies regardless of the AdminAction variant.
#[test]
fn test_auth_replay_force_release_salt_reuse_rejected() {
    let (env, client, admin) = setup();
    setup_multisig(&env, &client, &admin);

    // Create a company, carrier, and shipment to make the action referentially
    // valid (the proposal itself only needs a valid salt, not a valid shipment
    // for the propose step — but we set one up for completeness).
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    let deadline = env.ledger().timestamp() + 86_400;
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &dummy_hash(&env),
        &Vec::new(&env),
        &deadline,
    );

    let action = crate::types::AdminAction::ForceRelease(shipment_id);
    let s = salt(&env, 0xEE);

    client.propose_action(&admin, &action, &s);

    let result = client.try_propose_action(&admin, &action, &s);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, NavinError::ProposalSaltReused);
}
