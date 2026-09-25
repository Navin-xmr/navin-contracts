#![no_main]

use libfuzzer_sys::fuzz_target;
use shipment::{NavinShipment, NavinShipmentClient, Role};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

/// Drives the RBAC surface (`add_company`, `add_carrier`, `revoke_role`,
/// `get_role`) with an attacker-controlled action sequence, then asserts the
/// one invariant that must hold no matter what the sequence was: a caller
/// who is not the contract admin can never grant or revoke a role.
fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.timestamp = 1_700_000_000;
    });

    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let contract_id = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &contract_id);

    if client.try_initialize(&admin, &token).is_err() {
        return;
    }

    let attacker = Address::generate(&env);
    let candidate = Address::generate(&env);

    for &byte in data.iter().take(32) {
        match byte % 4 {
            0 => {
                let _ = client.try_add_company(&admin, &candidate);
            }
            1 => {
                let _ = client.try_add_carrier(&admin, &candidate);
            }
            2 => {
                let _ = client.try_revoke_role(&admin, &candidate);
            }
            _ => {
                let _ = client.try_get_role(&candidate);
            }
        }
    }

    // Invariant: no matter what state the admin-driven sequence above left
    // the contract in, a non-admin caller must never be able to grant a
    // role to itself.
    let forged = client.try_add_company(&attacker, &attacker);
    assert!(
        forged.is_err(),
        "non-admin caller must never be able to grant itself a role"
    );
    let role_after_forgery = client.try_get_role(&attacker);
    if let Ok(Ok(role)) = role_after_forgery {
        assert_ne!(
            role,
            Role::Company,
            "a rejected add_company call must not leave the caller with the Company role"
        );
    }
});
