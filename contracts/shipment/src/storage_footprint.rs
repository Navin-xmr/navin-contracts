//! # Storage Footprint Reporting Utility
//!
//! Provides utilities for analyzing and reporting approximate storage footprint
//! by key family to guide optimization work.
//!
//! ## Storage Key Families
//!
//! - **Shipment Data**: `Shipment(u64)` - Core shipment records
//! - **Escrow Tracking**: `Escrow(u64)` - Escrow balance per shipment
//! - **Settlement Records**: `Settlement(u64)`, `ActiveSettlement(u64)` - Payment tracking
//! - **Milestone Data**: `MilestoneEventCount(u64)` - Milestone event counters
//! - **Notes & Evidence**: `ShipmentNote(u64, u32)`, `DisputeEvidence(u64, u32)` - Append-only data
//! - **Status Tracking**: `StatusHash(u64, ShipmentStatus)` - Status transition hashes
//! - **Configuration**: `ContractConfig`, `FeeConfig`, `CreationQuotaConfig` - Config data
//! - **Counters**: `ShipmentCount`, `SettlementCounter`, `AuditEntryCount` - Global counters
//! - **Archived Data**: `ArchivedShipment(u64)` - Temporary storage for completed shipments

use soroban_sdk::{contracttype, Env, Symbol, Vec};

/// Represents the estimated storage footprint for a key family.
#[contracttype]
#[derive(Clone, Debug)]
pub struct FootprintEntry {
    /// Name of the key family (e.g., "Shipment", "Escrow", "Settlement").
    pub key_family: Symbol,
    /// Estimated number of entries in this family.
    pub entry_count: u64,
    /// Estimated average size per entry in bytes.
    pub avg_size_bytes: u64,
    /// Total estimated footprint for this family (entry_count * avg_size_bytes).
    pub total_bytes: u64,
}

/// Summary of storage footprint across all key families.
#[contracttype]
#[derive(Clone, Debug)]
pub struct StorageFootprintReport {
    /// Timestamp when the report was generated.
    pub generated_at: u64,
    /// Total number of shipments in the contract.
    pub total_shipments: u64,
    /// Breakdown by key family.
    pub footprints: Vec<FootprintEntry>,
    /// Total estimated storage footprint in bytes.
    pub total_bytes: u64,
}

