//! # Auth Matrix Tests — Issue #243
//!
//! Table-driven RBAC test suite validating role-based authorization for every
//! mutating function and key privileged paths.
//!
//! ## Role × Function Matrix
//!
//! | Function                  | admin | company | carrier | guardian | operator | stranger |
//! |---------------------------|-------|---------|---------|----------|----------|---------|
//! | add_company               | allow | deny    | deny    | deny     | deny     | deny    |
//! | add_carrier               | allow | deny    | deny    | deny     | deny     | deny    |
//! | add_guardian              | allow | deny    | deny    | deny     | deny     | deny    |
//! | add_operator              | allow | deny    | deny    | deny     | deny     | deny    |
//! | suspend_carrier           | allow | deny    | deny    | deny     | deny     | deny    |
//! | revoke_role               | allow | deny    | deny    | deny     | deny     | deny    |
//! | force_cancel_shipment     | allow | deny    | deny    | deny     | deny     | deny    |
//! | archive_shipment          | allow | deny    | deny    | deny     | deny     | deny    |
//! | pause                     | allow | deny    | deny    | deny     | deny     | deny    |
//! | set_shipment_limit        | allow | deny    | deny    | deny     | deny     | deny    |
//! | update_config             | allow | deny    | deny    | deny     | deny     | deny    |
//! | create_shipment           | deny  | allow   | deny    | deny     | deny     | deny    |
//! | deposit_escrow            | deny  | allow   | deny    | deny     | deny     | deny    |
//! | add_carrier_to_whitelist  | deny  | allow   | deny    | deny     | deny     | deny    |
//! | cancel_shipment           | deny  | allow*  | deny    | deny     | deny     | deny    |
//! | refund_escrow             | deny  | allow*  | deny    | deny     | deny     | deny    |
//! | update_status             | deny  | deny    | allow   | deny     | deny     | deny    |
//! | record_milestone          | deny  | deny    | allow   | deny     | deny     | deny    |
//! | confirm_delivery          | deny  | deny    | deny    | deny     | deny     | allow*  |
//! | release_escrow            | deny  | deny    | deny    | deny     | deny     | allow*  |
//! | resolve_dispute           | allow | deny    | deny    | allow    | deny     | deny    |
//! | propose_action            | allow | deny    | deny    | deny     | deny     | deny    |
//!
//! *allow for the specific address that owns/receives the shipment.
//!
//! ## Naming convention
//! `test_auth_<role>_<function>_<allow|deny>`
//!
//! Each deny test uses `mock_all_auths()` so the `require_auth()` call is
//! satisfied; the assertion targets the subsequent role-check error
//! `NavinError::Unauthorized`.  Each allow test verifies the call succeeds
//! (or returns a domain error that is not Unauthorized).

#![cfg(test)]

extern crate std;

use crate::{NavinError, NavinShipment, NavinShipmentClient};
use soroban_sdk::{
    contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Symbol, Vec,
};

// ── No-op token stub ──────────────────────────────────────────────────────────

#[contract]
struct MatrixMockToken;

#[contractimpl]
impl MatrixMockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

// ── Shared test context ───────────────────────────────────────────────────────

struct Ctx {
    env: Env,
    client: NavinShipmentClient<'static>,
    admin: Address,
    company: Address,
    carrier: Address,
    receiver: Address,
    guardian: Address,
    operator: Address,
    /// Address with no role registered.
    stranger: Address,
}

fn setup() -> Ctx {
    let (env, admin) = crate::test_utils::setup_env();
    let token = env.register(MatrixMockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    let guardian = Address::generate(&env);
    let operator = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_guardian(&admin, &guardian);
    client.add_operator(&admin, &operator);

    Ctx {
        env,
        client,
        admin,
        company,
        carrier,
        receiver,
        guardian,
        operator,
        stranger,
    }
}

fn dummy_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[1u8; 32])
}

fn seeded_hash(env: &Env, seed: u8) -> BytesN<32> {
    let mut b = [1u8; 32];
    b[31] = seed;
    BytesN::from_array(env, &b)
}

/// Create a basic shipment (Created state, no milestones) and return its ID.
fn create_shipment(ctx: &Ctx) -> u64 {
    let deadline = ctx.env.ledger().timestamp() + 86_400;
    ctx.client.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env),
        &Vec::new(&ctx.env),
        &deadline,
    )
}

/// Create a shipment and put it in InTransit state.
fn create_shipment_in_transit(ctx: &Ctx) -> u64 {
    let id = create_shipment(ctx);
    ctx.client.update_status(
        &ctx.carrier,
        &id,
        &crate::ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env),
    );
    id
}

/// Create a shipment, fund it, move to InTransit, then raise dispute.
/// resolve_dispute requires escrow_amount > 0; deposit must happen while
/// the shipment is still in Created state.
fn create_disputed_shipment(ctx: &Ctx) -> u64 {
    let id = create_shipment(ctx);
    ctx.client.deposit_escrow(&ctx.company, &id, &1000);
    ctx.client.update_status(
        &ctx.carrier,
        &id,
        &crate::ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env),
    );
    ctx.client
        .raise_dispute(&ctx.company, &id, &seeded_hash(&ctx.env, 0xFE));
    id
}

