//! # NFT Proof-of-Delivery Integration Tests
//!
//! Exercises the full cross-contract workflow between the shipment contract
//! and the `navin-nft` (`NavinShipmentNft`) contract:
//!
//! - create shipment → deliver → mint proof-of-delivery NFT to the receiver
//! - shipment expires → proof NFT is burned
//! - active dispute → proof NFT is locked (cannot be burned by expiry sync)
//!
//! The shipment contract does not call the NFT contract directly; the proof
//! NFT lifecycle is driven by an off-chain/system orchestrator that holds the
//! NFT admin role and reacts to shipment state. [`ProofOrchestrator`] models
//! that system actor so that a breaking change on either side (e.g. new
//! parameters in `mint`, renamed shipment statuses, changed expiry rules)
//! fails this suite instead of breaking the integration silently.
//!
//! Proof NFTs use the shipment ID as their token ID, giving a 1:1 mapping
//! between a shipment and its proof of delivery.

use navin_nft::{NavinShipmentNft, NavinShipmentNftClient, NftError};
use shipment::{NavinError, NavinShipment, NavinShipmentClient, ShipmentStatus};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, Vec,
};

// ── Mock token ───────────────────────────────────────────────────────────────

mod mock_token {
    use soroban_sdk::{contract, contractimpl, Address, Env};

    #[contract]
    pub struct MockToken;

    #[contractimpl]
    impl MockToken {
        pub fn decimals(_env: Env) -> u32 {
            7
        }

        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
        pub fn mint(_env: Env, _admin: Address, _to: Address, _amount: i128) {}
    }
}

// ── Test constants ───────────────────────────────────────────────────────────

const PROTOCOL_VERSION: u32 = 22;
const START_TIMESTAMP: u64 = 86_400;
const RATE_LIMIT_SECONDS: u64 = 61;
const DEADLINE_OFFSET: u64 = 3_600;

// ── Orchestrator ─────────────────────────────────────────────────────────────

/// System actor that owns the NFT admin role and keeps proof NFTs in sync
/// with shipment state.
struct ProofOrchestrator<'a> {
    shipment: &'a NavinShipmentClient<'static>,
    nft: &'a NavinShipmentNftClient<'static>,
    nft_admin: &'a Address,
}

impl ProofOrchestrator<'_> {
    /// Mints the proof-of-delivery NFT for a delivered shipment to its
    /// receiver. Returns the minted token ID (== shipment ID).
    fn mint_proof_on_delivery(&self, shipment_id: u64) -> Result<u64, NftError> {
        let shipment = self.shipment.get_shipment(&shipment_id);
        assert_eq!(
            shipment.status,
            ShipmentStatus::Delivered,
            "proof of delivery may only be minted for delivered shipments"
        );
        self.mint_proof(shipment_id)
    }

    /// Mints a proof NFT for the shipment to its receiver regardless of
    /// status (custody receipt issued ahead of delivery).
    fn mint_proof(&self, shipment_id: u64) -> Result<u64, NftError> {
        let shipment = self.shipment.get_shipment(&shipment_id);
        match self.nft.try_mint(&shipment.receiver, &shipment_id) {
            Ok(Ok(token_id)) => Ok(token_id),
            Err(Ok(err)) => Err(err),
            other => panic!("unexpected mint result: {:?}", other),
        }
    }

    /// Reconciles the proof NFT with the shipment's current state.
    ///
    /// - `Cancelled` (expired) → the proof NFT is burned by the NFT admin.
    /// - `Disputed` → the proof NFT is locked; nothing is burned.
    /// - anything else → no-op.
    ///
    /// Returns `true` when the proof NFT was burned.
    fn sync_proof(&self, shipment_id: u64) -> bool {
        let shipment = self.shipment.get_shipment(&shipment_id);
        match shipment.status {
            ShipmentStatus::Disputed => false,
            ShipmentStatus::Cancelled => {
                if self.nft.try_owner_of(&shipment_id).is_err() {
                    return false;
                }
                self.nft.burn(self.nft_admin, &shipment_id);
                true
            }
            _ => false,
        }
    }
}

