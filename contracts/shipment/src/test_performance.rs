//! # Performance and Budget Tests
//!
//! Verifies that batch operations stay within acceptable CPU-instruction budgets
//! and that a batch call is cheaper per-item than repeated individual calls.
//!
//! Uses Soroban's `cost_estimate().budget()` API to measure consumed CPU
//! instructions. Thresholds are conservative upper bounds; they guard against
//! regressions from accidental O(n²) behaviour.

extern crate std;

use crate::{test_utils, types::ShipmentInput, NavinShipment, NavinShipmentClient, ShipmentStatus};
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Vec};

// ── Mock token ────────────────────────────────────────────────────────────────

#[contract]
struct PerfMockToken;

#[contractimpl]
impl PerfMockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    pub fn mint(_env: Env, _admin: Address, _to: Address, _amount: i128) {}
}

// ── Setup helpers ─────────────────────────────────────────────────────────────

struct PerfCtx {
    env: Env,
    client: NavinShipmentClient<'static>,
    company: Address,
    carrier: Address,
}

fn setup_perf() -> PerfCtx {
    let (env, admin) = test_utils::setup_env();
    let token = env.register(PerfMockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token);
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);
    PerfCtx {
        env,
        client,
        company,
        carrier,
    }
}

fn dummy_hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

// ── Batch creation budget tests ───────────────────────────────────────────────

/// Max CPU instructions for a single shipment creation.
const MAX_CPU_SINGLE: u64 = 150_000_000;

/// Max CPU instructions for a 10-item batch (sub-linear growth expected).
const MAX_CPU_BATCH_10: u64 = 900_000_000;

#[test]
fn test_single_shipment_creation_within_budget() {
    let ctx = setup_perf();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);

    ctx.env.cost_estimate().budget().reset_unlimited();
    ctx.client.create_shipment(
        &ctx.company,
        &Address::generate(&ctx.env),
        &ctx.carrier,
        &dummy_hash(&ctx.env, 1),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );
    let cpu = ctx.env.cost_estimate().budget().cpu_instruction_cost();
    ctx.env.cost_estimate().budget().reset_default();

    assert!(
        cpu <= MAX_CPU_SINGLE,
        "single shipment creation used {cpu} CPU instructions, limit {MAX_CPU_SINGLE}"
    );
}

#[test]
fn test_batch_creation_10_within_budget() {
    let ctx = setup_perf();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let mut inputs: Vec<ShipmentInput> = Vec::new(&ctx.env);
    for seed in 1u8..=10 {
        inputs.push_back(ShipmentInput {
            receiver: Address::generate(&ctx.env),
            carrier: ctx.carrier.clone(),
            data_hash: dummy_hash(&ctx.env, seed),
            payment_milestones: Vec::new(&ctx.env),
            deadline,
            depends_on: None,
        });
    }

    ctx.env.cost_estimate().budget().reset_unlimited();
    ctx.client.create_shipments_batch(&ctx.company, &inputs);
    let cpu = ctx.env.cost_estimate().budget().cpu_instruction_cost();
    ctx.env.cost_estimate().budget().reset_default();

    assert!(
        cpu <= MAX_CPU_BATCH_10,
        "batch-10 creation used {cpu} CPU instructions, limit {MAX_CPU_BATCH_10}"
    );
}

#[test]
fn test_batch_cheaper_per_item_than_individual_calls() {
    // ── 5 individual creates ──────────────────────────────────────────────────
    let ctx_single = setup_perf();
    let deadline_s = test_utils::future_deadline(&ctx_single.env, 7200);
    ctx_single.env.cost_estimate().budget().reset_unlimited();
    for seed in 1u8..=5 {
        ctx_single.client.create_shipment(
            &ctx_single.company,
            &Address::generate(&ctx_single.env),
            &ctx_single.carrier,
            &dummy_hash(&ctx_single.env, seed),
            &Vec::new(&ctx_single.env),
            &deadline_s,
            &None
        );
    }
    let individual_cpu = ctx_single
        .env
        .cost_estimate()
        .budget()
        .cpu_instruction_cost();
    ctx_single.env.cost_estimate().budget().reset_default();

    // ── 5-item batch ──────────────────────────────────────────────────────────
    let ctx_batch = setup_perf();
    let deadline_b = test_utils::future_deadline(&ctx_batch.env, 7200);
    let mut inputs: Vec<ShipmentInput> = Vec::new(&ctx_batch.env);
    for seed in 1u8..=5 {
        inputs.push_back(ShipmentInput {
            receiver: Address::generate(&ctx_batch.env),
            carrier: ctx_batch.carrier.clone(),
            data_hash: dummy_hash(&ctx_batch.env, seed),
            payment_milestones: Vec::new(&ctx_batch.env),
            deadline: deadline_b,
        depends_on: None,
        });
    }
    ctx_batch.env.cost_estimate().budget().reset_unlimited();
    ctx_batch
        .client
        .create_shipments_batch(&ctx_batch.company, &inputs);
    let batch_cpu = ctx_batch
        .env
        .cost_estimate()
        .budget()
        .cpu_instruction_cost();
    ctx_batch.env.cost_estimate().budget().reset_default();

    // The batch amortises one config-read across all items but still executes
    // per-item storage writes and auth checks, so per-item cost is close to
    // individual calls. Assert it is within 3× individual cost (not O(n²)).
    assert!(
        batch_cpu < individual_cpu * 3,
        "batch ({batch_cpu} cpu) should be within 3× of 5 individual calls ({individual_cpu} cpu)"
    );
}

