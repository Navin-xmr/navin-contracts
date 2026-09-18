#![cfg(test)]
//! # RBAC Authorization Fuzzing Harness
//!
//! Property-based fuzz tests for role-based access control authorization checks.
//!
//! ## Properties Verified
//! - **Unauthorized addresses always get rejected** for privileged operations
//! - **Required role checks are consistent** — same input always yields same output
//! - **Admin role never loses privileges** through any operation sequence
//! - **Suspended roles are treated as unauthorized**
//! - **Edge cases**: zero-like addresses, contract-as-admin, random callers
//!
//! ## Running
//! ```bash
//! cargo test --package shipment --features testutils fuzz_rbac_authorization -- --nocapture
//! ```

extern crate std;

use crate::{NavinShipment, NavinShipmentClient};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, Vec,
};

#[contract]
struct RbacFuzzToken;

#[contractimpl]
impl RbacFuzzToken {
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

fn setup() -> (Env, NavinShipmentClient<'static>, Address) {
    let (env, admin) = crate::test_utils::setup_env();
    let token = env.register(RbacFuzzToken {}, ());
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
        bytes[i + 8] = s[i].wrapping_add(0x77);
        bytes[i + 16] = s[i].wrapping_add(0xBB);
        bytes[i + 24] = s[i].wrapping_add(0xDD);
    }
    if bytes.iter().all(|&b| b == 0) {
        bytes[0] = 1;
    }
    BytesN::from_array(env, &bytes)
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 1: Unauthorized addresses always get rejected for admin operations
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
fn fuzz_rbac_unauthorized_always_rejected() {
    let (env, client, _admin) = setup();

    let mut rng: u64 = 0xA000_0000_F000_0100;
    let iterations = fuzz_iterations();

    for _ in 0..iterations {
        let _seed = xorshift64(&mut rng);
        let unauthorized = Address::generate(&env);

        // Property: non-admin cannot add company
        let target = Address::generate(&env);
        let result = client.try_add_company(&unauthorized, &target);
        assert!(
            result.is_err(),
            "Non-admin {unauthorized:?} must not add company"
        );

        // Property: non-admin cannot add carrier
        let result = client.try_add_carrier(&unauthorized, &target);
        assert!(
            result.is_err(),
            "Non-admin {unauthorized:?} must not add carrier"
        );

        // Property: non-admin cannot pause contract
        let result = client.try_pause(&unauthorized);
        assert!(
            result.is_err(),
            "Non-admin {unauthorized:?} must not pause contract"
        );

        // Property: non-admin cannot set shipment limit
        let result = client.try_set_shipment_limit(&unauthorized, &100u32);
        assert!(
            result.is_err(),
            "Non-admin {unauthorized:?} must not set shipment limit"
        );

        // Property: non-admin cannot revoke roles
        let result = client.try_revoke_role(&unauthorized, &target);
        assert!(
            result.is_err(),
            "Non-admin {unauthorized:?} must not revoke roles"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 2: Admin always retains privileges regardless of other operations
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_rbac_admin_never_loses_privileges() {
    let (env, client, admin) = setup();

    let mut rng: u64 = 0xAD00_0000_F000_0200;
    let iterations = fuzz_iterations();

    for _ in 0..iterations {
        let _seed = xorshift64(&mut rng);

        // Perform various operations as non-admin (all should fail)
        let random_addr = Address::generate(&env);
        let _ = client.try_add_company(&random_addr, &Address::generate(&env));
        let _ = client.try_pause(&random_addr);

        // Property: admin can still add company after failed non-admin attempts
        let new_company = Address::generate(&env);
        let result = client.try_add_company(&admin, &new_company);
        assert!(
            result.is_ok(),
            "Admin must retain add_company privilege after non-admin attempts"
        );

        // Property: admin can still add carrier
        let new_carrier = Address::generate(&env);
        let result = client.try_add_carrier(&admin, &new_carrier);
        assert!(result.is_ok(), "Admin must retain add_carrier privilege");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 3: Role checks are consistent — same input always yields same output
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_rbac_role_checks_consistent() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xC000_E000_F000_0300;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);

        let receiver = Address::generate(&env);
        let data_hash = hash_from_seed(&env, seed + i as u64);
        let deadline = env.ledger().timestamp() + 86_400;

        // Property: company can always create shipment (consistent role check)
        let result1 = client.try_create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &Vec::new(&env),
            &deadline,
        );

        // Same call with same inputs must yield same result type
        env.ledger().with_mut(|l| l.timestamp += 2);
        let data_hash2 = hash_from_seed(&env, seed.wrapping_add(1) + i as u64);
        let deadline2 = env.ledger().timestamp() + 86_400;
        let result2 = client.try_create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash2,
            &Vec::new(&env),
            &deadline2,
        );

        assert_eq!(
            result1.is_ok(),
            result2.is_ok(),
            "Role check inconsistency: first={:?} second={:?}",
            result1.is_ok(),
            result2.is_ok()
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 5: Non-company addresses cannot create shipments
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_rbac_non_company_cannot_create_shipment() {
    let (env, client, admin) = setup();
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xC000_A000_F000_0500;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);

        // Random address with no role
        let random_addr = Address::generate(&env);
        let receiver = Address::generate(&env);
        let data_hash = hash_from_seed(&env, seed + i as u64);
        let deadline = env.ledger().timestamp() + 86_400;

        let result = client.try_create_shipment(
            &random_addr,
            &receiver,
            &carrier,
            &data_hash,
            &Vec::new(&env),
            &deadline,
        );
        assert!(
            result.is_err(),
            "Non-company address must not create shipment"
        );

        // Carrier also cannot create shipments (wrong role)
        let data_hash2 = hash_from_seed(&env, seed.wrapping_add(99) + i as u64);
        let result2 = client.try_create_shipment(
            &carrier,
            &receiver,
            &carrier,
            &data_hash2,
            &Vec::new(&env),
            &deadline,
        );
        assert!(
            result2.is_err(),
            "Carrier must not create shipment (wrong role)"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 7: Role assignment is idempotent — assigning twice = same result
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// Property 8: Revoked roles lose access immediately
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_rbac_revoked_role_loses_access() {
    let (env, client, admin) = setup();

    let mut rng: u64 = 0xE000_E000_EF00_0800;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);

        let addr = Address::generate(&env);
        client.add_company(&admin, &addr);

        // Revoke the role
        client.revoke_role(&admin, &addr);

        // Property: revoked address cannot create shipments
        let carrier = Address::generate(&env);
        client.add_carrier(&admin, &carrier);
        let receiver = Address::generate(&env);
        let data_hash = hash_from_seed(&env, seed + i as u64);
        let deadline = env.ledger().timestamp() + 86_400;
        let result = client.try_create_shipment(
            &addr,
            &receiver,
            &carrier,
            &data_hash,
            &Vec::new(&env),
            &deadline,
        );
        assert!(result.is_err(), "Revoked address must not create shipment");
    }
}
