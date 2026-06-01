//! Tests for the automatic breach-triggered dispute opening feature (issue #177).
//!
//! Covers:
//! - Toggle enabled: Critical breach auto-opens a dispute.
//! - Toggle disabled (default): Critical breach leaves status unchanged.
//! - Non-critical breach with toggle enabled: no auto-dispute.
//! - Shipment already Disputed: no double-open.
//! - Shipment Cancelled: auto-dispute is skipped.

extern crate std;

use crate::{BreachType, NavinShipment, NavinShipmentClient, Severity, ShipmentStatus};
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env};

// ── Minimal mock token ────────────────────────────────────────────────────────

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn decimals(_env: soroban_sdk::Env) -> u32 {
        7
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    pub fn mint(_env: Env, _admin: Address, _to: Address, _amount: i128) {}
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = super::test_utils::setup_env();
    let token = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    (env, client, admin, token)
}

/// Enable `auto_dispute_breach` by calling `update_config`.
fn enable_auto_dispute(client: &NavinShipmentClient, admin: &Address) {
    let mut cfg = client.get_contract_config();
    cfg.auto_dispute_breach = true;
    client.update_config(admin, &cfg);
}

/// Create a shipment and return its ID. Carrier is registered as a carrier role.
fn create_test_shipment(
    env: &Env,
    client: &NavinShipmentClient,
    admin: &Address,
    token: &Address,
) -> (u64, Address, Address, Address) {
    client.initialize(admin, token);

    let company = Address::generate(env);
    let receiver = Address::generate(env);
    let carrier = Address::generate(env);
    let data_hash = BytesN::from_array(env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.add_company(admin, &company);
    client.add_carrier(admin, &carrier);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(env),
        &deadline,
    );

    (id, company, receiver, carrier)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Critical breach with toggle enabled must transition the shipment to Disputed.
#[test]
fn test_auto_dispute_opens_on_critical_breach_when_enabled() {
    let (env, client, admin, token) = setup();
    let (id, _company, _receiver, carrier) = create_test_shipment(&env, &client, &admin, &token);
    enable_auto_dispute(&client, &admin);

    let breach_hash = BytesN::from_array(&env, &[42u8; 32]);
    client.report_condition_breach(
        &carrier,
        &id,
        &BreachType::TamperDetected,
        &Severity::Critical,
        &breach_hash,
    );

    let shipment = client.get_shipment(&id);
    assert_eq!(shipment.status, ShipmentStatus::Disputed);
}

/// Critical breach with toggle DISABLED (default) must NOT change the status.
#[test]
fn test_auto_dispute_disabled_critical_breach_leaves_status_unchanged() {
    let (env, client, admin, token) = setup();
    let (id, _company, _receiver, carrier) = create_test_shipment(&env, &client, &admin, &token);
    // toggle is false by default — no update_config call

    let breach_hash = BytesN::from_array(&env, &[43u8; 32]);
    client.report_condition_breach(
        &carrier,
        &id,
        &BreachType::TemperatureHigh,
        &Severity::Critical,
        &breach_hash,
    );

    let shipment = client.get_shipment(&id);
    assert_eq!(
        shipment.status,
        ShipmentStatus::Created,
        "Status must remain Created when toggle is disabled"
    );
}

/// Non-Critical breach with toggle enabled must NOT open a dispute.
#[test]
fn test_auto_dispute_ignores_non_critical_breach() {
    let (env, client, admin, token) = setup();
    let (id, _company, _receiver, carrier) = create_test_shipment(&env, &client, &admin, &token);
    enable_auto_dispute(&client, &admin);

    let breach_hash = BytesN::from_array(&env, &[44u8; 32]);

    // High severity — below the Critical threshold
    client.report_condition_breach(
        &carrier,
        &id,
        &BreachType::Impact,
        &Severity::High,
        &breach_hash,
    );
    assert_eq!(client.get_shipment(&id).status, ShipmentStatus::Created);

    // Medium severity
    client.report_condition_breach(
        &carrier,
        &id,
        &BreachType::HumidityHigh,
        &Severity::Medium,
        &breach_hash,
    );
    assert_eq!(client.get_shipment(&id).status, ShipmentStatus::Created);

    // Low severity
    client.report_condition_breach(
        &carrier,
        &id,
        &BreachType::TemperatureLow,
        &Severity::Low,
        &breach_hash,
    );
    assert_eq!(client.get_shipment(&id).status, ShipmentStatus::Created);
}

/// If the shipment is already Disputed, a Critical breach must not raise a
/// duplicate dispute (status stays Disputed, counters must not double-increment).
#[test]
fn test_auto_dispute_skips_already_disputed_shipment() {
    let (env, client, admin, token) = setup();
    let (id, company, _receiver, carrier) = create_test_shipment(&env, &client, &admin, &token);
    enable_auto_dispute(&client, &admin);

    // Manually raise a dispute first
    let reason_hash = BytesN::from_array(&env, &[50u8; 32]);
    client.raise_dispute(&company, &id, &reason_hash);
    assert_eq!(client.get_shipment(&id).status, ShipmentStatus::Disputed);

    let disputes_before = client.get_analytics().total_disputes;

    // Report a Critical breach — should be a no-op for dispute state
    let breach_hash = BytesN::from_array(&env, &[51u8; 32]);
    client.report_condition_breach(
        &carrier,
        &id,
        &BreachType::TamperDetected,
        &Severity::Critical,
        &breach_hash,
    );

    assert_eq!(client.get_shipment(&id).status, ShipmentStatus::Disputed);
    assert_eq!(
        client.get_analytics().total_disputes,
        disputes_before,
        "total_disputes must not be incremented for an already-disputed shipment"
    );
}

/// Cancelled (non-finalized) shipments must not be auto-disputed on Critical breaches.
///
/// We force the shipment into `Cancelled` state via direct storage manipulation
/// while keeping `finalized = false` so that `report_condition_breach` is still
/// callable. This isolates the auto-dispute guard from the finalization check.
#[test]
fn test_auto_dispute_skips_cancelled_shipment() {
    let (env, client, admin, token) = setup();
    let (id, _company, _receiver, carrier) = create_test_shipment(&env, &client, &admin, &token);
    enable_auto_dispute(&client, &admin);

    // Force shipment to Cancelled without going through cancel_shipment
    // (which would finalize the shipment and block breach reporting entirely)
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
        let old_status = shipment.status.clone();
        shipment.status = crate::ShipmentStatus::Cancelled;
        shipment.finalized = false; // keep non-finalized so breach reporting still works
        crate::storage::set_shipment(&env, &shipment);
        crate::storage::decrement_status_count(&env, &old_status);
        crate::storage::increment_status_count(&env, &crate::ShipmentStatus::Cancelled);
    });
    assert_eq!(client.get_shipment(&id).status, ShipmentStatus::Cancelled);

    // A Critical breach on a Cancelled shipment must NOT trigger auto-dispute
    let breach_hash = BytesN::from_array(&env, &[61u8; 32]);
    client.report_condition_breach(
        &carrier,
        &id,
        &BreachType::TemperatureHigh,
        &Severity::Critical,
        &breach_hash,
    );

    assert_eq!(client.get_shipment(&id).status, ShipmentStatus::Cancelled);
}