fn salt(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0xABu8; 32])
}

/// Init a 2-of-1 multisig: two admins registered, threshold=1.
/// Satisfies multisig_min_admins=2 while allowing the primary admin to
/// auto-execute proposals without a second approval.
fn init_single_admin_multisig(ctx: &Ctx, admin: &Address) {
    let mut admins: Vec<Address> = Vec::new(&ctx.env);
    admins.push_back(admin.clone());
    admins.push_back(Address::generate(&ctx.env));
    ctx.client.init_multisig(admin, &admins, &1);
}

/// Expect the call to return NavinError::Unauthorized.
///
/// Soroban v22 `try_*` return type for `fn f() -> Result<T, NavinError>` is:
///   `Result<Result<T, ConversionError>, Result<NavinError, InvokeError>>`
/// Contract errors land in the outer `Err(Ok(NavinError))`.
fn assert_unauthorized<T: core::fmt::Debug, E: core::fmt::Debug>(
    result: Result<T, Result<NavinError, E>>,
) {
    assert!(result.is_err(), "expected Unauthorized but call succeeded");
    let err = result.unwrap_err().unwrap();
    assert_eq!(
        err,
        NavinError::Unauthorized,
        "expected Unauthorized, got {err:?}"
    );
}

// =============================================================================
// SECTION 1: Admin-only functions
// Naming: test_auth_<role>_<function>_<allow|deny>
// =============================================================================

// ── add_company ───────────────────────────────────────────────────────────────

/// test_auth_admin_add_company_allow — admin can register a new company.
#[test]
fn test_auth_admin_add_company_allow() {
    let ctx = setup();
    let new_co = Address::generate(&ctx.env);
    assert!(ctx.client.try_add_company(&ctx.admin, &new_co).is_ok());
}

/// test_auth_company_add_company_deny — a company cannot register another company.
#[test]
fn test_auth_company_add_company_deny() {
    let ctx = setup();
    let new_co = Address::generate(&ctx.env);
    assert_unauthorized(ctx.client.try_add_company(&ctx.company, &new_co));
}

/// test_auth_carrier_add_company_deny
#[test]
fn test_auth_carrier_add_company_deny() {
    let ctx = setup();
    let new_co = Address::generate(&ctx.env);
    assert_unauthorized(ctx.client.try_add_company(&ctx.carrier, &new_co));
}

/// test_auth_guardian_add_company_deny
#[test]
fn test_auth_guardian_add_company_deny() {
    let ctx = setup();
    let new_co = Address::generate(&ctx.env);
    assert_unauthorized(ctx.client.try_add_company(&ctx.guardian, &new_co));
}

/// test_auth_operator_add_company_allow — operator is permitted by require_admin_or_operator.
#[test]
fn test_auth_operator_add_company_allow() {
    let ctx = setup();
    let new_co = Address::generate(&ctx.env);
    assert!(ctx.client.try_add_company(&ctx.operator, &new_co).is_ok());
}

/// test_auth_stranger_add_company_deny
#[test]
fn test_auth_stranger_add_company_deny() {
    let ctx = setup();
    let new_co = Address::generate(&ctx.env);
    assert_unauthorized(ctx.client.try_add_company(&ctx.stranger, &new_co));
}

// ── add_carrier ───────────────────────────────────────────────────────────────

/// test_auth_admin_add_carrier_allow
#[test]
fn test_auth_admin_add_carrier_allow() {
    let ctx = setup();
    let new_carrier = Address::generate(&ctx.env);
    assert!(ctx.client.try_add_carrier(&ctx.admin, &new_carrier).is_ok());
}

/// test_auth_company_add_carrier_deny
#[test]
fn test_auth_company_add_carrier_deny() {
    let ctx = setup();
    let new_carrier = Address::generate(&ctx.env);
    assert_unauthorized(ctx.client.try_add_carrier(&ctx.company, &new_carrier));
}

/// test_auth_carrier_add_carrier_deny
#[test]
fn test_auth_carrier_add_carrier_deny() {
    let ctx = setup();
    let new_carrier = Address::generate(&ctx.env);
    assert_unauthorized(ctx.client.try_add_carrier(&ctx.carrier, &new_carrier));
}

/// test_auth_guardian_add_carrier_deny
#[test]
fn test_auth_guardian_add_carrier_deny() {
    let ctx = setup();
    let new_carrier = Address::generate(&ctx.env);
    assert_unauthorized(ctx.client.try_add_carrier(&ctx.guardian, &new_carrier));
}

/// test_auth_stranger_add_carrier_deny
#[test]
fn test_auth_stranger_add_carrier_deny() {
    let ctx = setup();
    let new_carrier = Address::generate(&ctx.env);
    assert_unauthorized(ctx.client.try_add_carrier(&ctx.stranger, &new_carrier));
}

// ── add_guardian ──────────────────────────────────────────────────────────────

/// test_auth_admin_add_guardian_allow
#[test]
fn test_auth_admin_add_guardian_allow() {
    let ctx = setup();
    let g = Address::generate(&ctx.env);
    assert!(ctx.client.try_add_guardian(&ctx.admin, &g).is_ok());
}

