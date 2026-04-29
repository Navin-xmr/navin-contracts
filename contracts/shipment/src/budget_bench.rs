//! # Budget Benchmark Tests
//!
//! Measures Soroban CPU and memory budget consumption for the high-traffic
//! contract methods. Run these tests to establish baseline figures and detect
//! cost regressions before they reach the network.
//!
//! ## How to run
//! ```sh
//! cargo test --package shipment budget_bench -- --nocapture
//! ```
//!
//! ## Interpreting results
//! Each test prints a table like:
//! ```text
//! [budget] create_shipment  cpu=1_234_567  mem=56_789
//! ```
//!
//! | Column  | Unit              | Network limit (Soroban v22) |
//! |---------|-------------------|-----------------------------|
//! | `cpu`   | CPU instructions  | 100_000_000                 |
//! | `mem`   | bytes             | 41_943_040 (40 MiB)         |
//!
//! ### Regression guidance
//! - A delta of **±5 %** on either axis is noise — ignore it.
//! - A delta of **±10 %** warrants a code review before merging.
//! - A delta of **> 20 %** must be explained in the PR description.
//!
//! The committed baseline lives in `docs/BUDGET_REPORT.md`.  After any
//! intentional change to a hot path, update that file so future comparisons
//! remain meaningful.

#![cfg(test)]

extern crate std;

use crate::{NavinShipment, NavinShipmentClient, ShipmentStatus};
use soroban_sdk::{
    contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Symbol,
    Vec as SorobanVec,
};

// ---------------------------------------------------------------------------
// Minimal mock token — never actually moves funds in tests
// ---------------------------------------------------------------------------

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn decimals(_env: soroban_sdk::Env) -> u32 { 7 }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Boots a fresh environment with budget tracking enabled.
///
/// `Env::default()` starts with the budget meter active; we just need to
/// call `reset_unlimited()` on the env before each operation under test so
/// that setup work is not counted.
fn setup_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    (env, client, admin, token_contract)
}

/// Returns the current CPU-instruction and memory-byte counters for `env`.
fn read_budget(env: &Env) -> (u64, u64) {
    let cpu = env.cost_estimate().budget().cpu_instruction_cost();
    let mem = env.cost_estimate().budget().memory_bytes_cost();
    (cpu, mem)
}

/// Prints a labelled budget reading to stdout (visible with `--nocapture`).
fn print_budget(label: &str, cpu: u64, mem: u64) {
    std::println!("[budget] {:<45}  cpu={:<12}  mem={}", label, cpu, mem);
}

// ---------------------------------------------------------------------------
// Benchmark: initialize
// ---------------------------------------------------------------------------

#[test]
fn bench_initialize() {
    let (env, client, admin, token_contract) = setup_env();

    env.cost_estimate().budget().reset_default();
    client.initialize(&admin, &token_contract);
    let (cpu, mem) = read_budget(&env);

    print_budget("initialize", cpu, mem);

    // Sanity guard: must stay well inside network limits
    assert!(cpu < 100_000_000, "initialize exceeded CPU limit: {cpu}");
    assert!(mem < 41_943_040, "initialize exceeded memory limit: {mem}");
}

// ---------------------------------------------------------------------------
// Benchmark: create_shipment (single)
// ---------------------------------------------------------------------------

#[test]
fn bench_create_shipment() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    // Setup — not counted in the budget window
    env.cost_estimate().budget().reset_unlimited();
    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    env.cost_estimate().budget().reset_default();
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &SorobanVec::new(&env),
        &deadline,
        &None,
    );
    let (cpu, mem) = read_budget(&env);

    print_budget("create_shipment (single)", cpu, mem);

    assert!(
        cpu < 100_000_000,
        "create_shipment exceeded CPU limit: {cpu}"
    );
    assert!(
        mem < 41_943_040,
        "create_shipment exceeded memory limit: {mem}"
    );
}

// ---------------------------------------------------------------------------
// Benchmark: create_shipments_batch (max batch = 10)
// ---------------------------------------------------------------------------