// ── Setup ────────────────────────────────────────────────────────────────────

struct Ctx {
    env: Env,
    shipment: NavinShipmentClient<'static>,
    nft: NavinShipmentNftClient<'static>,
    nft_admin: Address,
    company: Address,
    carrier: Address,
}

impl Ctx {
    fn orchestrator(&self) -> ProofOrchestrator<'_> {
        ProofOrchestrator {
            shipment: &self.shipment,
            nft: &self.nft,
            nft_admin: &self.nft_admin,
        }
    }
}

fn setup() -> Ctx {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.protocol_version = PROTOCOL_VERSION;
        li.timestamp = START_TIMESTAMP;
        li.sequence_number = 1;
    });
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = env.register(mock_token::MockToken {}, ());
    let shipment = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    shipment.initialize(&admin, &token);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    shipment.add_company(&admin, &company);
    shipment.add_carrier(&admin, &carrier);
    shipment.add_carrier_to_whitelist(&company, &carrier);

    let nft_admin = Address::generate(&env);
    let nft = NavinShipmentNftClient::new(&env, &env.register(NavinShipmentNft, ()));
    nft.initialize(
        &nft_admin,
        &symbol_short!("NavinPOD"),
        &symbol_short!("NPOD"),
    );

    Ctx {
        env,
        shipment,
        nft,
        nft_admin,
        company,
        carrier,
    }
}

fn hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn advance_time(env: &Env, seconds: u64) {
    env.ledger().with_mut(|li| li.timestamp += seconds);
}

fn create_shipment(ctx: &Ctx, receiver: &Address, seed: u8) -> u64 {
    let deadline = ctx.env.ledger().timestamp() + DEADLINE_OFFSET;
    ctx.shipment.create_shipment(
        &ctx.company,
        receiver,
        &ctx.carrier,
        &hash(&ctx.env, seed),
        &Vec::new(&ctx.env),
        &deadline,
    )
}