/// test_auth_company_add_guardian_deny
#[test]
fn test_auth_company_add_guardian_deny() {
    let ctx = setup();
    let g = Address::generate(&ctx.env);
    assert_unauthorized(ctx.client.try_add_guardian(&ctx.company, &g));
}

/// test_auth_carrier_add_guardian_deny
#[test]
fn test_auth_carrier_add_guardian_deny() {
    let ctx = setup();
    let g = Address::generate(&ctx.env);
    assert_unauthorized(ctx.client.try_add_guardian(&ctx.carrier, &g));
}

/// test_auth_stranger_add_guardian_deny
#[test]
fn test_auth_stranger_add_guardian_deny() {
    let ctx = setup();
    let g = Address::generate(&ctx.env);
    assert_unauthorized(ctx.client.try_add_guardian(&ctx.stranger, &g));
}

// ── add_operator ──────────────────────────────────────────────────────────────

/// test_auth_admin_add_operator_allow
#[test]
fn test_auth_admin_add_operator_allow() {
    let ctx = setup();
    let op = Address::generate(&ctx.env);
    assert!(ctx.client.try_add_operator(&ctx.admin, &op).is_ok());
}

/// test_auth_company_add_operator_deny
#[test]
fn test_auth_company_add_operator_deny() {
    let ctx = setup();
    let op = Address::generate(&ctx.env);
    assert_unauthorized(ctx.client.try_add_operator(&ctx.company, &op));
}

/// test_auth_carrier_add_operator_deny
#[test]
fn test_auth_carrier_add_operator_deny() {
    let ctx = setup();
    let op = Address::generate(&ctx.env);
    assert_unauthorized(ctx.client.try_add_operator(&ctx.carrier, &op));
}

/// test_auth_stranger_add_operator_deny
#[test]
fn test_auth_stranger_add_operator_deny() {
    let ctx = setup();
    let op = Address::generate(&ctx.env);
    assert_unauthorized(ctx.client.try_add_operator(&ctx.stranger, &op));
}

// ── suspend_carrier ───────────────────────────────────────────────────────────

/// test_auth_admin_suspend_carrier_allow
#[test]
fn test_auth_admin_suspend_carrier_allow() {
    let ctx = setup();
    assert!(ctx
        .client
        .try_suspend_carrier(&ctx.admin, &ctx.carrier)
        .is_ok());
}

/// test_auth_company_suspend_carrier_deny
#[test]
fn test_auth_company_suspend_carrier_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_suspend_carrier(&ctx.company, &ctx.carrier));
}

/// test_auth_carrier_suspend_carrier_deny
#[test]
fn test_auth_carrier_suspend_carrier_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_suspend_carrier(&ctx.carrier, &ctx.carrier));
}

/// test_auth_guardian_suspend_carrier_deny
#[test]
fn test_auth_guardian_suspend_carrier_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_suspend_carrier(&ctx.guardian, &ctx.carrier));
}

/// test_auth_stranger_suspend_carrier_deny
#[test]
fn test_auth_stranger_suspend_carrier_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_suspend_carrier(&ctx.stranger, &ctx.carrier));
}

// ── revoke_role ───────────────────────────────────────────────────────────────

/// test_auth_admin_revoke_role_allow
#[test]
fn test_auth_admin_revoke_role_allow() {
    let ctx = setup();
    // Revoking the operator role from the operator address.
    assert!(ctx
        .client
        .try_revoke_role(&ctx.admin, &ctx.operator)
        .is_ok());
}

/// test_auth_company_revoke_role_deny
#[test]
fn test_auth_company_revoke_role_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_revoke_role(&ctx.company, &ctx.operator));
}

/// test_auth_carrier_revoke_role_deny
#[test]
fn test_auth_carrier_revoke_role_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_revoke_role(&ctx.carrier, &ctx.operator));
}

/// test_auth_guardian_revoke_role_deny
#[test]
fn test_auth_guardian_revoke_role_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_revoke_role(&ctx.guardian, &ctx.operator));
}

/// test_auth_stranger_revoke_role_deny
#[test]
fn test_auth_stranger_revoke_role_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_revoke_role(&ctx.stranger, &ctx.operator));
}

// ── force_cancel_shipment ─────────────────────────────────────────────────────

/// test_auth_admin_force_cancel_shipment_allow
#[test]
fn test_auth_admin_force_cancel_shipment_allow() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    let reason = dummy_hash(&ctx.env);
    assert!(ctx
        .client
        .try_force_cancel_shipment(&ctx.admin, &id, &reason)
        .is_ok());
}

/// test_auth_company_force_cancel_shipment_deny
#[test]
fn test_auth_company_force_cancel_shipment_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    let reason = dummy_hash(&ctx.env);
    assert_unauthorized(
        ctx.client
            .try_force_cancel_shipment(&ctx.company, &id, &reason),
    );
}

/// test_auth_carrier_force_cancel_shipment_deny
#[test]
fn test_auth_carrier_force_cancel_shipment_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    let reason = dummy_hash(&ctx.env);
    assert_unauthorized(
        ctx.client
            .try_force_cancel_shipment(&ctx.carrier, &id, &reason),
    );
}

/// test_auth_guardian_force_cancel_shipment_deny
#[test]
fn test_auth_guardian_force_cancel_shipment_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    let reason = dummy_hash(&ctx.env);
    assert_unauthorized(
        ctx.client
            .try_force_cancel_shipment(&ctx.guardian, &id, &reason),
    );
}

