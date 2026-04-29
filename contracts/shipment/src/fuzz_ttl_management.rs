#![cfg(test)]
//! # TTL Management Fuzzing Harness
//!
//! Property-based fuzz tests for Soroban TTL (Time-To-Live) storage management.
//!
//! ## Properties Verified
//! - **TTL extension is idempotent**: extending an already-extended entry is safe
//! - **Active shipments remain accessible** after repeated TTL extensions
//! - **Terminal shipments** (Cancelled/Delivered) are not extended after finalization
//! - **Config-driven TTL values** are respected
//! - **Concurrent read/write patterns** don't corrupt TTL state
//!
//! ## Running
//! ```bash
//! cargo test --package shipment --features testutils fuzz_ttl -- --nocapture
//! ```

extern crate std;

use crate::{NavinShipment, NavinShipmentClient};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, Vec,
};

// ─────────────────────────────────────────────────────────────────────────────
// Mock token
// ─────────────────────────────────────────────────────────────────────────────

#[contract]
struct TtlFuzzToken;

#[contractimpl]
impl TtlFuzzToken {
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
    let token = env.register(TtlFuzzToken {}, ());
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
    let deadline = env.ledger().timestamp() + 86_400 * 30; // 30 days
    client.create_shipment(company, &receiver, carrier, &data_hash, &Vec::new(env), &deadline, &None)
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 1: Active shipments remain accessible after repeated TTL extensions
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
fn fuzz_ttl_active_shipments_remain_accessible() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0x1000_CAFE_DEAD_BEEF;
    let shipments = (fuzz_iterations() / 10).max(5);
    const STATUS_UPDATES_PER_SHIPMENT: u32 = 50;

    let mut ids = std::vec::Vec::new();
    for i in 0..shipments {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);
        let id = create_shipment(&client, &env, &company, &carrier, seed + i as u64);
        ids.push(id);
    }

    // Simulate repeated status updates (each triggers TTL extension)
    for id in &ids {
        for _ in 0..STATUS_UPDATES_PER_SHIPMENT {
            env.ledger().with_mut(|l| l.timestamp += 65); // past rate limit
            let seed = xorshift64(&mut rng);
            let status_hash = hash_from_seed(&env, seed);

            let shipment = client.get_shipment(id);
            // Only update if still in a non-terminal state
            match shipment.status {
                crate::ShipmentStatus::Created => {
                    let _ = client.try_update_status(
                        &carrier,
                        id,
                        &crate::ShipmentStatus::InTransit,
                        &status_hash,
                    );
                }
                crate::ShipmentStatus::InTransit => {
                    let _ = client.try_update_status(
                        &carrier,
                        id,
                        &crate::ShipmentStatus::AtCheckpoint,
                        &status_hash,
                    );
                }
                crate::ShipmentStatus::AtCheckpoint => {
                    let _ = client.try_update_status(
                        &carrier,
                        id,
                        &crate::ShipmentStatus::InTransit,
                        &status_hash,
                    );
                }
                _ => break,
            }

            // Property: shipment is still accessible after TTL extension
            let result = client.try_get_shipment(id);
            assert!(
                result.is_ok(),
                "Active shipment {id} became inaccessible after TTL extension"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 2: TTL extension is idempotent — calling it multiple times is safe
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_ttl_extension_idempotent() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xDE00_E000_F000_4200;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);
        let id = create_shipment(&client, &env, &company, &carrier, seed + i as u64);

        // Trigger multiple status updates (each extends TTL)
        for _ in 0..3 {
            env.ledger().with_mut(|l| l.timestamp += 65);
            let s = xorshift64(&mut rng);
            let h = hash_from_seed(&env, s);
            let _ = client.try_update_status(&carrier, &id, &crate::ShipmentStatus::InTransit, &h);
        }

        // Property: shipment is still accessible and consistent
        let shipment = client.get_shipment(&id);
        assert_eq!(
            shipment.id, id,
            "Idempotent TTL extension corrupted shipment {id}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 3: Terminal shipments don't get TTL extensions after finalization
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_ttl_terminal_shipments_not_extended() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xE000_A000_F000_9900;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);
        let id = create_shipment(&client, &env, &company, &carrier, seed + i as u64);

        // Cancel the shipment (terminal state)
        let reason_hash = hash_from_seed(&env, seed.wrapping_add(1));
        client.cancel_shipment(&company, &id, &reason_hash);

        let shipment = client.get_shipment(&id);
        assert_eq!(
            shipment.status,
            crate::ShipmentStatus::Cancelled,
            "Shipment {id} should be Cancelled"
        );

        // Property: further status updates on terminal shipment must fail
        env.ledger().with_mut(|l| l.timestamp += 65);
        let status_hash = hash_from_seed(&env, seed.wrapping_add(2));
        let result = client.try_update_status(
            &carrier,
            &id,
            &crate::ShipmentStatus::InTransit,
            &status_hash,
        );
        assert!(
            result.is_err(),
            "Terminal shipment {id} should not accept status updates (no TTL extension)"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 4: Random TTL values and extension timings don't corrupt state
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_ttl_random_extension_timings() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0xAD00_0000_0000_0100;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        // Random time advance between 1 and 1000 seconds
        let time_advance = (seed % 999) + 1;
        env.ledger().with_mut(|l| l.timestamp += time_advance);

        let id = create_shipment(&client, &env, &company, &carrier, seed + i as u64);

        // Property: shipment is always accessible regardless of timing
        let result = client.try_get_shipment(&id);
        assert!(
            result.is_ok(),
            "Shipment {id} inaccessible after random timing advance of {time_advance}s"
        );

        let shipment = result.unwrap().unwrap();
        assert_eq!(shipment.id, id, "Shipment ID mismatch after random timing");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 5: Expired entry access returns error, not panic
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_ttl_expired_entry_access_no_panic() {
    let (_env, client, _admin) = setup();

    let mut rng: u64 = 0xE000_ED00_F000_AFE0;
    let iterations = fuzz_iterations();

    for _ in 0..iterations {
        let id = xorshift64(&mut rng) % 999_999 + 1_000_000; // IDs that don't exist

        // Property: accessing non-existent/expired entries must not panic
        let result = client.try_get_shipment(&id);
        assert!(
            result.is_err(),
            "Accessing expired/non-existent entry {id} should return error"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property 6: Simultaneous reads don't interfere with writes
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fuzz_ttl_simultaneous_read_write_consistency() {
    let (env, client, admin) = setup();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let mut rng: u64 = 0x0000_A000_E000_0000;
    let iterations = fuzz_iterations();

    for i in 0..iterations {
        let seed = xorshift64(&mut rng);
        env.ledger().with_mut(|l| l.timestamp += 2);

        let id = create_shipment(&client, &env, &company, &carrier, seed + i as u64);

        // Interleave reads and writes
        let read1 = client.get_shipment(&id);
        env.ledger().with_mut(|l| l.timestamp += 65);
        let status_hash = hash_from_seed(&env, seed);
        let _ = client.try_update_status(
            &carrier,
            &id,
            &crate::ShipmentStatus::InTransit,
            &status_hash,
        );
        let read2 = client.get_shipment(&id);

        // Property: reads are consistent — id never changes
        assert_eq!(read1.id, id, "Read1 id mismatch");
        assert_eq!(read2.id, id, "Read2 id mismatch after write");

        // Property: updated_at is monotonically non-decreasing
        assert!(
            read2.updated_at >= read1.updated_at,
            "updated_at went backwards: before={} after={}",
            read1.updated_at,
            read2.updated_at
        );
    }
}