fn move_in_transit(ctx: &Ctx, shipment_id: u64, seed: u8) {
    advance_time(&ctx.env, RATE_LIMIT_SECONDS);
    ctx.shipment.update_status(
        &ctx.carrier,
        &shipment_id,
        &ShipmentStatus::InTransit,
        &hash(&ctx.env, seed),
    );
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn test_delivery_proof_mints_nft_to_recipient() {
    let ctx = setup();
    let receiver = Address::generate(&ctx.env);
    let id = create_shipment(&ctx, &receiver, 1);
    ctx.shipment.deposit_escrow(&ctx.company, &id, &1_000);
    move_in_transit(&ctx, id, 2);

    ctx.shipment
        .confirm_delivery(&receiver, &id, &hash(&ctx.env, 3));
    assert_eq!(
        ctx.shipment.get_shipment(&id).status,
        ShipmentStatus::Delivered
    );

    let token_id = ctx.orchestrator().mint_proof_on_delivery(id).unwrap();

    assert_eq!(token_id, id);
    assert_eq!(ctx.nft.owner_of(&token_id), receiver);
    assert_eq!(ctx.nft.balance_of(&receiver), 1);
    assert_eq!(ctx.nft.total_supply(), 1);
    // Nobody else in the shipment received a proof NFT.
    assert_eq!(ctx.nft.balance_of(&ctx.company), 0);
    assert_eq!(ctx.nft.balance_of(&ctx.carrier), 0);
}

#[test]
fn test_delivery_proof_cannot_be_minted_twice() {
    let ctx = setup();
    let receiver = Address::generate(&ctx.env);
    let id = create_shipment(&ctx, &receiver, 4);
    move_in_transit(&ctx, id, 5);
    ctx.shipment
        .confirm_delivery(&receiver, &id, &hash(&ctx.env, 6));

    let orchestrator = ctx.orchestrator();
    orchestrator.mint_proof_on_delivery(id).unwrap();
    assert_eq!(
        orchestrator.mint_proof_on_delivery(id),
        Err(NftError::TokenAlreadyMinted)
    );
    assert_eq!(ctx.nft.total_supply(), 1);
}

#[test]
fn test_delivery_proofs_map_one_to_one_with_shipments() {
    let ctx = setup();
    let receiver_a = Address::generate(&ctx.env);
    let receiver_b = Address::generate(&ctx.env);
    let id_a = create_shipment(&ctx, &receiver_a, 7);
    let id_b = create_shipment(&ctx, &receiver_b, 8);
    assert_ne!(id_a, id_b);

    move_in_transit(&ctx, id_a, 9);
    move_in_transit(&ctx, id_b, 10);
    ctx.shipment
        .confirm_delivery(&receiver_a, &id_a, &hash(&ctx.env, 11));
    ctx.shipment
        .confirm_delivery(&receiver_b, &id_b, &hash(&ctx.env, 12));

    let orchestrator = ctx.orchestrator();
    orchestrator.mint_proof_on_delivery(id_a).unwrap();
    orchestrator.mint_proof_on_delivery(id_b).unwrap();

    assert_eq!(ctx.nft.owner_of(&id_a), receiver_a);
    assert_eq!(ctx.nft.owner_of(&id_b), receiver_b);
    assert_eq!(ctx.nft.total_supply(), 2);
}

#[test]
fn test_nft_proof_expiration_with_shipment_ttl() {
    let ctx = setup();
    let receiver = Address::generate(&ctx.env);
    let id = create_shipment(&ctx, &receiver, 20);
    move_in_transit(&ctx, id, 21);

    let orchestrator = ctx.orchestrator();
    orchestrator.mint_proof(id).unwrap();
    assert_eq!(ctx.nft.owner_of(&id), receiver);

    // Before the deadline the shipment is not expired and the proof stays.
    assert_eq!(
        ctx.shipment.try_check_deadline(&id),
        Err(Ok(NavinError::NotExpired))
    );
    assert!(!orchestrator.sync_proof(id));
    assert_eq!(ctx.nft.owner_of(&id), receiver);

    // Past the deadline the shipment expires and the proof NFT is burned.
    advance_time(&ctx.env, DEADLINE_OFFSET + 1);
    ctx.shipment.check_deadline(&id);
    assert_eq!(
        ctx.shipment.get_shipment(&id).status,
        ShipmentStatus::Cancelled
    );

    assert!(orchestrator.sync_proof(id));
    assert_eq!(
        ctx.nft.try_owner_of(&id),
        Err(Ok(NftError::TokenDoesNotExist))
    );
    assert_eq!(ctx.nft.balance_of(&receiver), 0);
    assert_eq!(ctx.nft.total_supply(), 0);

    // Re-syncing an already-burned proof is a no-op.
    assert!(!orchestrator.sync_proof(id));
}

#[test]
fn test_dispute_locks_proof_nft() {
    let ctx = setup();
    let receiver = Address::generate(&ctx.env);
    let id = create_shipment(&ctx, &receiver, 30);
    move_in_transit(&ctx, id, 31);

    let orchestrator = ctx.orchestrator();
    orchestrator.mint_proof(id).unwrap();

    ctx.shipment
        .raise_dispute(&receiver, &id, &hash(&ctx.env, 32));
    assert_eq!(
        ctx.shipment.get_shipment(&id).status,
        ShipmentStatus::Disputed
    );

    // Even once the deadline passes, a disputed shipment cannot expire...
    advance_time(&ctx.env, DEADLINE_OFFSET + 1);
    assert_eq!(
        ctx.shipment.try_check_deadline(&id),
        Err(Ok(NavinError::ShipmentAlreadyCompleted))
    );
    assert_eq!(
        ctx.shipment.get_shipment(&id).status,
        ShipmentStatus::Disputed
    );

    // ...so the proof NFT is locked and survives the expiry sync.
    assert!(!orchestrator.sync_proof(id));
    assert_eq!(ctx.nft.owner_of(&id), receiver);
    assert_eq!(ctx.nft.balance_of(&receiver), 1);
    assert_eq!(ctx.nft.total_supply(), 1);
}