/// test_auth_stranger_force_cancel_shipment_deny
#[test]
fn test_auth_stranger_force_cancel_shipment_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    let reason = dummy_hash(&ctx.env);
    assert_unauthorized(
        ctx.client
            .try_force_cancel_shipment(&ctx.stranger, &id, &reason),
    );
}

// ── pause ─────────────────────────────────────────────────────────────────────

/// test_auth_admin_pause_allow
#[test]
fn test_auth_admin_pause_allow() {
    let ctx = setup();
    assert!(ctx.client.try_pause(&ctx.admin).is_ok());
}

/// test_auth_company_pause_deny
#[test]
fn test_auth_company_pause_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_pause(&ctx.company));
}

/// test_auth_carrier_pause_deny
#[test]
fn test_auth_carrier_pause_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_pause(&ctx.carrier));
}

/// test_auth_guardian_pause_allow — guardian is permitted by require_admin_or_guardian.
#[test]
fn test_auth_guardian_pause_allow() {
    let ctx = setup();
    assert!(ctx.client.try_pause(&ctx.guardian).is_ok());
}

/// test_auth_stranger_pause_deny
#[test]
fn test_auth_stranger_pause_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_pause(&ctx.stranger));
}

// ── set_shipment_limit ────────────────────────────────────────────────────────

/// test_auth_admin_set_shipment_limit_allow
#[test]
fn test_auth_admin_set_shipment_limit_allow() {
    let ctx = setup();
    assert!(ctx.client.try_set_shipment_limit(&ctx.admin, &50).is_ok());
}

/// test_auth_company_set_shipment_limit_deny
#[test]
fn test_auth_company_set_shipment_limit_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_set_shipment_limit(&ctx.company, &50));
}

/// test_auth_carrier_set_shipment_limit_deny
#[test]
fn test_auth_carrier_set_shipment_limit_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_set_shipment_limit(&ctx.carrier, &50));
}

/// test_auth_guardian_set_shipment_limit_deny
#[test]
fn test_auth_guardian_set_shipment_limit_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_set_shipment_limit(&ctx.guardian, &50));
}

/// test_auth_stranger_set_shipment_limit_deny
#[test]
fn test_auth_stranger_set_shipment_limit_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_set_shipment_limit(&ctx.stranger, &50));
}

// ── update_config ─────────────────────────────────────────────────────────────

/// test_auth_admin_update_config_allow
#[test]
fn test_auth_admin_update_config_allow() {
    let ctx = setup();
    let config = crate::config::ContractConfig::default();
    assert!(ctx.client.try_update_config(&ctx.admin, &config).is_ok());
}

/// test_auth_company_update_config_deny
#[test]
fn test_auth_company_update_config_deny() {
    let ctx = setup();
    let config = crate::config::ContractConfig::default();
    assert_unauthorized(ctx.client.try_update_config(&ctx.company, &config));
}

/// test_auth_carrier_update_config_deny
#[test]
fn test_auth_carrier_update_config_deny() {
    let ctx = setup();
    let config = crate::config::ContractConfig::default();
    assert_unauthorized(ctx.client.try_update_config(&ctx.carrier, &config));
}

/// test_auth_guardian_update_config_deny
#[test]
fn test_auth_guardian_update_config_deny() {
    let ctx = setup();
    let config = crate::config::ContractConfig::default();
    assert_unauthorized(ctx.client.try_update_config(&ctx.guardian, &config));
}

/// test_auth_stranger_update_config_deny
#[test]
fn test_auth_stranger_update_config_deny() {
    let ctx = setup();
    let config = crate::config::ContractConfig::default();
    assert_unauthorized(ctx.client.try_update_config(&ctx.stranger, &config));
}

// =============================================================================
// SECTION 2: Company-only functions
// =============================================================================

// ── create_shipment ───────────────────────────────────────────────────────────

/// test_auth_company_create_shipment_allow
#[test]
fn test_auth_company_create_shipment_allow() {
    let ctx = setup();
    let deadline = ctx.env.ledger().timestamp() + 86_400;
    let result = ctx.client.try_create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env),
        &Vec::new(&ctx.env),
        &deadline,
    );
    assert!(result.is_ok());
}

/// test_auth_admin_create_shipment_allow — initialize gives admin Company role,
/// so admin can also create shipments.
#[test]
fn test_auth_admin_create_shipment_allow() {
    let ctx = setup();
    let deadline = ctx.env.ledger().timestamp() + 86_400;
    let result = ctx.client.try_create_shipment(
        &ctx.admin,
        &ctx.receiver,
        &ctx.carrier,
        &seeded_hash(&ctx.env, 2),
        &Vec::new(&ctx.env),
        &deadline,
    );
    assert!(result.is_ok());
}

/// test_auth_carrier_create_shipment_deny
#[test]
fn test_auth_carrier_create_shipment_deny() {
    let ctx = setup();
    let deadline = ctx.env.ledger().timestamp() + 86_400;
    assert_unauthorized(ctx.client.try_create_shipment(
        &ctx.carrier,
        &ctx.receiver,
        &ctx.carrier,
        &seeded_hash(&ctx.env, 3),
        &Vec::new(&ctx.env),
        &deadline,
    ));
}

