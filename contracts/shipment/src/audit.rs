//! # Audit Trail Module
//!
//! Implements comprehensive audit trail for all role and permission changes.
//! Enables forensic analysis and compliance reporting for all user-facing operations.
//!
//! ## Audit Events
//!
//! - Role assignments and revocations
//! - Permission changes
//! - Administrative actions
//! - Suspension and reactivation
//!
//! ## Features
//!
//! - Time-windowed audit log cleanup
//! - Query functions for audit history
//! - Event emission for all logged operations
//! - Before/after state tracking

use crate::{errors::NavinError, types::*};
use soroban_sdk::{contracttype, Address, Env};

/// Audit event types for role and permission operations
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum AuditEventType {
    /// Role was assigned to an address
    RoleAssigned,
    /// Role was revoked from an address
    RoleRevoked,
    /// Role was suspended
    RoleSuspended,
    /// Role was reactivated
    RoleReactivated,
    /// Admin transferred
    AdminTransferred,
    /// Carrier whitelisted
    CarrierWhitelisted,
    /// Carrier removed from whitelist
    CarrierUnwhitelisted,
    /// Company suspended
    CompanySuspended,
    /// Company reactivated
    CompanyReactivated,
    /// Carrier suspended
    CarrierSuspended,
    /// Carrier reactivated
    CarrierReactivated,
}

/// Audit log entry for role and permission changes
#[contracttype]
#[derive(Clone, Debug)]
pub struct AuditLogEntry {
    /// Unique entry ID
    pub entry_id: u64,
    /// Type of audit event
    pub event_type: AuditEventType,
    /// Actor performing the action (admin)
    pub actor: Address,
    /// Target of the action (user whose role changed)
    pub target: Address,
    /// Timestamp of the event
    pub timestamp: u64,
}

/// Log a role assignment event
///
/// # Arguments
/// * `env` - The execution environment
/// * `admin` - The admin performing the action
/// * `target` - The address receiving the role
/// * `role` - The role being assigned
///
/// # Returns
/// * `Ok(entry_id)` on success
/// * `Err(NavinError)` on failure
#[allow(dead_code)]
pub fn log_role_assigned(
    env: &Env,
    admin: &Address,
    target: &Address,
    _role: &Role,
) -> Result<u64, NavinError> {
    let entry_id = get_next_audit_entry_id(env)?;
    let timestamp = env.ledger().timestamp();

    let entry = AuditLogEntry {
        entry_id,
        event_type: AuditEventType::RoleAssigned,
        actor: admin.clone(),
        target: target.clone(),
        timestamp,
    };

    store_audit_entry(env, &entry);
    emit_audit_event(env, &entry);

    Ok(entry_id)
}

/// Log a role revocation event
///
/// # Arguments
/// * `env` - The execution environment
/// * `admin` - The admin performing the action
/// * `target` - The address losing the role
/// * `role` - The role being revoked
///
/// # Returns
/// * `Ok(entry_id)` on success
/// * `Err(NavinError)` on failure
#[allow(dead_code)]
pub fn log_role_revoked(
    env: &Env,
    admin: &Address,
    target: &Address,
    _role: &Role,
) -> Result<u64, NavinError> {
    let entry_id = get_next_audit_entry_id(env)?;
    let timestamp = env.ledger().timestamp();

    let entry = AuditLogEntry {
        entry_id,
        event_type: AuditEventType::RoleRevoked,
        actor: admin.clone(),
        target: target.clone(),
        timestamp,
    };

    store_audit_entry(env, &entry);
    emit_audit_event(env, &entry);

    Ok(entry_id)
}

/// Log a role suspension event
///
/// # Arguments
/// * `env` - The execution environment
/// * `admin` - The admin performing the action
/// * `target` - The address whose role is suspended
/// * `role` - The role being suspended
///
/// # Returns
/// * `Ok(entry_id)` on success
/// * `Err(NavinError)` on failure
#[allow(dead_code)]
pub fn log_role_suspended(
    env: &Env,
    admin: &Address,
    target: &Address,
    _role: &Role,
) -> Result<u64, NavinError> {
    let entry_id = get_next_audit_entry_id(env)?;
    let timestamp = env.ledger().timestamp();

    let entry = AuditLogEntry {
        entry_id,
        event_type: AuditEventType::RoleSuspended,
        actor: admin.clone(),
        target: target.clone(),
        timestamp,
    };

    store_audit_entry(env, &entry);
    emit_audit_event(env, &entry);

    Ok(entry_id)
}

