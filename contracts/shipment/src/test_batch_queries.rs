extern crate std;

use crate::{storage, test::setup_shipment_env, NavinError, ShipmentStatus};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

/// Directly mutate a shipment's status in persistent storage. Used to set up
/// status-filtered query fixtures without driving the full lifecycle.
fn set_status(
    env: &Env,
    client: &crate::NavinShipmentClient<'static>,
    id: u64,
    status: ShipmentStatus,
) {
    env.as_contract(&client.address, || {
        let mut shipment = storage::get_shipment(env, id).unwrap();
        shipment.status = status;
        storage::set_shipment(env, &shipment);
    });
}

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

// ── Issue #701: Batch participant validation ───────────────────────────────

/// Issue #701 — create_shipments_batch must reject when sender == receiver
#[test]
fn issue_701_batch_rejects_sender_equals_receiver() {
    use crate::ShipmentInput;
    use soroban_sdk::Symbol;

    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
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

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[1; 32]);

    let mut shipments = Vec::new(&env);
    shipments.push_back(ShipmentInput {
        receiver: company.clone(), // sender == receiver (invalid)
        carrier: carrier.clone(),
        data_hash: data_hash.clone(),
        payment_milestones: Vec::new(&env),
        deadline,
    });

    let result = client.try_create_shipments_batch(&company, &shipments);
    assert!(result.is_err(), "batch creation must fail when sender == receiver");
    assert_eq!(
        result.err(),
        Some(Ok(NavinError::InvalidShipmentParticipants)),
        "expected InvalidShipmentParticipants error"
    );
}

/// Issue #701 — create_shipments_batch must reject when sender == carrier
#[test]
fn issue_701_batch_rejects_sender_equals_carrier() {
    use crate::ShipmentInput;
    use soroban_sdk::Symbol;

    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
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

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[2; 32]);

    let mut shipments = Vec::new(&env);
    shipments.push_back(ShipmentInput {
        receiver: receiver.clone(),
        carrier: company.clone(), // sender == carrier (invalid)
        data_hash: data_hash.clone(),
        payment_milestones: Vec::new(&env),
        deadline,
    });

    let result = client.try_create_shipments_batch(&company, &shipments);
    assert!(result.is_err(), "batch creation must fail when sender == carrier");
    assert_eq!(
        result.err(),
        Some(Ok(NavinError::InvalidShipmentParticipants)),
        "expected InvalidShipmentParticipants error"
    );
}

/// Issue #701 — create_shipments_batch must reject when receiver == carrier
#[test]
fn issue_701_batch_rejects_receiver_equals_carrier() {
    use crate::ShipmentInput;
    use soroban_sdk::Symbol;

    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
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

// ── Carrier-page shipment lookup (issue #702) ──────────────────────────────────

/// Single page should return every shipment assigned to the queried carrier
/// and nothing for any other carrier.
#[test]
fn test_get_shipments_by_carrier_page_single_page() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier_a = Address::generate(&env);
    let carrier_b = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let a1 = create_shipment_for(&client, &env, &company, &receiver, &carrier_a, 71);
    let _b1 = create_shipment_for(&client, &env, &company, &receiver, &carrier_b, 72);
    let a2 = create_shipment_for(&client, &env, &company, &receiver, &carrier_a, 73);

    // Large limit captures both carrier_a shipments in one page.
    let page = client.get_shipments_by_carrier_page(&carrier_a, &0, &10);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap().id, a1);
    assert_eq!(page.get(1).unwrap().id, a2);

    // The other carrier sees only its own shipment.
    let only_b = client.get_shipments_by_carrier_page(&carrier_b, &0, &10);
    assert_eq!(only_b.len(), 1);
    assert_eq!(only_b.get(0).unwrap().id, _b1);
}

/// Pagination boundaries: offset/limit slicing must behave consistently with
/// the sender-page tests and never return shipments from other carriers.
#[test]
fn test_get_shipments_by_carrier_page_pagination() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier_a = Address::generate(&env);
    let carrier_b = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[3; 32]);

    let mut shipments = Vec::new(&env);
    shipments.push_back(ShipmentInput {
        receiver: receiver.clone(),
        carrier: receiver.clone(), // receiver == carrier (invalid)
        data_hash: data_hash.clone(),
        payment_milestones: Vec::new(&env),
        deadline,
    });

    let result = client.try_create_shipments_batch(&company, &shipments);
    assert!(result.is_err(), "batch creation must fail when receiver == carrier");
    assert_eq!(
        result.err(),
        Some(Ok(NavinError::InvalidShipmentParticipants)),
        "expected InvalidShipmentParticipants error"
    );
}

