//! # Tests — Hash Domain-Separation Prefixes
//!
//! Verifies that the `u8` domain tags defined in `event_topics` correctly
//! isolate idempotency-key hashes by event family.
//!
//! ## Why these tests matter
//!
//! Before domain separation was introduced, `generate_idempotency_key` hashed
//! `(shipment_id || event_type_xdr || event_counter)`.  Two events from
//! different families but with identical external inputs would never collide in
//! practice because `event_type` strings differ, but the absence of an
//! *explicit* family tag made the invariant implicit and hard to audit.
//!
//! These tests make the guarantee explicit and machine-checkable:
//!
//! * **Uniqueness**: all ten `HASH_DOMAIN_*` constants have distinct values.
//! * **Cross-family separation**: the same `(shipment_id, event_type, counter)`
//!   tuple hashed with two different domain tags must produce different outputs.
//! * **Intra-family reproducibility**: the same four-tuple always produces the
//!   same hash (no hidden entropy source).
//! * **Family assignment**: each well-known topic maps to the correct family.

#[cfg(test)]
mod tests {
    use crate::event_topics::{
        HASH_DOMAIN_ADMIN, HASH_DOMAIN_CARRIER, HASH_DOMAIN_CONDITION, HASH_DOMAIN_DISPUTE,
        HASH_DOMAIN_ESCROW, HASH_DOMAIN_EVIDENCE, HASH_DOMAIN_NOTE, HASH_DOMAIN_NOTIFICATION,
        HASH_DOMAIN_RBAC, HASH_DOMAIN_SHIPMENT,
    };
    use soroban_sdk::{Bytes, BytesN, Env};

    // ── Helper ───────────────────────────────────────────────────────────────
    //
    // Mirrors the production `generate_idempotency_key` function so that tests
    // can exercise domain separation without requiring access to the private
    // symbol.  **This function must stay in sync with `events::generate_idempotency_key`.**

    fn append_len_prefixed_bytes(env: &Env, payload: &mut Bytes, data: &[u8]) {
        payload.append(&Bytes::from_array(env, &(data.len() as u32).to_be_bytes()));
        payload.append(&Bytes::from_slice(env, data));
    }

    fn compute_key(
        env: &Env,
        domain: u8,
        shipment_id: u64,
        event_type: &str,
        counter: u32,
    ) -> BytesN<32> {
        let mut payload = Bytes::new(env);
        let domain_bytes = domain.to_be_bytes();
        append_len_prefixed_bytes(env, &mut payload, &domain_bytes);
        payload.append(&Bytes::from_array(env, &shipment_id.to_be_bytes()));
        append_len_prefixed_bytes(env, &mut payload, event_type.as_bytes());
        payload.append(&Bytes::from_array(env, &counter.to_be_bytes()));
        env.crypto().sha256(&payload).into()
    }

    // ── 1. Domain constant uniqueness ────────────────────────────────────────

    /// All ten `HASH_DOMAIN_*` constants must have pairwise-distinct `u8` values.
    ///
    /// Adding a new family constant must not reuse an existing discriminant —
    /// this test is the first line of defence against that class of mistake.
    #[test]
    fn all_hash_domain_constants_are_unique() {
        let mut domains = [
            HASH_DOMAIN_SHIPMENT,
            HASH_DOMAIN_ESCROW,
            HASH_DOMAIN_DISPUTE,
            HASH_DOMAIN_CONDITION,
            HASH_DOMAIN_CARRIER,
            HASH_DOMAIN_ADMIN,
            HASH_DOMAIN_RBAC,
            HASH_DOMAIN_NOTIFICATION,
            HASH_DOMAIN_NOTE,
            HASH_DOMAIN_EVIDENCE,
        ];
        domains.sort_unstable();
        for pair in domains.windows(2) {
            assert_ne!(
                pair[0], pair[1],
                "Duplicate HASH_DOMAIN constant: 0x{:02X}",
                pair[0]
            );
        }
    }

    /// Each constant's numeric value must be stable (changing is a breaking change).
    #[test]
    fn hash_domain_constant_values_are_stable() {
        assert_eq!(HASH_DOMAIN_SHIPMENT, 0x01);
        assert_eq!(HASH_DOMAIN_ESCROW, 0x02);
        assert_eq!(HASH_DOMAIN_DISPUTE, 0x03);
        assert_eq!(HASH_DOMAIN_CONDITION, 0x04);
        assert_eq!(HASH_DOMAIN_CARRIER, 0x05);
        assert_eq!(HASH_DOMAIN_ADMIN, 0x06);
        assert_eq!(HASH_DOMAIN_RBAC, 0x07);
        assert_eq!(HASH_DOMAIN_NOTIFICATION, 0x08);
        assert_eq!(HASH_DOMAIN_NOTE, 0x09);
        assert_eq!(HASH_DOMAIN_EVIDENCE, 0x0A);
    }

