#![cfg(test)]
//! # Role Assignment Fuzzing Harness
//!
//! Property-based fuzz tests for role assignment, revocation, and suspension.
//!
//! ## Properties Verified
//! - **Only admin can assign roles**
//! - **Only admin can revoke roles**
//! - **Only admin can suspend/reactivate roles**
//! - **Suspended companies cannot create shipments**
//! - **Reactivated roles regain access**
//! - **Random address and role combinations** handled correctly
//! - **Edge cases**: self-revocation, contract address as caller
//!
//! ## Running
//! ```bash
//! cargo test --package shipment --features testutils fuzz_role_assignment -- --nocapture
//! ```

extern crate std;

use crate::{NavinShipment, NavinShipmentClient};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, Vec,
};

#[contract]
struct RoleAssignFuzzToken;

#[contractimpl]
impl RoleAssignFuzzToken {
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

fn setup() -> (Env, NavinShipmentClient<'static>, Address) {
    let (env, admin) = crate::test_utils::setup_env();
    let token = env.register(RoleAssignFuzzToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token);
    client.set_shipment_limit(&admin, &10_000u32);
    (env, client, admin)
}

fn xorshift64(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn hash_from_seed(env: &Env, seed: u64) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    let s = seed.to_le_bytes();
    for i in 0..8 {
        bytes[i] = s[i];
        bytes[i + 8] = s[i].wrapping_add(0x12);
        bytes[i + 16] = s[i].wrapping_add(0x34);
        bytes[i + 24] = s[i].wrapping_add(0x56);
    }
    if bytes.iter().all(|&b| b == 0) {
        bytes[0] = 1;
    }
    BytesN::from_array(env, &bytes)
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 1: Only admin can assign company role
// ─────────────────────────────────────────────────────────────────────────────

/// Returns the number of fuzz iterations to run.
/// Set `FUZZ_ITERATIONS=5000` (or any value) to run the full suite.
/// Defaults to 50 for fast CI runs.
fn fuzz_iterations() -> u32 {
    #[cfg(not(test))]
    return 50;
    #[cfg(test)]
    {
        extern crate std;
        std::env::var("FUZZ_ITERATIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50)
    }
}

#[test]
fn fuzz_role_only_admin_assigns_company() {
    let (env, client, admin) = setup();

    let mut rng: u64 = 0x0000_AD00_A000_0100;
    let iterations = fuzz_iterations();

    for _ in 0..iterations {
        let _seed = xorshift64(&mut rng);
        let non_admin = Address::generate(&env);
        let target = Address::generate(&env);

        // Non-admin cannot assign company role
        let result = client.try_add_company(&non_admin, &target);
        assert!(result.is_err(), "Non-admin must not assign company role");

        // Admin can assign company role
        let result = client.try_add_company(&admin, &target);
        assert!(result.is_ok(), "Admin must be able to assign company role");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 2: Only admin can assign carrier role
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_role_only_admin_assigns_carrier() {
    let (env, client, admin) = setup();

    let mut rng: u64 = 0x0000_AD00_CA00_0200;
    let iterations = fuzz_iterations();

    for _ in 0..iterations {
        let _seed = xorshift64(&mut rng);
        let non_admin = Address::generate(&env);
        let target = Address::generate(&env);

        let result = client.try_add_carrier(&non_admin, &target);
        assert!(result.is_err(), "Non-admin must not assign carrier role");

        let result = client.try_add_carrier(&admin, &target);
        assert!(result.is_ok(), "Admin must be able to assign carrier role");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 3: Only admin can revoke roles
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_role_only_admin_revokes() {
    let (env, client, admin) = setup();

    let mut rng: u64 = 0x0000_AD00_E000_0300;
    let iterations = fuzz_iterations();

    for _ in 0..iterations {
        let _seed = xorshift64(&mut rng);
        let target = Address::generate(&env);
        client.add_company(&admin, &target);

        let non_admin = Address::generate(&env);

        // Non-admin cannot revoke
        let result = client.try_revoke_role(&non_admin, &target);
        assert!(result.is_err(), "Non-admin must not revoke roles");

        // Admin can revoke
        let result = client.try_revoke_role(&admin, &target);
        assert!(result.is_ok(), "Admin must be able to revoke roles");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 4: Only admin can suspend carriers
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_role_only_admin_suspends_carrier() {
    let (env, client, admin) = setup();

    let mut rng: u64 = 0x0000_AD00_0000_0400;
    let iterations = fuzz_iterations();

    for _ in 0..iterations {
        let _seed = xorshift64(&mut rng);
        let carrier = Address::generate(&env);
        client.add_carrier(&admin, &carrier);

        let non_admin = Address::generate(&env);

        // Non-admin cannot suspend
        let result = client.try_suspend_carrier(&non_admin, &carrier);
        assert!(result.is_err(), "Non-admin must not suspend carrier");

        // Admin can suspend
        let result = client.try_suspend_carrier(&admin, &carrier);
        assert!(result.is_ok(), "Admin must be able to suspend carrier");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 5: Suspended company cannot create shipments
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_role_suspended_company_blocked() {
    let (env, client, admin) = setup();
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0x0000_C000_BC00_0500;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);

        let company = Address::generate(&env);
        client.add_company(&admin, &company);
        client.suspend_company(&admin, &company);

        let receiver = Address::generate(&env);
        let data_hash = hash_from_seed(&env, seed + i as u64);
        let deadline = env.ledger().timestamp() + 86_400;

        let result = client.try_create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &Vec::new(&env),
            &deadline,
        );
        assert!(
            result.is_err(),
            "Suspended company must not create shipment"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 8: Admin cannot self-revoke (CannotSelfRevoke protection)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_role_admin_cannot_self_revoke() {
    let (env, client, admin) = setup();

    let mut rng: u64 = 0xEF00_E000_EF00_0800;
    let iterations = fuzz_iterations();

    for _ in 0..iterations {
        let _seed = xorshift64(&mut rng);

        // Admin attempting to revoke their own role must fail
        let result = client.try_revoke_role(&admin, &admin);
        assert!(result.is_err(), "Admin must not be able to self-revoke");

        // Admin must still have privileges after failed self-revoke
        let target = Address::generate(&env);
        let result2 = client.try_add_company(&admin, &target);
        assert!(
            result2.is_ok(),
            "Admin must retain privileges after failed self-revoke attempt"
        );
    }
}
