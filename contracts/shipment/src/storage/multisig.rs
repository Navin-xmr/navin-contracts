//! Persistent-storage helpers for multi-signature proposals, admin list,
//! threshold, proposal digests and proposal salts.

use crate::types::{DataKey, ProposalActionDigest};
use soroban_sdk::{Address, BytesN, Env, Vec};

/// Approximate ledger close time used to convert seconds into ledgers.
const SECONDS_PER_LEDGER: u64 = 5;

/// Extra ledgers kept beyond a proposal's expiry so the entry is still readable
/// right after expiry.
const PROPOSAL_TTL_MARGIN_LEDGERS: u32 = 17_280; // ~1 day

/// Ledgers a proposal-scoped entry must live so it outlives `expires_at`.
pub(super) fn proposal_ttl_ledgers(env: &Env, expires_at: u64) -> u32 {
    let remaining_secs = expires_at.saturating_sub(env.ledger().timestamp());
    let remaining_ledgers = remaining_secs.div_ceil(SECONDS_PER_LEDGER);
    u32::try_from(remaining_ledgers)
        .unwrap_or(u32::MAX)
        .saturating_add(PROPOSAL_TTL_MARGIN_LEDGERS)
        .min(env.storage().max_ttl())
}

// ── Admin list and threshold ──────────────────────────────────────────────────

/// Get the list of admin addresses for multi-sig.
pub fn get_admin_list(env: &Env) -> Option<Vec<Address>> {
    env.storage().instance().get(&DataKey::AdminList)
}

/// Set the list of admin addresses for multi-sig.
pub fn set_admin_list(env: &Env, admins: &Vec<Address>) {
    env.storage().instance().set(&DataKey::AdminList, admins);
}

/// Get the multi-sig threshold.
pub fn get_multisig_threshold(env: &Env) -> Option<u32> {
    env.storage().instance().get(&DataKey::MultiSigThreshold)
}

/// Set the multi-sig threshold.
pub fn set_multisig_threshold(env: &Env, threshold: u32) {
    env.storage()
        .instance()
        .set(&DataKey::MultiSigThreshold, &threshold);
}

/// Check if an address is in the admin list.
pub fn is_admin(env: &Env, address: &Address) -> bool {
    if let Some(admins) = get_admin_list(env) {
        for admin in admins.iter() {
            if admin == *address {
                return true;
            }
        }
    }
    false
}

// ── Proposals ─────────────────────────────────────────────────────────────────

/// Get the current proposal counter.
pub fn get_proposal_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::ProposalCounter)
        .unwrap_or(0)
}

/// Set the proposal counter.
pub fn set_proposal_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&DataKey::ProposalCounter, &counter);
}

/// Retrieve a proposal from persistent storage.
pub fn get_proposal(env: &Env, proposal_id: u64) -> Option<crate::types::Proposal> {
    env.storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id))
}

/// Persist a proposal to persistent storage.
pub fn set_proposal(env: &Env, proposal: &crate::types::Proposal) {
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(proposal.id), proposal);
}

/// Extend the TTL of a proposal and its action digest so both outlive `expires_at`.
pub fn extend_proposal_ttl(env: &Env, proposal_id: u64, expires_at: u64) {
    let extend_to = proposal_ttl_ledgers(env, expires_at);

    env.storage().instance().extend_ttl(extend_to, extend_to);

    let proposal_key = DataKey::Proposal(proposal_id);
    if env.storage().persistent().has(&proposal_key) {
        env.storage()
            .persistent()
            .extend_ttl(&proposal_key, extend_to, extend_to);
    }

    let digest_key = DataKey::ProposalDigest(proposal_id);
    if env.storage().persistent().has(&digest_key) {
        env.storage()
            .persistent()
            .extend_ttl(&digest_key, extend_to, extend_to);
    }
}

// ── Proposal digest ───────────────────────────────────────────────────────────

/// Store the action digest for a proposal.
pub fn set_proposal_digest(env: &Env, proposal_id: u64, digest: &ProposalActionDigest) {
    env.storage()
        .persistent()
        .set(&DataKey::ProposalDigest(proposal_id), digest);
}

/// Retrieve the action digest for a proposal.
pub fn get_proposal_digest(env: &Env, proposal_id: u64) -> Option<ProposalActionDigest> {
    env.storage()
        .persistent()
        .get(&DataKey::ProposalDigest(proposal_id))
}

// ── Proposal salt ─────────────────────────────────────────────────────────────

/// Check if a proposal salt has been used.
pub fn is_proposal_salt_used(env: &Env, salt: &BytesN<32>) -> bool {
    let key = DataKey::ProposalSalt(salt.clone());
    env.storage().persistent().has(&key) || env.storage().instance().has(&key)
}

/// Mark a proposal salt as used by `proposal_id`.
pub fn set_proposal_salt_used(env: &Env, salt: &BytesN<32>, proposal_id: u64, expires_at: u64) {
    let key = DataKey::ProposalSalt(salt.clone());
    env.storage().persistent().set(&key, &proposal_id);
    let extend_to = proposal_ttl_ledgers(env, expires_at);
    env.storage()
        .persistent()
        .extend_ttl(&key, extend_to, extend_to);
}

// ── Settlement proof minted marker ────────────────────────────────────────────

// NOTE: The settlement proof minted marker is accessed directly in lib.rs via
// SettlementProofKey::Minted — no storage helper is needed here.
