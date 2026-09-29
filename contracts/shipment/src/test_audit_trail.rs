#![cfg(test)]

use crate::audit::*;
use crate::errors::NavinError;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup_audit_env() -> (Env, Address, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let target = Address::generate(&env);
    (env, admin, target)
}

#[test]
fn test_audit_log_bounded_growth_limit() {
    let (env, admin, target) = setup_audit_env();

    // Fill storage up to MAX_AUDIT_LOG_ENTRIES
    for _ in 0..MAX_AUDIT_LOG_ENTRIES {
        let res = log_role_assigned(&env, &admin, &target, &crate::types::Role::Company);
        assert!(res.is_ok());
    }

    // Exceeding the limit must return AuditLogLimitExceeded
    let res = log_role_assigned(&env, &admin, &target, &crate::types::Role::Company);
    assert_eq!(res, Err(NavinError::AuditLogLimitExceeded));
}

#[test]
fn test_query_audit_history_pagination() {
    let (env, admin, target) = setup_audit_env();

    // Log 5 entries
    for _ in 0..5 {
        let _ = log_role_assigned(&env, &admin, &target, &crate::types::Role::Company);
    }

    // Query with start_id = 1, limit = 2
    let page = query_audit_history(&env, 0, u64::MAX, 1, 2);
    assert_eq!(page.len(), 2);

    // Query target with pagination
    let target_page = query_audit_history_for_target(&env, &target, 0, 3);
    assert_eq!(target_page.len(), 3);

    // Query actor with pagination
    let actor_page = query_audit_history_by_actor(&env, &admin, 2, 2);
    assert_eq!(actor_page.len(), 2);
}
