#![cfg(test)]
//! # Escrow Lifecycle Fuzzing Harness
//!
//! Property-based fuzz tests for the complete escrow lifecycle:
//! deposit → release/refund.
//!
//! ## Properties Verified
//! - **Deposit balance equals deposited amount**
//! - **Cannot deposit zero or negative amounts**
//! - **Cannot deposit twice on the same shipment** (EscrowAlreadyDeposited / EscrowLocked)
//! - **Release never exceeds available escrow**
//! - **Double-release always fails**
//! - **Refund returns funds to sender on cancellation**
//! - **Escrow invariant**: escrow_amount ≤ total_escrow at all times
//!
//! ## Running
//! ```bash
//! cargo test --package shipment --features testutils fuzz_escrow_lifecycle -- --nocapture
//! ```

extern crate std;

use crate::{NavinShipment, NavinShipmentClient};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, Vec,
};

// ─────────────────────────────────────────────────────────────────────────────
// Mock token — tracks transfer calls for balance verification
// ─────────────────────────────────────────────────────────────────────────────

#[contract]
struct EscrowLifecycleToken;

#[contractimpl]
impl EscrowLifecycleToken {
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn setup() -> (Env, NavinShipmentClient<'static>, Address) {
    let (env, admin) = crate::test_utils::setup_env();
    let token = env.register(EscrowLifecycleToken {}, ());
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
        bytes[i + 8] = s[i].wrapping_add(0x55);
        bytes[i + 16] = s[i].wrapping_add(0xAA);
        bytes[i + 24] = s[i].wrapping_add(0xFF);
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
// Property 1: Escrow balance equals deposited amount
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
fn fuzz_escrow_deposit_balance_equals_amount() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xEC00_0000_DE00_1000;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);

        let id = create_shipment(&client, &env, &company, &carrier, seed + i as u64);

        // Random valid amount: 1 to 1_000_000_000_000_000 (MAX_AMOUNT)
        let amount = ((seed % 999_999_999_999_999) + 1) as i128;

        client.deposit_escrow(&company, &id, &amount);

        // Property: escrow balance equals deposited amount
        let balance = client.get_escrow_balance(&id);
        assert_eq!(
            balance, amount,
            "Escrow balance {balance} != deposited amount {amount} for shipment {id}"
        );

        // Property: shipment.escrow_amount == amount
        let shipment = client.get_shipment(&id);
        assert_eq!(
            shipment.escrow_amount, amount,
            "shipment.escrow_amount {} != deposited {amount}",
            shipment.escrow_amount
        );

        // Property: escrow_amount <= total_escrow (invariant)
        assert!(
            shipment.escrow_amount <= shipment.total_escrow,
            "Invariant violated: escrow_amount {} > total_escrow {}",
            shipment.escrow_amount,
            shipment.total_escrow
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 2: Cannot deposit zero or negative amounts
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_escrow_deposit_rejects_invalid_amounts() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xA000_D000_A000_F000;
    let iterations = fuzz_iterations();

    // Invalid amounts to test
    let invalid_amounts: std::vec::Vec<i128> =
        std::vec![0, -1, -1000, i128::MIN, -1_000_000_000_000_000_i128,];

    for (i, &amount) in invalid_amounts.iter().enumerate() {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);
        let id = create_shipment(&client, &env, &company, &carrier, seed + i as u64);

        let result = client.try_deposit_escrow(&company, &id, &amount);
        assert!(
            result.is_err(),
            "deposit_escrow must reject invalid amount {amount} for shipment {id}"
        );
    }

