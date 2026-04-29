use crate::{NavinShipment, NavinShipmentClient};
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
        &None,
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
        &None,
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
            &None,
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
        &None,
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
        &None,
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