#[test]
fn bench_create_shipments_batch() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    env.cost_estimate().budget().reset_unlimited();
    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let mut inputs = SorobanVec::new(&env);
    for i in 0..10u8 {
        inputs.push_back(crate::ShipmentInput {
            receiver: Address::generate(&env),
            carrier: Address::generate(&env),
            data_hash: BytesN::from_array(&env, &[i + 1; 32]),
            payment_milestones: SorobanVec::new(&env),
            deadline,
            depends_on: None,
        });
    }

    env.cost_estimate().budget().reset_default();
    client.create_shipments_batch(&company, &inputs);
    let (cpu, mem) = read_budget(&env);

    print_budget("create_shipments_batch (10 items)", cpu, mem);

    assert!(
        cpu < 100_000_000,
        "create_shipments_batch exceeded CPU limit: {cpu}"
    );
    assert!(
        mem < 41_943_040,
        "create_shipments_batch exceeded memory limit: {mem}"
    );
}

// ---------------------------------------------------------------------------
// Benchmark: update_status (Created → InTransit)
// ---------------------------------------------------------------------------

#[test]
fn bench_update_status() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let update_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    env.cost_estimate().budget().reset_unlimited();
    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &SorobanVec::new(&env),
        &deadline,
        &None,
    );

    env.cost_estimate().budget().reset_default();
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &update_hash,
    );
    let (cpu, mem) = read_budget(&env);

    print_budget("update_status (Created → InTransit)", cpu, mem);

    assert!(cpu < 100_000_000, "update_status exceeded CPU limit: {cpu}");
    assert!(
        mem < 41_943_040,
        "update_status exceeded memory limit: {mem}"
    );
}

// ---------------------------------------------------------------------------
// Benchmark: deposit_escrow
// ---------------------------------------------------------------------------

#[test]
fn bench_deposit_escrow() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[3u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    env.cost_estimate().budget().reset_unlimited();
    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &SorobanVec::new(&env),
        &deadline,
        &None,
    );

    env.cost_estimate().budget().reset_default();
    client.deposit_escrow(&company, &shipment_id, &500_000i128);
    let (cpu, mem) = read_budget(&env);

    print_budget("deposit_escrow", cpu, mem);

    assert!(
        cpu < 100_000_000,
        "deposit_escrow exceeded CPU limit: {cpu}"
    );
    assert!(
        mem < 41_943_040,
        "deposit_escrow exceeded memory limit: {mem}"
    );
}

// ---------------------------------------------------------------------------
// Benchmark: release_escrow (after full lifecycle to Delivered)
// ---------------------------------------------------------------------------

#[test]
fn bench_release_escrow() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[4u8; 32]);
    let deadline = env.ledger().timestamp() + 7200;

    env.cost_estimate().budget().reset_unlimited();
    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &SorobanVec::new(&env),
        &deadline,
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &1_000_000i128);
    // Manually promote to Delivered so escrow remains intact for release
    env.as_contract(&client.address, || {
        let mut s = crate::storage::get_shipment(&env, shipment_id).unwrap();
        s.status = ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &s);
    });

    env.cost_estimate().budget().reset_default();
    client.release_escrow(&admin, &shipment_id);
    let (cpu, mem) = read_budget(&env);

    print_budget("release_escrow", cpu, mem);

    assert!(
        cpu < 100_000_000,
        "release_escrow exceeded CPU limit: {cpu}"
    );
    assert!(
        mem < 41_943_040,
        "release_escrow exceeded memory limit: {mem}"
    );
}

// ---------------------------------------------------------------------------
// Benchmark: refund_escrow (after cancellation)
// ---------------------------------------------------------------------------

#[test]
fn bench_refund_escrow() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    env.cost_estimate().budget().reset_unlimited();
    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &SorobanVec::new(&env),
        &deadline,
        &None,
    );
    // Deposit escrow; refund_escrow works from Created state and cancels internally
    client.deposit_escrow(&company, &shipment_id, &500_000i128);

    env.cost_estimate().budget().reset_default();
    client.refund_escrow(&company, &shipment_id);
    let (cpu, mem) = read_budget(&env);

    print_budget("refund_escrow", cpu, mem);

    assert!(cpu < 100_000_000, "refund_escrow exceeded CPU limit: {cpu}");
    assert!(
        mem < 41_943_040,
        "refund_escrow exceeded memory limit: {mem}"
    );
}

// ---------------------------------------------------------------------------
// Benchmark: raise_dispute
// ---------------------------------------------------------------------------

