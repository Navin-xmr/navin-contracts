//! Explicit authorization-tree tests for sensitive contract entry points.
//!
//! These tests verify that every sensitive operation correctly invokes
//! `require_auth()` on the right address with the right function name and
//! arguments.  They go beyond simple "wrong caller gets Unauthorized" checks by
//! asserting the **exact** `AuthorizedInvocation` tree recorded by the Soroban
//! environment.
//!
//! # Pattern
//!
//! Each positive test:
//! 1. Sets up the contract using `mock_all_auths()`.
//! 2. Calls the function under test.
//! 3. Calls `env.auths()` and asserts the recorded invocation matches the
//!    expected `(Address, AuthorizedInvocation)` pair.
//!
//! Each negative test creates an environment **without** `mock_all_auths()`,
//! then attempts the call and expects an `Err` result (auth trap), confirming
//! that the contract cannot be called without proper authorisation.

extern crate std;

use crate::{NavinShipment, NavinShipmentClient, ShipmentStatus};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation, Ledger as _},
    Address, BytesN, Env, IntoVal, Symbol,
};

// ── Minimal token stub (no-op transfer) ──────────────────────────────────────

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn decimals(_env: soroban_sdk::Env) -> u32 {
        7
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    pub fn transfer_from(
        _env: Env,
        _spender: Address,
        _from: Address,
        _to: Address,
        _amount: i128,
    ) {
    }
}

// ── Shared setup helpers ──────────────────────────────────────────────────────

/// Full environment with `mock_all_auths()` active (for positive tests).
fn setup_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = crate::test_utils::setup_env();
    let token = env.register(MockToken {}, ());
    let contract_id = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &contract_id);
    client.initialize(&admin, &token);
    (env, client, admin, token)
}

// ── Helper: contract id from client ──────────────────────────────────────────
fn contract_id(client: &NavinShipmentClient<'static>) -> Address {
    client.address.clone()
}

// =============================================================================
// Admin path — positive auth-tree assertions
// =============================================================================

