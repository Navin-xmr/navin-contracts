#![cfg(test)]

extern crate std;

use super::test_helpers::*;

// ForceRelease with reason_hash: verifies reason hash is persisted in event stream and queryable.
// Ensures audit trail contains the admin-provided reason for the force release.
// ForceRelease rejects zero reason_hash, matching force_cancel_shipment behavior.
// ForceRefund with reason_hash: verifies reason hash is persisted in event stream and queryable.
// Ensures audit trail contains the admin-provided reason for the force refund.
// ForceRefund rejects zero reason_hash, matching force_cancel_shipment behavior.
// Security regressions

/// `initialize` must require auth from the address it installs as admin.
///
/// Deploy and initialize are separate transactions, so an attacker can watch
/// for an uninitialized contract and call `initialize` naming themselves.
/// `AlreadyInitialized` then locks the real deployer out with no recovery.
#[test]
fn test_initialize_requires_admin_auth() {
    let env = Env::default();
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    let attacker = Address::generate(&env);

    // No auth mocked: the call must not succeed on the attacker's say-so.
    let result = client.try_initialize(&attacker, &token_contract);
    assert!(
        result.is_err(),
        "initialize must not succeed without auth from the admin being installed"
    );
}

/// With the admin's authorization present, initialization still works.
#[test]
fn test_initialize_succeeds_with_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    let admin = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    assert_eq!(client.get_admin(), admin);
}

/// The authorization recorded is the admin's own, so a third party cannot
/// front-run initialization by signing for themselves while naming someone
/// else.
#[test]
fn test_initialize_auth_is_attributed_to_the_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    let admin = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == admin),
        "initialize must require auth from the admin address itself"
    );
}

/// `remove_guardian` must not strip an unrelated role.
///
/// Without the role check this silently revokes the target's Company role,
/// because `revoke_role` removes whatever role the address happens to hold.
#[test]
fn test_remove_guardian_rejects_a_company_target() {
    let (env, client, admin, _token) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    client.add_company(&admin, &company);

    let result = client.try_remove_guardian(&admin, &company);
    assert_eq!(result, Err(Ok(crate::NavinError::RoleMismatch)));

    // The critical assertion: the Company role survived the failed call.
    assert!(
        client.try_get_role(&company).is_ok(),
        "a rejected remove_guardian must leave the target's real role intact"
    );
}

/// The mirror case for operators.
#[test]
fn test_remove_operator_rejects_a_guardian_target() {
    let (env, client, admin, _token) = setup_initialized_shipment_env();
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);

    let result = client.try_remove_operator(&admin, &guardian);
    assert_eq!(result, Err(Ok(crate::NavinError::RoleMismatch)));
}

/// Removing an address that holds no role at all is also a mismatch, rather
/// than a silent success.
#[test]
fn test_remove_guardian_rejects_an_unassigned_target() {
    let (env, client, admin, _token) = setup_initialized_shipment_env();
    let nobody = Address::generate(&env);

    assert_eq!(
        client.try_remove_guardian(&admin, &nobody),
        Err(Ok(crate::NavinError::RoleMismatch))
    );
}

/// The happy path still works: a real guardian is still removable.
#[test]
fn test_remove_guardian_still_removes_a_guardian() {
    let (env, client, admin, _token) = setup_initialized_shipment_env();
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);

    client.remove_guardian(&admin, &guardian);

    assert_eq!(
        client.try_remove_guardian(&admin, &guardian),
        Err(Ok(crate::NavinError::RoleMismatch)),
        "the role is gone, so a second removal is now a mismatch"
    );
}

/// A long `event_type` must not panic.
///
/// A Soroban `Symbol` allows up to 32 characters; the XDR decode writes into a
/// 32-byte buffer and slices `8 + char_count`, so 25-32 characters run past
/// the end. A panic aborts the whole invocation, so a merely-long event type
/// would take down the call that emitted it.
#[test]
fn test_compute_idempotency_key_rejects_oversized_symbols() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();

    // 25 characters — the first length that overflowed the buffer.
    let long_symbol = Symbol::new(&env, "abcdefghijklmnopqrstuvwxy");
    let result = client.try_compute_idempotency_key(&1u64, &long_symbol, &0u32);
    assert!(
        result.is_err(),
        "an oversized symbol must return an error rather than panicking"
    );
}

/// The maximum-length symbol is handled the same way.
#[test]
fn test_compute_idempotency_key_handles_max_length_symbol() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();

    // 32 characters — the SDK's maximum.
    let max_symbol = Symbol::new(&env, "abcdefghijklmnopqrstuvwxyz012345");
    let result = client.try_compute_idempotency_key(&1u64, &max_symbol, &0u32);
    assert!(
        result.is_err(),
        "must not panic at the maximum symbol length"
    );
}

/// Ordinary symbols still produce a stable key.
#[test]
fn test_compute_idempotency_key_still_works_for_short_symbols() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();

    let symbol = Symbol::new(&env, "created");
    let first = client.compute_idempotency_key(&1u64, &symbol, &0u32);
    let second = client.compute_idempotency_key(&1u64, &symbol, &0u32);

    assert_eq!(first, second, "the key must be deterministic");

    let other = client.compute_idempotency_key(&2u64, &symbol, &0u32);
    assert_ne!(
        first, other,
        "a different shipment must yield a different key"
    );
}

// check_batch_consistency admin entrypoint

/// Admin can invoke check_batch_consistency and gets an empty violation list
/// for a healthy batch.
#[test]
fn test_admin_can_check_batch_consistency() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &Vec::new(&env),
        &(env.ledger().timestamp() + 3600),
    );
    let id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[2u8; 32]),
        &Vec::new(&env),
        &(env.ledger().timestamp() + 3600),
    );

    let violations = client.check_batch_consistency(&admin, &Vec::from_array(&env, [id1, id2]));
    assert!(
        violations.is_empty(),
        "healthy batch must return no violations, got: {violations:?}"
    );
}

/// Non-admin caller is rejected when invoking check_batch_consistency.
#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_non_admin_cannot_check_batch_consistency() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &Vec::new(&env),
        &(env.ledger().timestamp() + 3600),
    );

    let outsider = Address::generate(&env);
    client.check_batch_consistency(&outsider, &Vec::from_array(&env, [id1]));
}
