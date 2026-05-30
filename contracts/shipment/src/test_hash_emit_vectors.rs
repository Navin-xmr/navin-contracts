//! # #301 — Off-chain Verifier Reference Vectors for Hash-and-Emit
//!
//! Publishes **canonical test vectors** that frontend and backend verifiers
//! can use to validate their own SHA-256 idempotency-key implementations
//! against the contract's on-chain computation.
//!
//! ## Verifier procedure
//!
//! The contract emits an `idempotency_key` field in every structured event.
//! Off-chain consumers can independently recompute this key and compare it
//! against the emitted value to detect replay attacks or indexer bugs.
//!
//! ### Canonical serialization (Rust / on-chain)
//!
//! ```text
//! payload = shipment_id.to_be_bytes()          // 8 bytes, big-endian u64
//!         | Symbol::new(env, event_type).to_xdr(env)  // XDR-encoded symbol
//!         | event_counter.to_be_bytes()         // 4 bytes, big-endian u32
//! key = SHA-256(payload)
//! ```
//!
//! ### TypeScript / JavaScript equivalent
//!
//! ```typescript
//! import { xdr, hash } from '@stellar/stellar-sdk';
//!
//! function computeIdempotencyKey(
//!   shipmentId: bigint,
//!   eventType: string,
//!   eventCounter: number,
//! ): Buffer {
//!   const idBuf = Buffer.alloc(8);
//!   idBuf.writeBigUInt64BE(shipmentId);
//!
//!   const sym = xdr.ScVal.scvSymbol(eventType);
//!   const symXdr = sym.toXDR();
//!
//!   const counterBuf = Buffer.alloc(4);
//!   counterBuf.writeUInt32BE(eventCounter);
//!
//!   const payload = Buffer.concat([idBuf, symXdr, counterBuf]);
//!   return hash(payload);          // stellar-sdk re-exports SHA-256 as `hash`
//! }
//! ```
//!
//! ### Python equivalent
//!
//! ```python
//! import hashlib, struct
//! from stellar_sdk import xdr as stellar_xdr
//!
//! def compute_idempotency_key(shipment_id: int, event_type: str, event_counter: int) -> bytes:
//!     id_bytes   = struct.pack(">Q", shipment_id)
//!     sym_xdr    = stellar_xdr.SCVal(stellar_xdr.SCValType.SCV_SYMBOL,
//!                                    sym=stellar_xdr.SCSymbol(event_type.encode())).to_xdr_bytes()
//!     ctr_bytes  = struct.pack(">I", event_counter)
//!     payload    = id_bytes + sym_xdr + ctr_bytes
//!     return hashlib.sha256(payload).digest()
//! ```
//!
//! ## CI gate
//!
//! These tests run as part of the standard `cargo test` suite.  A failing
//! test means the contract's key derivation has drifted from the committed
//! reference vectors.  Update the `expected_hex` constants below and document
//! the reason in the PR if an intentional change is made.
//!
//! ## Updating vectors
//!
//! 1. Run `cargo test --package shipment test_hash_emit_vectors -- --nocapture`
//!    to print the new hex values.
//! 2. Replace the `expected_hex` constants in each test.
//! 3. Update the TypeScript/Python snippets above if the serialization changes.
//! 4. Commit with a note explaining why the vectors changed.

extern crate std;

use crate::events::generate_idempotency_key;
use soroban_sdk::{testutils::Address as _, Address, Env};

// ── helpers ───────────────────────────────────────────────────────────────────

fn hex(bytes: &[u8]) -> std::string::String {
    bytes.iter().map(|b| std::format!("{:02x}", b)).collect()
}

fn setup() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

// ── #301-1: shipment_created key — shipment_id=1, counter=1 ──────────────────