/// Issue #701 — create_shipments_batch should succeed with distinct participants
#[test]
fn issue_701_batch_succeeds_with_distinct_participants() {
    use crate::ShipmentInput;
    use soroban_sdk::Symbol;

    let a1 = create_shipment_for(&client, &env, &company, &receiver, &carrier_a, 81);
    let a2 = create_shipment_for(&client, &env, &company, &receiver, &carrier_a, 82);
    let _b1 = create_shipment_for(&client, &env, &company, &receiver, &carrier_b, 83);
    let a3 = create_shipment_for(&client, &env, &company, &receiver, &carrier_a, 84);

    // Page 1: first 2 of carrier_a only.
    let page1 = client.get_shipments_by_carrier_page(&carrier_a, &0, &2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap().id, a1);
    assert_eq!(page1.get(1).unwrap().id, a2);

    // Page 2: remaining 1, offset past the first two.
    let page2 = client.get_shipments_by_carrier_page(&carrier_a, &2, &2);
    assert_eq!(page2.len(), 1);
    assert_eq!(page2.get(0).unwrap().id, a3);

    // Offset equal to the match count yields an empty page (boundary).
    let boundary = client.get_shipments_by_carrier_page(&carrier_a, &3, &2);
    assert_eq!(boundary.len(), 0);

    // Offset beyond the match count also yields an empty page.
    let past_end = client.get_shipments_by_carrier_page(&carrier_a, &100, &2);
    assert_eq!(past_end.len(), 0);
}

/// A carrier with zero shipments must return an empty result.
#[test]
fn test_get_shipments_by_carrier_page_empty_result() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier_a = Address::generate(&env);
    let carrier_b = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let _a1 = create_shipment_for(&client, &env, &company, &receiver, &carrier_a, 91);

    let empty = client.get_shipments_by_carrier_page(&carrier_b, &0, &10);
    assert_eq!(empty.len(), 0);
}

/// Zero limit must be rejected the same way as the other paged queries.
#[test]
fn test_get_shipments_by_carrier_page_rejects_zero_limit() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let result = client.try_get_shipments_by_carrier_page(&Address::generate(&_env), &0, &0);
    assert!(matches!(result, Err(Ok(NavinError::InvalidConfig))));
}

// ── Status-search cursor pagination (issue #705) ──────────────────────────────

/// Cursor must advance across multiple pages, returning each matching shipment
/// exactly once, and produce `None` once the result set is exhausted.
#[test]
fn test_search_shipments_by_status_cursor_advances() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // Only a subset are flipped to InTransit; the rest stay Created so the
    // filter is actually exercised against non-matching shipments.
    let s1 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 101);
    let _other1 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 102);
    let s2 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 103);
    let _other2 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 104);
    let s3 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 105);

    set_status(&env, &client, s1, ShipmentStatus::InTransit);
    set_status(&env, &client, s2, ShipmentStatus::InTransit);
    set_status(&env, &client, s3, ShipmentStatus::InTransit);

    let page1 = client.search_shipments_by_status(&ShipmentStatus::InTransit, &None, &1);
    assert_eq!(page1.shipment_ids.len(), 1);
    assert_eq!(page1.shipment_ids.get(0).unwrap(), s1);
    assert_eq!(page1.next_cursor, Some(s1));

    let page2 =
        client.search_shipments_by_status(&ShipmentStatus::InTransit, &page1.next_cursor, &1);
    assert_eq!(page2.shipment_ids.len(), 1);
    assert_eq!(page2.shipment_ids.get(0).unwrap(), s2);
    assert_eq!(page2.next_cursor, Some(s2));

    let page3 =
        client.search_shipments_by_status(&ShipmentStatus::InTransit, &page2.next_cursor, &1);
    assert_eq!(page3.shipment_ids.len(), 1);
    assert_eq!(page3.shipment_ids.get(0).unwrap(), s3);
    assert_eq!(page3.next_cursor, None);
}

