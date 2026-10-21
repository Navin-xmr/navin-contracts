//! Module visibility regression tests (issue #1064).
//!
//! `audit`, `consistency`, `diagnostics` and `error_map` are deliberately
//! `pub` in lib.rs. These tests reach each module's types through its public
//! path, through the generated `NavinShipmentClient` where an entry point
//! returns them, so narrowing a module's visibility fails here first rather
//! than surfacing as a `private_interfaces` or `dead_code` lint.

use crate::audit::AuditLogEntry;
use crate::consistency::ConsistencyViolation;
use crate::diagnostics::{run_system_health_check, SystemHealthStatus};
use crate::error_map::{get_error_info, ContractErrorInfo};
use crate::test_helpers::setup_initialized_shipment_env;
use crate::NavinError;
use soroban_sdk::{Address, Vec};

#[test]
fn test_error_map_types_are_returned_by_client() {
    let (_env, client, _admin, _token) = setup_initialized_shipment_env();
    let code = NavinError::Unauthorized as u32;

    let info: ContractErrorInfo = client.get_error_info(&code);

    assert_eq!(info.code, code);
    assert_eq!(info, get_error_info(code));
}

#[test]
fn test_audit_types_are_returned_by_client() {
    let (_env, client, admin, _token) = setup_initialized_shipment_env();

    let by_actor: Vec<AuditLogEntry> = client.query_audit_history_by_actor(&admin);
    let by_target: Vec<AuditLogEntry> = client.query_audit_history_for_target(&admin);
    let in_range: Vec<AuditLogEntry> = client.query_audit_history(&0, &u64::MAX);

    assert!(in_range.len() >= by_actor.len());
    assert!(in_range.len() >= by_target.len());
}

#[test]
fn test_consistency_types_are_returned_by_client() {
    let (env, client, admin, _token) = setup_initialized_shipment_env();

    let violations: Vec<ConsistencyViolation> =
        client.check_batch_consistency(&admin, &Vec::new(&env));

    assert!(violations.is_empty());
}

#[test]
fn test_consistency_entry_point_still_requires_admin() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();
    let stranger = <Address as soroban_sdk::testutils::Address>::generate(&env);

    let result = client.try_check_batch_consistency(&stranger, &Vec::new(&env));

    assert!(result.is_err());
}

#[test]
fn test_diagnostics_health_check_is_reachable() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();

    let health: SystemHealthStatus =
        env.as_contract(&client.address, || run_system_health_check(&env));

    assert_eq!(health.total_shipments, 0);
    assert_eq!(health.sum_of_escrow_balances, 0);
    assert!(health.anomalous_shipment_ids.is_empty());
    assert!(health.storage_inconsistencies.is_empty());
}
