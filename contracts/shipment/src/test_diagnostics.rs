use crate::{
    config,
    test_utils::{advance_ledger_time, setup_env},
    types::ShipmentStatus,
    NavinShipment, NavinShipmentClient,
};
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Vec};

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn decimals(_env: soroban_sdk::Env) -> u32 {
        7
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    pub fn transfer_from(
        _env: Env,
        _spender: Address,
        _from: Address,
        _to: Address,
        _amount: i128,
    ) {
    }
}

fn prepare_test() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = setup_env();
    let token = env.register(MockToken {}, ());
    let cid = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &cid);
    client.initialize(&admin, &token);
    (env, client, admin, token)
}
/// Same config always produces the same checksum (deterministic across reruns).
#[test]
fn test_config_checksum_is_stable_when_config_unchanged() {
    let (_env, client, _admin, _token) = prepare_test();

    let c1 = client.get_config_checksum();
    let c2 = client.get_config_checksum();
    assert_eq!(c1, c2);
}

/// Checksum changes when a critical config field is mutated.
#[test]
fn test_config_checksum_changes_after_config_update() {
    let (_env, client, admin, _token) = prepare_test();

    let before = client.get_config_checksum();

    let mut new_cfg = client.get_contract_config();
    new_cfg.batch_operation_limit += 1;
    client.update_config(&admin, &new_cfg);

    let after = client.get_config_checksum();
    assert_ne!(before, after);
}

/// Reverting a config change restores the original checksum.
#[test]
fn test_config_checksum_restored_after_revert() {
    let (_env, client, admin, _token) = prepare_test();

    let original = client.get_config_checksum();

    let mut mutated = client.get_contract_config();
    mutated.deadline_grace_seconds = 120;
    client.update_config(&admin, &mutated);
    assert_ne!(client.get_config_checksum(), original);

    // Revert
    let mut reverted = client.get_contract_config();
    reverted.deadline_grace_seconds = 0;
    client.update_config(&admin, &reverted);
    assert_eq!(client.get_config_checksum(), original);
}

/// Each distinct field mutation produces a distinct checksum.
#[test]
fn test_each_field_mutation_produces_unique_checksum() {
    let (_env, client, admin, _token) = prepare_test();

    let base = client.get_config_checksum();

    let mutations: &[fn(&mut crate::ContractConfig)] = &[
        |c| c.shipment_ttl_threshold += 1,
        |c| c.shipment_ttl_extension += 1,
        |c| c.min_status_update_interval += 10,
        |c| c.batch_operation_limit += 1,
        |c| c.max_metadata_entries += 1,
        |c| c.default_shipment_limit += 1,
        |c| c.proposal_expiry_seconds += 3600,
        |c| c.deadline_grace_seconds += 60,
        |c| c.auto_dispute_breach = !c.auto_dispute_breach,
        |c| c.max_milestones_per_shipment -= 1,
        |c| c.max_notes_per_shipment -= 1,
        |c| c.max_evidence_per_dispute -= 1,
        |c| c.max_breaches_per_shipment -= 1,
        |c| c.idempotency_window_seconds += 60,
        |c| c.creation_quota_max += 10,
        |c| c.creation_quota_window_seconds += 300,
    ];

    let base_cfg = client.get_contract_config();

    for mutate in mutations {
        let mut cfg = base_cfg.clone();
        mutate(&mut cfg);
        client.update_config(&admin, &cfg);
        assert_ne!(
            client.get_config_checksum(),
            base,
            "checksum must differ after field mutation"
        );
        // Restore
        client.update_config(&admin, &base_cfg);
        assert_eq!(
            client.get_config_checksum(),
            base,
            "checksum must be restored"
        );
    }
}

/// Checksum changes when idempotency/quota fields are mutated via update_config.
#[test]
fn test_config_checksum_reflects_idempotency_and_quota_fields() {
    let (_env, client, admin, _token) = prepare_test();

    let before = client.get_config_checksum();

    let mut cfg = client.get_contract_config();
    cfg.idempotency_window_seconds = 600;
    client.update_config(&admin, &cfg);
    assert_ne!(client.get_config_checksum(), before);

    // Revert and test creation_quota_max
    let mut cfg = client.get_contract_config();
    cfg.idempotency_window_seconds = 300;
    cfg.creation_quota_max = 50;
    client.update_config(&admin, &cfg);
    assert_ne!(client.get_config_checksum(), before);

    // Revert and test creation_quota_window_seconds
    let mut cfg = client.get_contract_config();
    cfg.creation_quota_max = 0;
    cfg.creation_quota_window_seconds = 7200;
    client.update_config(&admin, &cfg);
    assert_ne!(client.get_config_checksum(), before);
}

// ── Config checksum diagnostics query path ──────────────────────────────────