/// Multi-match page larger than one must still advance correctly and stop.
#[test]
fn test_search_shipments_by_status_cursor_multi_match_page() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[4; 32]);

    let mut shipments = Vec::new(&env);
    shipments.push_back(ShipmentInput {
        receiver: receiver.clone(),
        carrier: carrier.clone(),
        data_hash: data_hash.clone(),
        payment_milestones: Vec::new(&env),
        deadline,
    });

    let result = client.try_create_shipments_batch(&company, &shipments);
    assert!(result.is_ok(), "batch creation must succeed with all distinct participants");
    let ids = result.unwrap();
    assert_eq!(ids.len(), 1, "batch must return one shipment ID");
}

/// Issue #701 — atomicity: batch with one invalid participant rejects entire batch
#[test]
fn issue_701_batch_atomicity_one_invalid_rejects_all() {
    use crate::ShipmentInput;
    use soroban_sdk::Symbol;

    let s1 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 111);
    let s2 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 112);
    let s3 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 113);
    let s4 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 114);

    for id in [s1, s2, s3, s4] {
        set_status(&env, &client, id, ShipmentStatus::InTransit);
    }

    // Page size 2: first page returns two, cursor points at the last returned id.
    let page1 = client.search_shipments_by_status(&ShipmentStatus::InTransit, &None, &2);
    assert_eq!(page1.shipment_ids.len(), 2);
    assert_eq!(page1.shipment_ids.get(0).unwrap(), s1);
    assert_eq!(page1.shipment_ids.get(1).unwrap(), s2);
    assert_eq!(page1.next_cursor, Some(s2));

    let page2 =
        client.search_shipments_by_status(&ShipmentStatus::InTransit, &page1.next_cursor, &2);
    assert_eq!(page2.shipment_ids.len(), 2);
    assert_eq!(page2.shipment_ids.get(0).unwrap(), s3);
    assert_eq!(page2.shipment_ids.get(1).unwrap(), s4);
    assert_eq!(page2.next_cursor, None);
}

/// End-of-results: a cursor at/after the last id returns an empty page and a
/// `None` cursor without panicking or looping.
#[test]
fn test_search_shipments_by_status_cursor_end_of_results() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let s1 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 121);
    set_status(&env, &client, s1, ShipmentStatus::InTransit);

    // Cursor already at the only matching id -> no further matches.
    let after = client.search_shipments_by_status(&ShipmentStatus::InTransit, &Some(s1), &5);
    assert_eq!(after.shipment_ids.len(), 0);
    assert_eq!(after.next_cursor, None);

    // Cursor far beyond any shipment id -> loop must terminate immediately.
    let beyond = client.search_shipments_by_status(&ShipmentStatus::InTransit, &Some(9999), &5);
    assert_eq!(beyond.shipment_ids.len(), 0);
    assert_eq!(beyond.next_cursor, None);
}

/// Filtering by a status with zero matching shipments must return a clean
/// empty result and a `None` cursor.
#[test]
fn test_search_shipments_by_status_zero_match() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let deadline = env.ledger().timestamp() + 3600;

    let mut shipments = Vec::new(&env);
    
    // First shipment: valid
    shipments.push_back(ShipmentInput {
        receiver: receiver.clone(),
        carrier: carrier.clone(),
        data_hash: BytesN::from_array(&env, &[5; 32]),
        payment_milestones: Vec::new(&env),
        deadline,
    });
    
    // Second shipment: invalid (sender == receiver)
    shipments.push_back(ShipmentInput {
        receiver: company.clone(), // sender == receiver (invalid)
        carrier: carrier.clone(),
        data_hash: BytesN::from_array(&env, &[6; 32]),
        payment_milestones: Vec::new(&env),
        deadline,
    });

    let result = client.try_create_shipments_batch(&company, &shipments);
    assert!(result.is_err(), "batch must be fully rejected when any shipment is invalid");
    assert_eq!(
        result.err(),
        Some(Ok(NavinError::InvalidShipmentParticipants)),
        "expected InvalidShipmentParticipants error"
    );

    // Verify no shipments were created
    let count = client.get_active_shipment_count(&company);
    assert_eq!(count, 0, "no shipments should be created when batch is rejected");
    let _s1 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 131);
    let _s2 = create_shipment_for(&client, &env, &company, &receiver, &carrier, 132);

    let result = client.search_shipments_by_status(&ShipmentStatus::Cancelled, &None, &10);
    assert_eq!(result.shipment_ids.len(), 0);
    assert_eq!(result.next_cursor, None);
}