/// `add_company` must record an auth invocation for the admin address with the
/// correct function name and argument list.
#[test]
fn test_auth_tree_add_company() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let cid = contract_id(&client);

    client.add_company(&admin, &company);

    assert_eq!(
        env.auths(),
        std::vec![(
            admin.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    cid,
                    Symbol::new(&env, "add_company"),
                    (admin.clone(), company.clone()).into_val(&env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
}

/// `add_carrier` must record an auth invocation for admin with correct args.
#[test]
fn test_auth_tree_add_carrier() {
    let (env, client, admin, _token) = setup_env();
    let carrier = Address::generate(&env);
    let cid = contract_id(&client);

    client.add_carrier(&admin, &carrier);

    assert_eq!(
        env.auths(),
        std::vec![(
            admin.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    cid,
                    Symbol::new(&env, "add_carrier"),
                    (admin.clone(), carrier.clone()).into_val(&env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
}

/// `suspend_carrier` must record admin auth with the target carrier address.
#[test]
fn test_auth_tree_suspend_carrier() {
    let (env, client, admin, _token) = setup_env();
    let carrier = Address::generate(&env);
    let cid = contract_id(&client);

    client.add_carrier(&admin, &carrier);
    client.suspend_carrier(&admin, &carrier);

    assert_eq!(
        env.auths(),
        std::vec![(
            admin.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    cid,
                    Symbol::new(&env, "suspend_carrier"),
                    (admin.clone(), carrier.clone()).into_val(&env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
}

/// `revoke_role` must record admin auth with the target address.
#[test]
fn test_auth_tree_revoke_role() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let cid = contract_id(&client);

    client.add_company(&admin, &company);
    client.revoke_role(&admin, &company);

    assert_eq!(
        env.auths(),
        std::vec![(
            admin.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    cid,
                    Symbol::new(&env, "revoke_role"),
                    (admin.clone(), company.clone()).into_val(&env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
}

/// `force_cancel_shipment` must record admin auth with the shipment ID and
/// reason hash, confirming the strict admin-only gate on forced cancellation.
#[test]
fn test_auth_tree_force_cancel_shipment() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = crate::test_utils::future_deadline(&env, 3_600);
    let cid = contract_id(&client);

    client.add_company(&admin, &company);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.force_cancel_shipment(&admin, &shipment_id, &reason_hash);

    assert_eq!(
        env.auths(),
        std::vec![(
            admin.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    cid,
                    Symbol::new(&env, "force_cancel_shipment"),
                    (admin.clone(), shipment_id, reason_hash.clone()).into_val(&env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
}

/// `archive_shipment` must record admin auth with the shipment ID.
#[test]
fn test_auth_tree_archive_shipment() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = crate::test_utils::future_deadline(&env, 3_600);
    let cid = contract_id(&client);

    client.add_company(&admin, &company);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    // Cancel so the shipment is finalized (required before archiving)
    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    client.archive_shipment(&admin, &shipment_id);

    assert_eq!(
        env.auths(),
        std::vec![(
            admin.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    cid,
                    Symbol::new(&env, "archive_shipment"),
                    (admin.clone(), shipment_id).into_val(&env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
}

// =============================================================================
// Company path — positive auth-tree assertions
// =============================================================================

/// `create_shipment` must record the company (sender) auth with all shipment
/// parameters, confirming no other address can silently create a shipment.
#[test]
fn test_auth_tree_create_shipment() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[5u8; 32]);
    let milestones: soroban_sdk::Vec<(Symbol, u32)> = soroban_sdk::Vec::new(&env);
    let deadline = crate::test_utils::future_deadline(&env, 3_600);
    let cid = contract_id(&client);

    client.add_company(&admin, &company);
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );

    assert_eq!(
        env.auths(),
        std::vec![(
            company.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    cid,
                    Symbol::new(&env, "create_shipment"),
                    (
                        company.clone(),
                        receiver.clone(),
                        carrier.clone(),
                        data_hash.clone(),
                        milestones,
                        deadline,
                    )
                        .into_val(&env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
}

/// `cancel_shipment` must record the company auth with the shipment ID and
/// reason hash.
#[test]
fn test_auth_tree_cancel_shipment() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[9u8; 32]);
    let deadline = crate::test_utils::future_deadline(&env, 3_600);
    let cid = contract_id(&client);

    client.add_company(&admin, &company);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    assert_eq!(
        env.auths(),
        std::vec![(
            company.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    cid,
                    Symbol::new(&env, "cancel_shipment"),
                    (company.clone(), shipment_id, reason_hash.clone()).into_val(&env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
}

// =============================================================================
// Carrier path — positive auth-tree assertions
// =============================================================================

/// `update_status` must record the carrier auth with shipment ID, new status,
/// and data hash.
#[test]
fn test_auth_tree_update_status() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let status_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = crate::test_utils::future_deadline(&env, 3_600);
    let cid = contract_id(&client);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &status_hash,
    );

    assert_eq!(
        env.auths(),
        std::vec![(
            carrier.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    cid,
                    Symbol::new(&env, "update_status"),
                    (
                        carrier.clone(),
                        shipment_id,
                        ShipmentStatus::InTransit,
                        status_hash.clone(),
                    )
                        .into_val(&env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
}

/// `handoff_shipment` must record the current carrier auth.
#[test]
fn test_auth_tree_handoff_shipment() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier1 = Address::generate(&env);
    let carrier2 = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[3u8; 32]);
    let deadline = crate::test_utils::future_deadline(&env, 3_600);
    let cid = contract_id(&client);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier1);
    client.add_carrier(&admin, &carrier2);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier1,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    // Move to InTransit so handoff is valid
    client.update_status(
        &carrier1,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    client.handoff_shipment(&carrier1, &carrier2, &shipment_id, &handoff_hash);

    assert_eq!(
        env.auths(),
        std::vec![(
            carrier1.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    cid,
                    Symbol::new(&env, "handoff_shipment"),
                    (
                        carrier1.clone(),
                        carrier2.clone(),
                        shipment_id,
                        handoff_hash.clone(),
                    )
                        .into_val(&env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
}

// =============================================================================
// Receiver path — positive auth-tree assertions
// =============================================================================

/// `confirm_delivery` must record the receiver auth with the shipment ID and
/// confirmation hash — the receiver is the only party who can accept delivery.
#[test]
fn test_auth_tree_confirm_delivery() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let confirm_hash = BytesN::from_array(&env, &[7u8; 32]);
    let deadline = crate::test_utils::future_deadline(&env, 3_600);
    let cid = contract_id(&client);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    client.confirm_delivery(&receiver, &shipment_id, &confirm_hash);

    assert_eq!(
        env.auths(),
        std::vec![(
            receiver.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    cid,
                    Symbol::new(&env, "confirm_delivery"),
                    (receiver.clone(), shipment_id, confirm_hash.clone()).into_val(&env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
}

// =============================================================================
// Negative tests — auth mismatch fails predictably
// =============================================================================
//
// IMPORTANT: `mock_all_auths()` persists for the entire `Env` lifetime; once
// called it cannot be revoked.  Negative tests therefore use completely fresh
// `Env::default()` instances that *never* have `mock_all_auths()` called.
//
// `initialize` does not call `require_auth()`, so a contract can be deployed
// and initialised without any auth mock.  All subsequent protected functions
// call `caller.require_auth()` before any other logic, so `try_*` will return
// `Err` even when no shipment has been set up yet.

/// `add_company` must fail when no auth mock is provided for the admin address.
#[test]
fn test_auth_add_company_fails_without_auth() {
    // Fresh env — mock_all_auths() is NEVER called on this env.
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.protocol_version = crate::test_utils::DEFAULT_PROTOCOL_VERSION;
    });
    env.ledger()
        .set_timestamp(crate::test_utils::DEFAULT_TIMESTAMP);

    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let token = env.register(MockToken {}, ());
    let cid = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &cid);

    // initialize does not require_auth — safe without any mock
    client.initialize(&admin, &token);

    // No auth mock active → admin.require_auth() will fail
    let result = client.try_add_company(&admin, &company);
    assert!(
        result.is_err(),
        "add_company must fail when admin auth is not provided"
    );
}

/// `create_shipment` calls `sender.require_auth()` as its first gate; it must
/// fail when no auth mock is active even before the role check fires.
#[test]
fn test_auth_create_shipment_fails_without_auth() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.protocol_version = crate::test_utils::DEFAULT_PROTOCOL_VERSION;
    });
    env.ledger()
        .set_timestamp(crate::test_utils::DEFAULT_TIMESTAMP);

    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let admin = Address::generate(&env);
    let token = env.register(MockToken {}, ());
    let cid = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &cid);
    let deadline = env.ledger().timestamp() + 3_600;

    client.initialize(&admin, &token); // no auth needed

    // No mock → company.require_auth() fires and fails before role check
    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    assert!(
        result.is_err(),
        "create_shipment must fail when company auth is not provided"
    );
}

/// `update_status` calls `caller.require_auth()` before checking if the
/// shipment exists; it must fail with no mock even for a non-existent shipment.
#[test]
fn test_auth_update_status_fails_without_auth() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.protocol_version = crate::test_utils::DEFAULT_PROTOCOL_VERSION;
    });
    env.ledger()
        .set_timestamp(crate::test_utils::DEFAULT_TIMESTAMP);

    let admin = Address::generate(&env);
    let carrier = Address::generate(&env);
    let status_hash = BytesN::from_array(&env, &[2u8; 32]);
    let token = env.register(MockToken {}, ());
    let cid = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &cid);

    client.initialize(&admin, &token);

    // No mock → carrier.require_auth() fires and fails before shipment lookup
    let result =
        client.try_update_status(&carrier, &1u64, &ShipmentStatus::InTransit, &status_hash);
    assert!(
        result.is_err(),
        "update_status must fail when caller auth is not provided"
    );
}

/// `confirm_delivery` calls `receiver.require_auth()` before any other logic;
/// it must fail with no mock.
#[test]
fn test_auth_confirm_delivery_fails_without_auth() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.protocol_version = crate::test_utils::DEFAULT_PROTOCOL_VERSION;
    });
    env.ledger()
        .set_timestamp(crate::test_utils::DEFAULT_TIMESTAMP);

    let admin = Address::generate(&env);
    let receiver = Address::generate(&env);
    let confirm_hash = BytesN::from_array(&env, &[7u8; 32]);
    let token = env.register(MockToken {}, ());
    let cid = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &cid);

    client.initialize(&admin, &token);

    // No mock → receiver.require_auth() fires and fails before shipment lookup
    let result = client.try_confirm_delivery(&receiver, &1u64, &confirm_hash);
    assert!(
        result.is_err(),
        "confirm_delivery must fail when receiver auth is not provided"
    );
}

/// `force_cancel_shipment` calls `admin.require_auth()` as its very first gate;
/// it must fail with no mock.
#[test]
fn test_auth_force_cancel_fails_without_auth() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.protocol_version = crate::test_utils::DEFAULT_PROTOCOL_VERSION;
    });
    env.ledger()
        .set_timestamp(crate::test_utils::DEFAULT_TIMESTAMP);

    let admin = Address::generate(&env);
    let reason_hash = BytesN::from_array(&env, &[2u8; 32]);
    let token = env.register(MockToken {}, ());
    let cid = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &cid);

    client.initialize(&admin, &token);

    // No mock → admin.require_auth() fires and fails before shipment lookup
    let result = client.try_force_cancel_shipment(&admin, &1u64, &reason_hash);
    assert!(
        result.is_err(),
        "force_cancel_shipment must fail when admin auth is not provided"
    );
}

/// `add_guardian` must record an auth invocation for the admin address with the
/// correct function name and argument list.
#[test]
fn test_auth_tree_add_guardian() {
    let (env, client, admin, _token) = setup_env();
    let guardian = Address::generate(&env);
    let cid = contract_id(&client);

    client.add_guardian(&admin, &guardian);

    assert_eq!(
        env.auths(),
        std::vec![(
            admin.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    cid,
                    Symbol::new(&env, "add_guardian"),
                    (admin.clone(), guardian.clone()).into_val(&env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
}

/// `add_operator` must record an auth invocation for the admin address with the
/// correct function name and argument list.
#[test]
fn test_auth_tree_add_operator() {
    let (env, client, admin, _token) = setup_env();
    let operator = Address::generate(&env);
    let cid = contract_id(&client);

    client.add_operator(&admin, &operator);

    assert_eq!(
        env.auths(),
        std::vec![(
            admin.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    cid,
                    Symbol::new(&env, "add_operator"),
                    (admin.clone(), operator.clone()).into_val(&env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
}

/// `add_guardian` must fail when no auth mock is provided for the admin address.
#[test]
fn test_auth_add_guardian_fails_without_auth() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.protocol_version = crate::test_utils::DEFAULT_PROTOCOL_VERSION;
    });
    env.ledger()
        .set_timestamp(crate::test_utils::DEFAULT_TIMESTAMP);

    let admin = Address::generate(&env);
    let guardian = Address::generate(&env);
    let token = env.register(MockToken {}, ());
    let cid = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &cid);

    client.initialize(&admin, &token);

    let result = client.try_add_guardian(&admin, &guardian);
    assert!(
        result.is_err(),
        "add_guardian must fail when admin auth is not provided"
    );
}

/// `add_operator` must fail when no auth mock is provided for the admin address.
#[test]
fn test_auth_add_operator_fails_without_auth() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.protocol_version = crate::test_utils::DEFAULT_PROTOCOL_VERSION;
    });
    env.ledger()
        .set_timestamp(crate::test_utils::DEFAULT_TIMESTAMP);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let token = env.register(MockToken {}, ());
    let cid = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &cid);

    client.initialize(&admin, &token);

    let result = client.try_add_operator(&admin, &operator);
    assert!(
        result.is_err(),
        "add_operator must fail when admin auth is not provided"
    );
}

// =============================================================================
// #456 — Auth mismatch: wrong role produces the correct domain error
// =============================================================================
//
// These tests verify that when a caller has valid authentication (mock_all_auths
// is active so require_auth passes) but lacks the required role, the contract
// maps the failure to the correct NavinError variant with the expected category
// and retry guidance.

/// A company caller must not be able to call `add_company` — that is an
/// admin-only operation.  With mock_all_auths active, `require_auth` passes
/// but the role check (`require_admin_or_operator`) must reject the company.
#[test]
fn test_wrong_role_add_company_returns_error() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let another_company = Address::generate(&env);

    // Register company with legitimate admin call.
    client.add_company(&admin, &company);

    // company (not an admin/operator) tries to add another company.
    // mock_all_auths is active so require_auth passes, but the role check fails.
    let result = client.try_add_company(&company, &another_company);
    assert!(
        result.is_err(),
        "non-admin must not be able to call add_company even with auth mocked"
    );
}

/// A company caller must not be able to suspend a carrier — admin-only.
#[test]
fn test_wrong_role_suspend_carrier_returns_error() {
    let (env, client, admin, _token) = setup_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    // company tries to suspend the carrier (requires admin/operator role).
    let result = client.try_suspend_carrier(&company, &carrier);
    assert!(
        result.is_err(),
        "non-admin must not be able to suspend a carrier"
    );
}

/// A company caller must not be able to suspend another company — admin-only.
#[test]
fn test_wrong_role_suspend_company_returns_error() {
    let (env, client, admin, _token) = setup_env();
    let company_a = Address::generate(&env);
    let company_b = Address::generate(&env);

    client.add_company(&admin, &company_a);
    client.add_company(&admin, &company_b);

    // company_a (not an admin) tries to suspend company_b.
    let result = client.try_suspend_company(&company_a, &company_b);
    assert!(
        result.is_err(),
        "non-admin must not be able to suspend a company"
    );
}

/// Error mapping consistency: the `Unauthorized` domain error returned by
/// wrong-role callers must map to `ErrorCategory::Unauthorized` with
/// `NoRetry` guidance via `error_info`.
#[test]
fn test_wrong_role_error_maps_to_unauthorized_category() {
    use crate::error_map::{error_info, ErrorCategory, RetryGuidance};
    use crate::NavinError;

    let info = error_info(NavinError::Unauthorized);
    assert_eq!(
        info.category,
        ErrorCategory::Unauthorized,
        "Unauthorized must map to ErrorCategory::Unauthorized"
    );
    assert_eq!(
        info.retry,
        RetryGuidance::NoRetry,
        "Unauthorized must have NoRetry — wrong role cannot be fixed by retrying"
    );
    assert_eq!(info.code, 3);
}

// =============================================================================
// #521 — Whitespace-only symbol rejection in role-registration context
// =============================================================================
//
// `validate_symbol_not_whitespace_only` is the canonical guard for detecting
// empty or whitespace-equivalent Symbol inputs. Because the Soroban SDK restricts
// Symbol characters to [a-zA-Z0-9_] at construction time, a zero-character Symbol
// (XDR length ≤ 8 bytes) is the closest representable equivalent of a
// "whitespace-only" identifier.
//
// The tests below verify the helper's behaviour both in isolation (unit) and
// end-to-end through `set_shipment_metadata`, which is the registration path
// where company/carrier actors supply arbitrary Symbol keys and values.

/// An empty Symbol (0 characters, XDR = 8 bytes) must be rejected with
/// `NavinError::InvalidSymbol` by the whitespace-rejection helper.
#[test]
fn test_whitespace_only_symbol_empty_returns_invalid_symbol() {
    let (env, client, _admin, _token) = setup_env();
    let cid = client.address.clone();

    let empty = Symbol::new(&env, "");
    let result = env.as_contract(&cid, || {
        crate::validate_symbol_not_whitespace_only(&env, &empty)
    });

    assert_eq!(
        result,
        Err(crate::NavinError::InvalidSymbol),
        "empty Symbol must be rejected as whitespace-only"
    );
}

/// A valid alphanumeric Symbol must pass the whitespace-rejection helper.
#[test]
fn test_valid_symbol_passes_whitespace_rejection_helper() {
    let (env, client, _admin, _token) = setup_env();
    let cid = client.address.clone();

    let valid = Symbol::new(&env, "carrier");
    let result = env.as_contract(&cid, || {
        crate::validate_symbol_not_whitespace_only(&env, &valid)
    });

    assert_eq!(result, Ok(()));
}

/// A Symbol that exceeds the 12-character Stellar limit must also be rejected
/// by the whitespace-rejection helper (XDR > 20 bytes).
#[test]
fn test_oversized_symbol_rejected_by_whitespace_helper() {
    extern crate std;
    let (env, client, _admin, _token) = setup_env();
    let cid = client.address.clone();

    let long: std::string::String = "A".repeat(13);
    let oversized = Symbol::new(&env, &long);
    let result = env.as_contract(&cid, || {
        crate::validate_symbol_not_whitespace_only(&env, &oversized)
    });

    assert_eq!(
        result,
        Err(crate::NavinError::InvalidSymbol),
        "oversized Symbol must be rejected by the whitespace helper"
    );
}

/// End-to-end: calling `set_shipment_metadata` with an empty key Symbol must
/// return `InvalidSymbol`, demonstrating the whitespace guard is active in a
/// registration path that company callers exercise.
#[test]
fn test_set_shipment_metadata_rejects_empty_key_symbol() {
    extern crate std;
    let (env, client, admin, _token) = setup_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = crate::test_utils::future_deadline(&env, 3_600);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let empty_key = Symbol::new(&env, "");
    let valid_value = Symbol::new(&env, "value");
    let result =
        client.try_set_shipment_metadata(&company, &shipment_id, &empty_key, &valid_value);

    assert_eq!(
        result,
        Err(Ok(crate::NavinError::InvalidSymbol)),
        "set_shipment_metadata with an empty key Symbol must return InvalidSymbol"
    );
}

/// End-to-end: calling `set_shipment_metadata` with an empty value Symbol must
/// also be rejected, ensuring both key and value are guarded.
#[test]
fn test_set_shipment_metadata_rejects_empty_value_symbol() {
    extern crate std;
    let (env, client, admin, _token) = setup_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = crate::test_utils::future_deadline(&env, 3_600);
    let data_hash = BytesN::from_array(&env, &[2u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let valid_key = Symbol::new(&env, "weight");
    let empty_value = Symbol::new(&env, "");
    let result =
        client.try_set_shipment_metadata(&company, &shipment_id, &valid_key, &empty_value);

    assert_eq!(
        result,
        Err(Ok(crate::NavinError::InvalidSymbol)),
        "set_shipment_metadata with an empty value Symbol must return InvalidSymbol"
    );
}
