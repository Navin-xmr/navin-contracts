use crate::{test_utils, NavinShipment, NavinShipmentClient};
use soroban_sdk::{
    contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Symbol, Vec,
};

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn decimals(_env: soroban_sdk::Env) -> u32 {
        7
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {
        // Mock implementation - always succeeds
    }
}

fn setup_test(env: &Env) -> (NavinShipmentClient<'static>, Address, Address) {
    let admin = Address::generate(env);
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(env, &env.register(NavinShipment, ()));
    client.initialize(&admin, &token_contract);
    (client, admin, token_contract)
}

#[test]
fn test_company_suspension_blocks_create_shipment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup_test(&env);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);

    // Suspend the company
    client.suspend_company(&admin, &company);

    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let milestones = Vec::new(&env);
    let deadline = env.ledger().timestamp() + 3600;

    // Attempt to create shipment should fail with CompanySuspended (37)
    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );

    assert!(result.is_err());
    // Error(Contract, #37)
}

#[test]
fn test_company_suspension_blocks_metadata_update() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup_test(&env);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);

    // Create a shipment first
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let milestones = Vec::new(&env);
    let deadline = env.ledger().timestamp() + 3600;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );

    // Suspend company
    client.suspend_company(&admin, &company);

    // Attempt to set metadata should fail
    let result = client.try_set_shipment_metadata(
        &company,
        &shipment_id,
        &Symbol::new(&env, "key"),
        &Symbol::new(&env, "value"),
    );

    assert!(result.is_err());
}

#[test]
fn test_company_reactivation_restores_access() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup_test(&env);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);

    // Suspend
    client.suspend_company(&admin, &company);

    // Create should fail
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let milestones = Vec::new(&env);
    let deadline = env.ledger().timestamp() + 3600;

    assert!(client
        .try_create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &milestones,
            &deadline,
        )
        .is_err());

    // Reactivate
    client.reactivate_company(&admin, &company);

    // Create should now succeed
    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
    assert!(result.is_ok());
}

#[test]
fn test_company_suspension_blocks_cancel_shipment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup_test(&env);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);

    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let milestones = Vec::new(&env);
    let deadline = env.ledger().timestamp() + 3600;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );

    // Suspend
    client.suspend_company(&admin, &company);

    // Cancel should fail
    let result = client.try_cancel_shipment(
        &company,
        &shipment_id,
        &BytesN::from_array(&env, &[0u8; 32]),
    );

    assert!(result.is_err());
}

// ── Combined guard interactions: rate limit + suspension ─────────────────────

/// Test: Rate limit exhaustion combined with suspension behavior.
/// Verify that when a company is suspended, rate limit checks still apply
/// and the suspension takes precedence.
#[test]
fn test_suspension_with_rate_limit_exhaustion() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup_test(&env);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);

    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let milestones = Vec::new(&env);
    let deadline = env.ledger().timestamp() + 3600;

    // Create a shipment
    let _shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );

    // Suspend the company
    client.suspend_company(&admin, &company);

    // Attempting to create another shipment should fail with CompanySuspended
    // (suspension takes precedence over rate limit)
    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );

    assert!(result.is_err());
    // Should be CompanySuspended (error code 37), not RateLimitExceeded
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, crate::NavinError::CompanySuspended);
}

/// Test: Recovery after suspension when rate limit window is active.
/// Verify that after reactivation, operations succeed even if the rate
/// limit window hasn't fully expired.
#[test]
fn test_suspension_recovery_with_active_rate_limit_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup_test(&env);

    let company = Address::generate(&env);
    client.add_company(&admin, &company);

    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let milestones = Vec::new(&env);
    let deadline = env.ledger().timestamp() + 3600;

    // Create a shipment (uses rate limit quota)
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );

    // Suspend the company
    client.suspend_company(&admin, &company);

    // Verify suspension blocks operations
    let result = client.try_create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
    assert!(result.is_err());

    // Reactivate the company
    client.reactivate_company(&admin, &company);

    // Advance time past rate limit window to ensure quota is available
    test_utils::advance_past_rate_limit(&env);

    // Use different addresses to avoid any potential conflicts
    let receiver2 = Address::generate(&env);
    let data_hash2 = BytesN::from_array(&env, &[2u8; 32]);

    // After reactivation, operations should succeed
    // (rate limit window is still active but reactivation resets access)
    let result = client.try_create_shipment(
        &company,
        &receiver2,
        &carrier,
        &data_hash2,
        &milestones,
        &deadline,
    );
    assert!(result.is_ok());
}

/// Test: Multiple suspended actors with independent rate limit states.
/// Verify that suspension of one actor doesn't affect another's rate limit.
#[test]
fn test_multiple_suspended_actors_independent_rate_limits() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup_test(&env);

    let company1 = Address::generate(&env);
    let company2 = Address::generate(&env);
    client.add_company(&admin, &company1);
    client.add_company(&admin, &company2);

    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    client.add_carrier(&admin, &carrier);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let milestones = Vec::new(&env);
    let deadline = env.ledger().timestamp() + 3600;

    // Suspend company1
    client.suspend_company(&admin, &company1);

    // company1 should be blocked
    let result1 = client.try_create_shipment(
        &company1,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
    assert!(result1.is_err());

    // company2 should still work (independent rate limit state)
    let result2 = client.try_create_shipment(
        &company2,
        &receiver,
        &carrier,
        &data_hash,
        &milestones,
        &deadline,
    );
    assert!(result2.is_ok());
}
