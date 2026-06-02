//! # Cross-Shipment State Consistency Verification Framework
//!
//! Verifies consistency relationships between related shipments, including
//! batch-created groups. Detects anomalies that indicate state corruption
//! or logic bugs.
//!
//! ## Invariants Checked
//!
//! **Per-shipment invariants:**
//! - `EscrowMismatch`: `shipment.escrow_amount` matches the dedicated escrow storage entry.
//! - `InvalidFinalization`: `finalized == true` only when status is terminal AND escrow is zero.
//! - `MilestoneViolation`: every entry in `paid_milestones` must appear in `payment_milestones`.
//! - `TimestampAnomaly`: `updated_at >= created_at`.
//! - `DeadlineAnomaly`: `deadline > created_at`.
//!
//! **Batch (cross-shipment) invariants:**
//! - `BatchSenderMismatch`: all shipments in a batch must share the same `sender`.
//! - `BatchTimestampMismatch`: all shipments in a batch must share the same `created_at`.

use crate::{storage, types::ShipmentStatus};
use soroban_sdk::{contracttype, Address, Env, Vec};

/// A detected violation of a consistency invariant.
///
/// Each variant carries the `shipment_id` of the offending shipment so that
/// an admin can correlate the report back to storage for manual correction.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ConsistencyViolation {
    /// `shipment.escrow_amount` does not match the escrow storage entry.
    EscrowMismatch(u64),
    /// `finalized == true` but status is not terminal, or escrow balance is non-zero.
    InvalidFinalization(u64),
    /// A symbol in `paid_milestones` is not present in `payment_milestones`.
    MilestoneViolation(u64),
    /// `updated_at < created_at` — clock went backwards or fields were corrupted.
    TimestampAnomaly(u64),
    /// `deadline <= created_at` — deadline was set in the past relative to creation.
    DeadlineAnomaly(u64),
    /// Shipment ID is tracked by the counter but no storage entry exists.
    MissingShipment(u64),
    /// Batch member has a different `sender` than the first shipment in the batch.
    BatchSenderMismatch(u64),
    /// Batch member has a different `created_at` than the first shipment in the batch.
    BatchTimestampMismatch(u64),
}

/// Check all per-shipment invariants for a single shipment.
///
/// Returns a `Vec<ConsistencyViolation>`. An empty vec means the shipment is healthy.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `shipment_id` - ID of the shipment to verify.
pub fn check_shipment_invariants(env: &Env, shipment_id: u64) -> Vec<ConsistencyViolation> {
    let mut violations: Vec<ConsistencyViolation> = Vec::new(env);

    let shipment = match storage::get_shipment(env, shipment_id) {
        Some(s) => s,
        None => {
            violations.push_back(ConsistencyViolation::MissingShipment(shipment_id));
            return violations;
        }
    };

    // Escrow: struct field must match dedicated storage entry.
    let stored_escrow = storage::get_escrow(env, shipment_id);
    if shipment.escrow_amount != stored_escrow {
        violations.push_back(ConsistencyViolation::EscrowMismatch(shipment_id));
    }

    // Finalization: finalized shipments must be terminal with zero escrow.
    if shipment.finalized {
        let is_terminal = matches!(
            shipment.status,
            ShipmentStatus::Delivered | ShipmentStatus::Cancelled
        );
        if !is_terminal || shipment.escrow_amount != 0 {
            violations.push_back(ConsistencyViolation::InvalidFinalization(shipment_id));
        }
    }

    // Milestones: every paid entry must appear in the payment schedule.
    'outer: for paid in shipment.paid_milestones.iter() {
        for (name, _pct) in shipment.payment_milestones.iter() {
            if name == paid {
                continue 'outer;
            }
        }
        violations.push_back(ConsistencyViolation::MilestoneViolation(shipment_id));
        break;
    }

    // Timestamps: updated_at must not precede created_at.
    if shipment.updated_at < shipment.created_at {
        violations.push_back(ConsistencyViolation::TimestampAnomaly(shipment_id));
    }

    // Deadline: must be strictly after creation time.
    if shipment.deadline <= shipment.created_at {
        violations.push_back(ConsistencyViolation::DeadlineAnomaly(shipment_id));
    }

    violations
}

/// Check consistency for a set of related shipments (e.g., a batch group).
///
/// Runs all per-shipment invariants for every ID, then verifies the two
/// cross-shipment group invariants:
/// - Uniform `sender` across all members.
/// - Uniform `created_at` across all members (same batch transaction).
///
/// # Arguments
/// * `env` - Execution environment.
/// * `ids` - Ordered slice of shipment IDs that form the batch.
pub fn check_batch_consistency(env: &Env, ids: &Vec<u64>) -> Vec<ConsistencyViolation> {
    let mut violations: Vec<ConsistencyViolation> = Vec::new(env);

    if ids.is_empty() {
        return violations;
    }

    // Per-shipment checks.
    for id in ids.iter() {
        let per_ship = check_shipment_invariants(env, id);
        for v in per_ship.iter() {
            violations.push_back(v);
        }
    }

    // Cross-shipment group invariants: gather reference values from first member.
    let mut ref_sender: Option<Address> = None;
    let mut ref_created_at: Option<u64> = None;

    for id in ids.iter() {
        let shipment = match storage::get_shipment(env, id) {
            Some(s) => s,
            None => continue, // MissingShipment already recorded above.
        };

        match &ref_sender {
            None => ref_sender = Some(shipment.sender.clone()),
            Some(expected) if *expected != shipment.sender => {
                violations.push_back(ConsistencyViolation::BatchSenderMismatch(id));
            }
            _ => {}
        }

        match ref_created_at {
            None => ref_created_at = Some(shipment.created_at),
            Some(expected) if expected != shipment.created_at => {
                violations.push_back(ConsistencyViolation::BatchTimestampMismatch(id));
            }
            _ => {}
        }
    }

    violations
}

/// Scan all shipments (IDs 1 through the global counter) and return every
/// consistency violation found.
///
/// This is an O(n) scan and is intended only for admin/operator use, not for
/// hot-path contract logic.
///
/// # Arguments
/// * `env` - Execution environment.
pub fn check_all_consistency(env: &Env) -> Vec<ConsistencyViolation> {
    let total = storage::get_shipment_count(env);
    let mut violations: Vec<ConsistencyViolation> = Vec::new(env);

    for id in 1..=total {
        let per_ship = check_shipment_invariants(env, id);
        for v in per_ship.iter() {
            violations.push_back(v);
        }
    }

    violations
}