#[test]
fn bench_raise_dispute() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[8u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[9u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    env.cost_estimate().budget().reset_unlimited();
    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &SorobanVec::new(&env),
        &deadline,
        &None,
    );
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[10u8; 32]),
    );

    env.cost_estimate().budget().reset_default();
    client.raise_dispute(&receiver, &shipment_id, &reason_hash);
    let (cpu, mem) = read_budget(&env);

    print_budget("raise_dispute", cpu, mem);

    assert!(cpu < 100_000_000, "raise_dispute exceeded CPU limit: {cpu}");
    assert!(
        mem < 41_943_040,
        "raise_dispute exceeded memory limit: {mem}"
    );
}

// ---------------------------------------------------------------------------
// Benchmark: resolve_dispute (RefundToCompany)
// ---------------------------------------------------------------------------

#[test]
fn bench_resolve_dispute() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[11u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[12u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    env.cost_estimate().budget().reset_unlimited();
    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &SorobanVec::new(&env),
        &deadline,
        &None,
    );
    client.deposit_escrow(&company, &shipment_id, &500_000i128);
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[13u8; 32]),
    );
    client.raise_dispute(&receiver, &shipment_id, &reason_hash);

    env.cost_estimate().budget().reset_default();
    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::RefundToCompany,
        &BytesN::from_array(&env, &[14u8; 32]),
    );
    let (cpu, mem) = read_budget(&env);

    print_budget("resolve_dispute (RefundToCompany)", cpu, mem);

    assert!(
        cpu < 100_000_000,
        "resolve_dispute exceeded CPU limit: {cpu}"
    );
    assert!(
        mem < 41_943_040,
        "resolve_dispute exceeded memory limit: {mem}"
    );
}

// ---------------------------------------------------------------------------
// Benchmark: record_milestone (single)
// ---------------------------------------------------------------------------

#[test]
fn bench_record_milestone() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[14u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    env.cost_estimate().budget().reset_unlimited();
    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &SorobanVec::new(&env),
        &deadline,
        &None,
    );
    // Use as_contract to set InTransit without rate-limit concerns
    env.as_contract(&client.address, || {
        let mut s = crate::storage::get_shipment(&env, shipment_id).unwrap();
        s.status = ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &s);
    });

    env.cost_estimate().budget().reset_default();
    client.record_milestone(
        &carrier,
        &shipment_id,
        &Symbol::new(&env, "warehouse"),
        &BytesN::from_array(&env, &[16u8; 32]),
    );
    let (cpu, mem) = read_budget(&env);

    print_budget("record_milestone (single)", cpu, mem);

    assert!(
        cpu < 100_000_000,
        "record_milestone exceeded CPU limit: {cpu}"
    );
    assert!(
        mem < 41_943_040,
        "record_milestone exceeded memory limit: {mem}"
    );
}

// ---------------------------------------------------------------------------
// Benchmark: confirm_delivery
// ---------------------------------------------------------------------------

#[test]
fn bench_confirm_delivery() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[17u8; 32]);
    let deadline = env.ledger().timestamp() + 7200;

    env.cost_estimate().budget().reset_unlimited();
    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &SorobanVec::new(&env),
        &deadline,
        &None,
    );
    // confirm_delivery itself performs the → Delivered transition from InTransit
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[18u8; 32]),
    );

    env.cost_estimate().budget().reset_default();
    client.confirm_delivery(
        &receiver,
        &shipment_id,
        &BytesN::from_array(&env, &[17u8; 32]),
    );
    let (cpu, mem) = read_budget(&env);

    print_budget("confirm_delivery", cpu, mem);

    assert!(
        cpu < 100_000_000,
        "confirm_delivery exceeded CPU limit: {cpu}"
    );
    assert!(
        mem < 41_943_040,
        "confirm_delivery exceeded memory limit: {mem}"
    );
}

// ---------------------------------------------------------------------------
// Benchmark: cancel_shipment
// ---------------------------------------------------------------------------

#[test]
fn bench_cancel_shipment() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[20u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    env.cost_estimate().budget().reset_unlimited();
    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &SorobanVec::new(&env),
        &deadline,
        &None,
    );

    env.cost_estimate().budget().reset_default();
    client.cancel_shipment(
        &company,
        &shipment_id,
        &BytesN::from_array(&env, &[99u8; 32]),
    );
    let (cpu, mem) = read_budget(&env);

    print_budget("cancel_shipment", cpu, mem);

    assert!(
        cpu < 100_000_000,
        "cancel_shipment exceeded CPU limit: {cpu}"
    );
    assert!(
        mem < 41_943_040,
        "cancel_shipment exceeded memory limit: {mem}"
    );
}