/// Verify the auto-dispute toggle is stored/retrieved correctly via get_contract_config.
#[test]
fn test_auto_dispute_toggle_persisted_in_config() {
    let (_env, client, admin, token) = setup();
    client.initialize(&admin, &token);

    // Default: disabled
    assert!(
        !client.get_contract_config().auto_dispute_breach,
        "Toggle must be false by default"
    );

    // Enable
    let mut cfg = client.get_contract_config();
    cfg.auto_dispute_breach = true;
    client.update_config(&admin, &cfg);
    assert!(
        client.get_contract_config().auto_dispute_breach,
        "Toggle must be true after enabling"
    );

    // Disable again
    let mut cfg = client.get_contract_config();
    cfg.auto_dispute_breach = false;
    client.update_config(&admin, &cfg);
    assert!(
        !client.get_contract_config().auto_dispute_breach,
        "Toggle must be false after disabling"
    );
}

/// Enabling then disabling the toggle must not regress normal breach reporting.
#[test]
fn test_no_regression_when_toggle_re_disabled() {
    let (env, client, admin, token) = setup();
    let (id, _company, _receiver, carrier) = create_test_shipment(&env, &client, &admin, &token);

    // Enable then immediately disable
    enable_auto_dispute(&client, &admin);
    let mut cfg = client.get_contract_config();
    cfg.auto_dispute_breach = false;
    client.update_config(&admin, &cfg);

    let breach_hash = BytesN::from_array(&env, &[70u8; 32]);
    client.report_condition_breach(
        &carrier,
        &id,
        &BreachType::TamperDetected,
        &Severity::Critical,
        &breach_hash,
    );

    assert_eq!(
        client.get_shipment(&id).status,
        ShipmentStatus::Created,
        "Status must stay Created after toggle is re-disabled"
    );
}

/// Auto-dispute increments the analytics total_disputes counter exactly once.
#[test]
fn test_auto_dispute_increments_total_disputes() {
    let (env, client, admin, token) = setup();
    let (id, _company, _receiver, carrier) = create_test_shipment(&env, &client, &admin, &token);
    enable_auto_dispute(&client, &admin);

    let before = client.get_analytics().total_disputes;
    let breach_hash = BytesN::from_array(&env, &[80u8; 32]);
    client.report_condition_breach(
        &carrier,
        &id,
        &BreachType::Impact,
        &Severity::Critical,
        &breach_hash,
    );

    assert_eq!(
        client.get_analytics().total_disputes,
        before + 1,
        "total_disputes must be incremented by exactly 1"
    );
}
