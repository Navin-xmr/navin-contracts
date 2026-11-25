//! Persistent-storage helpers for dispute evidence hashes.

use crate::types::DisputeKey;
use soroban_sdk::{BytesN, Env};

/// Read the number of evidence entries recorded for a shipment's dispute.
pub fn get_evidence_count(env: &Env, shipment_id: u64) -> u32 {
    env.storage()
        .persistent()
        .get(&DisputeKey::EvidenceCount(shipment_id))
        .unwrap_or(0)
}

/// Append one evidence hash to a shipment's dispute and return its index.
///
/// Callers are responsible for enforcing the per-dispute cap before calling
/// this; the counter saturates rather than wrapping so a full slot never
/// silently overwrites index 0.
pub fn append_evidence(env: &Env, shipment_id: u64, evidence_hash: &BytesN<32>) -> u32 {
    let index = get_evidence_count(env, shipment_id);
    env.storage()
        .persistent()
        .set(&DisputeKey::Evidence(shipment_id, index), evidence_hash);
    env.storage().persistent().set(
        &DisputeKey::EvidenceCount(shipment_id),
        &index.saturating_add(1),
    );
    index
}

/// Read a single evidence hash by index.
pub fn get_evidence(env: &Env, shipment_id: u64, index: u32) -> Option<BytesN<32>> {
    env.storage()
        .persistent()
        .get(&DisputeKey::Evidence(shipment_id, index))
}