// ---------------------------------------------------------------------------
// Benchmark: handoff_shipment (carrier transfer)
// ---------------------------------------------------------------------------

#[test]
fn bench_handoff_shipment() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier_a = Address::generate(&env);
    let carrier_b = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[21u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    env.cost_estimate().budget().reset_unlimited();
    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier_a);
    client.add_carrier(&admin, &carrier_b);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier_a,
        &data_hash,
        &SorobanVec::new(&env),
        &deadline,
        &None,
    );
    client.update_status(
        &carrier_a,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[22u8; 32]),
    );

    env.cost_estimate().budget().reset_default();
    client.handoff_shipment(
        &carrier_a,
        &carrier_b,
        &shipment_id,
        &BytesN::from_array(&env, &[23u8; 32]),
    );
    let (cpu, mem) = read_budget(&env);

    print_budget("handoff_shipment", cpu, mem);

    assert!(
        cpu < 100_000_000,
        "handoff_shipment exceeded CPU limit: {cpu}"
    );
    assert!(
        mem < 41_943_040,
        "handoff_shipment exceeded memory limit: {mem}"
    );
}

// ---------------------------------------------------------------------------
// Comprehensive budget summary — prints all numbers in one run
// ---------------------------------------------------------------------------

/// Runs every high-traffic operation sequentially and prints a consolidated
/// budget table.  Useful for a quick side-by-side comparison between releases.
///
/// Each operation is measured in isolation: `reset_default()` resets the
/// budget meter immediately before the call under test, and the counter is
/// read immediately after.
#[test]
fn bench_full_lifecycle_summary() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 7200;

    // ---- initialize (measured) ----
    env.cost_estimate().budget().reset_default();
    client.initialize(&admin, &token_contract);
    let (cpu, mem) = read_budget(&env);
    print_budget("summary::initialize", cpu, mem);

    // ---- setup (not measured) ----
    env.cost_estimate().budget().reset_unlimited();
    client.add_company(&admin, &company);

    // ---- create_shipment (measured) ----
    env.cost_estimate().budget().reset_default();
    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &SorobanVec::new(&env),
        &deadline,
        &None,
    );
    let (cpu, mem) = read_budget(&env);
    print_budget("summary::create_shipment", cpu, mem);

    // ---- deposit_escrow (measured) ----
    env.cost_estimate().budget().reset_default();
    client.deposit_escrow(&company, &id, &1_000_000i128);
    let (cpu, mem) = read_budget(&env);
    print_budget("summary::deposit_escrow", cpu, mem);

    // ---- update_status Created → InTransit (measured) ----
    env.cost_estimate().budget().reset_default();
    client.update_status(
        &carrier,
        &id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[2u8; 32]),
    );
    let (cpu, mem) = read_budget(&env);
    print_budget("summary::update_status (InTransit)", cpu, mem);

    // ---- confirm_delivery InTransit → Delivered (measured)
    //      confirm_delivery performs the → Delivered transition internally.
    // ----
    env.cost_estimate().budget().reset_default();
    client.confirm_delivery(&receiver, &id, &BytesN::from_array(&env, &[1u8; 32]));
    let (cpu, mem) = read_budget(&env);
    print_budget("summary::confirm_delivery (InTransit→Delivered)", cpu, mem);

    // ---- release_escrow (measured)
    //      Create a fresh shipment in Delivered state with escrow for this
    //      benchmark; confirm_delivery already cleared escrow on shipment `id`.
    // ----
    env.cost_estimate().budget().reset_unlimited();
    let id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[10u8; 32]),
        &SorobanVec::new(&env),
        &deadline,
        &None,
    );
    client.deposit_escrow(&company, &id2, &500_000i128);
    env.as_contract(&client.address, || {
        let mut s = crate::storage::get_shipment(&env, id2).unwrap();
        s.status = ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &s);
    });

    env.cost_estimate().budget().reset_default();
    client.release_escrow(&admin, &id2);
    let (cpu, mem) = read_budget(&env);
    print_budget("summary::release_escrow", cpu, mem);

    std::println!(
        "\nBaseline captured on soroban-sdk {}",
        env!("CARGO_PKG_VERSION")
    );
}
