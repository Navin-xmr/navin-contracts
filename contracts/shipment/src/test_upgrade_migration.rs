#![cfg(test)]

extern crate std;

use super::test_helpers::*;


#[test]
fn test_upgrade_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let wasm: &[u8] = crate::test_utils::UPGRADE_WASM;
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    client.initialize(&admin, &token_contract);
    assert_eq!(client.get_version(), 1);

    // Drain events emitted by initialize so we can assert only on upgrade events
    let _ = env.events().all();

    client.upgrade(&admin, &new_wasm_hash, &2);

    // Capture events immediately after upgrade before any further calls flush the queue
    let events = env.events().all();

    let version: u32 = env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::Version)
            .unwrap()
    });
    assert_eq!(version, 2);
    let event_found = events.iter().any(|e| {
        if let Ok(topic) = Symbol::try_from_val(&env, &e.1.get(0).unwrap()) {
            topic == Symbol::new(&env, "contract_upgraded")
        } else {
            false
        }
    });
    assert!(event_found, "Contract upgraded event should be present");
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_upgrade_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let non_admin = Address::generate(&env);
    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);

    client.initialize(&admin, &token_contract);

    client.upgrade(&non_admin, &new_wasm_hash, &2);
}

// Contract Metadata Tests

#[test]
fn test_get_contract_metadata_after_init() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let meta = client.get_contract_metadata();
    assert_eq!(meta.version, 1);
    assert_eq!(meta.admin, admin);
    assert_eq!(meta.shipment_count, 0);
    assert!(meta.initialized);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_version_fails_before_initialization() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    client.get_version();
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_contract_metadata_fails_before_initialization() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    client.get_contract_metadata();
}

#[test]
fn test_get_version_after_upgrade() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let wasm: &[u8] = crate::test_utils::UPGRADE_WASM;
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    client.initialize(&admin, &token_contract);
    assert_eq!(client.get_version(), 1);

    client.upgrade(&admin, &new_wasm_hash, &2);

    let version: u32 = env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::Version)
            .unwrap()
    });
    assert_eq!(version, 2);
}

#[test]
fn test_get_contract_metadata_after_upgrade() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let wasm: &[u8] = crate::test_utils::UPGRADE_WASM;
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    client.initialize(&admin, &token_contract);

    let meta_before = client.get_contract_metadata();
    assert_eq!(meta_before.version, 1);
    assert_eq!(meta_before.admin, admin);
    assert_eq!(meta_before.shipment_count, 0);
    assert!(meta_before.initialized);

    client.upgrade(&admin, &new_wasm_hash, &2);

    let version: u32 = env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::Version)
            .unwrap()
    });
    assert_eq!(version, 2);
}

#[test]
fn test_get_hash_algo_version() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    assert_eq!(client.get_hash_algo_version(), crate::DEFAULT_HASH_ALGO);
}

#[test]
fn test_dry_run_migration_success() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let report = client.dry_run_migration(&2);
    assert_eq!(report.current_version, 1);
    assert_eq!(report.target_version, 2);
    assert_eq!(report.affected_shipments, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #47)")]
fn test_upgrade_invalid_edge_fails() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &token_contract);

    // Jump from 1 to 3 is not allowed
    client.upgrade(&admin, &new_wasm_hash, &3);
}

#[test]
#[should_panic(expected = "Error(Contract, #47)")]
fn test_dry_run_invalid_edge_fails() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Rollback from 1 to 0 is not allowed
    client.dry_run_migration(&0);
}

// Carrier Handoff Tests
