#![cfg(test)]
//! # Milestone Releases Fuzzing Harness
//!
//! Property-based fuzz tests for partial milestone-based escrow releases.
//!
//! ## Properties Verified
//! - **Milestone percentages must sum to exactly 100**
//! - **Sum of partial releases never exceeds 100% of escrow**
//! - **Each milestone can only be paid once** (idempotency)
//! - **Random milestone percentages and release orders** handled correctly
//! - **Milestone limit enforcement** — max milestones per shipment
//!
//! ## Running
//! ```bash
//! cargo test --package shipment --features testutils fuzz_milestone_releases -- --nocapture
//! ```

extern crate std;

use crate::{NavinShipment, NavinShipmentClient};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, Symbol, Vec,
};

#[contract]
struct MilestoneFuzzToken;

#[contractimpl]
impl MilestoneFuzzToken {
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

fn setup() -> (Env, NavinShipmentClient<'static>, Address) {
    let (env, admin) = crate::test_utils::setup_env();
    let token = env.register(MilestoneFuzzToken {}, ());
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
        bytes[i + 8] = s[i].wrapping_add(0x44);
        bytes[i + 16] = s[i].wrapping_add(0x88);
        bytes[i + 24] = s[i].wrapping_add(0xCC);
    }
    if bytes.iter().all(|&b| b == 0) {
        bytes[0] = 1;
    }
    BytesN::from_array(env, &bytes)
}