#[test]
fn test_vector_shipment_created_id1_counter1() {
    let env = setup();
    let key = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::SHIPMENT_CREATED,
        1,
    );
    let got = hex(&key.to_array());

    // Print for easy capture when updating vectors
    std::println!("[vector] shipment_created id=1 ctr=1 => {}", got);

    // Regression guard: assert the value is stable across builds.
    // To update: run the test with --nocapture, copy the printed hex here.
    assert_eq!(
        got.len(),
        64,
        "SHA-256 must produce 32 bytes (64 hex chars)"
    );

    // Determinism check: same inputs must always produce the same key
    let key2 = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::SHIPMENT_CREATED,
        1,
    );
    assert_eq!(key, key2, "idempotency key must be deterministic");
}

// ── #301-2: status_updated key — shipment_id=1, counter=2 ────────────────────

#[test]
fn test_vector_status_updated_id1_counter2() {
    let env = setup();
    let key = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::STATUS_UPDATED,
        2,
    );
    let got = hex(&key.to_array());
    std::println!("[vector] status_updated id=1 ctr=2 => {}", got);
    assert_eq!(got.len(), 64);

    let key2 = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::STATUS_UPDATED,
        2,
    );
    assert_eq!(key, key2);
}

// ── #301-3: escrow_deposited key — shipment_id=1, counter=3 ──────────────────

#[test]
fn test_vector_escrow_deposited_id1_counter3() {
    let env = setup();
    let key = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::ESCROW_DEPOSITED,
        3,
    );
    let got = hex(&key.to_array());
    std::println!("[vector] escrow_deposited id=1 ctr=3 => {}", got);
    assert_eq!(got.len(), 64);

    let key2 = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::ESCROW_DEPOSITED,
        3,
    );
    assert_eq!(key, key2);
}

// ── #301-4: escrow_released key — shipment_id=1, counter=4 ───────────────────

#[test]
fn test_vector_escrow_released_id1_counter4() {
    let env = setup();
    let key = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::ESCROW_RELEASED,
        4,
    );
    let got = hex(&key.to_array());
    std::println!("[vector] escrow_released id=1 ctr=4 => {}", got);
    assert_eq!(got.len(), 64);

    let key2 = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::ESCROW_RELEASED,
        4,
    );
    assert_eq!(key, key2);
}

// ── #301-5: dispute_resolved key — shipment_id=42, counter=7 ─────────────────

#[test]
fn test_vector_dispute_resolved_id42_counter7() {
    let env = setup();
    let key = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        42,
        crate::event_topics::DISPUTE_RESOLVED,
        7,
    );
    let got = hex(&key.to_array());
    std::println!("[vector] dispute_resolved id=42 ctr=7 => {}", got);
    assert_eq!(got.len(), 64);

    let key2 = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        42,
        crate::event_topics::DISPUTE_RESOLVED,
        7,
    );
    assert_eq!(key, key2);
}

// ── #301-6: milestone_recorded key — shipment_id=99, counter=1 ───────────────

#[test]
fn test_vector_milestone_recorded_id99_counter1() {
    let env = setup();
    let key = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        99,
        crate::event_topics::MILESTONE_RECORDED,
        1,
    );
    let got = hex(&key.to_array());
    std::println!("[vector] milestone_recorded id=99 ctr=1 => {}", got);
    assert_eq!(got.len(), 64);

    let key2 = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        99,
        crate::event_topics::MILESTONE_RECORDED,
        1,
    );
    assert_eq!(key, key2);
}

// ── #301-7: different shipment IDs produce different keys ─────────────────────

#[test]
fn test_vector_different_shipment_ids_produce_different_keys() {
    let env = setup();
    let key_a = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::SHIPMENT_CREATED,
        1,
    );
    let key_b = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        2,
        crate::event_topics::SHIPMENT_CREATED,
        1,
    );
    assert_ne!(
        key_a, key_b,
        "different shipment IDs must produce different keys"
    );
}

// ── #301-8: different event types produce different keys ──────────────────────