/// The config checksum query path used by diagnostics/indexers must return
/// a stable checksum across multiple invocations and match a raw recompute.
#[test]
fn test_config_checksum_diagnostics_query_path() {
    let (env, client, admin, _token) = prepare_test();

    // Query path: get_config_checksum (what indexers/diagnostics use)
    let q1 = client.get_config_checksum();
    let q2 = client.get_config_checksum();
    assert_eq!(q1, q2, "diagnostics query path must be idempotent");

    // Recompute from raw config to pin the expected behaviour
    let cfg = client.get_contract_config();
    let recomputed = env.as_contract(&client.address, || {
        config::compute_config_checksum(&cfg, &env)
    });
    assert_eq!(
        q1, recomputed,
        "diagnostics query path must match raw compute"
    );

    // After a mutation, the checksum changes predictably
    let mut mutated = cfg.clone();
    mutated.batch_operation_limit += 1;
    client.update_config(&admin, &mutated);
    let q3 = client.get_config_checksum();
    assert_ne!(
        q1, q3,
        "checksum must change after config mutation via diagnostics query path"
    );

    // Restore and verify the original checksum returns
    client.update_config(&admin, &cfg);
    let q4 = client.get_config_checksum();
    assert_eq!(q1, q4, "original checksum must be restored after revert");
}

#[test]
fn test_get_non_terminal_count_alignment() {
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;

    let _id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &Vec::new(&env),
        &deadline,
    );
    let id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[2u8; 32]),
        &Vec::new(&env),
        &deadline,
    );
    let id3 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[3u8; 32]),
        &Vec::new(&env),
        &deadline,
    );
    let id4 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[4u8; 32]),
        &Vec::new(&env),
        &deadline,
    );
    let id5 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[5u8; 32]),
        &Vec::new(&env),
        &deadline,
    );

    // Initial state: 5 Created
    assert_eq!(client.get_non_terminal_count(), 5);

    // id2 -> InTransit
    client.update_status(
        &carrier,
        &id2,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[2u8; 32]),
    );
    assert_eq!(client.get_non_terminal_count(), 5);

    // id3 -> AtCheckpoint
    client.update_status(
        &carrier,
        &id3,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[3u8; 32]),
    );
    advance_ledger_time(&env, 3600);
    client.update_status(
        &carrier,
        &id3,
        &ShipmentStatus::AtCheckpoint,
        &BytesN::from_array(&env, &[3u8; 32]),
    );
    assert_eq!(client.get_non_terminal_count(), 5);

    // id4 -> Disputed
    client.update_status(
        &carrier,
        &id4,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[4u8; 32]),
    );
    advance_ledger_time(&env, 3600);
    client.raise_dispute(&company, &id4, &BytesN::from_array(&env, &[4u8; 32]));
    assert_eq!(client.get_non_terminal_count(), 5);

    // id5 -> Delivered (Terminal)
    client.update_status(
        &carrier,
        &id5,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[5u8; 32]),
    );
    advance_ledger_time(&env, 3600);
    client.confirm_delivery(&receiver, &id5, &BytesN::from_array(&env, &[5u8; 32]));
    assert_eq!(client.get_non_terminal_count(), 4);
}

// ── get_shipment_creator — non-existent shipment tests (issue #520) ──────────

/// Querying the creator of a shipment ID that was never created must return
/// ShipmentNotFound without panicking or crashing the node.
#[test]
fn test_get_shipment_creator_returns_not_found_for_nonexistent_id() {
    use crate::NavinError;
    let (_, client, _, _) = prepare_test();

    let result = client.try_get_shipment_creator(&9999u64);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "get_shipment_creator must return ShipmentNotFound for an ID that was never created"
    );
}

/// ID 0 is never assigned by the counter (counter starts at 1); querying it
/// must return ShipmentNotFound gracefully.
#[test]
fn test_get_shipment_creator_returns_not_found_for_zero_id() {
    use crate::NavinError;
    let (_, client, _, _) = prepare_test();

    let result = client.try_get_shipment_creator(&0u64);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "get_shipment_creator must return ShipmentNotFound for ID 0"
    );
}

/// Querying a large arbitrary shipment ID that does not exist must return
/// ShipmentNotFound — no storage panic or key error.
#[test]
fn test_get_shipment_creator_returns_not_found_for_large_invalid_id() {
    use crate::NavinError;
    let (_, client, _, _) = prepare_test();

    let result = client.try_get_shipment_creator(&u64::MAX);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "get_shipment_creator must return ShipmentNotFound for u64::MAX ID"
    );
}

/// Confirm that a real shipment returns the correct sender (creator) address.
#[test]
fn test_get_shipment_creator_returns_sender_for_valid_shipment() {
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let creator = client.get_shipment_creator(&shipment_id);
    assert_eq!(
        creator, company,
        "get_shipment_creator must return the original sender address"
    );
}

// ── get_shipment_carrier — non-existent shipment tests (issue #534) ──────────

/// Querying the carrier of a shipment ID that was never created must return
/// ShipmentNotFound without panicking or crashing the node.
#[test]
fn test_get_shipment_carrier_returns_not_found_for_nonexistent_id() {
    use crate::NavinError;
    let (_, client, _, _) = prepare_test();

    let result = client.try_get_shipment_carrier(&9999u64);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "get_shipment_carrier must return ShipmentNotFound for an ID that was never created"
    );
}

