use crate::test_helpers::*;
use crate::test_utils::dummy_hash;
use crate::types::*;
use crate::NavinError;
use navin_nft::{NavinShipmentNft, NavinShipmentNftClient};
use soroban_sdk::{Address, BytesN, Env};

fn seeded_hash(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [1u8; 32];
    bytes[31] = seed;
    BytesN::from_array(env, &bytes)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn test_deposit_escrow_rejects_admin_as_non_sender() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &dummy_hash(&env),
        &soroban_sdk::Vec::new(&env),
        &(env.ledger().timestamp() + 86400),
    );

    let result = client.try_deposit_escrow(&admin, &shipment_id, &1_000);
    assert_eq!(result, Err(Ok(NavinError::Unauthorized)));
    assert_eq!(client.get_settlement_count(), 0);
}

#[test]
fn test_configure_settlement_nft_rejects_non_nft_address() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let not_an_nft = Address::generate(&env);

    let result = client.try_configure_settlement_nft(
        &admin,
        &not_an_nft,
        &soroban_sdk::symbol_short!("SettleNFT"),
        &soroban_sdk::symbol_short!("SETTLE"),
    );
    assert_eq!(result, Err(Ok(NavinError::NftConfigurationFailed)));
}

/// An unknown settlement ID must return `SettlementNotFound`, not
/// `ShipmentNotFound`, so integrators can tell the two apart.
#[test]
fn test_get_settlement_unknown_id_returns_settlement_not_found() {
    let (_env, client, _admin, _token_contract) = setup_initialized_shipment_env();

    let result = client.try_get_settlement(&999);
    assert_eq!(result, Err(Ok(NavinError::SettlementNotFound)));
}

/// Test that deposit_escrow creates a settlement record in Pending state
/// and transitions to Completed on success.
#[test]
fn test_deposit_escrow_settlement_success() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let escrow_amount: i128 = 1000;

    // Deposit escrow - should create settlement record
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    // Verify settlement was created
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 1);

    // Get the settlement record
    let settlement = client.get_settlement(&1);
    assert_eq!(settlement.settlement_id, 1);
    assert_eq!(settlement.shipment_id, shipment_id);
    assert_eq!(settlement.operation, SettlementOperation::Deposit);
    assert_eq!(settlement.state, SettlementState::Completed);
    assert_eq!(settlement.amount, escrow_amount);
    assert_eq!(settlement.from, company);
    assert_eq!(settlement.to, client.address.clone());
    assert!(settlement.completed_at.is_some());
    assert!(settlement.error_code.is_none());

    // Verify no active settlement remains
    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

/// Test that deposit_escrow returns an error when token transfer fails.
/// Because Soroban reverts state on panic, no settlement record is persisted.
#[test]
fn test_deposit_escrow_settlement_failure() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env_with_failing_token();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let escrow_amount: i128 = 1000;

    // Attempt to deposit escrow via the failing token - should return error
    let result = client.try_deposit_escrow(&company, &shipment_id, &escrow_amount);
    assert!(result.is_err());

    // On Soroban, the failed call is reverted atomically, so no settlement is persisted.
    // Verify settlement was NOT created (transaction rolled back)
    // Soroban reverts all state when a contract call panics/errors,
    // so no settlement record is persisted.
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 0);

    // Verify no active settlement
    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

/// Test that release_escrow creates a settlement record and transitions correctly.
#[test]
fn test_release_escrow_settlement_success() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let escrow_amount: i128 = 5000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    // Transition to Delivered
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    // Release escrow - should create settlement record
    client.release_escrow(&receiver, &shipment_id);

    // Verify two settlements: deposit + release
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 2);

    // Get the release settlement record
    let settlement = client.get_settlement(&2);
    assert_eq!(settlement.settlement_id, 2);
    assert_eq!(settlement.shipment_id, shipment_id);
    assert_eq!(settlement.operation, SettlementOperation::Release);
    assert_eq!(settlement.state, SettlementState::Completed);
    assert_eq!(settlement.amount, escrow_amount);
    assert_eq!(settlement.from, client.address.clone());
    assert_eq!(settlement.to, carrier);
    assert!(settlement.completed_at.is_some());
    assert!(settlement.error_code.is_none());

    // Verify no active settlement remains
    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

