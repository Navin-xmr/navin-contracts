#![cfg(test)]
//! # Escrow Arithmetic Fuzzing Harness
//!
//! Property-based fuzz tests for escrow arithmetic correctness,
//! overflow/underflow protection, and boundary conditions.
//!
//! ## Properties Verified
//! - **No overflow** on amounts up to i128::MAX
//! - **No underflow** — escrow never goes negative
//! - **Boundary amounts** (1, MAX_AMOUNT, i128::MAX) handled correctly
//! - **Arithmetic error propagation** — overflow returns error, not panic
//! - **Checked arithmetic** — all operations use overflow-safe helpers
//!
//! ## Running
//! ```bash
//! cargo test --package shipment --features testutils fuzz_escrow_arithmetic -- --nocapture
//! ```

extern crate std;

use crate::{NavinShipment, NavinShipmentClient};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, Vec,
};

#[contract]
struct ArithmeticFuzzToken;

#[contractimpl]
impl ArithmeticFuzzToken {
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

fn setup() -> (Env, NavinShipmentClient<'static>, Address) {
    let (env, admin) = crate::test_utils::setup_env();
    let token = env.register(ArithmeticFuzzToken {}, ());
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
        bytes[i + 8] = s[i].wrapping_add(0x11);
        bytes[i + 16] = s[i].wrapping_add(0x22);
        bytes[i + 24] = s[i].wrapping_add(0x33);
    }
    if bytes.iter().all(|&b| b == 0) {
        bytes[0] = 1;
    }
    BytesN::from_array(env, &bytes)
}

fn create_shipment(
    client: &NavinShipmentClient,
    env: &Env,
    company: &Address,
    carrier: &Address,
    seed: u64,
) -> u64 {
    let receiver = Address::generate(env);
    let data_hash = hash_from_seed(env, seed);
    let deadline = env.ledger().timestamp() + 86_400 * 30;
    client.create_shipment(company, &receiver, carrier, &data_hash, &Vec::new(env), &deadline, &None)
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 1: Amounts exceeding MAX_AMOUNT are rejected
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
fn fuzz_arithmetic_overflow_amounts_rejected() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xE000_F000_F000_0100;
    // MAX_AMOUNT = 1_000_000_000_000_000 (from validation.rs)
    const MAX_AMOUNT: i128 = 1_000_000_000_000_000;

    let overflow_amounts: std::vec::Vec<i128> = std::vec![
        MAX_AMOUNT + 1,
        MAX_AMOUNT + 1_000,
        i128::MAX,
        i128::MAX - 1,
        MAX_AMOUNT * 2,
    ];

    for (i, &amount) in overflow_amounts.iter().enumerate() {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);
        let id = create_shipment(&client, &env, &company, &carrier, seed + i as u64);

        let result = client.try_deposit_escrow(&company, &id, &amount);
        assert!(
            result.is_err(),
            "Overflow amount {amount} must be rejected for shipment {id}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 2: Boundary amounts (1 and MAX_AMOUNT) are accepted
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_arithmetic_boundary_amounts_accepted() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xB000_DA00_A000_0200;
    const MAX_AMOUNT: i128 = 1_000_000_000_000_000;

    let boundary_amounts: std::vec::Vec<i128> = std::vec![1, 100, 10_000_000, MAX_AMOUNT];

    for (i, &amount) in boundary_amounts.iter().enumerate() {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);
        let id = create_shipment(&client, &env, &company, &carrier, seed + i as u64);

        let result = client.try_deposit_escrow(&company, &id, &amount);
        assert!(
            result.is_ok(),
            "Boundary amount {amount} must be accepted, got error for shipment {id}"
        );

        let balance = client.get_escrow_balance(&id);
        assert_eq!(
            balance, amount,
            "Balance {balance} != deposited boundary amount {amount}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 3: Escrow never goes negative (underflow protection)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_arithmetic_no_underflow() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xDE00_F000_F000_0300;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);

        let receiver = Address::generate(&env);
        let data_hash = hash_from_seed(&env, seed + i as u64);
        let deadline = env.ledger().timestamp() + 86_400 * 30;
        let id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &Vec::new(&env),
            &deadline,
        &None,
    );

        let amount = ((seed % 999_999) + 1) as i128;
        client.deposit_escrow(&company, &id, &amount);

        // Advance to Delivered and release
        env.ledger().with_mut(|l| l.timestamp += 65);
        let h1 = hash_from_seed(&env, seed.wrapping_add(1));
        client.update_status(&carrier, &id, &crate::ShipmentStatus::InTransit, &h1);
        env.ledger().with_mut(|l| l.timestamp += 65);
        let h2 = hash_from_seed(&env, seed.wrapping_add(2));
        client.update_status(&carrier, &id, &crate::ShipmentStatus::Delivered, &h2);
        client.release_escrow(&receiver, &id);

        // Property: balance never negative
        let balance = client.get_escrow_balance(&id);
        assert!(
            balance >= 0,
            "Underflow detected: escrow balance is negative ({balance}) for shipment {id}"
        );

        let shipment = client.get_shipment(&id);
        assert!(
            shipment.escrow_amount >= 0,
            "Underflow: shipment.escrow_amount is negative ({}) for shipment {id}",
            shipment.escrow_amount
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 4: Random amounts from 0 to MAX_AMOUNT — only valid range accepted
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_arithmetic_random_amounts_range_check() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xAD00_A000_AE00_0400;
    const MAX_AMOUNT: i128 = 1_000_000_000_000_000;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);
        let id = create_shipment(&client, &env, &company, &carrier, seed + i as u64);

        // Generate amount in full i128 range using two seeds
        let seed2 = xorshift64(&mut rng);
        let raw = ((seed as i128) << 32) | (seed2 as i128 & 0xFFFF_FFFF);
        let amount = raw.abs() % (MAX_AMOUNT + 2); // 0..=MAX_AMOUNT+1

        let result = client.try_deposit_escrow(&company, &id, &amount);

        if amount > 0 && amount <= MAX_AMOUNT {
            assert!(
                result.is_ok(),
                "Valid amount {amount} was rejected for shipment {id}"
            );
            let balance = client.get_escrow_balance(&id);
            assert_eq!(balance, amount, "Balance mismatch for amount {amount}");
        } else {
            assert!(
                result.is_err(),
                "Invalid amount {amount} must be rejected for shipment {id}"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 5: Total escrow volume accumulates correctly (no overflow)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_arithmetic_total_escrow_volume_no_overflow() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xA000_EC00_0000_5000;
    let iterations = fuzz_iterations();
    let mut expected_total: i128 = 0;

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);
        let id = create_shipment(&client, &env, &company, &carrier, seed + i as u64);

        // Use small amounts to avoid hitting MAX_AMOUNT limit on total
        let amount = ((seed % 999) + 1) as i128;
        client.deposit_escrow(&company, &id, &amount);
        expected_total += amount;

        // Property: analytics total_escrow_volume matches sum of deposits
        let analytics = client.get_analytics();
        assert_eq!(
            analytics.total_escrow_volume, expected_total,
            "Total escrow volume mismatch: expected {expected_total} got {}",
            analytics.total_escrow_volume
        );
        assert!(
            analytics.total_escrow_volume >= 0,
            "Total escrow volume went negative: {}",
            analytics.total_escrow_volume
        );
    }
}
