//! Tests for IoT sensor data hash verification.
//!
//! Issue #698: a shipment's data hash used to live in a single
//! per-`(shipment_id, status)` slot, so a shipment that ping-ponged
//! `InTransit -> AtCheckpoint -> InTransit` silently lost the hash recorded for
//! its first transit leg. These tests pin the per-visit addressing that
//! replaced it: every visit gets its own `visit_index`, and hashes stay
//! individually addressable and verifiable for as long as the bounded log
//! retains them.

#[cfg(test)]
mod tests {
    use crate::test_utils::*;
    use crate::types::*;
    use crate::{NavinError, NavinShipment, NavinShipmentClient};
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Vec};

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

    fn setup_initialized() -> (Env, NavinShipmentClient<'static>, Address, Address) {
        let (env, admin) = setup_env();
        let token_contract = env.register(MockToken {}, ());
        let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
        client.initialize(&admin, &token_contract);
        (env, client, admin, token_contract)
    }

    /// Distinct hash per visit; `update_status` rejects a repeated
    /// (shipment_id, status, hash) triple, so no two visits may share one.
    ///
    /// Seeds are 1-based because `create_shipment` rejects an all-zero hash
    /// (`InvalidHash`).
    fn visit_hash(env: &Env, seed: u8) -> BytesN<32> {
        BytesN::from_array(env, &[seed.wrapping_add(1); 32])
    }

    fn create_shipment(client: &NavinShipmentClient, admin: &Address, hash: &BytesN<32>) -> u64 {
        let env = &client.env;
        let company = Address::generate(env);
        let carrier = Address::generate(env);
        let receiver = Address::generate(env);
        client.add_company(admin, &company);
        client.add_carrier(admin, &carrier);
        client.add_carrier_to_whitelist(&company, &carrier);
        client.create_shipment(
            &company,
            &receiver,
            &carrier,
            hash,
            &Vec::new(env),
            &future_deadline(env, 30 * 86_400),
        )
    }

    /// `count` alternating `AtCheckpoint` / `InTransit` targets, starting from
    /// `AtCheckpoint` so the caller can apply them to a shipment already
    /// `InTransit`. `AtCheckpoint -> AtCheckpoint` is rejected (#542), so
    /// revisits have to alternate.
    fn ping_pong(count: u32) -> alloc::vec::Vec<ShipmentStatus> {
        (0..count)
            .map(|i| {
                if i % 2 == 0 {
                    ShipmentStatus::AtCheckpoint
                } else {
                    ShipmentStatus::InTransit
                }
            })
            .collect()
    }

    fn carrier_of(client: &NavinShipmentClient, shipment_id: u64) -> Address {
        client.get_shipment(&shipment_id).carrier
    }

    /// Advance a shipment `Created -> InTransit` and return the carrier.
    fn start_transit(
        env: &Env,
        client: &NavinShipmentClient,
        shipment_id: u64,
        hash: &BytesN<32>,
    ) -> Address {
        let carrier = carrier_of(client, shipment_id);
        client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, hash);
        advance_past_rate_limit(env);
        carrier
    }

    // ── #698: bidirectional revisits ────────────────────────────────────────

    /// The core regression. `InTransit` is reached twice, so a per-status slot
    /// would have kept only the second hash; both must remain readable.
    #[test]
    fn test_status_hash_preserved_across_bidirectional_revisit() {
        let (env, client, admin, _token) = setup_initialized();
        let shipment_id = create_shipment(&client, &admin, &visit_hash(&env, 0));

        let first_transit = visit_hash(&env, 1);
        let carrier = start_transit(&env, &client, shipment_id, &first_transit);

        let checkpoint = visit_hash(&env, 2);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::AtCheckpoint,
            &checkpoint,
        );
        advance_past_rate_limit(&env);

        let second_transit = visit_hash(&env, 3);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &second_transit,
        );

        // Both transit legs are individually addressable.
        let leg1 = client.get_status_hash(&shipment_id, &ShipmentStatus::InTransit, &0);
        let leg2 = client.get_status_hash(&shipment_id, &ShipmentStatus::InTransit, &1);
        assert_eq!(leg1.data_hash, first_transit);
        assert_eq!(leg2.data_hash, second_transit);
        assert_eq!(leg1.visit_index, 0);
        assert_eq!(leg2.visit_index, 1);
        assert_eq!(leg1.status, ShipmentStatus::InTransit);
        assert_eq!(leg2.status, ShipmentStatus::InTransit);
        // The retained hash is the one from the latest leg, as before.
        assert!(leg1.seq < leg2.seq);
        assert_eq!(client.get_shipment(&shipment_id).data_hash, second_transit);

        // The checkpoint visit was not clobbered by the return leg.
        let at_checkpoint = client.get_status_hash(&shipment_id, &ShipmentStatus::AtCheckpoint, &0);
        assert_eq!(at_checkpoint.data_hash, checkpoint);

        // And both legs still verify independently.
        assert!(client.verify_data_hash(
            &shipment_id,
            &ShipmentStatus::InTransit,
            &0,
            &first_transit
        ));
        assert!(client.verify_data_hash(
            &shipment_id,
            &ShipmentStatus::InTransit,
            &1,
            &second_transit
        ));
        assert!(client.verify_data_hash(
            &shipment_id,
            &ShipmentStatus::AtCheckpoint,
            &0,
            &checkpoint
        ));

        client.assert_data_hash(&shipment_id, &ShipmentStatus::InTransit, &0, &first_transit);
        client.assert_data_hash(
            &shipment_id,
            &ShipmentStatus::InTransit,
            &1,
            &second_transit,
        );
    }

    /// A hash from an earlier leg must not satisfy a later leg's assertion, and
    /// vice versa — the whole point of visit-scoped addressing.
    #[test]
    fn test_status_hash_does_not_cross_match_between_visits() {
        let (env, client, admin, _token) = setup_initialized();
        let shipment_id = create_shipment(&client, &admin, &visit_hash(&env, 0));

        let first_transit = visit_hash(&env, 1);
        let carrier = start_transit(&env, &client, shipment_id, &first_transit);
        let checkpoint = visit_hash(&env, 2);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::AtCheckpoint,
            &checkpoint,
        );
        advance_past_rate_limit(&env);
        let second_transit = visit_hash(&env, 3);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &second_transit,
        );

        // The checkpoint hash is not an acceptable stand-in for either leg.
        assert!(!client.verify_data_hash(
            &shipment_id,
            &ShipmentStatus::InTransit,
            &0,
            &checkpoint
        ));
        assert_eq!(
            client.try_assert_data_hash(&shipment_id, &ShipmentStatus::InTransit, &0, &checkpoint),
            Err(Ok(NavinError::DataHashMismatch))
        );

        // The first leg rejects the second leg's hash...
        assert!(!client.verify_data_hash(
            &shipment_id,
            &ShipmentStatus::InTransit,
            &0,
            &second_transit
        ));
        assert_eq!(
            client.try_assert_data_hash(
                &shipment_id,
                &ShipmentStatus::InTransit,
                &0,
                &second_transit
            ),
            Err(Ok(NavinError::DataHashMismatch))
        );

        // ...and the second leg rejects the first leg's hash.
        assert!(!client.verify_data_hash(
            &shipment_id,
            &ShipmentStatus::InTransit,
            &1,
            &first_transit
        ));
        assert_eq!(
            client.try_assert_data_hash(
                &shipment_id,
                &ShipmentStatus::InTransit,
                &1,
                &first_transit
            ),
            Err(Ok(NavinError::DataHashMismatch))
        );
    }

    /// `AtCheckpoint -> AtCheckpoint` is rejected (#542), so revisits are driven
    /// through `InTransit`. The log records one entry per visit, not per status.
    #[test]
    fn test_status_hash_history_has_one_entry_per_visit() {
        let (env, client, admin, _token) = setup_initialized();
        let shipment_id = create_shipment(&client, &admin, &visit_hash(&env, 0));

        let carrier = start_transit(&env, &client, shipment_id, &visit_hash(&env, 1));

        // Ping-pong through `InTransit` for 3 more visits.
        for (step, status) in ping_pong(3).iter().enumerate() {
            let seed = step as u8 + 2;
            client.update_status(&carrier, &shipment_id, status, &visit_hash(&env, seed));
            advance_past_rate_limit(&env);
        }

        // 4 update_status calls => 4 entries.
        let history = client.get_status_hash_history(&shipment_id);
        assert_eq!(history.len(), 4);

        // Oldest first, with strictly increasing sequence numbers.
        for i in 1..history.len() {
            assert!(history.get(i - 1).unwrap().seq < history.get(i).unwrap().seq);
        }

        // InTransit was visited twice, AtCheckpoint once.
        let mut in_transit = 0;
        let mut at_checkpoint = 0;
        for record in history.iter() {
            match record.status {
                ShipmentStatus::InTransit => in_transit += 1,
                ShipmentStatus::AtCheckpoint => at_checkpoint += 1,
                _ => panic!("unexpected status in log"),
            }
            assert_eq!(record.actor, carrier);
        }
        assert_eq!(in_transit, 2);
        assert_eq!(at_checkpoint, 2);
    }

    /// The log is bounded: past the cap it keeps the newest
    /// `MAX_STATUS_HASHES_PER_SHIPMENT` visits and drops the oldest, rather than
    /// growing without limit.
    #[test]
    fn test_status_hash_log_is_bounded() {
        let (env, client, admin, _token) = setup_initialized();
        let shipment_id = create_shipment(&client, &admin, &visit_hash(&env, 0));

        // MAX extra visits on top of the initial transit leg.
        let extra = MAX_STATUS_HASHES_PER_SHIPMENT + 4;
        let carrier = start_transit(&env, &client, shipment_id, &visit_hash(&env, 1));

        for (step, status) in ping_pong(extra).iter().enumerate() {
            let seed = step as u8 + 2;
            client.update_status(&carrier, &shipment_id, status, &visit_hash(&env, seed));
            advance_past_rate_limit(&env);
        }

        let history = client.get_status_hash_history(&shipment_id);
        assert_eq!(history.len(), MAX_STATUS_HASHES_PER_SHIPMENT);

        // Wraparound evicted the first visits, so the original transit leg is
        // gone from the bounded log entirely — it is not silently aliased onto
        // a later visit.
        assert_eq!(
            client.try_get_status_hash(&shipment_id, &ShipmentStatus::InTransit, &0),
            Err(Ok(NavinError::StatusHashNotFound))
        );
        assert_eq!(
            client.try_get_status_hash(&shipment_id, &ShipmentStatus::InTransit, &1),
            Err(Ok(NavinError::StatusHashNotFound))
        );

        // Every retained visit is still individually addressable.
        for record in history.iter() {
            assert!(client.verify_data_hash(
                &shipment_id,
                &record.status,
                &record.visit_index,
                &record.data_hash
            ));
        }

        // The newest visit is still the last InTransit recorded.
        let newest_in_transit = history
            .iter()
            .rfind(|r| r.status == ShipmentStatus::InTransit)
            .unwrap();
        assert!(client.verify_data_hash(
            &shipment_id,
            &ShipmentStatus::InTransit,
            &newest_in_transit.visit_index,
            &newest_in_transit.data_hash
        ));
    }

    // ── Lookup edge cases ───────────────────────────────────────────────────

    #[test]
    fn test_status_hash_lookup_errors() {
        let (env, client, admin, _token) = setup_initialized();
        let shipment_id = create_shipment(&client, &admin, &visit_hash(&env, 0));
        let carrier = start_transit(&env, &client, shipment_id, &visit_hash(&env, 1));

        // A status never entered.
        assert_eq!(
            client.try_get_status_hash(&shipment_id, &ShipmentStatus::AtCheckpoint, &0),
            Err(Ok(NavinError::StatusHashNotFound))
        );
        // A visit index past what was recorded.
        assert_eq!(
            client.try_get_status_hash(&shipment_id, &ShipmentStatus::InTransit, &1),
            Err(Ok(NavinError::StatusHashNotFound))
        );
        // A shipment that does not exist.
        assert_eq!(
            client.try_get_status_hash(&999, &ShipmentStatus::InTransit, &0),
            Err(Ok(NavinError::ShipmentNotFound))
        );
        assert_eq!(
            client.try_verify_data_hash(&999, &ShipmentStatus::InTransit, &0, &visit_hash(&env, 1)),
            Err(Ok(NavinError::ShipmentNotFound))
        );
        assert_eq!(
            client.try_get_status_hash_history(&999),
            Err(Ok(NavinError::ShipmentNotFound))
        );

        // Sanity: the recorded visit is found.
        assert_eq!(
            client
                .get_status_hash(&shipment_id, &ShipmentStatus::InTransit, &0)
                .data_hash,
            visit_hash(&env, 1)
        );
        let _ = carrier;
    }

    // ── Lifecycle: TTL and archival ─────────────────────────────────────────

    /// Hashes for a live shipment must not expire out from under it.
    #[test]
    fn test_status_hash_ttl_extended_with_shipment() {
        let (env, client, admin, _token) = setup_initialized();
        let shipment_id = create_shipment(&client, &admin, &visit_hash(&env, 0));
        start_transit(&env, &client, shipment_id, &visit_hash(&env, 1));

        client.extend_shipment_ttl(&shipment_id);

        env.as_contract(&client.address, || {
            let log = crate::storage::get_status_hash_log(&env, shipment_id);
            let newest = log.entries.get(log.next_seq - 1).expect("log retained");
            assert_eq!(newest.data_hash, visit_hash(&env, 1));
        });
    }

    /// Archiving a terminal shipment drops the whole hash log — including the
    /// archived shipment itself, which `purge_status_hashes` previously failed
    /// to match because it only looked in persistent storage.
    #[test]
    fn test_status_hash_log_purged_on_archival() {
        let (env, client, admin, _token) = setup_initialized();
        let shipment_id = create_shipment(&client, &admin, &visit_hash(&env, 0));
        let hash = visit_hash(&env, 1);
        let carrier = start_transit(&env, &client, shipment_id, &hash);

        let checkpoint = visit_hash(&env, 2);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::AtCheckpoint,
            &checkpoint,
        );
        advance_past_rate_limit(&env);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::Delivered,
            &visit_hash(&env, 3),
        );

        client.archive_shipment(&admin, &shipment_id);

        assert!(client.get_status_hash_history(&shipment_id).is_empty());
        assert_eq!(
            client.try_get_status_hash(&shipment_id, &ShipmentStatus::AtCheckpoint, &0),
            Err(Ok(NavinError::StatusHashNotFound))
        );
    }
}