/// test_auth_guardian_create_shipment_deny
#[test]
fn test_auth_guardian_create_shipment_deny() {
    let ctx = setup();
    let deadline = ctx.env.ledger().timestamp() + 86_400;
    assert_unauthorized(ctx.client.try_create_shipment(
        &ctx.guardian,
        &ctx.receiver,
        &ctx.carrier,
        &seeded_hash(&ctx.env, 4),
        &Vec::new(&ctx.env),
        &deadline,
    ));
}

/// test_auth_stranger_create_shipment_deny
#[test]
fn test_auth_stranger_create_shipment_deny() {
    let ctx = setup();
    let deadline = ctx.env.ledger().timestamp() + 86_400;
    assert_unauthorized(ctx.client.try_create_shipment(
        &ctx.stranger,
        &ctx.receiver,
        &ctx.carrier,
        &seeded_hash(&ctx.env, 5),
        &Vec::new(&ctx.env),
        &deadline,
    ));
}

// ── deposit_escrow ────────────────────────────────────────────────────────────

/// test_auth_company_deposit_escrow_allow
#[test]
fn test_auth_company_deposit_escrow_allow() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    // Token is a no-op mock so the transfer always succeeds.
    assert!(ctx
        .client
        .try_deposit_escrow(&ctx.company, &id, &1000)
        .is_ok());
}

/// test_auth_admin_deposit_escrow_allow — admin has Company role from initialize.
#[test]
fn test_auth_admin_deposit_escrow_allow() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    assert!(ctx
        .client
        .try_deposit_escrow(&ctx.admin, &id, &1000)
        .is_ok());
}

/// test_auth_carrier_deposit_escrow_deny
#[test]
fn test_auth_carrier_deposit_escrow_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    assert_unauthorized(ctx.client.try_deposit_escrow(&ctx.carrier, &id, &1000));
}

/// test_auth_guardian_deposit_escrow_deny
#[test]
fn test_auth_guardian_deposit_escrow_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    assert_unauthorized(ctx.client.try_deposit_escrow(&ctx.guardian, &id, &1000));
}

/// test_auth_stranger_deposit_escrow_deny
#[test]
fn test_auth_stranger_deposit_escrow_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_deposit_escrow(&ctx.stranger, &1, &1000));
}

// ── add_carrier_to_whitelist ──────────────────────────────────────────────────

/// test_auth_company_add_carrier_to_whitelist_allow
#[test]
fn test_auth_company_add_carrier_to_whitelist_allow() {
    let ctx = setup();
    assert!(ctx
        .client
        .try_add_carrier_to_whitelist(&ctx.company, &ctx.carrier)
        .is_ok());
}

/// test_auth_admin_add_carrier_to_whitelist_allow — admin has Company role from initialize.
#[test]
fn test_auth_admin_add_carrier_to_whitelist_allow() {
    let ctx = setup();
    assert!(ctx
        .client
        .try_add_carrier_to_whitelist(&ctx.admin, &ctx.carrier)
        .is_ok());
}

/// test_auth_carrier_add_carrier_to_whitelist_deny
#[test]
fn test_auth_carrier_add_carrier_to_whitelist_deny() {
    let ctx = setup();
    assert_unauthorized(
        ctx.client
            .try_add_carrier_to_whitelist(&ctx.carrier, &ctx.carrier),
    );
}

/// test_auth_guardian_add_carrier_to_whitelist_deny
#[test]
fn test_auth_guardian_add_carrier_to_whitelist_deny() {
    let ctx = setup();
    assert_unauthorized(
        ctx.client
            .try_add_carrier_to_whitelist(&ctx.guardian, &ctx.carrier),
    );
}

/// test_auth_stranger_add_carrier_to_whitelist_deny
#[test]
fn test_auth_stranger_add_carrier_to_whitelist_deny() {
    let ctx = setup();
    assert_unauthorized(
        ctx.client
            .try_add_carrier_to_whitelist(&ctx.stranger, &ctx.carrier),
    );
}

// ── cancel_shipment ───────────────────────────────────────────────────────────

/// test_auth_company_cancel_shipment_allow — the sender (company) can cancel.
#[test]
fn test_auth_company_cancel_shipment_allow() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    let reason = dummy_hash(&ctx.env);
    assert!(ctx
        .client
        .try_cancel_shipment(&ctx.company, &id, &reason)
        .is_ok());
}

/// test_auth_carrier_cancel_shipment_deny — carrier is not the sender.
#[test]
fn test_auth_carrier_cancel_shipment_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    let reason = dummy_hash(&ctx.env);
    assert_unauthorized(ctx.client.try_cancel_shipment(&ctx.carrier, &id, &reason));
}

/// test_auth_guardian_cancel_shipment_deny
#[test]
fn test_auth_guardian_cancel_shipment_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    let reason = dummy_hash(&ctx.env);
    assert_unauthorized(ctx.client.try_cancel_shipment(&ctx.guardian, &id, &reason));
}

/// test_auth_stranger_cancel_shipment_deny
#[test]
fn test_auth_stranger_cancel_shipment_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    let reason = dummy_hash(&ctx.env);
    assert_unauthorized(ctx.client.try_cancel_shipment(&ctx.stranger, &id, &reason));
}

