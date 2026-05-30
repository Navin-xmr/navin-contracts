extern crate std;

use crate::{test::setup_shipment_env, NavinError, ShipmentStatus};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Vec};

fn create_shipment_for(
    client: &crate::NavinShipmentClient<'static>,
    env: &soroban_sdk::Env,
    sender: &Address,
    receiver: &Address,
    carrier: &Address,
    marker: u8,
) -> u64 {
    let data_hash = BytesN::from_array(env, &[marker; 32]);
    let deadline = env.ledger().timestamp() + 3600;
    client.create_shipment(
        sender,
        receiver,
        carrier,
        &data_hash,
        &Vec::new(env),
        &deadline,
        &None,
    )
}

#[test]
fn test_get_shipments_batch_preserves_order_with_missing_ids() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id1 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 1);
    let id2 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 2);

    let mut ids = Vec::new(&env);
    ids.push_back(id2);
    ids.push_back(9999);
    ids.push_back(id1);

    let result = client.get_shipments_batch(&ids);
    assert_eq!(result.len(), 3);
    assert_eq!(result.get(0).unwrap().unwrap().id, id2);
    assert!(result.get(1).unwrap().is_none());
    assert_eq!(result.get(2).unwrap().unwrap().id, id1);
}

#[test]
fn test_get_shipments_batch_rejects_requests_over_hard_limit() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let mut ids = Vec::new(&env);
    for i in 0..51_u64 {
        ids.push_back(i + 1);
    }

    let result = client.try_get_shipments_batch(&ids);
    assert!(matches!(result, Err(Ok(NavinError::BatchTooLarge))));
}

#[test]
fn test_get_shipments_by_sender_with_pagination() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company_a = Address::generate(&env);
    let company_b = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company_a);
    client.add_company(&admin, &company_b);

    let a1 = create_shipment_for(&client, &env, &company_a, &receiver, &carrier, 11);
    let _b1 = create_shipment_for(&client, &env, &company_b, &receiver, &carrier, 12);
    let a2 = create_shipment_for(&client, &env, &company_a, &receiver, &carrier, 13);

    let page = client.get_shipments_by_sender_page(&company_a, &1, &1);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap().id, a2);
    assert_ne!(page.get(0).unwrap().id, a1);
}

#[test]
fn test_get_shipments_by_carrier_filters_subset() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier_a = Address::generate(&env);
    let carrier_b = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let _id1 = create_shipment_for(&client, &env, &company, &receiver, &carrier_a, 21);
    let id2 = create_shipment_for(&client, &env, &company, &receiver, &carrier_b, 22);
    let _id3 = create_shipment_for(&client, &env, &company, &receiver, &carrier_a, 23);

    let filtered = client.get_shipments_by_carrier(&carrier_b, &10);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered.get(0).unwrap().id, id2);
}

#[test]
fn test_get_shipments_by_status_paginated() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let s1 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 31);
    let s2 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 32);

    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, s1).unwrap();
        shipment.status = ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);

        let mut shipment = crate::storage::get_shipment(&env, s2).unwrap();
        shipment.status = ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &shipment);
    });

    let page = client.get_shipments_by_status_page(&ShipmentStatus::InTransit, &1, &1);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap().id, s2);
}

#[test]
fn test_get_shipments_by_status_rejects_zero_limit() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let result = client.try_get_shipments_by_status(&ShipmentStatus::Created, &0);
    assert!(matches!(result, Err(Ok(NavinError::InvalidConfig))));
}

// ── Regression: batch vs individual read consistency ─────────────────────────

/// Create shipments with distinct senders and carriers, then verify that every
/// field returned by `get_shipments_batch` exactly matches the corresponding
/// `get_shipment` single-fetch result.
#[test]
fn test_batch_and_individual_reads_agree() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let sender_a = Address::generate(&env);
    let sender_b = Address::generate(&env);
    let carrier_a = Address::generate(&env);
    let carrier_b = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &sender_a);
    client.add_company(&admin, &sender_b);

    // Three shipments: two different senders, two different carriers.
    let id1 = create_shipment_for(&client, &env, &sender_a, &receiver, &carrier_a, 0xAA);
    let id2 = create_shipment_for(&client, &env, &sender_b, &receiver, &carrier_b, 0xBB);
    let id3 = create_shipment_for(&client, &env, &sender_a, &receiver, &carrier_b, 0xCC);

    let mut ids = Vec::new(&env);
    ids.push_back(id1);
    ids.push_back(id2);
    ids.push_back(id3);

    let batch = client.get_shipments_batch(&ids);
    assert_eq!(batch.len(), 3);

    for (i, id) in [id1, id2, id3].iter().enumerate() {
        let from_batch = batch.get(i as u32).unwrap().unwrap();
        let individual = client.get_shipment(id);

        assert_eq!(from_batch.id, individual.id);
        assert_eq!(from_batch.sender, individual.sender);
        assert_eq!(from_batch.carrier, individual.carrier);
        assert_eq!(from_batch.receiver, individual.receiver);
        assert_eq!(from_batch.status, individual.status);
        assert_eq!(from_batch.data_hash, individual.data_hash);
        assert_eq!(from_batch.escrow_amount, individual.escrow_amount);
    }
}

/// `get_shipment_count` must equal the number of shipments actually created,
/// checked after each individual insert.
#[test]
fn test_shipment_count_increments_after_each_create() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    assert_eq!(client.get_shipment_count(), 0);

    for n in 1u8..=5 {
        create_shipment_for(&client, &env, &company, &receiver, &carrier, n);
        assert_eq!(client.get_shipment_count(), n as u64);
    }
}

