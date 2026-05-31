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
fn test_shipment_count_increments_after_each_create_regression() {
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

// ── Regression: sender and carrier offset pagination ─────────────────────────

/// Offset-based pagination uses the returned page length as the next cursor.
fn next_page_offset(current_offset: u32, page_len: u32, total_matches: u32) -> Option<u32> {
    let advanced = current_offset.saturating_add(page_len);
    if advanced < total_matches {
        Some(advanced)
    } else {
        None
    }
}

fn collect_sender_ids(
    env: &soroban_sdk::Env,
    client: &crate::NavinShipmentClient<'static>,
    sender: &Address,
    limit: u32,
) -> soroban_sdk::Vec<u64> {
    let all = client.get_shipments_by_sender(sender, &limit);
    let mut ids = Vec::new(env);
    for s in all.iter() {
        ids.push_back(s.id);
    }
    ids
}

fn collect_carrier_ids(
    env: &soroban_sdk::Env,
    client: &crate::NavinShipmentClient<'static>,
    carrier: &Address,
    limit: u32,
) -> soroban_sdk::Vec<u64> {
    let all = client.get_shipments_by_carrier(carrier, &limit);
    let mut ids = Vec::new(env);
    for s in all.iter() {
        ids.push_back(s.id);
    }
    ids
}

/// Page-one and page-two sender pagination with explicit next_cursor validation.
#[test]
fn test_sender_pagination_page_one_and_two_with_next_cursor() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let sender = Address::generate(&env);
    let other_sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &sender);
    client.add_company(&admin, &other_sender);

    let id1 = create_shipment_for(&client, &env, &sender, &receiver, &carrier, 0x41);
    let _other = create_shipment_for(&client, &env, &other_sender, &receiver, &carrier, 0x42);
    let id2 = create_shipment_for(&client, &env, &sender, &receiver, &carrier, 0x43);
    let id3 = create_shipment_for(&client, &env, &sender, &receiver, &carrier, 0x44);

    let total = 3_u32;
    let page_size = 2_u32;

    let page1 = client.get_shipments_by_sender_page(&sender, &0, &page_size);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap().id, id1);
    assert_eq!(page1.get(1).unwrap().id, id2);
    assert_eq!(next_page_offset(0, page1.len(), total), Some(2));

    let next_cursor = next_page_offset(0, page1.len(), total).unwrap();
    let page2 = client.get_shipments_by_sender_page(&sender, &next_cursor, &page_size);
    assert_eq!(page2.len(), 1);
    assert_eq!(page2.get(0).unwrap().id, id3);
    assert_eq!(next_page_offset(next_cursor, page2.len(), total), None);
}

/// Carrier pagination mirrors sender pagination semantics.
#[test]
fn test_carrier_pagination_page_one_and_two_with_next_cursor() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier_a = Address::generate(&env);
    let carrier_b = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id1 = create_shipment_for(&client, &env, &company, &receiver, &carrier_a, 0x51);
    let _other = create_shipment_for(&client, &env, &company, &receiver, &carrier_b, 0x52);
    let id2 = create_shipment_for(&client, &env, &company, &receiver, &carrier_a, 0x53);
    let id3 = create_shipment_for(&client, &env, &company, &receiver, &carrier_a, 0x54);

    let total = 3_u32;
    let page_size = 2_u32;

    let page1 = client.get_shipments_by_carrier_page(&carrier_a, &0, &page_size);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap().id, id1);
    assert_eq!(page1.get(1).unwrap().id, id2);
    assert_eq!(next_page_offset(0, page1.len(), total), Some(2));

    let next_cursor = next_page_offset(0, page1.len(), total).unwrap();
    let page2 = client.get_shipments_by_carrier_page(&carrier_a, &next_cursor, &page_size);
    assert_eq!(page2.len(), 1);
    assert_eq!(page2.get(0).unwrap().id, id3);
    assert_eq!(next_page_offset(next_cursor, page2.len(), total), None);
}

/// Offset zero is the canonical page start and returns the first slice.
#[test]
fn test_sender_pagination_zero_offset_start() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &sender);

    let id1 = create_shipment_for(&client, &env, &sender, &receiver, &carrier, 0x61);
    let id2 = create_shipment_for(&client, &env, &sender, &receiver, &carrier, 0x62);

    let page = client.get_shipments_by_sender_page(&sender, &0, &2);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap().id, id1);
    assert_eq!(page.get(1).unwrap().id, id2);
}