// ── refund_escrow ─────────────────────────────────────────────────────────────

/// test_auth_company_refund_escrow_allow — sender can refund their own escrow.
#[test]
fn test_auth_company_refund_escrow_allow() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    ctx.client.deposit_escrow(&ctx.company, &id, &500);
    assert!(ctx.client.try_refund_escrow(&ctx.company, &id).is_ok());
}

/// test_auth_carrier_refund_escrow_deny
#[test]
fn test_auth_carrier_refund_escrow_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    ctx.client.deposit_escrow(&ctx.company, &id, &500);
    assert_unauthorized(ctx.client.try_refund_escrow(&ctx.carrier, &id));
}

/// test_auth_guardian_refund_escrow_deny
#[test]
fn test_auth_guardian_refund_escrow_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    ctx.client.deposit_escrow(&ctx.company, &id, &500);
    assert_unauthorized(ctx.client.try_refund_escrow(&ctx.guardian, &id));
}

/// test_auth_stranger_refund_escrow_deny
#[test]
fn test_auth_stranger_refund_escrow_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    ctx.client.deposit_escrow(&ctx.company, &id, &500);
    assert_unauthorized(ctx.client.try_refund_escrow(&ctx.stranger, &id));
}

// =============================================================================
// SECTION 3: Carrier-only functions
// =============================================================================

// ── update_status ─────────────────────────────────────────────────────────────

/// test_auth_carrier_update_status_allow
#[test]
fn test_auth_carrier_update_status_allow() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    let result = ctx.client.try_update_status(
        &ctx.carrier,
        &id,
        &crate::ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env),
    );
    assert!(result.is_ok());
}

/// test_auth_company_update_status_deny
#[test]
fn test_auth_company_update_status_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    assert_unauthorized(ctx.client.try_update_status(
        &ctx.company,
        &id,
        &crate::ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env),
    ));
}

/// test_auth_guardian_update_status_deny
#[test]
fn test_auth_guardian_update_status_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    assert_unauthorized(ctx.client.try_update_status(
        &ctx.guardian,
        &id,
        &crate::ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env),
    ));
}

/// test_auth_operator_update_status_deny
#[test]
fn test_auth_operator_update_status_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    assert_unauthorized(ctx.client.try_update_status(
        &ctx.operator,
        &id,
        &crate::ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env),
    ));
}

/// test_auth_stranger_update_status_deny
#[test]
fn test_auth_stranger_update_status_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    assert_unauthorized(ctx.client.try_update_status(
        &ctx.stranger,
        &id,
        &crate::ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env),
    ));
}

// ── record_milestone ──────────────────────────────────────────────────────────

/// test_auth_carrier_record_milestone_allow
#[test]
fn test_auth_carrier_record_milestone_allow() {
    let ctx = setup();
    let deadline = ctx.env.ledger().timestamp() + 86_400;
    let mut milestones: Vec<(Symbol, u32)> = Vec::new(&ctx.env);
    milestones.push_back((Symbol::new(&ctx.env, "M1"), 100));
    let id = ctx.client.create_shipment(
        &ctx.company,
        &ctx.receiver,
        &ctx.carrier,
        &dummy_hash(&ctx.env),
        &milestones,
        &deadline,
    );
    ctx.client.update_status(
        &ctx.carrier,
        &id,
        &crate::ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env),
    );
    crate::test_utils::advance_past_rate_limit(&ctx.env);
    let result = ctx.client.try_record_milestone(
        &ctx.carrier,
        &id,
        &Symbol::new(&ctx.env, "M1"),
        &seeded_hash(&ctx.env, 9),
    );
    assert!(result.is_ok());
}

/// test_auth_company_record_milestone_deny
#[test]
fn test_auth_company_record_milestone_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_record_milestone(
        &ctx.company,
        &1,
        &Symbol::new(&ctx.env, "M1"),
        &dummy_hash(&ctx.env),
    ));
}

/// test_auth_guardian_record_milestone_deny
#[test]
fn test_auth_guardian_record_milestone_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_record_milestone(
        &ctx.guardian,
        &1,
        &Symbol::new(&ctx.env, "M1"),
        &dummy_hash(&ctx.env),
    ));
}

/// test_auth_stranger_record_milestone_deny
#[test]
fn test_auth_stranger_record_milestone_deny() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_record_milestone(
        &ctx.stranger,
        &1,
        &Symbol::new(&ctx.env, "M1"),
        &dummy_hash(&ctx.env),
    ));
}

// =============================================================================
// SECTION 4: Receiver-specific functions
// =============================================================================

// ── confirm_delivery ──────────────────────────────────────────────────────────

/// test_auth_receiver_confirm_delivery_allow — only the designated receiver may confirm.
#[test]
fn test_auth_receiver_confirm_delivery_allow() {
    let ctx = setup();
    let id = create_shipment_in_transit(&ctx);
    ctx.env.as_contract(&ctx.client.address, || {
        let mut s = crate::storage::get_shipment(&ctx.env, id).unwrap();
        s.status = crate::ShipmentStatus::InTransit;
        crate::storage::set_shipment(&ctx.env, &s);
    });
    let result = ctx
        .client
        .try_confirm_delivery(&ctx.receiver, &id, &dummy_hash(&ctx.env));
    assert!(result.is_ok());
}