/// Generate a storage footprint report for the contract.
///
/// This function estimates storage usage by key family based on:
/// - Number of shipments (primary driver)
/// - Number of settlements per shipment
/// - Number of notes and evidence entries
/// - Configuration and counter entries
///
/// # Arguments
/// * `env` - Execution environment.
///
/// # Returns
/// * `StorageFootprintReport` - Detailed breakdown of storage usage.
///
/// # Examples
/// ```rust
/// // let report = storage_footprint::generate_report(&env);
/// // println!("Total storage: {} bytes", report.total_bytes);
/// ```
pub fn generate_report(env: &Env) -> StorageFootprintReport {
    let mut footprints: Vec<FootprintEntry> = Vec::new(env);
    let mut total_bytes: u64 = 0;

    let total_shipments = crate::storage::get_shipment_counter(env);

    // Shipment Data: ~500 bytes per shipment (includes metadata, milestones, etc.)
    let shipment_size = 500u64;
    let shipment_total = total_shipments.saturating_mul(shipment_size);
    footprints.push_back(FootprintEntry {
        key_family: Symbol::new(env, "Shipment"),
        entry_count: total_shipments,
        avg_size_bytes: shipment_size,
        total_bytes: shipment_total,
    });
    total_bytes = total_bytes.saturating_add(shipment_total);

    // Escrow Tracking: ~32 bytes per shipment (i128 + overhead)
    let escrow_size = 32u64;
    let escrow_total = total_shipments.saturating_mul(escrow_size);
    footprints.push_back(FootprintEntry {
        key_family: Symbol::new(env, "Escrow"),
        entry_count: total_shipments,
        avg_size_bytes: escrow_size,
        total_bytes: escrow_total,
    });
    total_bytes = total_bytes.saturating_add(escrow_total);

    // Settlement Records: ~200 bytes per settlement, assume 1-2 per shipment on average
    let settlement_counter = crate::storage::get_settlement_counter(env);
    let settlement_size = 200u64;
    let settlement_total = settlement_counter.saturating_mul(settlement_size);
    footprints.push_back(FootprintEntry {
        key_family: Symbol::new(env, "Settlement"),
        entry_count: settlement_counter,
        avg_size_bytes: settlement_size,
        total_bytes: settlement_total,
    });
    total_bytes = total_bytes.saturating_add(settlement_total);

    // Milestone Event Counters: ~16 bytes per shipment
    let milestone_counter_size = 16u64;
    let milestone_counter_total = total_shipments.saturating_mul(milestone_counter_size);
    footprints.push_back(FootprintEntry {
        key_family: Symbol::new(env, "MilestoneEventCount"),
        entry_count: total_shipments,
        avg_size_bytes: milestone_counter_size,
        total_bytes: milestone_counter_total,
    });
    total_bytes = total_bytes.saturating_add(milestone_counter_total);

    // Notes & Evidence: Assume average 5 notes and 2 evidence per shipment
    // Each entry ~256 bytes (hash + metadata)
    let notes_per_shipment = 5u64;
    let evidence_per_shipment = 2u64;
    let note_evidence_size = 256u64;
    let total_note_entries = total_shipments.saturating_mul(notes_per_shipment);
    let total_evidence_entries = total_shipments.saturating_mul(evidence_per_shipment);
    let note_evidence_total = (total_note_entries.saturating_add(total_evidence_entries))
        .saturating_mul(note_evidence_size);
    footprints.push_back(FootprintEntry {
        key_family: Symbol::new(env, "NotesAndEvidence"),
        entry_count: total_note_entries.saturating_add(total_evidence_entries),
        avg_size_bytes: note_evidence_size,
        total_bytes: note_evidence_total,
    });
    total_bytes = total_bytes.saturating_add(note_evidence_total);

    // Status Hashes: ~64 bytes per shipment (hash + status)
    let status_hash_size = 64u64;
    let status_hash_total = total_shipments.saturating_mul(status_hash_size);
    footprints.push_back(FootprintEntry {
        key_family: Symbol::new(env, "StatusHash"),
        entry_count: total_shipments,
        avg_size_bytes: status_hash_size,
        total_bytes: status_hash_total,
    });
    total_bytes = total_bytes.saturating_add(status_hash_total);

    // Configuration & Counters: ~1KB total (fixed overhead)
    let config_size = 1024u64;
    footprints.push_back(FootprintEntry {
        key_family: Symbol::new(env, "Configuration"),
        entry_count: 1,
        avg_size_bytes: config_size,
        total_bytes: config_size,
    });
    total_bytes = total_bytes.saturating_add(config_size);

    // Archived Shipments: Assume 10% of shipments are archived, ~400 bytes each
    let archived_count = total_shipments / 10;
    let archived_size = 400u64;
    let archived_total = archived_count.saturating_mul(archived_size);
    footprints.push_back(FootprintEntry {
        key_family: Symbol::new(env, "ArchivedShipment"),
        entry_count: archived_count,
        avg_size_bytes: archived_size,
        total_bytes: archived_total,
    });
    total_bytes = total_bytes.saturating_add(archived_total);

    StorageFootprintReport {
        generated_at: env.ledger().timestamp(),
        total_shipments,
        footprints,
        total_bytes,
    }
}

/// Identify high-footprint key families for optimization.
///
/// Returns a sorted list of key families by total footprint (descending).
/// Useful for identifying which areas consume the most storage.
///
/// # Arguments
/// * `report` - The storage footprint report.
///
/// # Returns
/// * `Vec<FootprintEntry>` - Key families sorted by total_bytes (descending).
pub fn identify_high_footprint_families(
    _env: &Env,
    report: &StorageFootprintReport,
) -> Vec<FootprintEntry> {
    let mut sorted = report.footprints.clone();

    // Simple bubble sort (acceptable for small number of families)
    let len = sorted.len();
    for i in 0..len {
        for j in 0..(len - i - 1) {
            let a = sorted.get(j).unwrap();
            let b = sorted.get(j + 1).unwrap();
            if a.total_bytes < b.total_bytes {
                // Swap
                let temp = a.clone();
                sorted.set(j, b.clone());
                sorted.set(j + 1, temp);
            }
        }
    }

    sorted
}

/// Calculate the percentage of total storage used by a key family.
///
/// # Arguments
/// * `entry` - The footprint entry.
/// * `total_bytes` - Total storage footprint.
///
/// # Returns
/// * `u32` - Percentage (0-100).
pub fn calculate_percentage(entry: &FootprintEntry, total_bytes: u64) -> u32 {
    if total_bytes == 0 {
        return 0;
    }
    ((entry.total_bytes as u128 * 100) / total_bytes as u128) as u32
}

#[cfg(test)]
mod tests {}