/// Test that refund_escrow creates a settlement record and transitions correctly.
#[test]
fn test_refund_escrow_settlement_success() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let escrow_amount: i128 = 3000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    // Refund escrow - should create settlement record
    client.refund_escrow(&company, &shipment_id);

    // Verify two settlements: deposit + refund
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 2);

    // Get the refund settlement record
    let settlement = client.get_settlement(&2);
    assert_eq!(settlement.settlement_id, 2);
    assert_eq!(settlement.shipment_id, shipment_id);
    assert_eq!(settlement.operation, SettlementOperation::Refund);
    assert_eq!(settlement.state, SettlementState::Completed);
    assert_eq!(settlement.amount, escrow_amount);
    assert_eq!(settlement.from, client.address.clone());
    assert_eq!(settlement.to, company);
    assert!(settlement.completed_at.is_some());
    assert!(settlement.error_code.is_none());

    // Verify no active settlement remains
    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

/// Test that refund_escrow returns an error when token transfer fails.
/// Because Soroban reverts state on panic, the settlement record is not persisted.
#[test]
fn test_refund_escrow_settlement_failure() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env_with_failing_token();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // Manually set escrow to bypass the failing transfer during deposit
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.escrow_amount = 3000;
        crate::storage::set_shipment(&env, &shipment);
        crate::storage::set_escrow(&env, shipment_id, 3000);
    });

    // Attempt to refund escrow - should fail because token.transfer panics
    let result = client.try_refund_escrow(&company, &shipment_id);
    assert!(result.is_err());

    // On Soroban, the failed call is reverted atomically, so no settlement is persisted.
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 0);

    // Verify no active settlement remains
    // Verify settlement was NOT created (transaction rolled back)
    // Soroban reverts all state when a contract call panics,
    // so no settlement record is persisted.
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 0);

    // Verify no active settlement
    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
}

/// Test that release_escrow returns an error when token transfer fails.
/// Because Soroban reverts state on panic, the settlement record is not persisted.
#[test]
fn test_release_escrow_settlement_failure() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env_with_failing_token();
    let nft = NavinShipmentNftClient::new(
        &env,
        &env.register(
            NavinShipmentNft,
            (
                client.address.clone(),
                soroban_sdk::symbol_short!("SettleNFT"),
                soroban_sdk::symbol_short!("SETTLE"),
            ),
        ),
    );
    client.configure_settlement_nft(
        &admin,
        &nft.address,
        &soroban_sdk::symbol_short!("SettleNFT"),
        &soroban_sdk::symbol_short!("SETTLE"),
    );

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // Manually set escrow to bypass the failing transfer during deposit
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.escrow_amount = 5000;
        shipment.status = ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
        crate::storage::set_escrow(&env, shipment_id, 5000);
    });

    // Attempt to release escrow - should fail because token.transfer panics
    let result = client.try_release_escrow(&receiver, &shipment_id);
    assert!(result.is_err());

    // Verify settlement was NOT created
    let settlement_count = client.get_settlement_count();
    assert_eq!(settlement_count, 0);

    // Verify no active settlement
    let active = client.get_active_settlement(&shipment_id);
    assert!(active.is_none());
    assert_eq!(nft.total_supply(), 0);
}

