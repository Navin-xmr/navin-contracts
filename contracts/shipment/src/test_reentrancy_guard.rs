extern crate std;

use crate::{test::setup_shipment_env, types::DataKey, NavinError};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Vec};

fn setup_single_shipment() -> (
    soroban_sdk::Env,
    crate::NavinShipmentClient<'static>,
    Address,
    Address,
    Address,
    Address,
    u64,
) {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let data_hash = BytesN::from_array(&env, &[9u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    (env, client, admin, company, receiver, carrier, shipment_id)
}

#[test]
fn test_deposit_escrow_rejected_when_reentrancy_lock_is_preheld() {
    let (env, client, _admin, company, _receiver, _carrier, shipment_id) = setup_single_shipment();

    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &true);
    });

    let result = client.try_deposit_escrow(&company, &shipment_id, &1000);
    assert_eq!(result, Err(Ok(NavinError::ReentrancyDetected)));
}

#[test]
fn test_release_escrow_rejected_when_reentrancy_lock_is_preheld() {
    let (env, client, _admin, company, receiver, _carrier, shipment_id) = setup_single_shipment();

    client.deposit_escrow(&company, &shipment_id, &1000);
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &true);
    });

    let result = client.try_release_escrow(&receiver, &shipment_id);
    assert_eq!(result, Err(Ok(NavinError::ReentrancyDetected)));
}

#[test]
fn test_refund_escrow_rejected_when_reentrancy_lock_is_preheld() {
    let (env, client, _admin, company, _receiver, _carrier, shipment_id) = setup_single_shipment();

    client.deposit_escrow(&company, &shipment_id, &1000);
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &true);
    });

    let result = client.try_refund_escrow(&company, &shipment_id);
    assert_eq!(result, Err(Ok(NavinError::ReentrancyDetected)));
}

#[test]
fn test_reentrancy_lock_is_released_after_successful_operation() {
    let (env, client, _admin, company, _receiver, _carrier, shipment_id) = setup_single_shipment();

    client.deposit_escrow(&company, &shipment_id, &1000);

    env.as_contract(&client.address, || {
        let locked = env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::ReentrancyLock)
            .unwrap_or(false);
        assert!(!locked, "reentrancy lock should be cleared after operation");
    });
}

#[test]
fn test_reentrancy_lock_released_after_failed_operation() {
    let (env, client, _admin, company, receiver, _carrier, shipment_id) = setup_single_shipment();

    client.deposit_escrow(&company, &shipment_id, &1000);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    let result = client.try_deposit_escrow(&company, &shipment_id, &500);
    assert!(result.is_err(), "operation should fail due to wrong status");

    env.as_contract(&client.address, || {
        let locked = env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::ReentrancyLock)
            .unwrap_or(false);
        assert!(
            !locked,
            "lock must be released after failed operation inside guard"
        );
    });

    let release_result = client.try_release_escrow(&receiver, &shipment_id);
    assert!(
        release_result.is_ok(),
        "subsequent operations must succeed after lock release"
    );
}

#[test]
fn test_nested_fixture_bypass_attempt_rejected() {
    let (env, client, _admin, company, receiver, _carrier, shipment_id) = setup_single_shipment();

    client.deposit_escrow(&company, &shipment_id, &1000);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &true);
    });

    let lock_before = env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::ReentrancyLock)
            .unwrap_or(false)
    });
    assert!(lock_before, "lock should be held before bypass attempt");

    let result = client.try_release_escrow(&receiver, &shipment_id);
    assert_eq!(
        result,
        Err(Ok(NavinError::ReentrancyDetected)),
        "bypass attempt must be rejected"
    );

    env.as_contract(&client.address, || {
        let locked = env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::ReentrancyLock)
            .unwrap_or(false);
        assert!(
            locked,
            "lock must remain held after bypass rejection (outer op still active)"
        );
    });

    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &false);
    });

    let result = client.try_release_escrow(&receiver, &shipment_id);
    assert!(
        result.is_ok(),
        "operation must succeed after manual lock clear"
    );
}

#[test]
fn test_multiple_guard_operations_lock_stays_blocked() {
    let (env, client, _admin, company, receiver, _carrier, shipment_id) = setup_single_shipment();

    client.deposit_escrow(&company, &shipment_id, &1000);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &true);
    });

    let _ = client.try_release_escrow(&receiver, &shipment_id);
    let _ = client.try_refund_escrow(&company, &shipment_id);
    let _ = client.try_deposit_escrow(&company, &shipment_id, &500);

    env.as_contract(&client.address, || {
        let locked = env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::ReentrancyLock)
            .unwrap_or(false);
        assert!(locked, "lock must remain held after all bypass rejections");
    });

    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &false);
    });

    let result = client.try_deposit_escrow(&company, &shipment_id, &500);
    assert!(
        result.is_err(),
        "deposit on delivered should still fail for status reason"
    );
}

// ── Additional tests for issue #616 ──────────────────────────────────────────

#[test]
fn test_reentrancy_detected_error_code_is_15() {
    let (env, client, _admin, company, _receiver, _carrier, shipment_id) = setup_single_shipment();

    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &true);
    });

    let result = client.try_deposit_escrow(&company, &shipment_id, &500);
    assert_eq!(
        result,
        Err(Ok(NavinError::ReentrancyDetected)),
        "ReentrancyDetected discriminant must be 15"
    );
    assert_eq!(NavinError::ReentrancyDetected as u32, 15);
}

#[test]
fn test_normal_deposit_succeeds_without_lock() {
    let (env, client, _admin, company, _receiver, _carrier, shipment_id) = setup_single_shipment();

    // Confirm lock is NOT held before the call
    env.as_contract(&client.address, || {
        let locked = env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::ReentrancyLock)
            .unwrap_or(false);
        assert!(!locked, "lock must not be held before a normal operation");
    });

    let result = client.try_deposit_escrow(&company, &shipment_id, &1000);
    assert!(
        result.is_ok(),
        "deposit without reentrancy lock must succeed"
    );
}

#[test]
fn test_normal_refund_succeeds_without_lock() {
    let (env, client, _admin, company, _receiver, _carrier, shipment_id) = setup_single_shipment();

    client.deposit_escrow(&company, &shipment_id, &500);

    // Ensure lock is clear
    env.as_contract(&client.address, || {
        let locked = env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::ReentrancyLock)
            .unwrap_or(false);
        assert!(!locked, "lock must be released after successful deposit");
    });

    let result = client.try_refund_escrow(&company, &shipment_id);
    assert!(
        result.is_ok(),
        "refund without reentrancy lock must succeed"
    );
}

#[test]
fn test_lock_preheld_blocks_all_three_escrow_ops() {
    let (env, client, _admin, company, receiver, _carrier, shipment_id) = setup_single_shipment();

    client.deposit_escrow(&company, &shipment_id, &1000);

    // Pre-hold the lock to simulate mid-execution state
    env.as_contract(&client.address, || {
        let mut s = crate::storage::get_shipment(&env, shipment_id).unwrap();
        s.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &s);
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &true);
    });

    let dep = client.try_deposit_escrow(&company, &shipment_id, &100);
    let rel = client.try_release_escrow(&receiver, &shipment_id);
    let ref_ = client.try_refund_escrow(&company, &shipment_id);

    assert_eq!(dep, Err(Ok(NavinError::ReentrancyDetected)), "deposit must be blocked");
    assert_eq!(rel, Err(Ok(NavinError::ReentrancyDetected)), "release must be blocked");
    assert_eq!(ref_, Err(Ok(NavinError::ReentrancyDetected)), "refund must be blocked");
}