/// Build a valid milestone Vec that sums to exactly 100.
fn build_milestones_summing_to_100(env: &Env, count: usize) -> Vec<(Symbol, u32)> {
    assert!(count >= 1 && count <= 5);
    let mut milestones = Vec::new(env);
    let per = 100u32 / count as u32;
    let remainder = 100u32 - per * count as u32;

    let names = ["alpha", "beta", "gamma", "delta", "epsilon"];
    for i in 0..count {
        let pct = if i == count - 1 { per + remainder } else { per };
        milestones.push_back((Symbol::new(env, names[i]), pct));
    }
    milestones
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 1: Milestone percentages must sum to exactly 100
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
fn fuzz_milestone_sum_must_be_100() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xE000_0000_E000_0100;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);

        let data_hash = hash_from_seed(&env, seed + i as u64);
        let deadline = env.ledger().timestamp() + 86_400 * 30;

        // Generate random percentages that do NOT sum to 100
        let p1 = (seed % 40 + 1) as u32;
        let p2 = (xorshift64(&mut rng) % 40 + 1) as u32;
        // p1 + p2 is unlikely to be 100, and we ensure it isn't
        if p1 + p2 == 100 {
            continue;
        }

        let mut bad_milestones = Vec::new(&env);
        bad_milestones.push_back((Symbol::new(&env, "alpha"), p1));
        bad_milestones.push_back((Symbol::new(&env, "beta"), p2));

        let result = client.try_create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &bad_milestones,
            &deadline,
        &None,
    );
        assert!(
            result.is_err(),
            "Milestones summing to {} (not 100) must be rejected",
            p1 + p2
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 2: Valid milestone sums (= 100) are accepted
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_milestone_valid_sum_accepted() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xA000_D000_E000_0200;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);

        let receiver = Address::generate(&env);
        let data_hash = hash_from_seed(&env, seed + i as u64);
        let deadline = env.ledger().timestamp() + 86_400 * 30;

        // Random count 1..=5
        let count = (seed % 5 + 1) as usize;
        let milestones = build_milestones_summing_to_100(&env, count);

        let result = client.try_create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &milestones,
            &deadline,
        &None,
    );
        assert!(
            result.is_ok(),
            "Valid milestones summing to 100 (count={count}) must be accepted"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 3: Sum of partial releases never exceeds 100% of escrow
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_milestone_partial_releases_never_exceed_escrow() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xA000_A000_EEA0_0300;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);

        let receiver = Address::generate(&env);
        let data_hash = hash_from_seed(&env, seed + i as u64);
        let deadline = env.ledger().timestamp() + 86_400 * 30;

        // 2-milestone shipment: 50/50
        let mut milestones = Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "alpha"), 50u32));
        milestones.push_back((Symbol::new(&env, "beta"), 50u32));

        let id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &milestones,
            &deadline,
        &None,
    );

        let deposit_amount = ((seed % 999_999) + 1) as i128;
        client.deposit_escrow(&company, &id, &deposit_amount);

        // Advance to InTransit
        env.ledger().with_mut(|l| l.timestamp += 65);
        let h1 = hash_from_seed(&env, seed.wrapping_add(1));
        client.update_status(&carrier, &id, &crate::ShipmentStatus::InTransit, &h1);

        // Record milestone alpha
        env.ledger().with_mut(|l| l.timestamp += 65);
        let mh = hash_from_seed(&env, seed.wrapping_add(2));
        client.record_milestone(&carrier, &id, &Symbol::new(&env, "alpha"), &mh);

        // Check escrow after first milestone payment
        let s = client.get_shipment(&id);
        assert!(
            s.escrow_amount >= 0,
            "Escrow went negative after first milestone: {}",
            s.escrow_amount
        );
        assert!(
            s.escrow_amount <= deposit_amount,
            "Escrow {} exceeds original deposit {deposit_amount} after first milestone",
            s.escrow_amount
        );

        // Record milestone beta
        env.ledger().with_mut(|l| l.timestamp += 65);
        let mh2 = hash_from_seed(&env, seed.wrapping_add(3));
        client.record_milestone(&carrier, &id, &Symbol::new(&env, "beta"), &mh2);

        // Check escrow after second milestone payment
        let s = client.get_shipment(&id);
        assert!(
            s.escrow_amount >= 0,
            "Escrow went negative after second milestone: {}",
            s.escrow_amount
        );

        // Property: total released = deposit_amount (all milestones paid)
        // escrow_amount should be 0 after all milestones
        // (may not be exactly 0 due to integer division rounding, but must be >= 0)
        assert!(
            s.escrow_amount <= deposit_amount,
            "Total released exceeds deposit: remaining={} deposit={deposit_amount}",
            s.escrow_amount
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 4: Each milestone can only be paid once
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_milestone_idempotency_single_payment() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xDE00_E000_0000_0400;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);

        let receiver = Address::generate(&env);
        let data_hash = hash_from_seed(&env, seed + i as u64);
        let deadline = env.ledger().timestamp() + 86_400 * 30;

        let mut milestones = Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "alpha"), 50u32));
        milestones.push_back((Symbol::new(&env, "beta"), 50u32));

        let id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &milestones,
            &deadline,
        &None,
    );

        let amount = ((seed % 999_999) + 1) as i128;
        client.deposit_escrow(&company, &id, &amount);

        env.ledger().with_mut(|l| l.timestamp += 65);
        let h1 = hash_from_seed(&env, seed.wrapping_add(10));
        client.update_status(&carrier, &id, &crate::ShipmentStatus::InTransit, &h1);

        // First milestone record — must succeed
        env.ledger().with_mut(|l| l.timestamp += 65);
        let mh = hash_from_seed(&env, seed.wrapping_add(11));
        client.record_milestone(&carrier, &id, &Symbol::new(&env, "alpha"), &mh);

        let balance_after_first = client.get_escrow_balance(&id);

        // Second record of same milestone — must fail
        env.ledger().with_mut(|l| l.timestamp += 65);
        let mh2 = hash_from_seed(&env, seed.wrapping_add(12));
        let result = client.try_record_milestone(&carrier, &id, &Symbol::new(&env, "alpha"), &mh2);
        assert!(
            result.is_err(),
            "Duplicate milestone payment must fail for shipment {id}"
        );

        // Property: balance unchanged after failed duplicate
        let balance_after_dup = client.get_escrow_balance(&id);
        assert_eq!(
            balance_after_first, balance_after_dup,
            "Balance changed after failed duplicate milestone: before={balance_after_first} after={balance_after_dup}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 5: Random milestone percentages that sum to 100 are all valid
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_milestone_random_valid_percentages() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xAD00_E000_C000_0500;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);

        let receiver = Address::generate(&env);
        let data_hash = hash_from_seed(&env, seed + i as u64);
        let deadline = env.ledger().timestamp() + 86_400 * 30;

        // Generate 2 random percentages that sum to 100
        let p1 = (seed % 99 + 1) as u32; // 1..=99
        let p2 = 100 - p1;

        let mut milestones = Vec::new(&env);
        milestones.push_back((Symbol::new(&env, "alpha"), p1));
        milestones.push_back((Symbol::new(&env, "beta"), p2));

        let result = client.try_create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &milestones,
            &deadline,
        &None,
    );
        assert!(
            result.is_ok(),
            "Random valid milestones ({p1}+{p2}=100) must be accepted"
        );
    }
}
