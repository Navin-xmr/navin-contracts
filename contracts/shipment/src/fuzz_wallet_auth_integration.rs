#![cfg(test)]
//! # Wallet-Signed Auth Integration Tests
//!
//! End-to-end integration tests approximating wallet-signed invocation behavior
//! for key user flows. Validates that auth checks and state transitions match
//! expected behavior as if invoked by a real wallet signer.
//!
//! ## Test Setup Assumptions
//! - `env.mock_all_auths()` simulates wallet-signed authorization for all addresses.
//! - Each address acts as its own signer (single-key wallet).
//! - Token transfers always succeed (mock token).
//! - Ledger timestamp starts at 86400 (Unix day 1) and advances explicitly.
//!
//! ## Flows Covered
//! 1. Company wallet creates shipment → carrier wallet updates status → receiver wallet releases escrow
//! 2. Company wallet deposits escrow → cancels → refunds
//! 3. Carrier wallet raises dispute → admin wallet resolves
//! 4. Admin wallet pauses → all operations blocked → admin unpauses → operations resume
//!
//! ## Running
//! ```bash
//! cargo test --package shipment --features testutils wallet_auth -- --nocapture
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
struct WalletAuthToken;

#[contractimpl]
impl WalletAuthToken {
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

// ─────────────────────────────────────────────────────────────────────────────
// Setup
// ─────────────────────────────────────────────────────────────────────────────

struct TestContext {
    env: Env,
    client: NavinShipmentClient<'static>,
    admin: Address,
    company: Address,
    carrier: Address,
    receiver: Address,
}

fn setup_full() -> TestContext {
    let (env, admin) = crate::test_utils::setup_env();
    let token = env.register(WalletAuthToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    TestContext {
        env,
        client,
        admin,
        company,
        carrier,
        receiver,
    }
}

fn non_zero_hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed.max(1); 32])
}

// ─────────────────────────────────────────────────────────────────────────────
// Flow 1: Happy path — create → deposit → transit → deliver → release
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn wallet_auth_happy_path_full_lifecycle() {
    let ctx = setup_full();
    let TestContext {
        env,
        client,
        admin: _,
        company,
        carrier,
        receiver,
    } = ctx;

    let data_hash = non_zero_hash(&env, 1);
    let deadline = env.ledger().timestamp() + 86_400 * 30;

    // Company wallet: create shipment
    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );
    let s = client.get_shipment(&id);
    assert_eq!(s.status, crate::ShipmentStatus::Created);
    assert_eq!(s.sender, company);
    assert_eq!(s.carrier, carrier);
    assert_eq!(s.receiver, receiver);

    // Company wallet: deposit escrow
    client.deposit_escrow(&company, &id, &1_000_000i128);
    assert_eq!(client.get_escrow_balance(&id), 1_000_000);

    // Carrier wallet: update to InTransit
    env.ledger().with_mut(|l| l.timestamp += 65);
    client.update_status(
        &carrier,
        &id,
        &crate::ShipmentStatus::InTransit,
        &non_zero_hash(&env, 2),
    );
    assert_eq!(
        client.get_shipment(&id).status,
        crate::ShipmentStatus::InTransit
    );

    // Carrier wallet: update to Delivered
    env.ledger().with_mut(|l| l.timestamp += 65);
    client.update_status(
        &carrier,
        &id,
        &crate::ShipmentStatus::Delivered,
        &non_zero_hash(&env, 3),
    );
    assert_eq!(
        client.get_shipment(&id).status,
        crate::ShipmentStatus::Delivered
    );

    // Receiver wallet: release escrow to carrier
    client.release_escrow(&receiver, &id);
    assert_eq!(client.get_escrow_balance(&id), 0);

    // Auth outcome: shipment finalized
    let s = client.get_shipment(&id);
    assert!(s.finalized, "Shipment must be finalized after release");
}

// ─────────────────────────────────────────────────────────────────────────────
// Flow 2: Cancel and refund path
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn wallet_auth_cancel_refund_path() {
    let ctx = setup_full();
    let TestContext {
        env,
        client,
        admin: _,
        company,
        carrier,
        receiver,
    } = ctx;

    let data_hash = non_zero_hash(&env, 10);
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

    // Company deposits escrow
    client.deposit_escrow(&company, &id, &500_000i128);

    // Company cancels — auto-zeros escrow and finalizes
    client.cancel_shipment(&company, &id, &non_zero_hash(&env, 11));
    assert_eq!(
        client.get_shipment(&id).status,
        crate::ShipmentStatus::Cancelled
    );
    assert_eq!(client.get_escrow_balance(&id), 0);

    // Auth outcome: shipment finalized after cancel
    let s = client.get_shipment(&id);
    assert!(s.finalized, "Shipment must be finalized after cancel");
}