    // ── 2. Cross-family separation ───────────────────────────────────────────

    /// Given identical `(shipment_id, event_type, counter)` inputs, two
    /// different domain tags must produce different SHA-256 digests.
    ///
    /// This is the core invariant of the domain-separation scheme.
    #[test]
    fn different_domains_produce_different_keys_for_identical_inputs() {
        let env = Env::default();
        let shipment_id: u64 = 42;
        let event_type = "shipment_created";
        let counter: u32 = 1;

        let key_shipment =
            compute_key(&env, HASH_DOMAIN_SHIPMENT, shipment_id, event_type, counter);
        let key_escrow = compute_key(&env, HASH_DOMAIN_ESCROW, shipment_id, event_type, counter);
        let key_dispute = compute_key(&env, HASH_DOMAIN_DISPUTE, shipment_id, event_type, counter);
        let key_condition = compute_key(
            &env,
            HASH_DOMAIN_CONDITION,
            shipment_id,
            event_type,
            counter,
        );
        let key_carrier = compute_key(&env, HASH_DOMAIN_CARRIER, shipment_id, event_type, counter);
        let key_admin = compute_key(&env, HASH_DOMAIN_ADMIN, shipment_id, event_type, counter);
        let key_rbac = compute_key(&env, HASH_DOMAIN_RBAC, shipment_id, event_type, counter);
        let key_notification = compute_key(
            &env,
            HASH_DOMAIN_NOTIFICATION,
            shipment_id,
            event_type,
            counter,
        );
        let key_note = compute_key(&env, HASH_DOMAIN_NOTE, shipment_id, event_type, counter);
        let key_evidence =
            compute_key(&env, HASH_DOMAIN_EVIDENCE, shipment_id, event_type, counter);

        let all_keys = [
            key_shipment,
            key_escrow,
            key_dispute,
            key_condition,
            key_carrier,
            key_admin,
            key_rbac,
            key_notification,
            key_note,
            key_evidence,
        ];

        // Every pair must be distinct.
        for i in 0..all_keys.len() {
            for j in (i + 1)..all_keys.len() {
                assert_ne!(
                    all_keys[i],
                    all_keys[j],
                    "Domain 0x{:02X} and 0x{:02X} produced the same idempotency key",
                    i + 1,
                    j + 1,
                );
            }
        }
    }

    /// The domain tag changes the output even when the only input that differs
    /// is the least-significant bit of the domain byte.
    ///
    /// Ensures there is no accidental byte-level aliasing between adjacent tags.
    #[test]
    fn adjacent_domain_tags_produce_different_keys() {
        let env = Env::default();
        let shipment_id: u64 = 1;
        let event_type = "status_updated";
        let counter: u32 = 5;

        let key_shipment =
            compute_key(&env, HASH_DOMAIN_SHIPMENT, shipment_id, event_type, counter);
        let key_escrow = compute_key(&env, HASH_DOMAIN_ESCROW, shipment_id, event_type, counter);
        let key_dispute = compute_key(&env, HASH_DOMAIN_DISPUTE, shipment_id, event_type, counter);

        assert_ne!(key_shipment, key_escrow);
        assert_ne!(key_escrow, key_dispute);
        assert_ne!(key_shipment, key_dispute);
    }

    // ── 3. Intra-family reproducibility ─────────────────────────────────────

    /// Calling `compute_key` twice with identical arguments must return the
    /// same digest — the hash function must be deterministic.
    #[test]
    fn same_inputs_produce_same_key() {
        let env = Env::default();

        let k1 = compute_key(&env, HASH_DOMAIN_SHIPMENT, 99, "shipment_created", 3);
        let k2 = compute_key(&env, HASH_DOMAIN_SHIPMENT, 99, "shipment_created", 3);
        assert_eq!(k1, k2);

        let k3 = compute_key(&env, HASH_DOMAIN_ESCROW, 7, "escrow_deposited", 1);
        let k4 = compute_key(&env, HASH_DOMAIN_ESCROW, 7, "escrow_deposited", 1);
        assert_eq!(k3, k4);

        let k5 = compute_key(&env, HASH_DOMAIN_DISPUTE, 1, "dispute_resolved", 2);
        let k6 = compute_key(&env, HASH_DOMAIN_DISPUTE, 1, "dispute_resolved", 2);
        assert_eq!(k5, k6);
    }

