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

/// Long topics the contract emits must produce a key (issue #1220).
///
/// A Soroban `Symbol` allows up to 32 characters (40 bytes of XDR). The decode
/// buffer used to be 32 bytes, so every symbol over 24 characters was
/// rejected, including `milestone_payment_released` and five audit topics.
#[test]
fn test_compute_idempotency_key_accepts_long_emitted_topics() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();

    let topic = Symbol::new(&env, crate::event_topics::MILESTONE_PAYMENT_RELEASED);
    let first = client.compute_idempotency_key(&1u64, &topic, &0u32);
    assert_eq!(
        first,
        client.compute_idempotency_key(&1u64, &topic, &0u32),
        "the key must be deterministic for a 26-character topic"
    );
    assert_ne!(
        first,
        client.compute_idempotency_key(&1u64, &topic, &1u32),
        "a different counter must yield a different key"
    );

    // Long topics must not collide with a prefix of themselves.
    let prefix = Symbol::new(&env, "milestone_payment_releas");
    assert_ne!(first, client.compute_idempotency_key(&1u64, &prefix, &0u32));
}

/// Every topic constant the contract emits yields a key.
#[test]
fn test_compute_idempotency_key_ok_for_every_topic_constant() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();

    for topic in crate::event_topics::ALL_TOPICS {
        let symbol = Symbol::new(&env, topic);
        assert!(
            client
                .try_compute_idempotency_key(&1u64, &symbol, &0u32)
                .is_ok(),
            "compute_idempotency_key failed for topic '{}'",
            topic
        );
    }
}

/// The maximum-length symbol (32 characters) is accepted.
#[test]
fn test_compute_idempotency_key_handles_max_length_symbol() {
    let (env, client, _admin, _token) = setup_initialized_shipment_env();

    let max_symbol = Symbol::new(&env, "abcdefghijklmnopqrstuvwxyz012345");
    let short = Symbol::new(&env, "abcdefghijklmnopqrstuvwx");
    let key = client.compute_idempotency_key(&1u64, &max_symbol, &0u32);
    assert_ne!(
        key,
        client.compute_idempotency_key(&1u64, &short, &0u32),
        "the full 32 characters must reach the preimage"
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

#[test]
fn test_add_guardian_rejects_already_assigned() {
    let (env, client, admin, _token) = setup_initialized_shipment_env();
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);

    let result = client.try_add_guardian(&admin, &guardian);
    assert_eq!(result, Err(Ok(crate::NavinError::RoleAlreadyAssigned)));
}

#[test]
fn test_add_operator_rejects_already_assigned() {
    let (env, client, admin, _token) = setup_initialized_shipment_env();
    let operator = Address::generate(&env);
    client.add_operator(&admin, &operator);

    let result = client.try_add_operator(&admin, &operator);
    assert_eq!(result, Err(Ok(crate::NavinError::RoleAlreadyAssigned)));
}