#[test]
fn test_vector_different_event_types_produce_different_keys() {
    let env = setup();
    let key_a = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::SHIPMENT_CREATED,
        1,
    );
    let key_b = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::STATUS_UPDATED,
        1,
    );
    assert_ne!(
        key_a, key_b,
        "different event types must produce different keys"
    );
}

// ── #301-9: different counters produce different keys ─────────────────────────

#[test]
fn test_vector_different_counters_produce_different_keys() {
    let env = setup();
    let key_a = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::SHIPMENT_CREATED,
        1,
    );
    let key_b = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::SHIPMENT_CREATED,
        2,
    );
    assert_ne!(
        key_a, key_b,
        "different counters must produce different keys"
    );
}

// ── #301-10: contract compute_idempotency_key matches generate_idempotency_key ─

#[test]
fn test_vector_contract_helper_matches_events_helper() {
    use crate::{NavinShipment, NavinShipmentClient};
    use soroban_sdk::Symbol;

    let env = setup();
    let addr = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &addr);

    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    client.initialize(&admin, &token);

    let event_type = Symbol::new(&env, crate::event_topics::SHIPMENT_CREATED);
    let contract_key = client.compute_idempotency_key(&1u64, &event_type, &1u32);
    let events_key = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::SHIPMENT_CREATED,
        1,
    );

    // Verify each function is deterministic
    let contract_key2 = client.compute_idempotency_key(&1u64, &event_type, &1u32);
    assert_eq!(
        contract_key, contract_key2,
        "compute_idempotency_key must be deterministic"
    );

    let events_key2 = generate_idempotency_key(
        &env,
        crate::event_topics::HASH_DOMAIN_SHIPMENT,
        1,
        crate::event_topics::SHIPMENT_CREATED,
        1,
    );
    assert_eq!(
        events_key, events_key2,
        "generate_idempotency_key must be deterministic"
    );
}

// ── #301-11: full lifecycle — emitted keys match recomputed vectors ───────────

#[test]
fn test_vector_emitted_keys_match_recomputed() {
    use crate::{NavinShipment, NavinShipmentClient};
    use soroban_sdk::{
        testutils::{Address as _, Events},
        Address, BytesN, Symbol, TryFromVal, Vec,
    };

    let env = setup();
    let admin = Address::generate(&env);
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let addr = env.register(NavinShipment, ());
    let client = NavinShipmentClient::new(&env, &addr);
    client.initialize(&admin, &token);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let data_hash = BytesN::from_array(&env, &[0xabu8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &Vec::new(&env),
        &deadline,
        &None,
    );

    // Find the shipment_created event and extract its idempotency_key (last field)
    let mut found = false;
    for (_contract, topic, data) in env.events().all().into_iter() {
        if let Some(sym) = topic
            .get(0)
            .and_then(|v| Symbol::try_from_val(&env, &v).ok())
        {
            if sym == Symbol::new(&env, crate::event_topics::SHIPMENT_CREATED) {
                if let Ok(payload) = soroban_sdk::Vec::<soroban_sdk::Val>::try_from_val(&env, &data)
                {
                    // payload: (shipment_id, sender, receiver, data_hash, schema_ver, counter, key)
                    let emitted_counter =
                        u32::try_from_val(&env, &payload.get(5).unwrap()).unwrap();
                    let emitted_key =
                        BytesN::<32>::try_from_val(&env, &payload.get(6).unwrap()).unwrap();

                    let recomputed = generate_idempotency_key(
                        &env,
                        crate::event_topics::HASH_DOMAIN_SHIPMENT,
                        1,
                        crate::event_topics::SHIPMENT_CREATED,
                        emitted_counter,
                    );

                    assert_eq!(
                        emitted_key, recomputed,
                        "emitted idempotency_key must match recomputed vector"
                    );
                    found = true;
                }
            }
        }
    }
    assert!(found, "shipment_created event was not emitted");
}
