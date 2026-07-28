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

// ── Batch vs. single-read consistency (issue #445) ───────────────────────────

#[test]
fn test_batch_results_match_individual_reads() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id1 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 0xA1);
    let id2 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 0xA2);
    let id3 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 0xA3);

    let mut ids = Vec::new(&env);
    ids.push_back(id1);
    ids.push_back(id2);
    ids.push_back(id3);

    let batch = client.get_shipments_batch(&ids);

    assert_eq!(batch.len(), 3);
    for (i, expected_id) in [id1, id2, id3].iter().enumerate() {
        let batch_result = batch.get(i as u32).unwrap();
        let single_result = client.get_shipment(expected_id);
        let batch_shipment = batch_result.unwrap();
        assert_eq!(
            batch_shipment.id, single_result.id,
            "batch[{}] id must match single read",
            i
        );
        assert_eq!(
            batch_shipment.status, single_result.status,
            "batch[{}] status must match single read",
            i
        );
        assert_eq!(
            batch_shipment.sender, single_result.sender,
            "batch[{}] sender must match single read",
            i
        );
    }
}

#[test]
fn test_missing_id_returns_none_in_batch_and_single() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id1 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 0xB1);

    let missing_id: u64 = 9999;

    let mut ids = Vec::new(&env);
    ids.push_back(id1);
    ids.push_back(missing_id);

    let batch = client.get_shipments_batch(&ids);

    // Existing id returns Some
    assert!(
        batch.get(0).unwrap().is_some(),
        "existing id must be Some in batch"
    );
    // Missing id returns None — matching behaviour of a missing single read
    assert!(
        batch.get(1).unwrap().is_none(),
        "missing id must be None in batch, matching single read None"
    );
    // Single get for the missing id panics (ShipmentNotFound), so we use try_
    let single_missing = client.try_get_shipment(&missing_id);
    assert!(
        single_missing.is_err(),
        "single get of missing id must return an error"
    );
}

#[test]
fn test_batch_query_is_deterministic_across_calls() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id1 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 0xC1);
    let id2 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 0xC2);

    let mut ids = Vec::new(&env);
    ids.push_back(id1);
    ids.push_back(9999_u64); // missing
    ids.push_back(id2);

    let first_call = client.get_shipments_batch(&ids);
    let second_call = client.get_shipments_batch(&ids);

    assert_eq!(
        first_call.len(),
        second_call.len(),
        "batch length must be stable across calls"
    );
    for i in 0..first_call.len() {
        let a = first_call.get(i).unwrap();
        let b = second_call.get(i).unwrap();
        match (a, b) {
            (Some(sa), Some(sb)) => {
                assert_eq!(sa.id, sb.id, "slot {} id must be identical", i);
                assert_eq!(sa.status, sb.status, "slot {} status must be identical", i);
            }
            (None, None) => {} // both missing — stable
            _ => panic!("slot {} presence changed between calls", i),
        }
    }
}

#[test]
fn test_batch_all_present_matches_individual_reads_field_by_field() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let ids_arr: [u64; 5] = [
        create_shipment_for(&client, &env, &company, &receiver, &carrier, 0xD1),
        create_shipment_for(&client, &env, &company, &receiver, &carrier, 0xD2),
        create_shipment_for(&client, &env, &company, &receiver, &carrier, 0xD3),
        create_shipment_for(&client, &env, &company, &receiver, &carrier, 0xD4),
        create_shipment_for(&client, &env, &company, &receiver, &carrier, 0xD5),
    ];

    let mut ids_vec = Vec::new(&env);
    for id in &ids_arr {
        ids_vec.push_back(*id);
    }

    let batch = client.get_shipments_batch(&ids_vec);
    assert_eq!(batch.len(), 5);

    for (i, expected_id) in ids_arr.iter().enumerate() {
        let single = client.get_shipment(expected_id);
        let batch_item = batch.get(i as u32).unwrap().unwrap();
        assert_eq!(batch_item.id, single.id);
        assert_eq!(batch_item.sender, single.sender);
        assert_eq!(batch_item.carrier, single.carrier);
        assert_eq!(batch_item.receiver, single.receiver);
        assert_eq!(batch_item.status, single.status);
        assert_eq!(batch_item.escrow_amount, single.escrow_amount);
        assert_eq!(batch_item.finalized, single.finalized);
    }
}

// ── Receiver-side shipment lookup (issue #644) ────────────────────────────────

#[test]
fn test_get_shipments_by_receiver_with_pagination() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver_a = Address::generate(&env);
    let receiver_b = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let r1 = create_shipment_for(&client, &env, &company, &receiver_a, &carrier, 41);
    let _rb = create_shipment_for(&client, &env, &company, &receiver_b, &carrier, 42);
    let r2 = create_shipment_for(&client, &env, &company, &receiver_a, &carrier, 43);

    let all_receiver_a = client.get_shipments_by_receiver(&receiver_a, &10);
    assert_eq!(all_receiver_a.len(), 2);
    assert_eq!(all_receiver_a.get(0).unwrap().id, r1);
    assert_eq!(all_receiver_a.get(1).unwrap().id, r2);

    let page = client.get_shipments_by_receiver_page(&receiver_a, &1, &1);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap().id, r2);
}

// ── Cursor-based shipment pagination (issue #645) ─────────────────────────────

#[test]
fn test_search_shipments_by_sender_cursor_pagination() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company_a = Address::generate(&env);
    let company_b = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company_a);
    client.add_company(&admin, &company_b);

    let a1 = create_shipment_for(&client, &env, &company_a, &receiver, &carrier, 51);
    let _b1 = create_shipment_for(&client, &env, &company_b, &receiver, &carrier, 52);
    let a2 = create_shipment_for(&client, &env, &company_a, &receiver, &carrier, 53);
    let a3 = create_shipment_for(&client, &env, &company_a, &receiver, &carrier, 54);

    // Page 1: page size 2
    let page1 = client.search_shipments_by_sender(&company_a, &None, &2);
    assert_eq!(page1.shipment_ids.len(), 2);
    assert_eq!(page1.shipment_ids.get(0).unwrap(), a1);
    assert_eq!(page1.shipment_ids.get(1).unwrap(), a2);
    assert_eq!(page1.next_cursor, Some(a2));

    // Page 2: pass cursor Some(a2)
    let page2 = client.search_shipments_by_sender(&company_a, &page1.next_cursor, &2);
    assert_eq!(page2.shipment_ids.len(), 1);
    assert_eq!(page2.shipment_ids.get(0).unwrap(), a3);
    assert_eq!(page2.next_cursor, None);
}

#[test]
fn test_search_shipments_by_carrier_cursor_pagination() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier_a = Address::generate(&env);
    let carrier_b = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let c1 = create_shipment_for(&client, &env, &company, &receiver, &carrier_a, 61);
    let _cb = create_shipment_for(&client, &env, &company, &receiver, &carrier_b, 62);
    let c2 = create_shipment_for(&client, &env, &company, &receiver, &carrier_a, 63);

    let page1 = client.search_shipments_by_carrier(&carrier_a, &None, &1);
    assert_eq!(page1.shipment_ids.len(), 1);
    assert_eq!(page1.shipment_ids.get(0).unwrap(), c1);
    assert_eq!(page1.next_cursor, Some(c1));

    let page2 = client.search_shipments_by_carrier(&carrier_a, &page1.next_cursor, &1);
    assert_eq!(page2.shipment_ids.len(), 1);
    assert_eq!(page2.shipment_ids.get(0).unwrap(), c2);
    assert_eq!(page2.next_cursor, None);
}