// ─────────────────────────────────────────────────────────────────────────────
// Flow 3: Dispute raised by receiver → admin resolves to carrier
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn wallet_auth_dispute_resolution_to_carrier() {
    let ctx = setup_full();
    let TestContext {
        env,
        client,
        admin,
        company,
        carrier,
        receiver,
    } = ctx;

    let data_hash = non_zero_hash(&env, 20);
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

    client.deposit_escrow(&company, &id, &750_000i128);

    env.ledger().with_mut(|l| l.timestamp += 65);
    client.update_status(
        &carrier,
        &id,
        &crate::ShipmentStatus::InTransit,
        &non_zero_hash(&env, 21),
    );

    // Receiver raises dispute
    client.raise_dispute(&receiver, &id, &non_zero_hash(&env, 22));
    assert_eq!(
        client.get_shipment(&id).status,
        crate::ShipmentStatus::Disputed
    );

    // Admin resolves: release to carrier
    client.resolve_dispute(
        &admin,
        &id,
        &crate::DisputeResolution::ReleaseToCarrier,
        &non_zero_hash(&env, 23),
    );

    // Auth outcome: escrow released to carrier
    assert_eq!(client.get_escrow_balance(&id), 0);
    let s = client.get_shipment(&id);
    assert!(
        s.finalized,
        "Shipment must be finalized after dispute resolution"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Flow 4: Dispute resolved with refund to company
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn wallet_auth_dispute_resolution_refund_to_company() {
    let ctx = setup_full();
    let TestContext {
        env,
        client,
        admin,
        company,
        carrier,
        receiver,
    } = ctx;

    let data_hash = non_zero_hash(&env, 30);
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

    client.deposit_escrow(&company, &id, &300_000i128);

    env.ledger().with_mut(|l| l.timestamp += 65);
    client.update_status(
        &carrier,
        &id,
        &crate::ShipmentStatus::InTransit,
        &non_zero_hash(&env, 31),
    );

    client.raise_dispute(&company, &id, &non_zero_hash(&env, 32));

    // Admin resolves: refund to company
    client.resolve_dispute(
        &admin,
        &id,
        &crate::DisputeResolution::RefundToCompany,
        &non_zero_hash(&env, 33),
    );

    assert_eq!(client.get_escrow_balance(&id), 0);
    let s = client.get_shipment(&id);
    assert!(
        s.finalized,
        "Shipment must be finalized after refund resolution"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Flow 5: Admin pause blocks all operations; unpause restores them
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn wallet_auth_pause_blocks_operations() {
    let ctx = setup_full();
    let TestContext {
        env,
        client,
        admin,
        company,
        carrier,
        receiver,
    } = ctx;

    // Admin pauses contract
    client.pause(&admin);
    assert!(client.is_paused());

    // Company wallet: create shipment must fail while paused
    let data_hash = non_zero_hash(&env, 40);
    let deadline = env.ledger().timestamp() + 86_400;
    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );
    assert!(result.is_err(), "create_shipment must fail while paused");

    // Admin unpauses
    client.unpause(&admin);
    assert!(!client.is_paused());

    // Company wallet: create shipment must succeed after unpause
    env.ledger().with_mut(|l| l.timestamp += 2);
    let data_hash2 = non_zero_hash(&env, 41);
    let deadline2 = env.ledger().timestamp() + 86_400;
    let result2 = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash2,
        &Vec::new(&env),
        &deadline2,
        &None,
    );
    assert!(
        result2.is_ok(),
        "create_shipment must succeed after unpause"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Flow 6: Deadline expiry auto-cancel
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn wallet_auth_deadline_expiry_auto_cancel() {
    let ctx = setup_full();
    let TestContext {
        env,
        client,
        admin: _,
        company,
        carrier,
        receiver,
    } = ctx;

    let data_hash = non_zero_hash(&env, 50);
    // Short deadline: 1 hour from now
    let deadline = env.ledger().timestamp() + 3_600;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    client.deposit_escrow(&company, &id, &100_000i128);

    // Advance past deadline
    env.ledger().with_mut(|l| l.timestamp += 3_601 + 86_400); // past deadline + grace

    // Anyone can trigger deadline check
    client.check_deadline(&id);

    // Auth outcome: shipment auto-cancelled
    let s = client.get_shipment(&id);
    assert_eq!(
        s.status,
        crate::ShipmentStatus::Cancelled,
        "Shipment must be auto-cancelled after deadline expiry"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Flow 7: Unauthorized wallet cannot perform privileged operations
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn wallet_auth_unauthorized_wallet_rejected() {
    let ctx = setup_full();
    let TestContext {
        env,
        client,
        admin: _,
        company,
        carrier,
        receiver,
    } = ctx;

    let data_hash = non_zero_hash(&env, 60);
    let deadline = env.ledger().timestamp() + 86_400;

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    client.deposit_escrow(&company, &id, &200_000i128);

    // Random wallet (no role) cannot release escrow
    let random_wallet = Address::generate(&env);
    let result = client.try_release_escrow(&random_wallet, &id);
    assert!(result.is_err(), "Random wallet must not release escrow");

    // Random wallet cannot cancel shipment
    let result2 = client.try_cancel_shipment(&random_wallet, &id, &non_zero_hash(&env, 61));
    assert!(result2.is_err(), "Random wallet must not cancel shipment");

    // Random wallet cannot update status
    env.ledger().with_mut(|l| l.timestamp += 65);
    let result3 = client.try_update_status(
        &random_wallet,
        &id,
        &crate::ShipmentStatus::InTransit,
        &non_zero_hash(&env, 62),
    );
    assert!(result3.is_err(), "Random wallet must not update status");
}
