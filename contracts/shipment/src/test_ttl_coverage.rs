//! Regression tests for persistent keys that previously never had their TTL
//! renewed:
//!
//! - #852: `MilestoneEventCount` / `BreachEventCount` payload-size guards.
//! - #853: `DeadlineWarningEmitted` one-time warning flag.
//! - #854: `Settlement` / `ActiveSettlement` records.
//! - #855: `Proposal` / `ProposalDigest` multi-sig entries.

use crate::test::*;
use crate::types::{DataKey, SettlementKey};
use crate::{storage, NavinError};
use soroban_sdk::{
    testutils::{storage::Persistent, Address as _, Ledger},
    Address, BytesN, Env, Vec,
};

const THRESHOLD: u32 = 518_000;
const EXTEND_TO: u32 = 518_400;

/// Advance the ledger sequence (and timestamp at ~5s per ledger).
fn advance_ledgers(env: &Env, ledgers: u32) {
    env.ledger().with_mut(|l| {
        l.sequence_number += ledgers;
        l.timestamp += u64::from(ledgers) * 5;
    });
}

/// Ledgers past which a freshly written persistent entry with the minimum
/// TTL would have expired.
fn past_min_persistent_ttl(env: &Env) -> u32 {
    env.ledger().get().min_persistent_entry_ttl + 10
}

fn create_basic_shipment(env: &Env, client: &crate::NavinShipmentClient, admin: &Address) -> u64 {
    let company = Address::generate(env);
    let carrier = Address::generate(env);
    let receiver = Address::generate(env);

    client.add_company(admin, &company);
    client.add_carrier(admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = BytesN::from_array(env, &[7u8; 32]);
    let deadline = env.ledger().timestamp() + 30 * 86_400;
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(env),
        &deadline,
    )
}

// ── #852: milestone / breach counters ───────────────────────────────────────

#[test]
fn test_extend_shipment_ttl_renews_event_counters() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();
    let shipment_id = 1u64;

    env.as_contract(&client.address, || {
        storage::increment_milestone_event_count(&env, shipment_id);
        storage::increment_milestone_event_count(&env, shipment_id);
        storage::increment_breach_event_count(&env, shipment_id);

        storage::extend_shipment_ttl(&env, shipment_id, THRESHOLD, EXTEND_TO);

        let persistent = env.storage().persistent();
        assert!(persistent.get_ttl(&DataKey::MilestoneEventCount(shipment_id)) >= EXTEND_TO);
        assert!(persistent.get_ttl(&DataKey::BreachEventCount(shipment_id)) >= EXTEND_TO);
    });
}

#[test]
fn test_event_counter_caps_survive_original_ttl_window() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();
    let shipment_id = 1u64;

    let cfg = client.get_contract_config();
    env.as_contract(&client.address, || {
        for _ in 0..cfg.max_milestones_per_shipment {
            storage::increment_milestone_event_count(&env, shipment_id);
        }
        for _ in 0..cfg.max_breaches_per_shipment {
            storage::increment_breach_event_count(&env, shipment_id);
        }
        storage::extend_shipment_ttl(&env, shipment_id, THRESHOLD, EXTEND_TO);
    });

    // Move past the TTL the counters were originally written with.
    advance_ledgers(&env, past_min_persistent_ttl(&env));

    env.as_contract(&client.address, || {
        // Counters are still at the cap, so the guards keep rejecting.
        assert!(
            storage::get_milestone_event_count(&env, shipment_id)
                >= cfg.max_milestones_per_shipment
        );
        assert!(
            storage::get_breach_event_count(&env, shipment_id) >= cfg.max_breaches_per_shipment
        );
    });
}

// ── #853: deadline warning flag ─────────────────────────────────────────────

#[test]
fn test_extend_shipment_ttl_renews_deadline_warning_flag() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();
    let shipment_id = 1u64;

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::DeadlineWarningEmitted(shipment_id), &true);

        storage::extend_shipment_ttl(&env, shipment_id, THRESHOLD, EXTEND_TO);

        assert!(
            env.storage()
                .persistent()
                .get_ttl(&DataKey::DeadlineWarningEmitted(shipment_id))
                >= EXTEND_TO
        );
    });
}

#[test]
fn test_deadline_warning_stays_noop_after_flag_original_ttl() {
    let (env, client, admin, _token) = setup_initialized_shipment_env();
    let shipment_id = create_basic_shipment(&env, &client, &admin);
    let shipment = client.get_shipment(&shipment_id);
    let data_hash = BytesN::from_array(&env, &[9u8; 32]);

    // Enter the grace window before the deadline and emit the warning.
    let grace = client.get_contract_config().deadline_grace_seconds;
    env.ledger()
        .set_timestamp(shipment.deadline.saturating_sub(grace));
    client.check_deadline_warning(&shipment_id, &data_hash);

    env.as_contract(&client.address, || {
        assert!(env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&DataKey::DeadlineWarningEmitted(shipment_id))
            .unwrap_or(false));
    });

    // Renew the shipment, then move past the flag's original TTL while
    // staying inside the grace window.
    client.extend_shipment_ttl(&shipment_id);
    env.ledger().with_mut(|l| {
        l.sequence_number += past_min_persistent_ttl(&env);
    });

    env.as_contract(&client.address, || {
        assert!(
            env.storage()
                .persistent()
                .get::<DataKey, bool>(&DataKey::DeadlineWarningEmitted(shipment_id))
                .unwrap_or(false),
            "warning flag must outlive its original TTL"
        );
    });

    // Still a no-op: the flag is present so nothing is re-emitted.
    client.check_deadline_warning(&shipment_id, &data_hash);
}

