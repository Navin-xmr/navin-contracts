use crate::{
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

#[test]
fn test_clean_health_check() {
    let (env, client, admin, _token) = prepare_test();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let _shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    let health = client.check_contract_health(&admin);
    assert_eq!(health.total_shipments, 1);
    assert_eq!(health.active_shipments_counted, 1);
    assert_eq!(health.sum_of_escrow_balances, 0);
    assert_eq!(health.anomalous_shipment_ids.len(), 0);
    assert_eq!(health.storage_inconsistencies.len(), 0);
}

#[test]
fn test_detect_anomalies_and_escrow() {
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash1 = BytesN::from_array(&env, &[1u8; 32]);
    let data_hash2 = BytesN::from_array(&env, &[2u8; 32]);

    let id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash1,
        &Vec::new(&env),
        &deadline,
        &None,
    );
    advance_ledger_time(&env, 1);
    let id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash2,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    client.deposit_escrow(&company, &id1, &1500);
    client.deposit_escrow(&company, &id2, &500);

    client.update_status(&carrier, &id1, &ShipmentStatus::InTransit, &data_hash1);

    // Simulate crossing the deadline threshold
    advance_ledger_time(&env, 4000); // Exceeds deadline (+3600)

    let health = client.check_contract_health(&admin);
    assert_eq!(health.total_shipments, 2);
    assert_eq!(health.active_shipments_counted, 2);

    // Sum should be accurate
    assert_eq!(health.sum_of_escrow_balances, 2000);

    // id1 is strictly InTransit and late!
    assert!(health.anomalous_shipment_ids.contains(id1));
    // id2 is still physically 'Created', which might be fine to remain late without anomaly or catch elsewhere depending on business rules, but in our code it strictly checks InTransit.
    assert!(!health.anomalous_shipment_ids.contains(id2));

    assert_eq!(
        health.storage_inconsistencies.len(),
        0,
        "Storage inconsistencies found: {:?}",
        health.storage_inconsistencies
    );
}

#[test]
fn test_detect_storage_inconsistencies() {
    // This is purely for unit verification that run_system_health_check directly exposes the
    // internal variables. We can force storage modification inside tests using raw storage functions.
    let (env, client, admin, _token) = prepare_test();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let deadline = env.ledger().timestamp() + 3600;
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    let cid = client.address.clone();
    env.as_contract(&cid, || {
        crate::storage::remove_escrow(&env, shipment_id);

        // Set escrow high within the shipment object to simulate orphaned balance
        let mut ship = crate::storage::get_shipment(&env, shipment_id).unwrap();
        ship.escrow_amount = 5000;
        crate::storage::set_shipment(&env, &ship);
    });

    let health = client.check_contract_health(&admin);
    // Because escrow_amount is 5000 but the Escrow persisted entry is killed by remove_escrow
    assert!(health.storage_inconsistencies.contains(shipment_id));
}

// ── Regression: config checksum stability and mutation ───────────────────────

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