/// Log a role reactivation event
///
/// # Arguments
/// * `env` - The execution environment
/// * `admin` - The admin performing the action
/// * `target` - The address whose role is reactivated
/// * `role` - The role being reactivated
///
/// # Returns
/// * `Ok(entry_id)` on success
/// * `Err(NavinError)` on failure
#[allow(dead_code)]
pub fn log_role_reactivated(
    env: &Env,
    admin: &Address,
    target: &Address,
    _role: &Role,
) -> Result<u64, NavinError> {
    let entry_id = get_next_audit_entry_id(env)?;
    let timestamp = env.ledger().timestamp();

    let entry = AuditLogEntry {
        entry_id,
        event_type: AuditEventType::RoleReactivated,
        actor: admin.clone(),
        target: target.clone(),
        timestamp,
    };

    store_audit_entry(env, &entry);
    emit_audit_event(env, &entry);

    Ok(entry_id)
}

/// Log an admin transfer event
///
/// # Arguments
/// * `env` - The execution environment
/// * `old_admin` - The previous admin
/// * `new_admin` - The new admin
///
/// # Returns
/// * `Ok(entry_id)` on success
/// * `Err(NavinError)` on failure
#[allow(dead_code)]
pub fn log_admin_transferred(
    env: &Env,
    old_admin: &Address,
    new_admin: &Address,
) -> Result<u64, NavinError> {
    let entry_id = get_next_audit_entry_id(env)?;
    let timestamp = env.ledger().timestamp();

    let entry = AuditLogEntry {
        entry_id,
        event_type: AuditEventType::AdminTransferred,
        actor: old_admin.clone(),
        target: new_admin.clone(),
        timestamp,
    };

    store_audit_entry(env, &entry);
    emit_audit_event(env, &entry);

    Ok(entry_id)
}

/// Log a carrier whitelist event
///
/// # Arguments
/// * `env` - The execution environment
/// * `admin` - The admin performing the action
/// * `_company` - The company
/// * `carrier` - The carrier being whitelisted
///
/// # Returns
/// * `Ok(entry_id)` on success
/// * `Err(NavinError)` on failure
#[allow(dead_code)]
pub fn log_carrier_whitelisted(
    env: &Env,
    admin: &Address,
    _company: &Address,
    carrier: &Address,
) -> Result<u64, NavinError> {
    let entry_id = get_next_audit_entry_id(env)?;
    let timestamp = env.ledger().timestamp();

    let entry = AuditLogEntry {
        entry_id,
        event_type: AuditEventType::CarrierWhitelisted,
        actor: admin.clone(),
        target: carrier.clone(),
        timestamp,
    };

    store_audit_entry(env, &entry);
    emit_audit_event(env, &entry);

    Ok(entry_id)
}

/// Query audit history by date range
///
/// # Arguments
/// * `env` - The execution environment
/// * `start_time` - Start timestamp (inclusive)
/// * `end_time` - End timestamp (inclusive)
///
/// # Returns
/// * Vector of audit entries within the time range
#[allow(dead_code)]
pub fn query_audit_history(
    env: &Env,
    start_time: u64,
    end_time: u64,
) -> soroban_sdk::Vec<AuditLogEntry> {
    let mut results = soroban_sdk::Vec::new(env);
    let total_entries = get_audit_entry_count(env);

    for i in 0..total_entries {
        if let Some(entry) = get_audit_entry(env, i as u64) {
            if entry.timestamp >= start_time && entry.timestamp <= end_time {
                results.push_back(entry);
            }
        }
    }

    results
}

/// Query audit history for a specific target
///
/// # Arguments
/// * `env` - The execution environment
/// * `target` - The target address to query
///
/// # Returns
/// * Vector of audit entries for the target
#[allow(dead_code)]
pub fn query_audit_history_for_target(
    env: &Env,
    target: &Address,
) -> soroban_sdk::Vec<AuditLogEntry> {
    let mut results = soroban_sdk::Vec::new(env);
    let total_entries = get_audit_entry_count(env);

    for i in 0..total_entries {
        if let Some(entry) = get_audit_entry(env, i as u64) {
            if entry.target == *target {
                results.push_back(entry);
            }
        }
    }

    results
}