/// Partial last page returns fewer items than the requested limit.
#[test]
fn test_sender_pagination_partial_last_page() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &sender);

    for marker in 0x71_u8..=0x75_u8 {
        create_shipment_for(&client, &env, &sender, &receiver, &carrier, marker);
    }

    let page_size = 3_u32;
    let page1 = client.get_shipments_by_sender_page(&sender, &0, &page_size);
    assert_eq!(page1.len(), 3);
    assert_eq!(next_page_offset(0, page1.len(), 5), Some(3));

    let page2 = client.get_shipments_by_sender_page(&sender, &3, &page_size);
    assert_eq!(page2.len(), 2);
    assert_eq!(next_page_offset(3, page2.len(), 5), None);
}

/// Exact page boundary: full pages have a next_cursor; the final page does not.
#[test]
fn test_carrier_pagination_exact_page_boundary() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    for marker in 0x81_u8..=0x84_u8 {
        create_shipment_for(&client, &env, &company, &receiver, &carrier, marker);
    }

    let page_size = 2_u32;
    let page1 = client.get_shipments_by_carrier_page(&carrier, &0, &page_size);
    assert_eq!(page1.len(), 2);
    assert_eq!(next_page_offset(0, page1.len(), 4), Some(2));

    let page2 = client.get_shipments_by_carrier_page(&carrier, &2, &page_size);
    assert_eq!(page2.len(), 2);
    assert_eq!(next_page_offset(2, page2.len(), 4), None);
}

/// Cursor past the end returns an empty page without panicking.
#[test]
fn test_sender_pagination_empty_page_past_end() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &sender);
    create_shipment_for(&client, &env, &sender, &receiver, &carrier, 0x91);

    let page = client.get_shipments_by_sender_page(&sender, &10, &5);
    assert_eq!(page.len(), 0);
}

/// Unknown sender returns an empty page safely.
#[test]
fn test_sender_pagination_empty_for_unknown_sender() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let sender = Address::generate(&env);
    let unknown = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &sender);
    create_shipment_for(&client, &env, &sender, &receiver, &carrier, 0xA1);

    let page = client.get_shipments_by_sender_page(&unknown, &0, &10);
    assert_eq!(page.len(), 0);
    assert_eq!(next_page_offset(0, page.len(), 0), None);
}

/// Paginated slices must be stable subsets of the full sender query.
#[test]
fn test_sender_pagination_stable_slices_match_full_query() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &sender);

    for marker in 0xB1_u8..=0xB4_u8 {
        create_shipment_for(&client, &env, &sender, &receiver, &carrier, marker);
    }

    let all_ids = collect_sender_ids(&env, &client, &sender, 10);
    assert_eq!(all_ids.len(), 4);

    let mut paged_ids = Vec::new(&env);
    let page_size = 2_u32;
    let mut offset = 0_u32;
    loop {
        let page = client.get_shipments_by_sender_page(&sender, &offset, &page_size);
        if page.is_empty() {
            break;
        }
        for s in page.iter() {
            paged_ids.push_back(s.id);
        }
        offset = match next_page_offset(offset, page.len(), all_ids.len()) {
            Some(next) => next,
            None => break,
        };
    }

    assert_eq!(paged_ids.len(), all_ids.len());
    for i in 0..all_ids.len() {
        assert_eq!(paged_ids.get(i).unwrap(), all_ids.get(i).unwrap());
    }
}

/// Paginated carrier slices must be stable subsets of the full carrier query.
#[test]
fn test_carrier_pagination_stable_slices_match_full_query() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    for marker in 0xC1_u8..=0xC3_u8 {
        create_shipment_for(&client, &env, &company, &receiver, &carrier, marker);
    }

    let all_ids = collect_carrier_ids(&env, &client, &carrier, 10);
    assert_eq!(all_ids.len(), 3);

    let mut paged_ids = Vec::new(&env);
    let page_size = 1_u32;
    let mut offset = 0_u32;
    loop {
        let page = client.get_shipments_by_carrier_page(&carrier, &offset, &page_size);
        if page.is_empty() {
            break;
        }
        for s in page.iter() {
            paged_ids.push_back(s.id);
        }
        offset = match next_page_offset(offset, page.len(), all_ids.len()) {
            Some(next) => next,
            None => break,
        };
    }

    assert_eq!(paged_ids.len(), all_ids.len());
    for i in 0..all_ids.len() {
        assert_eq!(paged_ids.get(i).unwrap(), all_ids.get(i).unwrap());
    }
}

/// Zero limit is rejected for sender pagination.
#[test]
fn test_sender_pagination_rejects_zero_limit() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let sender = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &sender);

    let result = client.try_get_shipments_by_sender_page(&sender, &0, &0);
    assert!(matches!(result, Err(Ok(NavinError::InvalidConfig))));
}

/// Zero limit is rejected for carrier pagination.
#[test]
fn test_carrier_pagination_rejects_zero_limit() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    let result = client.try_get_shipments_by_carrier_page(&carrier, &0, &0);
    assert!(matches!(result, Err(Ok(NavinError::InvalidConfig))));
}