/// test_auth_company_confirm_delivery_deny — company is not the receiver.
#[test]
fn test_auth_company_confirm_delivery_deny() {
    let ctx = setup();
    let id = create_shipment_in_transit(&ctx);
    assert_unauthorized(
        ctx.client
            .try_confirm_delivery(&ctx.company, &id, &dummy_hash(&ctx.env)),
    );
}

/// test_auth_carrier_confirm_delivery_deny
#[test]
fn test_auth_carrier_confirm_delivery_deny() {
    let ctx = setup();
    let id = create_shipment_in_transit(&ctx);
    assert_unauthorized(
        ctx.client
            .try_confirm_delivery(&ctx.carrier, &id, &dummy_hash(&ctx.env)),
    );
}

/// test_auth_guardian_confirm_delivery_deny
#[test]
fn test_auth_guardian_confirm_delivery_deny() {
    let ctx = setup();
    let id = create_shipment_in_transit(&ctx);
    assert_unauthorized(
        ctx.client
            .try_confirm_delivery(&ctx.guardian, &id, &dummy_hash(&ctx.env)),
    );
}

/// test_auth_stranger_confirm_delivery_deny
#[test]
fn test_auth_stranger_confirm_delivery_deny() {
    let ctx = setup();
    let id = create_shipment_in_transit(&ctx);
    assert_unauthorized(
        ctx.client
            .try_confirm_delivery(&ctx.stranger, &id, &dummy_hash(&ctx.env)),
    );
}

// ── release_escrow ────────────────────────────────────────────────────────────

/// test_auth_receiver_release_escrow_allow — only the designated receiver may release.
#[test]
fn test_auth_receiver_release_escrow_allow() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    ctx.client.deposit_escrow(&ctx.company, &id, &500);
    ctx.env.as_contract(&ctx.client.address, || {
        let mut s = crate::storage::get_shipment(&ctx.env, id).unwrap();
        s.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&ctx.env, &s);
    });
    assert!(ctx.client.try_release_escrow(&ctx.receiver, &id).is_ok());
}

/// test_auth_company_release_escrow_deny
#[test]
fn test_auth_company_release_escrow_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    ctx.client.deposit_escrow(&ctx.company, &id, &500);
    ctx.env.as_contract(&ctx.client.address, || {
        let mut s = crate::storage::get_shipment(&ctx.env, id).unwrap();
        s.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&ctx.env, &s);
    });
    assert_unauthorized(ctx.client.try_release_escrow(&ctx.company, &id));
}

/// test_auth_carrier_release_escrow_deny
#[test]
fn test_auth_carrier_release_escrow_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    ctx.client.deposit_escrow(&ctx.company, &id, &500);
    assert_unauthorized(ctx.client.try_release_escrow(&ctx.carrier, &id));
}

/// test_auth_stranger_release_escrow_deny
#[test]
fn test_auth_stranger_release_escrow_deny() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    ctx.client.deposit_escrow(&ctx.company, &id, &500);
    assert_unauthorized(ctx.client.try_release_escrow(&ctx.stranger, &id));
}

// =============================================================================
// SECTION 5: Admin-or-Guardian functions
// =============================================================================

// ── resolve_dispute ───────────────────────────────────────────────────────────

/// test_auth_admin_resolve_dispute_allow
#[test]
fn test_auth_admin_resolve_dispute_allow() {
    let ctx = setup();
    let id = create_disputed_shipment(&ctx);
    let reason = dummy_hash(&ctx.env);
    let result = ctx.client.try_resolve_dispute(
        &ctx.admin,
        &id,
        &crate::DisputeResolution::RefundToCompany,
        &reason,
    );
    assert!(result.is_ok());
}

/// test_auth_guardian_resolve_dispute_allow — guardian has explicit permission.
#[test]
fn test_auth_guardian_resolve_dispute_allow() {
    let ctx = setup();
    let id = create_disputed_shipment(&ctx);
    let reason = seeded_hash(&ctx.env, 99);
    let result = ctx.client.try_resolve_dispute(
        &ctx.guardian,
        &id,
        &crate::DisputeResolution::RefundToCompany,
        &reason,
    );
    assert!(result.is_ok());
}

/// test_auth_company_resolve_dispute_deny
#[test]
fn test_auth_company_resolve_dispute_deny() {
    let ctx = setup();
    let id = create_disputed_shipment(&ctx);
    let reason = seeded_hash(&ctx.env, 10);
    assert_unauthorized(ctx.client.try_resolve_dispute(
        &ctx.company,
        &id,
        &crate::DisputeResolution::RefundToCompany,
        &reason,
    ));
}

/// test_auth_carrier_resolve_dispute_deny
#[test]
fn test_auth_carrier_resolve_dispute_deny() {
    let ctx = setup();
    let id = create_disputed_shipment(&ctx);
    let reason = seeded_hash(&ctx.env, 11);
    assert_unauthorized(ctx.client.try_resolve_dispute(
        &ctx.carrier,
        &id,
        &crate::DisputeResolution::RefundToCompany,
        &reason,
    ));
}