// ── #854: settlement records ────────────────────────────────────────────────

#[test]
fn test_extend_shipment_ttl_renews_settlement_records() {
    let (env, client, admin, _token) = setup_initialized_shipment_env();
    let shipment_id = create_basic_shipment(&env, &client, &admin);
    let shipment = client.get_shipment(&shipment_id);

    client.deposit_escrow(&shipment.sender, &shipment_id, &1_000);

    let settlement_count = client.get_settlement_count();
    assert!(settlement_count > 0);

    env.as_contract(&client.address, || {
        let latest = storage::get_latest_settlement(&env, shipment_id)
            .expect("latest settlement must be recorded");
        assert_eq!(latest, settlement_count);
    });

    client.extend_shipment_ttl(&shipment_id);

    env.as_contract(&client.address, || {
        let persistent = env.storage().persistent();
        let latest = storage::get_latest_settlement(&env, shipment_id).unwrap();
        assert!(persistent.get_ttl(&DataKey::Settlement(latest)) >= EXTEND_TO);
        assert!(persistent.get_ttl(&SettlementKey::Latest(shipment_id)) >= EXTEND_TO);
        if let Some(active) = storage::get_active_settlement(&env, shipment_id) {
            assert!(persistent.get_ttl(&DataKey::ActiveSettlement(shipment_id)) >= EXTEND_TO);
            assert!(persistent.get_ttl(&DataKey::Settlement(active)) >= EXTEND_TO);
        }
    });

    advance_ledgers(&env, past_min_persistent_ttl(&env));

    let settlement = client.get_settlement(&settlement_count);
    assert_eq!(settlement.shipment_id, shipment_id);
}

#[test]
fn test_extend_shipment_ttl_renews_active_settlement() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();
    let shipment_id = 42u64;

    env.as_contract(&client.address, || {
        let from = Address::generate(&env);
        let to = Address::generate(&env);
        let record = crate::types::SettlementRecord {
            settlement_id: 7,
            shipment_id,
            operation: crate::types::SettlementOperation::Deposit,
            state: crate::types::SettlementState::Pending,
            amount: 100,
            from,
            to,
            initiated_at: env.ledger().timestamp(),
            completed_at: None,
            error_code: None,
        };
        storage::set_settlement(&env, &record);
        storage::set_active_settlement(&env, shipment_id, 7);

        storage::extend_shipment_ttl(&env, shipment_id, THRESHOLD, EXTEND_TO);

        let persistent = env.storage().persistent();
        assert!(persistent.get_ttl(&DataKey::ActiveSettlement(shipment_id)) >= EXTEND_TO);
        assert!(persistent.get_ttl(&DataKey::Settlement(7)) >= EXTEND_TO);
    });
}

// ── #855: proposals and digests ─────────────────────────────────────────────

fn setup_multisig(
    expiry_seconds: u64,
) -> (Env, crate::NavinShipmentClient<'static>, Address, Address) {
    let (env, client, admin, _token) = setup_initialized_shipment_env();
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3);
    client.init_multisig(&admin, &admins, &3);

    let mut cfg = client.get_contract_config();
    cfg.proposal_expiry_seconds = expiry_seconds;
    client.update_config(&admin, &cfg);

    (env, client, admin, admin2)
}

fn assert_proposal_ttl_covers_expiry(
    env: &Env,
    client: &crate::NavinShipmentClient,
    proposal_id: u64,
) {
    let proposal = client.get_proposal(&proposal_id);
    let remaining_ledgers =
        u32::try_from((proposal.expires_at - env.ledger().timestamp()) / 5).unwrap();
    env.as_contract(&client.address, || {
        let persistent = env.storage().persistent();
        assert!(persistent.get_ttl(&DataKey::Proposal(proposal_id)) > remaining_ledgers);
        assert!(persistent.get_ttl(&DataKey::ProposalDigest(proposal_id)) > remaining_ledgers);
    });
}

#[test]
fn test_proposal_ttl_covers_default_expiry() {
    let (env, client, admin, _admin2) = setup_multisig(604_800);
    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
    let proposal_id = client.propose_action(&admin, &action);

    assert_proposal_ttl_covers_expiry(&env, &client, proposal_id);
}

#[test]
fn test_proposal_readable_near_max_expiry() {
    let expiry = 2_592_000u64; // 30 days, the maximum allowed.
    let (env, client, admin, admin2) = setup_multisig(expiry);
    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
    let proposal_id = client.propose_action(&admin, &action);

    assert_proposal_ttl_covers_expiry(&env, &client, proposal_id);

    // Advance close to, but not past, the proposal's expiry.
    advance_ledgers(&env, u32::try_from(expiry / 5).unwrap() - 100);

    let proposal = client.get_proposal(&proposal_id);
    assert!(env.ledger().timestamp() <= proposal.expires_at);
    let digest = client.get_proposal_action_digest(&proposal_id);
    assert_eq!(digest.proposal_id, proposal_id);

    // Approving still works rather than surfacing `ProposalNotFound`.
    client.approve_action(&admin2, &proposal_id);
    assert_proposal_ttl_covers_expiry(&env, &client, proposal_id);
}

#[test]
fn test_expired_proposal_reports_expired_not_missing() {
    let expiry = 2_592_000u64;
    let (env, client, admin, admin2) = setup_multisig(expiry);
    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
    let proposal_id = client.propose_action(&admin, &action);

    // Just past expiry, but within the retention margin.
    advance_ledgers(&env, u32::try_from(expiry / 5).unwrap() + 10);

    let result = client.try_approve_action(&admin2, &proposal_id);
    assert_eq!(result, Err(Ok(NavinError::ProposalExpired)));
}