/// Test settlement state transitions through full lifecycle.
#[test]
fn test_settlement_full_lifecycle() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // Step 1: Deposit escrow
    client.deposit_escrow(&company, &shipment_id, &10000);
    let settlement1 = client.get_settlement(&1);
    assert_eq!(settlement1.state, SettlementState::Completed);
    assert_eq!(settlement1.operation, SettlementOperation::Deposit);

    // Step 2: Transition to Delivered
    env.as_contract(&client.address, || {
        let mut shipment = crate::storage::get_shipment(&env, shipment_id).unwrap();
        shipment.status = ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &shipment);
    });

    // Step 3: Release escrow
    client.release_escrow(&receiver, &shipment_id);
    let settlement2 = client.get_settlement(&2);
    assert_eq!(settlement2.state, SettlementState::Completed);
    assert_eq!(settlement2.operation, SettlementOperation::Release);

    // Verify total settlements
    assert_eq!(client.get_settlement_count(), 2);

    // Verify no active settlements
    assert!(client.get_active_settlement(&shipment_id).is_none());
}

/// Test that settlement records are queryable and contain correct metadata.
#[test]
fn test_settlement_record_metadata() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = dummy_hash(&env);
    let deadline = env.ledger().timestamp() + 86400;

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let before_timestamp = env.ledger().timestamp();
    client.deposit_escrow(&company, &shipment_id, &5000);
    let after_timestamp = env.ledger().timestamp();

    let settlement = client.get_settlement(&1);

    // Verify all metadata fields
    assert_eq!(settlement.settlement_id, 1);
    assert_eq!(settlement.shipment_id, shipment_id);
    assert_eq!(settlement.amount, 5000);
    assert_eq!(settlement.from, company);
    assert_eq!(settlement.to, client.address.clone());
    assert!(settlement.initiated_at >= before_timestamp);
    assert!(settlement.initiated_at <= after_timestamp);
    assert!(settlement.completed_at.is_some());
    assert!(settlement.completed_at.unwrap() >= settlement.initiated_at);
}

/// Test that multiple shipments can have independent settlement records.
#[test]
fn test_multiple_shipments_independent_settlements() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash1 = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 86400;

    // Use distinct data hashes so the idempotency check treats them as separate operations.
    let shipment_id1 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash1,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let shipment_id2 = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &seeded_hash(&env, 2),
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    // Deposit escrow for both
    client.deposit_escrow(&company, &shipment_id1, &1000);
    client.deposit_escrow(&company, &shipment_id2, &2000);

    // Verify settlements are independent
    let settlement1 = client.get_settlement(&1);
    let settlement2 = client.get_settlement(&2);

    assert_eq!(settlement1.shipment_id, shipment_id1);
    assert_eq!(settlement1.amount, 1000);

    assert_eq!(settlement2.shipment_id, shipment_id2);
    assert_eq!(settlement2.amount, 2000);

    assert_eq!(client.get_settlement_count(), 2);
}

// ── #1224: every escrow-moving path goes through the settlement pipeline ─────

/// Shared setup: company/carrier registered, shipment created, escrow deposited.
fn shipment_with_escrow(
    env: &Env,
    client: &crate::NavinShipmentClient,
    admin: &Address,
    escrow_amount: i128,
) -> (Address, Address, u64) {
    let company = Address::generate(env);
    let carrier = Address::generate(env);
    let receiver = Address::generate(env);

    client.add_company(admin, &company);
    client.add_carrier(admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &dummy_hash(env),
        &soroban_sdk::Vec::new(env),
        &(env.ledger().timestamp() + 86400),
    );
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);
    (company, carrier, shipment_id)
}

/// cancel_shipment must refund through the settlement pipeline: a completed
/// Refund settlement record exists and the escrow key is cleaned up.
#[test]
fn test_cancel_shipment_creates_refund_settlement() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let (company, _carrier, shipment_id) = shipment_with_escrow(&env, &client, &admin, 1000);
    assert_eq!(client.get_settlement_count(), 1);

    let reason_hash = BytesN::from_array(&env, &[7u8; 32]);
    client.cancel_shipment(&company, &shipment_id, &reason_hash);

    // deposit + refund settlements, both completed
    assert_eq!(client.get_settlement_count(), 2);
    let refund = client.get_settlement(&2);
    assert_eq!(refund.operation, SettlementOperation::Refund);
    assert_eq!(refund.state, SettlementState::Completed);
    assert_eq!(refund.amount, 1000);
    assert_eq!(refund.to, company);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);

    env.as_contract(&client.address, || {
        assert!(
            !crate::storage::has_escrow_entry(&env, shipment_id),
            "escrow key must be removed after settlement"
        );
    });
}

