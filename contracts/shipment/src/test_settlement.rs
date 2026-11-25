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

// ── Settlement proof NFT (issue #1229) ────────────────────────────────────────
//
// The proof NFT is minted best-effort, and only for the final settlement (the
// Release/Refund that leaves the shipment with no escrow).

mod settlement_proof {
    use super::*;
    use crate::{NavinShipmentClient, SettlementProofKey};
    use soroban_sdk::{
        contract, contractimpl, symbol_short, testutils::Events, Symbol, TryFromVal, Vec,
    };

    /// NFT contract that passes `configure_settlement_nft` (it reports the
    /// shipment contract as its admin) but whose `mint` always fails.
    #[contract]
    struct AlwaysFailingNft;

    #[contractimpl]
    impl AlwaysFailingNft {
        pub fn __constructor(env: Env, admin: Address) {
            env.storage()
                .instance()
                .set(&symbol_short!("admin"), &admin);
        }

        pub fn get_admin(env: Env) -> Address {
            env.storage()
                .instance()
                .get(&symbol_short!("admin"))
                .unwrap()
        }

        pub fn mint(_env: Env, _to: Address, _token_id: u64) -> u64 {
            panic!("settlement NFT mint always fails");
        }
    }

    /// Contract address that opts in to receiving settlement proof NFTs. The
    /// NFT only mints to contract addresses that implement `can_receive_nft`.
    #[contract]
    struct OptInReceiver;

    #[contractimpl]
    impl OptInReceiver {
        pub fn can_receive_nft(_env: Env, _token_id: u64) -> bool {
            true
        }
    }

    /// Whether `topic` was emitted by the most recent contract invocation.
    fn emitted(env: &Env, topic: &str) -> bool {
        env.events()
            .all()
            .into_iter()
            .any(|(_contract, topics, _data)| {
                topics
                    .get(0)
                    .and_then(|v| Symbol::try_from_val(env, &v).ok())
                    .is_some_and(|sym| sym == Symbol::new(env, topic))
            })
    }

    fn proof_recorded(env: &Env, client: &NavinShipmentClient, shipment_id: u64) -> bool {
        env.as_contract(&client.address, || {
            env.storage()
                .persistent()
                .has(&SettlementProofKey::Minted(shipment_id))
        })
    }

    /// Create a funded shipment (10_000 escrow). Returns (id, company, receiver).
    fn funded_shipment(
        env: &Env,
        client: &NavinShipmentClient,
        admin: &Address,
        carrier: &Address,
        milestones: &Vec<(Symbol, u32)>,
    ) -> (u64, Address, Address) {
        let company = Address::generate(env);
        let receiver = Address::generate(env);
        client.add_company(admin, &company);
        client.add_carrier(admin, carrier);
        client.add_carrier_to_whitelist(&company, carrier);

        let id = client.create_shipment(
            &company,
            &receiver,
            carrier,
            &dummy_hash(env),
            milestones,
            &(env.ledger().timestamp() + 86_400),
        );
        client.deposit_escrow(&company, &id, &10_000);
        (id, company, receiver)
    }

    fn failing_nft_env() -> (Env, NavinShipmentClient<'static>, Address) {
        let (env, client, admin, _token) = setup_initialized_shipment_env();
        let nft = env.register(AlwaysFailingNft, (client.address.clone(),));
        client.configure_settlement_nft(
            &admin,
            &nft,
            &symbol_short!("SettleNFT"),
            &symbol_short!("SETTLE"),
        );
        (env, client, admin)
    }

    /// A broken NFT contract must not block escrow release.
    #[test]
    fn test_release_escrow_completes_when_nft_mint_always_fails() {
        let (env, client, admin) = failing_nft_env();
        let carrier = Address::generate(&env);
        let (id, _company, receiver) =
            funded_shipment(&env, &client, &admin, &carrier, &Vec::new(&env));
        env.as_contract(&client.address, || {
            let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
            shipment.status = ShipmentStatus::Delivered;
            crate::storage::set_shipment(&env, &shipment);
        });

        client.release_escrow(&receiver, &id);
        let failed_event = emitted(&env, crate::event_topics::PROOF_MINT_FAILED);

        let shipment = client.get_shipment(&id);
        assert_eq!(shipment.escrow_amount, 0, "escrow must still be released");
        assert!(shipment.finalized);
        let settlement = client.get_settlement(&2);
        assert_eq!(settlement.operation, SettlementOperation::Release);
        assert_eq!(settlement.state, SettlementState::Completed);
        assert!(failed_event, "a failed mint must emit proof_mint_failed");
        assert!(!proof_recorded(&env, &client, id));
    }