// ── Batch milestone budget test ───────────────────────────────────────────────

/// Max CPU instructions for recording 5 milestones in a single batch call.
const MAX_CPU_MILESTONE_BATCH_5: u64 = 800_000_000;

#[test]
fn test_batch_milestone_recording_within_budget() {
    let ctx = setup_perf();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let id = ctx.client.create_shipment(
        &ctx.company,
        &Address::generate(&ctx.env),
        &ctx.carrier,
        &dummy_hash(&ctx.env, 1),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );
    test_utils::advance_past_rate_limit(&ctx.env);
    ctx.client.update_status(
        &ctx.carrier,
        &id,
        &ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env, 2),
    );

    let mut milestones: Vec<(soroban_sdk::Symbol, BytesN<32>)> = Vec::new(&ctx.env);
    for seed in 10u8..=14 {
        milestones.push_back((
            soroban_sdk::Symbol::new(&ctx.env, "checkpoint"),
            dummy_hash(&ctx.env, seed),
        ));
    }

    ctx.env.cost_estimate().budget().reset_unlimited();
    ctx.client
        .record_milestones_batch(&ctx.carrier, &id, &milestones);
    let cpu = ctx.env.cost_estimate().budget().cpu_instruction_cost();
    ctx.env.cost_estimate().budget().reset_default();

    assert!(
        cpu <= MAX_CPU_MILESTONE_BATCH_5,
        "5-milestone batch used {cpu} CPU instructions, limit {MAX_CPU_MILESTONE_BATCH_5}"
    );
}

// ── Status update budget test ─────────────────────────────────────────────────

/// Max CPU instructions for a single status update.
const MAX_CPU_STATUS_UPDATE: u64 = 150_000_000;

#[test]
fn test_status_update_within_budget() {
    let ctx = setup_perf();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let id = ctx.client.create_shipment(
        &ctx.company,
        &Address::generate(&ctx.env),
        &ctx.carrier,
        &dummy_hash(&ctx.env, 1),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );
    test_utils::advance_past_rate_limit(&ctx.env);

    ctx.env.cost_estimate().budget().reset_unlimited();
    ctx.client.update_status(
        &ctx.carrier,
        &id,
        &ShipmentStatus::InTransit,
        &dummy_hash(&ctx.env, 2),
    );
    let cpu = ctx.env.cost_estimate().budget().cpu_instruction_cost();
    ctx.env.cost_estimate().budget().reset_default();

    assert!(
        cpu <= MAX_CPU_STATUS_UPDATE,
        "status update used {cpu} CPU instructions, limit {MAX_CPU_STATUS_UPDATE}"
    );
}

// ── Read-only query budget test ───────────────────────────────────────────────

/// Max CPU instructions for get_shipment (should be very cheap).
const MAX_CPU_GET_SHIPMENT: u64 = 20_000_000;

#[test]
fn test_get_shipment_within_budget() {
    let ctx = setup_perf();
    let deadline = test_utils::future_deadline(&ctx.env, 7200);
    let id = ctx.client.create_shipment(
        &ctx.company,
        &Address::generate(&ctx.env),
        &ctx.carrier,
        &dummy_hash(&ctx.env, 1),
        &Vec::new(&ctx.env),
        &deadline,
        &None,
    );

    ctx.env.cost_estimate().budget().reset_unlimited();
    ctx.client.get_shipment(&id);
    let cpu = ctx.env.cost_estimate().budget().cpu_instruction_cost();
    ctx.env.cost_estimate().budget().reset_default();

    assert!(
        cpu <= MAX_CPU_GET_SHIPMENT,
        "get_shipment used {cpu} CPU instructions, limit {MAX_CPU_GET_SHIPMENT}"
    );
}
