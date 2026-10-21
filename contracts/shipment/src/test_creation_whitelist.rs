#![cfg(test)]

extern crate std;

use super::test_helpers::*;


#[test]
fn test_add_carrier_to_whitelist() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier_to_whitelist(&company, &carrier);

    assert!(client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_remove_carrier_from_whitelist() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier_to_whitelist(&company, &carrier);
    assert!(client.is_carrier_whitelisted(&company, &carrier));

    client.remove_carrier_from_whitelist(&company, &carrier);

    assert!(!client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_is_carrier_whitelisted_returns_false_for_non_whitelisted() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    assert!(!client.is_carrier_whitelisted(&company, &carrier));
}

#[test]
fn test_multiple_carriers_whitelist() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier1 = Address::generate(&env);
    let carrier2 = Address::generate(&env);
    let carrier3 = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier_to_whitelist(&company, &carrier1);
    client.add_carrier_to_whitelist(&company, &carrier2);

    assert!(client.is_carrier_whitelisted(&company, &carrier1));
    assert!(client.is_carrier_whitelisted(&company, &carrier2));
    assert!(!client.is_carrier_whitelisted(&company, &carrier3));

    client.remove_carrier_from_whitelist(&company, &carrier1);

    assert!(!client.is_carrier_whitelisted(&company, &carrier1));
    assert!(client.is_carrier_whitelisted(&company, &carrier2));
}

#[test]
fn test_whitelist_per_company() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company1 = Address::generate(&env);
    let company2 = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_company(&admin, &company1);
    client.add_carrier_to_whitelist(&company1, &carrier);

    assert!(client.is_carrier_whitelisted(&company1, &carrier));
    assert!(!client.is_carrier_whitelisted(&company2, &carrier));

    client.add_company(&admin, &company2);
    client.add_carrier_to_whitelist(&company2, &carrier);

    assert!(client.is_carrier_whitelisted(&company1, &carrier));
    assert!(client.is_carrier_whitelisted(&company2, &carrier));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_whitelist_functions_fail_before_initialization() {
    let (env, client, _admin, _token_contract) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.is_carrier_whitelisted(&company, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_add_whitelist_fails_before_initialization() {
    let (env, client, _admin, _token_contract) = setup_shipment_env();

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.add_carrier_to_whitelist(&company, &carrier);
}

// Deposit Escrow Tests

// Status Update Tests
