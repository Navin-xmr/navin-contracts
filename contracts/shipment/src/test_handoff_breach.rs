#![cfg(test)]

extern crate std;

use super::test_helpers::*;

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_handoff_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let unauthorized_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &current_carrier);
    client.add_carrier(&admin, &new_carrier);
    // Note: unauthorized_carrier is NOT added as a carrier

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &current_carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.update_status(
        &current_carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    // Try to handoff from unauthorized carrier
    client.handoff_shipment(
        &unauthorized_carrier,
        &new_carrier,
        &shipment_id,
        &handoff_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_handoff_wrong_current_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let wrong_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &current_carrier);
    client.add_carrier(&admin, &wrong_carrier);
    client.add_carrier(&admin, &new_carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &current_carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.update_status(
        &current_carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    // Try to handoff from wrong carrier (not the assigned one)
    client.handoff_shipment(&wrong_carrier, &new_carrier, &shipment_id, &handoff_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_handoff_invalid_new_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_carrier = Address::generate(&env);
    let invalid_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &current_carrier);
    // Note: invalid_carrier is NOT added as a carrier

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &current_carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.update_status(
        &current_carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );

    // Try to handoff to invalid carrier (doesn't have Carrier role)
    client.handoff_shipment(
        &current_carrier,
        &invalid_carrier,
        &shipment_id,
        &handoff_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_handoff_nonexistent_shipment() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let current_carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);
    let handoff_hash = BytesN::from_array(&env, &[2u8; 32]);
    let nonexistent_shipment_id = 999u64;

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &current_carrier);
    client.add_carrier(&admin, &new_carrier);

    // Try to handoff a non-existent shipment
    client.handoff_shipment(
        &current_carrier,
        &new_carrier,
        &nonexistent_shipment_id,
        &handoff_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_create_shipment_fails_before_initialization() {
    let (env, client, _admin, _token_contract) = setup_shipment_env();
    let sender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    // Contract not initialized — should panic with NotInitialized (#2)
    client.create_shipment(
        &sender,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
}

// Issue #1: report_condition_breach

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_report_condition_breach_unauthorized_non_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let rogue = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let breach_hash = BytesN::from_array(&env, &[2u8; 32]);
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

    // Non-carrier address cannot report a breach
    client.report_condition_breach(
        &rogue,
        &shipment_id,
        &BreachType::Impact,
        &Severity::Medium,
        &breach_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_report_condition_breach_wrong_carrier() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let other_carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let breach_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier(&admin, &other_carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // A registered carrier that is NOT assigned to this shipment cannot report
    client.report_condition_breach(
        &other_carrier,
        &shipment_id,
        &BreachType::TamperDetected,
        &Severity::Critical,
        &breach_hash,
    );
}

// Issue #2: verify_delivery_proof

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_verify_delivery_proof_nonexistent_shipment() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.verify_delivery_proof(&999u64, &BytesN::from_array(&_env, &[1u8; 32]));
}

// Issue #3: Rate limiting

// RBAC and Access Control Tests