/// Test batch-operation limits enforcement
#[test]
fn test_batch_operation_limits_enforcement() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Get current config to understand current limits
    let config = client.get_contract_config();
    
    // Test with current batch_operation_limit
    let limit = config.batch_operation_limit;
    
    // Create a batch that exceeds the limit
    let mut ids = Vec::new(&env);
    for i in 0..(limit + 1) as u64 {
        ids.push_back(i + 1);
    }
    
    let result = client.try_get_shipments_batch(&ids);
    assert!(
        matches!(result, Err(Ok(crate::NavinError::BatchTooLarge))),
        "batch_operation_limit should reject batches larger than configured limit"
    );
    
    // Test with batch that matches the limit exactly
    let mut ids_exact = Vec::new(&env);
    for i in 0..limit as u64 {
        ids_exact.push_back(i + 1);
    }
    
    // This should succeed
    let result_exact = client.try_get_shipments_batch(&ids_exact);
    assert!(
        result_exact.is_ok(),
        "batch_operation_limit should allow batches up to configured limit"
    );
}

/// `get_shipment_count` must equal the number of shipments actually created,
/// checked after each individual insert.
#[test]
fn test_shipment_count_increments_after_each_create() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    assert_eq!(client.get_shipment_count(), 0);

    for n in 1u8..=5 {
        create_shipment_for(&client, &env, &company, &receiver, &carrier, n);
        assert_eq!(client.get_shipment_count(), n as u64);
    }
}

/// Batch query over a mix of valid and non-existent IDs: valid entries must
/// match individual reads and missing IDs must produce `None`.
#[test]
fn test_batch_handles_missing_ids_gracefully() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id1 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 0x01);
    let id2 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 0x02);

    // Mix real IDs with IDs that were never created.
    let mut ids = Vec::new(&env);
    ids.push_back(id1);
    ids.push_back(9000_u64);
    ids.push_back(id2);
    ids.push_back(9001_u64);

    let batch = client.get_shipments_batch(&ids);
    assert_eq!(batch.len(), 4);

    // Real IDs agree with individual reads.
    let s1 = batch.get(0).unwrap().unwrap();
    assert_eq!(s1.id, client.get_shipment(&id1).id);

    // Missing IDs return None.
    assert!(batch.get(1).unwrap().is_none());

    let s2 = batch.get(2).unwrap().unwrap();
    assert_eq!(s2.id, client.get_shipment(&id2).id);

    assert!(batch.get(3).unwrap().is_none());
}

/// `get_shipment_count` must equal the total number of shipments created across
/// multiple senders and carriers.
#[test]
fn test_shipment_count_matches_across_multiple_senders_and_carriers() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let sender_a = Address::generate(&env);
    let sender_b = Address::generate(&env);
    let carrier_a = Address::generate(&env);
    let carrier_b = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &sender_a);
    client.add_company(&admin, &sender_b);

    create_shipment_for(&client, &env, &sender_a, &receiver, &carrier_a, 0x10);
    assert_eq!(client.get_shipment_count(), 1);

    create_shipment_for(&client, &env, &sender_b, &receiver, &carrier_b, 0x20);
    assert_eq!(client.get_shipment_count(), 2);

    create_shipment_for(&client, &env, &sender_a, &receiver, &carrier_b, 0x30);
    assert_eq!(client.get_shipment_count(), 3);

    create_shipment_for(&client, &env, &sender_b, &receiver, &carrier_a, 0x40);
    assert_eq!(client.get_shipment_count(), 4);

    // Batch over all IDs must return 4 non-None entries.
    let mut ids = Vec::new(&env);
    for i in 1..=4_u64 {
        ids.push_back(i);
    }
    let batch = client.get_shipments_batch(&ids);
    assert_eq!(batch.len(), 4);
    for i in 0..4_u32 {
        assert!(batch.get(i).unwrap().is_some());
    }
}

/// Sender-filter and carrier-filter queries must be consistent with the full
/// batch result: every shipment returned by a filter must appear in the batch
/// with identical fields.
#[test]
fn test_filter_queries_consistent_with_batch() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let sender_a = Address::generate(&env);
    let sender_b = Address::generate(&env);
    let carrier_a = Address::generate(&env);
    let carrier_b = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &sender_a);
    client.add_company(&admin, &sender_b);

    let id1 = create_shipment_for(&client, &env, &sender_a, &receiver, &carrier_a, 0x11);
    let id2 = create_shipment_for(&client, &env, &sender_b, &receiver, &carrier_b, 0x22);
    let id3 = create_shipment_for(&client, &env, &sender_a, &receiver, &carrier_b, 0x33);

    // Fetch all via batch.
    let mut ids = Vec::new(&env);
    ids.push_back(id1);
    ids.push_back(id2);
    ids.push_back(id3);
    let batch = client.get_shipments_batch(&ids);

    // sender_a filter must return id1 and id3.
    let by_sender = client.get_shipments_by_sender(&sender_a, &10);
    assert_eq!(by_sender.len(), 2);
    for s in by_sender.iter() {
        // Find the matching entry in the batch and compare.
        let batch_entry = batch
            .iter()
            .find_map(|opt| opt.filter(|b| b.id == s.id))
            .expect("shipment from sender filter must be in batch");
        assert_eq!(batch_entry.sender, s.sender);
        assert_eq!(batch_entry.carrier, s.carrier);
        assert_eq!(batch_entry.status, s.status);
    }

    // carrier_b filter must return id2 and id3.
    let by_carrier = client.get_shipments_by_carrier(&carrier_b, &10);
    assert_eq!(by_carrier.len(), 2);
    for s in by_carrier.iter() {
        let batch_entry = batch
            .iter()
            .find_map(|opt| opt.filter(|b| b.id == s.id))
            .expect("shipment from carrier filter must be in batch");
        assert_eq!(batch_entry.carrier, s.carrier);
        assert_eq!(batch_entry.status, s.status);
    }
}
