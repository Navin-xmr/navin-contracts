use crate::{NavinShipment, NavinShipmentClient, ShipmentStatus};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Symbol, Vec};

#[soroban_sdk::contract]
struct MockToken;
#[soroban_sdk::contractimpl]
impl MockToken {
    pub fn decimals(_env: soroban_sdk::Env) -> u32 {
        7
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

fn setup_shipment_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let (env, admin) = crate::test_utils::setup_env();

    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));

    (env, client, admin, token_contract)
}

#[test]
fn test_finalization_on_delivery_settlement() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    // Initial state: not finalized
    let shipment = client.get_shipment(&shipment_id);
    assert!(!shipment.finalized);

    // Step 1: Deposit escrow
    client.deposit_escrow(&company, &shipment_id, &1000);

    // Step 2: Transition to Delivered - this should release remaining escrow and finalize
    client.update_status(
        &carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &data_hash,
    );
    client.confirm_delivery(&receiver, &shipment_id, &data_hash);

    // Should be finalized because status is Delivered and escrow is released (cleared to 0)
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Delivered);
    assert_eq!(shipment.escrow_amount, 0);
    assert!(shipment.finalized);
}

#[test]
fn test_finalization_on_cancel_with_zero_escrow() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    // Initial state: not finalized
    let shipment = client.get_shipment(&shipment_id);
    assert!(!shipment.finalized);

    // Cancel without escrow should finalize immediately
    client.cancel_shipment(&company, &shipment_id, &data_hash);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
    assert!(shipment.finalized);
}

#[test]
#[should_panic(expected = "Error(Contract, #38)")]
fn test_mutation_rejected_after_finalization() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    // Finalize it
    client.cancel_shipment(&company, &shipment_id, &data_hash);
    let shipment = client.get_shipment(&shipment_id);
    assert!(shipment.finalized);

    // Try to update metadata - should panic with ShipmentFinalized (38)
    client.set_shipment_metadata(
        &company,
        &shipment_id,
        &Symbol::new(&env, "key"),
        &Symbol::new(&env, "val"),
    );
}

#[test]
fn test_archival_permitted_after_finalization() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    // Finalize it
    client.cancel_shipment(&company, &shipment_id, &data_hash);
    let shipment = client.get_shipment(&shipment_id);
    assert!(shipment.finalized);

    // Archiving should succeed (proving the finalize lock exception)
    client.archive_shipment(&admin, &shipment_id);

    // Verify it's still readable (fallback to temporary storage works)
    let archived = client.get_shipment(&shipment_id);
    assert_eq!(archived.id, shipment_id);
}