    /// A broken NFT contract must not block escrow refund either.
    #[test]
    fn test_refund_escrow_completes_when_nft_mint_always_fails() {
        let (env, client, admin) = failing_nft_env();
        let carrier = Address::generate(&env);
        let (id, company, _receiver) =
            funded_shipment(&env, &client, &admin, &carrier, &Vec::new(&env));

        client.refund_escrow(&company, &id);
        let failed_event = emitted(&env, crate::event_topics::PROOF_MINT_FAILED);

        let shipment = client.get_shipment(&id);
        assert_eq!(shipment.escrow_amount, 0, "escrow must still be refunded");
        assert_eq!(shipment.status, ShipmentStatus::Cancelled);
        let settlement = client.get_settlement(&2);
        assert_eq!(settlement.operation, SettlementOperation::Refund);
        assert_eq!(settlement.state, SettlementState::Completed);
        assert!(failed_event, "a failed mint must emit proof_mint_failed");
    }

    /// Partial milestone releases never mint (and never attempt to, so a
    /// failing NFT is not even touched); the settlement that empties the
    /// escrow is the one that mints the proof.
    #[test]
    fn test_proof_minted_only_by_final_settlement() {
        let (env, client, admin, _token) = setup_initialized_shipment_env();
        let nft = NavinShipmentNftClient::new(
            &env,
            &env.register(
                NavinShipmentNft,
                (
                    client.address.clone(),
                    symbol_short!("SettleNFT"),
                    symbol_short!("SETTLE"),
                ),
            ),
        );
        client.configure_settlement_nft(
            &admin,
            &nft.address,
            &symbol_short!("SettleNFT"),
            &symbol_short!("SETTLE"),
        );

        let mut milestones = Vec::new(&env);
        milestones.push_back((symbol_short!("alpha"), 40u32));
        milestones.push_back((symbol_short!("beta"), 35u32));
        milestones.push_back((symbol_short!("gamma"), 25u32));
        let carrier = env.register(OptInReceiver, ());
        let (id, _company, _receiver) =
            funded_shipment(&env, &client, &admin, &carrier, &milestones);
        env.as_contract(&client.address, || {
            let mut shipment = crate::storage::get_shipment(&env, id).unwrap();
            shipment.status = ShipmentStatus::InTransit;
            crate::storage::set_shipment(&env, &shipment);
        });

        client.release_milestone_payment(&carrier, &id, &symbol_short!("alpha"));
        client.release_milestone_payment(&carrier, &id, &symbol_short!("beta"));
        assert_eq!(client.get_shipment(&id).escrow_amount, 2_500);
        assert_eq!(nft.total_supply(), 0, "partial releases must not mint");
        assert!(!proof_recorded(&env, &client, id));

        client.release_milestone_payment(&carrier, &id, &symbol_short!("gamma"));
        let failed_event = emitted(&env, crate::event_topics::PROOF_MINT_FAILED);
        assert!(!failed_event);
        assert_eq!(client.get_shipment(&id).escrow_amount, 0);
        assert_eq!(nft.total_supply(), 1, "the final release mints the proof");
        assert_eq!(nft.owner_of(&id), carrier);
        assert!(proof_recorded(&env, &client, id));
    }
}

/// Integration tests for the NFT admin pass-through entry points added to fix
/// the permanently-unreachable admin functions on the settlement NFT.
mod settlement_nft_admin {
    use crate::test_helpers::*;
    use crate::test_utils::dummy_hash;
    use crate::types::ShipmentStatus;
    use crate::NavinError;
    use navin_nft::{NavinShipmentNft, NavinShipmentNftClient};
    use soroban_sdk::{Address, Env, String, Vec};

    fn setup_with_nft() -> (
        Env,
        NavinShipmentClient<'static>,
        Address,
        NavinShipmentNftClient<'static>,
    ) {
        let (env, client, admin, _token) = setup_initialized_shipment_env();
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
        (env, client, admin, nft)
    }

    // ── set_settlement_nft_base_uri ──────────────────────────────────────────

    /// Shipment admin can set the base URI, after which `get_token_uri` on a
    /// minted proof token no longer fails with `BaseUriNotSet`.
    #[test]
    fn admin_can_set_base_uri_and_proof_token_uri_resolves() {
        let (env, client, admin, nft) = setup_with_nft();

        // Mint a proof by going through a full deposit → deliver → release cycle.
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
            &Vec::new(&env),
            &(env.ledger().timestamp() + 86_400),
        );
        client.deposit_escrow(&company, &shipment_id, &1_000);
        env.as_contract(&client.address, || {
            let mut s = crate::storage::get_shipment(&env, shipment_id).unwrap();
            s.status = ShipmentStatus::Delivered;
            crate::storage::set_shipment(&env, &s);
        });
        client.release_escrow(&receiver, &shipment_id);

        // The proof token now exists on the NFT contract.
        assert_eq!(nft.total_supply(), 1);

        // Before setting the base URI, `get_token_uri` returns `BaseUriNotSet`.
        let err = nft.try_get_token_uri(&shipment_id).err();
        assert!(
            err.is_some(),
            "get_token_uri must fail before base URI is set"
        );