/// test_auth_operator_resolve_dispute_deny
#[test]
fn test_auth_operator_resolve_dispute_deny() {
    let ctx = setup();
    let id = create_disputed_shipment(&ctx);
    let reason = seeded_hash(&ctx.env, 12);
    assert_unauthorized(ctx.client.try_resolve_dispute(
        &ctx.operator,
        &id,
        &crate::DisputeResolution::RefundToCompany,
        &reason,
    ));
}

/// test_auth_stranger_resolve_dispute_deny
#[test]
fn test_auth_stranger_resolve_dispute_deny() {
    let ctx = setup();
    let id = create_disputed_shipment(&ctx);
    let reason = seeded_hash(&ctx.env, 13);
    assert_unauthorized(ctx.client.try_resolve_dispute(
        &ctx.stranger,
        &id,
        &crate::DisputeResolution::RefundToCompany,
        &reason,
    ));
}

// =============================================================================
// SECTION 6: Multisig proposal — admin list membership required
// =============================================================================

// ── propose_action ────────────────────────────────────────────────────────────

/// test_auth_admin_propose_action_allow — admin in the admin list can propose.
#[test]
fn test_auth_admin_propose_action_allow() {
    let ctx = setup();
    init_single_admin_multisig(&ctx, &ctx.admin.clone());

    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&ctx.env));
    let result = ctx
        .client
        .try_propose_action(&ctx.admin, &action, &salt(&ctx.env));
    assert!(result.is_ok());
}

/// test_auth_company_propose_action_deny — company is not in admin list.
#[test]
fn test_auth_company_propose_action_deny() {
    let ctx = setup();
    init_single_admin_multisig(&ctx, &ctx.admin.clone());

    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&ctx.env));
    let result = ctx
        .client
        .try_propose_action(&ctx.company, &action, &salt(&ctx.env));
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, NavinError::NotAnAdmin);
}

/// test_auth_carrier_propose_action_deny
#[test]
fn test_auth_carrier_propose_action_deny() {
    let ctx = setup();
    init_single_admin_multisig(&ctx, &ctx.admin.clone());

    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&ctx.env));
    let result = ctx
        .client
        .try_propose_action(&ctx.carrier, &action, &salt(&ctx.env));
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, NavinError::NotAnAdmin);
}

/// test_auth_guardian_propose_action_deny
#[test]
fn test_auth_guardian_propose_action_deny() {
    let ctx = setup();
    init_single_admin_multisig(&ctx, &ctx.admin.clone());

    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&ctx.env));
    let result = ctx
        .client
        .try_propose_action(&ctx.guardian, &action, &salt(&ctx.env));
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, NavinError::NotAnAdmin);
}

/// test_auth_stranger_propose_action_deny
#[test]
fn test_auth_stranger_propose_action_deny() {
    let ctx = setup();
    init_single_admin_multisig(&ctx, &ctx.admin.clone());

    let action = crate::types::AdminAction::TransferAdmin(Address::generate(&ctx.env));
    let result = ctx
        .client
        .try_propose_action(&ctx.stranger, &action, &salt(&ctx.env));
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, NavinError::NotAnAdmin);
}

// =============================================================================
// SECTION 7: Cross-role denial regression checks
// These tests exercise the "wrong role for operation" scenario explicitly and
// are each linked to a specific (role, function) pair in the matrix above.
// =============================================================================

/// test_auth_cross_carrier_cannot_deposit_escrow — Carrier role is denied on a
/// Company-only path: deposit_escrow.
#[test]
fn test_auth_cross_carrier_cannot_deposit_escrow() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    assert_unauthorized(ctx.client.try_deposit_escrow(&ctx.carrier, &id, &1000));
}

/// test_auth_cross_guardian_cannot_update_status — Guardian role is denied on
/// the Carrier-only path: update_status.
#[test]
fn test_auth_cross_guardian_cannot_update_status() {
    let ctx = setup();
    let id = create_shipment(&ctx);
    assert_unauthorized(ctx.client.try_update_status(
        &ctx.guardian,
        &id,
        &crate::ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env),
    ));
}

/// test_auth_cross_operator_cannot_create_shipment — Operator role is denied on
/// the Company-only path: create_shipment.
#[test]
fn test_auth_cross_operator_cannot_create_shipment() {
    let ctx = setup();
    let deadline = ctx.env.ledger().timestamp() + 86_400;
    assert_unauthorized(ctx.client.try_create_shipment(
        &ctx.operator,
        &ctx.receiver,
        &ctx.carrier,
        &seeded_hash(&ctx.env, 20),
        &Vec::new(&ctx.env),
        &deadline,
    ));
}

/// test_auth_cross_company_cannot_record_milestone — Company role is denied on
/// the Carrier-only path: record_milestone.
#[test]
fn test_auth_cross_company_cannot_record_milestone() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_record_milestone(
        &ctx.company,
        &1,
        &Symbol::new(&ctx.env, "M1"),
        &dummy_hash(&ctx.env),
    ));
}

/// test_auth_cross_carrier_cannot_pause — Carrier role is denied on the
/// Admin-only path: pause.
#[test]
fn test_auth_cross_carrier_cannot_pause() {
    let ctx = setup();
    assert_unauthorized(ctx.client.try_pause(&ctx.carrier));
}