/// check_deadline auto-expiry must also refund through the settlement pipeline.
#[test]
fn test_check_deadline_creates_refund_settlement() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let (_company, _carrier, shipment_id) = shipment_with_escrow(&env, &client, &admin, 500);

    // Jump past deadline + default grace period (86400s).
    use soroban_sdk::testutils::Ledger;
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 86400 + 86400 + 1);
    client.check_deadline(&shipment_id);

    assert_eq!(client.get_settlement_count(), 2);
    let refund = client.get_settlement(&2);
    assert_eq!(refund.operation, SettlementOperation::Refund);
    assert_eq!(refund.state, SettlementState::Completed);
    assert_eq!(refund.amount, 500);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
}

/// ForceRelease via multi-sig must release through the settlement pipeline.
#[test]
fn test_force_release_creates_release_settlement() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let (_company, carrier, shipment_id) = shipment_with_escrow(&env, &client, &admin, 700);

    let admin2 = Address::generate(&env);
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &2);

    let reason_hash = BytesN::from_array(&env, &[9u8; 32]);
    let action = crate::types::AdminAction::ForceRelease(shipment_id, reason_hash.clone());
    let proposal_id = client.propose_action(&admin, &action);
    client.approve_action(&admin2, &proposal_id);

    assert_eq!(client.get_settlement_count(), 2);
    let release = client.get_settlement(&2);
    assert_eq!(release.operation, SettlementOperation::Release);
    assert_eq!(release.state, SettlementState::Completed);
    assert_eq!(release.amount, 700);
    assert_eq!(release.to, carrier);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Delivered);
    assert_eq!(shipment.escrow_amount, 0);
}

/// ForceRefund via multi-sig must refund through the settlement pipeline.
#[test]
fn test_force_refund_creates_refund_settlement() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let (company, _carrier, shipment_id) = shipment_with_escrow(&env, &client, &admin, 800);

    let admin2 = Address::generate(&env);
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &2);

    let reason_hash = BytesN::from_array(&env, &[10u8; 32]);
    let action = crate::types::AdminAction::ForceRefund(shipment_id, reason_hash);
    let proposal_id = client.propose_action(&admin, &action);
    client.approve_action(&admin2, &proposal_id);

    assert_eq!(client.get_settlement_count(), 2);
    let refund = client.get_settlement(&2);
    assert_eq!(refund.operation, SettlementOperation::Refund);
    assert_eq!(refund.state, SettlementState::Completed);
    assert_eq!(refund.amount, 800);
    assert_eq!(refund.to, company);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);
    assert_eq!(shipment.escrow_amount, 0);
}

/// PartialRefund dispute resolution must produce two settlements (release +
/// refund) for the split shares, all through the settlement pipeline.
#[test]
fn test_resolve_dispute_partial_refund_creates_split_settlements() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let (company, carrier, shipment_id) = shipment_with_escrow(&env, &client, &admin, 1001);

    let dispute_reason = BytesN::from_array(&env, &[11u8; 32]);
    client.raise_dispute(&company, &shipment_id, &dispute_reason);

    let resolve_reason = BytesN::from_array(&env, &[12u8; 32]);
    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::types::DisputeResolution::PartialRefund,
        &resolve_reason,
    );

    // deposit + release(500) + refund(501)
    assert_eq!(client.get_settlement_count(), 3);
    let release = client.get_settlement(&2);
    assert_eq!(release.operation, SettlementOperation::Release);
    assert_eq!(release.state, SettlementState::Completed);
    assert_eq!(release.amount, 500);
    assert_eq!(release.to, carrier);
    let refund = client.get_settlement(&3);
    assert_eq!(refund.operation, SettlementOperation::Refund);
    assert_eq!(refund.state, SettlementState::Completed);
    assert_eq!(refund.amount, 501);
    assert_eq!(refund.to, company);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::PartiallyRefunded);
    assert_eq!(shipment.escrow_amount, 0);
}