/// ID 0 is never assigned by the counter (counter starts at 1); querying it
/// must return ShipmentNotFound gracefully.
#[test]
fn test_get_shipment_carrier_returns_not_found_for_zero_id() {
    use crate::NavinError;
    let (_, client, _, _) = prepare_test();

    let result = client.try_get_shipment_carrier(&0u64);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "get_shipment_carrier must return ShipmentNotFound for ID 0"
    );
}

/// Querying a large arbitrary shipment ID that does not exist must return
/// ShipmentNotFound — no storage panic or key error.
#[test]
fn test_get_shipment_carrier_returns_not_found_for_large_invalid_id() {
    use crate::NavinError;
    let (_, client, _, _) = prepare_test();

    let result = client.try_get_shipment_carrier(&u64::MAX);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "get_shipment_carrier must return ShipmentNotFound for u64::MAX ID"
    );
}

/// Confirm that a real shipment returns the correct carrier address.
#[test]
fn test_get_shipment_carrier_returns_carrier_for_valid_shipment() {
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[8u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let returned_carrier = client.get_shipment_carrier(&shipment_id);
    assert_eq!(
        returned_carrier, carrier,
        "get_shipment_carrier must return the original carrier address"
    );
}

// =============================================================================
// Issue #534 — get_non_terminal_count decrements on refund resolution
// =============================================================================

/// Verify that `get_non_terminal_count` decrements by 1 when a shipment is
/// resolved via `refund_escrow`. Cancelled is a terminal state so the count
/// must drop immediately after the refund.
#[test]
fn test_non_terminal_count_decrements_on_refund() {
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[10u8; 32]);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let count_before = client.get_non_terminal_count();
    assert_eq!(
        count_before, 1,
        "Non-terminal count should be 1 after creation"
    );

    // Deposit escrow so refund can proceed
    client.deposit_escrow(&company, &shipment_id, &1_000);

    // refund_escrow transitions the shipment to Cancelled (terminal)
    client.refund_escrow(&company, &shipment_id);

    let count_after = client.get_non_terminal_count();
    assert_eq!(
        count_after, 0,
        "Non-terminal count must decrement to 0 after refund_escrow"
    );
}

/// Verify count decrements correctly when only one of many shipments is refunded.
#[test]
fn test_non_terminal_count_decrements_for_one_of_many_on_refund() {
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;

    let id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[20u8; 32]),
        &Vec::new(&env),
        &deadline,
    );
    let _id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[21u8; 32]),
        &Vec::new(&env),
        &deadline,
    );
    let _id3 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[22u8; 32]),
        &Vec::new(&env),
        &deadline,
    );

    assert_eq!(client.get_non_terminal_count(), 3);

    client.deposit_escrow(&company, &id1, &500);
    client.refund_escrow(&company, &id1);

    assert_eq!(
        client.get_non_terminal_count(),
        2,
        "Only the refunded shipment should reduce the non-terminal count"
    );
}

// ── get_shipment_receiver — non-existent shipment tests (issue #528) ─────────

/// Querying the receiver of a shipment ID that was never created must return
/// ShipmentNotFound without panicking or crashing the node.
#[test]
fn test_get_shipment_receiver_returns_not_found_for_nonexistent_id() {
    use crate::NavinError;
    let (_, client, _, _) = prepare_test();

    let result = client.try_get_shipment_receiver(&9999u64);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "get_shipment_receiver must return ShipmentNotFound for an ID that was never created"
    );
}

/// ID 0 is never assigned by the counter (counter starts at 1); querying the
/// receiver for it must return ShipmentNotFound gracefully.
#[test]
fn test_get_shipment_receiver_returns_not_found_for_zero_id() {
    use crate::NavinError;
    let (_, client, _, _) = prepare_test();

    let result = client.try_get_shipment_receiver(&0u64);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "get_shipment_receiver must return ShipmentNotFound for ID 0"
    );
}

/// Querying a large arbitrary shipment ID that does not exist must return
/// ShipmentNotFound — no storage panic or key error.
#[test]
fn test_get_shipment_receiver_returns_not_found_for_large_invalid_id() {
    use crate::NavinError;
    let (_, client, _, _) = prepare_test();

    let result = client.try_get_shipment_receiver(&u64::MAX);
    assert_eq!(
        result,
        Err(Ok(NavinError::ShipmentNotFound)),
        "get_shipment_receiver must return ShipmentNotFound for u64::MAX ID"
    );
}

/// Confirm that a real shipment returns the correct receiver address.
#[test]
fn test_get_shipment_receiver_returns_receiver_for_valid_shipment() {
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[30u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
    );

    let result = client.try_get_shipment_receiver(&shipment_id);
    assert_eq!(
        result,
        Ok(Ok(receiver.clone())),
        "get_shipment_receiver must return the original receiver address"
    );
    assert_eq!(
        client.get_shipment_receiver(&shipment_id),
        receiver,
        "get_shipment_receiver must return the receiver address for a valid shipment"
    );
}
