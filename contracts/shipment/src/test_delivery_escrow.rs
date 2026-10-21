#![cfg(test)]

extern crate std;

use super::test_helpers::*;

#[test]
fn test_confirm_delivery_success_in_transit() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let confirmation_hash = BytesN::from_array(&env, &[99u8; 32]);

    let (receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::InTransit,
    );

    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Delivered);

    // Verify confirmation hash was persisted on-chain
    let stored_hash = env.as_contract(&client.address, || {
        crate::storage::get_confirmation_hash(&env, shipment_id)
    });
    assert_eq!(stored_hash, Some(confirmation_hash));
}

#[test]
fn test_confirm_delivery_success_at_checkpoint() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let confirmation_hash = BytesN::from_array(&env, &[88u8; 32]);

    let (receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::AtCheckpoint,
    );

    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Delivered);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_confirm_delivery_wrong_receiver() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let confirmation_hash = BytesN::from_array(&env, &[77u8; 32]);
    let imposter = Address::generate(&env);

    let (_receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::InTransit,
    );

    // imposter is NOT the designated receiver — must fail with Unauthorized (error code 3)
    client.confirm_delivery(&imposter, &shipment_id, &confirmation_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_confirm_delivery_wrong_status() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let confirmation_hash = BytesN::from_array(&env, &[66u8; 32]);

    // Shipment starts in Created status, which is invalid for confirmation
    let (receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::Created,
    );

    // Must fail with InvalidStatus (error code 8)
    client.confirm_delivery(&receiver, &shipment_id, &confirmation_hash);
}

// Release Escrow Tests

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_release_escrow_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    client.release_escrow(&unauthorized, &shipment_id);
}

// Refund Escrow Tests

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_refund_escrow_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    let escrow_amount: i128 = 3000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.refund_escrow(&unauthorized, &shipment_id);
}

// Dispute Tests

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_raise_dispute_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[96u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.raise_dispute(&outsider, &shipment_id, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_resolve_dispute_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[92u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    let escrow_amount: i128 = 5000;

    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    client.raise_dispute(&company, &shipment_id, &reason_hash);

    client.resolve_dispute(
        &outsider,
        &shipment_id,
        &crate::DisputeResolution::ReleaseToCarrier,
        &reason_hash,
    );
}

// Milestone Event Tests

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_record_milestone_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[12u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let outsider = Address::generate(&env);
    let checkpoint = soroban_sdk::Symbol::new(&env, "port_arrival");

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::types::ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);
    });

    // Attempt to record with outsider should fail with CarrierNotAuthorized = 7
    client.record_milestone(&outsider, &shipment_id, &checkpoint, &data_hash);
}

// Batch Milestone Recording Tests

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_record_milestones_batch_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // Set shipment to InTransit status
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = crate::types::ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);
    });

    let outsider = Address::generate(&env);
    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((
        Symbol::new(&env, "warehouse"),
        BytesN::from_array(&env, &[10u8; 32]),
    ));

    // Should fail with Unauthorized error (code 3)
    client.record_milestones_batch(&outsider, &shipment_id, &milestones);
}

// TTL Extension Tests

// Cancel Shipment Tests

#[test]
fn test_cancel_shipment_without_escrow() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[2u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[88u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
    assert!(shipment.finalized);
    assert_eq!(client.get_escrow_balance(&shipment_id), 0);

    let events = env.events().all();
    let emitted_refund = events.iter().any(|(_contract, topics, _data)| {
        topics
            .iter()
            .any(|v| Symbol::from_val(&env, &v) == Symbol::new(&env, "escrow_refunded"))
    });
    assert!(
        !emitted_refund,
        "cancel_shipment must not refund escrow when none is held"
    );
}

#[test]
fn test_cancel_shipment_by_admin() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[3u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[66u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    client.cancel_shipment(&admin, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
    assert!(shipment.finalized);
    assert_eq!(client.get_escrow_balance(&shipment_id), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_cancel_shipment_delivered_should_fail() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let reason_hash = BytesN::from_array(&env, &[77u8; 32]);

    let (_receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::Delivered,
    );

    let shipment = client.get_shipment(&shipment_id);
    let company = shipment.sender;

    client.cancel_shipment(&company, &shipment_id, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_cancel_shipment_disputed_should_fail() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let reason_hash = BytesN::from_array(&env, &[55u8; 32]);

    let (_receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::Disputed,
    );

    let shipment = client.get_shipment(&shipment_id);
    let company = shipment.sender;

    client.cancel_shipment(&company, &shipment_id, &reason_hash);
}

// Escrow Lifecycle Integration Tests

#[test]
fn test_escrow_cancel_path_create_deposit_cancel_refund() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[4u8; 32]);
    let reason_hash = BytesN::from_array(&env, &[44u8; 32]);
    let escrow_amount: i128 = 5_000;
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
    assert!(shipment.finalized);
    assert_eq!(client.get_escrow_balance(&shipment_id), 0);
    env.as_contract(&client.address, || {
        assert!(
            !crate::storage::has_escrow_entry(&env, shipment_id),
            "escrow storage must be cleaned up exactly once on the cancel-refund path"
        );
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #18)")]
fn test_milestone_payment_invalid_sum_fails() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back((Symbol::new(&env, "m1"), 50));
    milestones.push_back((Symbol::new(&env, "m2"), 60)); // Total 110%

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
}

// Contract Upgrade Tests