// ── #1225: ContractConfig::auto_mint_nft gates settlement proof minting ──────

/// Register a settlement NFT and return its client.
fn register_settlement_nft<'a>(
    env: &'a Env,
    client: &crate::NavinShipmentClient,
    admin: &Address,
) -> NavinShipmentNftClient<'a> {
    let nft = NavinShipmentNftClient::new(
        env,
        &env.register(
            NavinShipmentNft,
            (
                client.address.clone(),
                soroban_sdk::symbol_short!("SettleNFT"),
                soroban_sdk::symbol_short!("SETTLE"),
            ),
        ),
    );
    client.configure_settlement_nft(
        admin,
        &nft.address,
        &soroban_sdk::symbol_short!("SettleNFT"),
        &soroban_sdk::symbol_short!("SETTLE"),
    );
    nft
}

/// Drive a shipment to Delivered with escrow held, then release to carrier.
fn deliver_and_release(
    env: &Env,
    client: &crate::NavinShipmentClient,
    admin: &Address,
    company: &Address,
    carrier: &Address,
    receiver: &Address,
) -> u64 {
    let shipment_id = client.create_shipment(
        company,
        receiver,
        carrier,
        &dummy_hash(env),
        &soroban_sdk::Vec::new(env),
        &(env.ledger().timestamp() + 86400),
    );
    client.deposit_escrow(company, &shipment_id, &5_000);
    client.update_status(
        admin,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(env, &[0x71u8; 32]),
    );
    client.update_status(
        admin,
        &shipment_id,
        &ShipmentStatus::AtCheckpoint,
        &BytesN::from_array(env, &[0x72u8; 32]),
    );
    client.update_status(
        admin,
        &shipment_id,
        &ShipmentStatus::Delivered,
        &BytesN::from_array(env, &[0x73u8; 32]),
    );
    client.release_escrow(receiver, &shipment_id);
    shipment_id
}

/// With `auto_mint_nft = false` (the default), a configured settlement NFT
/// must NOT be minted to on release — the flag is the master switch (#1225).
#[test]
fn test_auto_mint_nft_disabled_skips_minting() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let nft = register_settlement_nft(&env, &client, &admin);
    assert!(!client.get_contract_config().auto_mint_nft);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let shipment_id = deliver_and_release(&env, &client, &admin, &company, &carrier, &receiver);

    // The settlement still completed — only the NFT mint is skipped.
    let release = client.get_settlement(&2);
    assert_eq!(release.state, SettlementState::Completed);
    assert_eq!(
        nft.total_supply(),
        0,
        "no proof NFT may be minted while auto_mint_nft is false"
    );
    let _ = shipment_id;
}

/// With `auto_mint_nft = true`, the same release mints exactly one proof NFT.
#[test]
fn test_auto_mint_nft_enabled_mints_settlement_proof() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let nft = register_settlement_nft(&env, &client, &admin);

    let mut cfg = client.get_contract_config();
    cfg.auto_mint_nft = true;
    client.update_config(&admin, &cfg);
    assert!(client.get_contract_config().auto_mint_nft);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let shipment_id = deliver_and_release(&env, &client, &admin, &company, &carrier, &receiver);

    assert_eq!(
        nft.total_supply(),
        1,
        "exactly one proof NFT must be minted when auto_mint_nft is true"
    );
    // The proof is minted to the settlement recipient (the carrier on release).
    assert_eq!(nft.owner_of(&shipment_id), carrier);
}