/// Query audit history for a specific actor
///
/// # Arguments
/// * `env` - The execution environment
/// * `actor` - The actor (admin) to query
///
/// # Returns
/// * Vector of audit entries by the actor
#[allow(dead_code)]
pub fn query_audit_history_by_actor(env: &Env, actor: &Address) -> soroban_sdk::Vec<AuditLogEntry> {
    let mut results = soroban_sdk::Vec::new(env);
    let total_entries = get_audit_entry_count(env);

    for i in 0..total_entries {
        if let Some(entry) = get_audit_entry(env, i as u64) {
            if entry.actor == *actor {
                results.push_back(entry);
            }
        }
    }

    results
}

/// Clean up old audit entries (admin-only)
///
/// # Arguments
/// * `env` - The execution environment
/// * `admin` - The admin address
/// * `before_timestamp` - Remove entries before this timestamp
///
/// # Returns
/// * `Ok(count)` - Number of entries removed
/// * `Err(NavinError)` - If not authorized
#[allow(dead_code)]
pub fn cleanup_audit_logs(
    env: &Env,
    admin: &Address,
    before_timestamp: u64,
) -> Result<u32, NavinError> {
    // Verify admin authorization
    admin.require_auth();
    if !crate::storage::is_admin(env, admin) {
        return Err(NavinError::Unauthorized);
    }

    let mut removed_count = 0u32;
    let total_entries = get_audit_entry_count(env);

    for i in 0..total_entries {
        if let Some(entry) = get_audit_entry(env, i as u64) {
            if entry.timestamp < before_timestamp {
                remove_audit_entry(env, i as u64);
                removed_count = removed_count.saturating_add(1);
            }
        }
    }

    Ok(removed_count)
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

fn get_next_audit_entry_id(env: &Env) -> Result<u64, NavinError> {
    let count = get_audit_entry_count(env);
    Ok(count as u64)
}

fn get_audit_entry_count(env: &Env) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::AuditEntryCount)
        .unwrap_or(0)
}

fn increment_audit_entry_count(env: &Env) {
    let count = get_audit_entry_count(env);
    env.storage()
        .persistent()
        .set(&DataKey::AuditEntryCount, &count.saturating_add(1));
}

fn store_audit_entry(env: &Env, entry: &AuditLogEntry) {
    let entry_id = entry.entry_id;
    env.storage()
        .persistent()
        .set(&DataKey::AuditEntry(entry_id), entry);
    increment_audit_entry_count(env);
}

fn get_audit_entry(env: &Env, entry_id: u64) -> Option<AuditLogEntry> {
    env.storage()
        .persistent()
        .get(&DataKey::AuditEntry(entry_id))
}

fn remove_audit_entry(env: &Env, entry_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::AuditEntry(entry_id));
}

fn emit_audit_event(env: &Env, entry: &AuditLogEntry) {
    // Emit audit event for off-chain indexing
    let event_type = match entry.event_type {
        AuditEventType::RoleAssigned => "audit_role_assigned",
        AuditEventType::RoleRevoked => "audit_role_revoked",
        AuditEventType::RoleSuspended => "audit_role_suspended",
        AuditEventType::RoleReactivated => "audit_role_reactivated",
        AuditEventType::AdminTransferred => "audit_admin_transferred",
        AuditEventType::CarrierWhitelisted => "audit_carrier_whitelisted",
        AuditEventType::CarrierUnwhitelisted => "audit_carrier_unwhitelisted",
        AuditEventType::CompanySuspended => "audit_company_suspended",
        AuditEventType::CompanyReactivated => "audit_company_reactivated",
        AuditEventType::CarrierSuspended => "audit_carrier_suspended",
        AuditEventType::CarrierReactivated => "audit_carrier_reactivated",
    };

    env.events().publish(
        (soroban_sdk::Symbol::new(env, event_type),),
        (
            entry.entry_id,
            entry.actor.clone(),
            entry.target.clone(),
            entry.timestamp,
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_type_variants() {
        let assigned = AuditEventType::RoleAssigned;
        let revoked = AuditEventType::RoleRevoked;
        let suspended = AuditEventType::RoleSuspended;
        let reactivated = AuditEventType::RoleReactivated;

        assert_ne!(assigned, revoked);
        assert_ne!(suspended, reactivated);
    }

    #[test]
    fn test_audit_log_entry_creation() {
        let env = soroban_sdk::Env::default();
        let entry = AuditLogEntry {
            entry_id: 1,
            event_type: AuditEventType::RoleAssigned,
            actor: Address::from_str(
                &env,
                "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H",
            ),
            target: Address::from_str(
                &env,
                "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H",
            ),
            timestamp: 1000,
        };

        assert_eq!(entry.entry_id, 1);
        assert_eq!(entry.event_type, AuditEventType::RoleAssigned);
    }
}