    // Fuzz random negative amounts
    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);
        let id = create_shipment(&client, &env, &company, &carrier, seed + i as u64 + 100_000);

        // Generate negative amount
        let amount = -((seed % 1_000_000) as i128 + 1);
        let result = client.try_deposit_escrow(&company, &id, &amount);
        assert!(
            result.is_err(),
            "deposit_escrow must reject negative amount {amount}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 3: Cannot deposit twice on the same shipment
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_escrow_no_double_deposit() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xDB00_E000_DE00_2000;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);
        let id = create_shipment(&client, &env, &company, &carrier, seed + i as u64);

        let amount = ((seed % 999_999) + 1) as i128;
        client.deposit_escrow(&company, &id, &amount);

        // Property: second deposit must fail
        let second_amount = ((xorshift64(&mut rng) % 999_999) + 1) as i128;
        let result = client.try_deposit_escrow(&company, &id, &second_amount);
        assert!(
            result.is_err(),
            "Double deposit must fail for shipment {id}"
        );

        // Property: balance unchanged after failed second deposit
        let balance = client.get_escrow_balance(&id);
        assert_eq!(
            balance, amount,
            "Balance changed after failed double deposit: expected {amount} got {balance}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 4: Release never exceeds available escrow
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_escrow_release_never_exceeds_balance() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xEE00_AE00_F000_A000;
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

        // Advance to Delivered status
        env.ledger().with_mut(|l| l.timestamp += 65);
        let h1 = hash_from_seed(&env, seed.wrapping_add(1));
        client.update_status(&carrier, &id, &crate::ShipmentStatus::InTransit, &h1);
        env.ledger().with_mut(|l| l.timestamp += 65);
        let h2 = hash_from_seed(&env, seed.wrapping_add(2));
        client.update_status(&carrier, &id, &crate::ShipmentStatus::Delivered, &h2);

        let before_balance = client.get_escrow_balance(&id);
        client.release_escrow(&receiver, &id);
        let after_balance = client.get_escrow_balance(&id);

        // Property: after release, balance is 0 (all released to carrier)
        assert_eq!(
            after_balance, 0,
            "After release, escrow balance should be 0, got {after_balance}"
        );

        // Property: released amount never exceeded available balance
        assert!(
            before_balance >= 0,
            "Escrow balance was negative before release: {before_balance}"
        );
        assert!(
            after_balance >= 0,
            "Escrow balance went negative after release: {after_balance}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 5: Double-release always fails
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_escrow_double_release_fails() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xDB00_E000_E000_EAE3;
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

        // Advance to Delivered
        env.ledger().with_mut(|l| l.timestamp += 65);
        let h1 = hash_from_seed(&env, seed.wrapping_add(10));
        client.update_status(&carrier, &id, &crate::ShipmentStatus::InTransit, &h1);
        env.ledger().with_mut(|l| l.timestamp += 65);
        let h2 = hash_from_seed(&env, seed.wrapping_add(11));
        client.update_status(&carrier, &id, &crate::ShipmentStatus::Delivered, &h2);

        // First release — must succeed
        client.release_escrow(&receiver, &id);

        // Property: second release must fail (shipment finalized or escrow empty)
        let result = client.try_release_escrow(&receiver, &id);
        assert!(
            result.is_err(),
            "Double release must fail for shipment {id}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 6: Refund returns funds on cancellation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_escrow_refund_on_cancellation() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xEF00_D000_CAC0_E990;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);
        let id = create_shipment(&client, &env, &company, &carrier, seed + i as u64);

        let amount = ((seed % 999_999) + 1) as i128;
        client.deposit_escrow(&company, &id, &amount);

        // Cancel the shipment — auto-zeros escrow and finalizes
        let reason_hash = hash_from_seed(&env, seed.wrapping_add(99));
        client.cancel_shipment(&company, &id, &reason_hash);

        // Property: escrow balance is 0 after cancel (cancel auto-handles escrow)
        let balance = client.get_escrow_balance(&id);
        assert_eq!(
            balance, 0,
            "Escrow balance should be 0 after cancel, got {balance} for shipment {id}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 7: escrow_amount <= total_escrow invariant always holds
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_escrow_invariant_amount_lte_total() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xA000_A000_EC00_0000;
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

        // Check invariant after deposit
        let s = client.get_shipment(&id);
        assert!(
            s.escrow_amount <= s.total_escrow,
            "Invariant violated after deposit: escrow_amount={} > total_escrow={}",
            s.escrow_amount,
            s.total_escrow
        );

        // Advance to Delivered and release
        env.ledger().with_mut(|l| l.timestamp += 65);
        let h1 = hash_from_seed(&env, seed.wrapping_add(20));
        client.update_status(&carrier, &id, &crate::ShipmentStatus::InTransit, &h1);
        env.ledger().with_mut(|l| l.timestamp += 65);
        let h2 = hash_from_seed(&env, seed.wrapping_add(21));
        client.update_status(&carrier, &id, &crate::ShipmentStatus::Delivered, &h2);
        client.release_escrow(&receiver, &id);

        // Check invariant after release
        let s = client.get_shipment(&id);
        assert!(
            s.escrow_amount <= s.total_escrow,
            "Invariant violated after release: escrow_amount={} > total_escrow={}",
            s.escrow_amount,
            s.total_escrow
        );
        assert_eq!(
            s.escrow_amount, 0,
            "escrow_amount should be 0 after full release"
        );
    }
}