        // Set the base URI via the shipment contract (the NFT admin).
        let base = String::from_str(&env, "https://navin.example/proof/");
        client.set_settlement_nft_base_uri(&admin, &base);

        // Now `get_token_uri` resolves successfully.
        let uri = nft.get_token_uri(&shipment_id);
        assert!(uri.len() > 0, "URI must be non-empty after base URI is set");
    }

    /// Non-admin cannot call `set_settlement_nft_base_uri`.
    #[test]
    fn non_admin_cannot_set_base_uri() {
        let (env, client, admin, _nft) = setup_with_nft();
        let stranger = Address::generate(&env);
        let _ = admin; // suppress unused warning
        let base = String::from_str(&env, "https://navin.example/proof/");
        let result = client.try_set_settlement_nft_base_uri(&stranger, &base);
        assert_eq!(result, Err(Ok(NavinError::Unauthorized)));
    }

    /// `set_settlement_nft_base_uri` returns `InvalidConfig` when no NFT has
    /// been configured.
    #[test]
    fn set_base_uri_returns_invalid_config_when_no_nft_configured() {
        let (env, client, admin, _token) = setup_initialized_shipment_env();
        let base = String::from_str(&env, "https://navin.example/proof/");
        let result = client.try_set_settlement_nft_base_uri(&admin, &base);
        assert_eq!(result, Err(Ok(NavinError::InvalidConfig)));
    }

    // ── pause_settlement_nft / unpause_settlement_nft ────────────────────────

    /// Shipment admin can pause and then unpause the proof NFT.
    #[test]
    fn admin_can_pause_and_unpause_nft() {
        let (env, client, admin, nft) = setup_with_nft();

        assert!(!nft.is_paused(), "NFT must start unpaused");
        client.pause_settlement_nft(&admin);
        assert!(nft.is_paused(), "NFT must be paused after admin call");
        client.unpause_settlement_nft(&admin);
        assert!(!nft.is_paused(), "NFT must be unpaused after admin call");
    }

    /// Non-admin cannot pause the NFT.
    #[test]
    fn non_admin_cannot_pause_nft() {
        let (env, client, _admin, _nft) = setup_with_nft();
        let stranger = Address::generate(&env);
        let result = client.try_pause_settlement_nft(&stranger);
        assert_eq!(result, Err(Ok(NavinError::Unauthorized)));
    }

    /// Non-admin cannot unpause the NFT.
    #[test]
    fn non_admin_cannot_unpause_nft() {
        let (env, client, admin, _nft) = setup_with_nft();
        client.pause_settlement_nft(&admin);
        let stranger = Address::generate(&env);
        let result = client.try_unpause_settlement_nft(&stranger);
        assert_eq!(result, Err(Ok(NavinError::Unauthorized)));
    }

    /// `pause_settlement_nft` returns `InvalidConfig` when no NFT is configured.
    #[test]
    fn pause_returns_invalid_config_when_no_nft_configured() {
        let (env, client, admin, _token) = setup_initialized_shipment_env();
        let result = client.try_pause_settlement_nft(&admin);
        assert_eq!(result, Err(Ok(NavinError::InvalidConfig)));
    }

    // ── transfer_settlement_nft_admin ────────────────────────────────────────

    /// Shipment admin can propose a transfer of NFT admin custody.
    #[test]
    fn admin_can_transfer_nft_admin_custody() {
        let (env, client, admin, nft) = setup_with_nft();
        let new_nft_admin = Address::generate(&env);

        // Propose the transfer via the shipment contract.
        client.transfer_settlement_nft_admin(&admin, &new_nft_admin);

        // The NFT contract now has a pending proposed admin.
        let proposed = nft.get_proposed_admin();
        assert_eq!(proposed, new_nft_admin);
    }

    /// Non-admin cannot transfer NFT admin custody.
    #[test]
    fn non_admin_cannot_transfer_nft_admin() {
        let (env, client, _admin, _nft) = setup_with_nft();
        let stranger = Address::generate(&env);
        let new_nft_admin = Address::generate(&env);
        let result = client.try_transfer_settlement_nft_admin(&stranger, &new_nft_admin);
        assert_eq!(result, Err(Ok(NavinError::Unauthorized)));
    }

    /// `transfer_settlement_nft_admin` returns `InvalidConfig` when no NFT is
    /// configured.
    #[test]
    fn transfer_nft_admin_returns_invalid_config_when_no_nft() {
        let (env, client, admin, _token) = setup_initialized_shipment_env();
        let new_nft_admin = Address::generate(&env);
        let result = client.try_transfer_settlement_nft_admin(&admin, &new_nft_admin);
        assert_eq!(result, Err(Ok(NavinError::InvalidConfig)));
    }
}