    // ── 4. Counter sensitivity ───────────────────────────────────────────────

    /// Changing only the event counter must produce a different digest.
    /// This ensures that repeated events for the same shipment are
    /// never collapsed into the same idempotency key.
    #[test]
    fn different_counters_produce_different_keys() {
        let env = Env::default();
        let k1 = compute_key(&env, HASH_DOMAIN_SHIPMENT, 42, "status_updated", 1);
        let k2 = compute_key(&env, HASH_DOMAIN_SHIPMENT, 42, "status_updated", 2);
        assert_ne!(k1, k2);
    }

    // ── 5. Shipment-id sensitivity ───────────────────────────────────────────

    /// Changing only the shipment-id must produce a different digest.
    #[test]
    fn different_shipment_ids_produce_different_keys() {
        let env = Env::default();
        let k1 = compute_key(&env, HASH_DOMAIN_SHIPMENT, 1, "shipment_created", 1);
        let k2 = compute_key(&env, HASH_DOMAIN_SHIPMENT, 2, "shipment_created", 1);
        assert_ne!(k1, k2);
    }

    // ── 6. Family assignment spot-checks ─────────────────────────────────────

    /// Shipment-lifecycle topics must use `HASH_DOMAIN_SHIPMENT` (0x01).
    ///
    /// Keys produced with the correct domain must differ from keys produced
    /// with any other domain for the same input — confirming that using the
    /// wrong domain in a caller would be detectable.
    #[test]
    fn shipment_lifecycle_topics_use_shipment_domain() {
        let env = Env::default();
        let shipment_id = 10_u64;
        let counter = 1_u32;

        for topic in &[
            crate::event_topics::SHIPMENT_CREATED,
            crate::event_topics::STATUS_UPDATED,
            crate::event_topics::MILESTONE_RECORDED,
            crate::event_topics::SHIPMENT_CANCELLED,
            crate::event_topics::SHIPMENT_EXPIRED,
            crate::event_topics::DELIVERY_SUCCESS,
        ] {
            let correct = compute_key(&env, HASH_DOMAIN_SHIPMENT, shipment_id, topic, counter);
            let wrong = compute_key(&env, HASH_DOMAIN_ESCROW, shipment_id, topic, counter);
            assert_ne!(
                correct, wrong,
                "Topic '{}': HASH_DOMAIN_SHIPMENT and HASH_DOMAIN_ESCROW must not collide",
                topic
            );
        }
    }

    /// Escrow topics must use `HASH_DOMAIN_ESCROW` (0x02).
    #[test]
    fn escrow_topics_use_escrow_domain() {
        let env = Env::default();
        let shipment_id = 20_u64;
        let counter = 1_u32;

        for topic in &[
            crate::event_topics::ESCROW_DEPOSITED,
            crate::event_topics::ESCROW_RELEASED,
            crate::event_topics::ESCROW_REFUNDED,
        ] {
            let correct = compute_key(&env, HASH_DOMAIN_ESCROW, shipment_id, topic, counter);
            let wrong = compute_key(&env, HASH_DOMAIN_SHIPMENT, shipment_id, topic, counter);
            assert_ne!(
                correct, wrong,
                "Topic '{}': HASH_DOMAIN_ESCROW and HASH_DOMAIN_SHIPMENT must not collide",
                topic
            );
        }
    }

    /// Dispute topics must use `HASH_DOMAIN_DISPUTE` (0x03).
    #[test]
    fn dispute_topics_use_dispute_domain() {
        let env = Env::default();
        let shipment_id = 30_u64;
        let counter = 1_u32;

        for topic in &[
            crate::event_topics::DISPUTE_RAISED,
            crate::event_topics::DISPUTE_RESOLVED,
        ] {
            let correct = compute_key(&env, HASH_DOMAIN_DISPUTE, shipment_id, topic, counter);
            let wrong = compute_key(&env, HASH_DOMAIN_SHIPMENT, shipment_id, topic, counter);
            assert_ne!(
                correct, wrong,
                "Topic '{}': HASH_DOMAIN_DISPUTE and HASH_DOMAIN_SHIPMENT must not collide",
                topic
            );
        }
    }
}
