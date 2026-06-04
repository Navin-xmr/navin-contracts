//! Tests for issue #7 — archived restore consistency regression tests.
//!
//! Verifies that after archival the persisted data in temporary storage is
//! byte-for-byte consistent with the data that existed in persistent storage
//! immediately before archival.  Also documents that re-archiving an already-
//! archived shipment is rejected, and that the get_shipment read path falls
//! back transparently to the archived copy.

#[cfg(test)]
mod tests {
    extern crate std;
    use crate::{
        test_utils,
        types::{DataKey, Shipment, StoragePresenceState},
        NavinError, NavinShipment, NavinShipmentClient, PersistentRestoreDiagnostics,
        ShipmentStatus,
    };
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Vec};

    #[contract]
    struct MockToken;

    #[contractimpl]
    impl MockToken {
        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
        pub fn decimals(_env: Env) -> u32 {
            7
        }
        pub fn transfer_from(
            _env: Env,
            _spender: Address,
            _from: Address,
            _to: Address,
            _amount: i128,
        ) {
        }
    }

    fn setup() -> (Env, NavinShipmentClient<'static>, Address) {
        let (env, admin) = test_utils::setup_env();
        let contract_id = env.register(NavinShipment, ());
        let client = NavinShipmentClient::new(&env, &contract_id);
        let token_id = env.register(MockToken, ());
        client.initialize(&admin, &token_id);
        (env, client, admin)
    }

    /// Helper: create → InTransit → Delivered → archive.
    /// Returns (shipment_id, company, receiver, carrier, data_hash).
    fn create_and_archive(
        env: &Env,
        client: &NavinShipmentClient<'static>,
        admin: &Address,
        seed: u8,
    ) -> (u64, Address, Address, Address, BytesN<32>) {
        let company = Address::generate(env);
        let receiver = Address::generate(env);
        let carrier = Address::generate(env);
        let data_hash = BytesN::from_array(env, &[seed; 32]);
        let deadline = test_utils::future_deadline(env, 7200);

        client.add_company(admin, &company);
        client.add_carrier(admin, &carrier);

        let shipment_id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &Vec::new(env),
            &deadline,
        );

        test_utils::advance_past_rate_limit(env);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &data_hash,
        );
        client.confirm_delivery(&receiver, &shipment_id, &data_hash);
        client.archive_shipment(admin, &shipment_id);

        (shipment_id, company, receiver, carrier, data_hash)
    }

    // ── Core consistency: archived data matches pre-archive state ─────────────

    /// The archived copy must retain the same shipment_id, sender (company),
    /// receiver, carrier, data_hash, and terminal status as the live record.
    #[test]
    fn archived_shipment_fields_are_consistent_with_pre_archive_state() {
        let (env, client, admin) = setup();
        let (shipment_id, company, receiver, carrier, data_hash) =
            create_and_archive(&env, &client, &admin, 0x01);

        // get_shipment falls back to the archived copy transparently.
        let archived = client.get_shipment(&shipment_id);

        assert_eq!(archived.id, shipment_id, "id must be preserved");
        assert_eq!(archived.sender, company, "sender must be preserved");
        assert_eq!(archived.receiver, receiver, "receiver must be preserved");
        assert_eq!(archived.carrier, carrier, "carrier must be preserved");
        assert_eq!(archived.data_hash, data_hash, "data_hash must be preserved");
        assert_eq!(
            archived.status,
            ShipmentStatus::Delivered,
            "terminal status must be preserved"
        );
    }

    /// Escrow amount in the archived copy must be zero (cleared on delivery).
    #[test]
    fn archived_shipment_escrow_is_zero_after_delivery() {
        let (env, client, admin) = setup();
        let (shipment_id, _, _, _, _) = create_and_archive(&env, &client, &admin, 0x02);

        let archived = client.get_shipment(&shipment_id);
        assert_eq!(
            archived.escrow_amount,
            0,
            "escrow_amount must be 0 in the archived copy after delivery"
        );
    }

    /// The finalized flag in the archived copy must be true.
    #[test]
    fn archived_shipment_is_finalized() {
        let (env, client, admin) = setup();
        let (shipment_id, _, _, _, _) = create_and_archive(&env, &client, &admin, 0x03);

        let archived = client.get_shipment(&shipment_id);
        assert!(
            archived.finalized,
            "finalized flag must be true in the archived copy"
        );
    }

    // ── get_restore_diagnostics after archival ────────────────────────────────

    /// After archival, get_restore_diagnostics must report ArchivedExpected,
    /// with archived_shipment_present = true and persistent_shipment_present = false.
    #[test]
    fn archived_shipment_restore_diagnostics_report_archived_expected() {
        use crate::types::StoragePresenceState;

        let (env, client, admin) = setup();
        let (shipment_id, _, _, _, _) = create_and_archive(&env, &client, &admin, 0x04);

        let diag = client.get_restore_diagnostics(&shipment_id);

        assert_eq!(
            diag.state,
            StoragePresenceState::ArchivedExpected,
            "diagnostics must report ArchivedExpected after archival"
        );
        assert!(
            diag.archived_shipment_present,
            "archived_shipment_present must be true"
        );
        assert!(
            !diag.persistent_shipment_present,
            "persistent_shipment_present must be false — persistent entry removed on archive"
        );
        assert_eq!(
            diag.shipment_id, shipment_id,
            "echoed shipment_id must match the queried ID"
        );
    }

    /// Before archival, get_restore_diagnostics must report ActivePersistent.
    /// After archival, it must flip to ArchivedExpected.
    #[test]
    fn restore_diagnostics_state_transitions_from_active_to_archived() {
        use crate::types::StoragePresenceState;

        let (env, client, admin) = setup();
        let company = Address::generate(&env);
        let receiver = Address::generate(&env);
        let carrier = Address::generate(&env);
        let data_hash = BytesN::from_array(&env, &[0x05u8; 32]);
        let deadline = test_utils::future_deadline(&env, 7200);

        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        let shipment_id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &Vec::new(&env),
            &deadline,
        );

        // Pre-archive: must be ActivePersistent.
        let pre = client.get_restore_diagnostics(&shipment_id);
        assert_eq!(pre.state, StoragePresenceState::ActivePersistent);

        // Transition to Delivered and archive.
        test_utils::advance_past_rate_limit(&env);
        client.update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::InTransit,
            &data_hash,
        );
        client.confirm_delivery(&receiver, &shipment_id, &data_hash);
        client.archive_shipment(&admin, &shipment_id);

        // Post-archive: must be ArchivedExpected.
        let post = client.get_restore_diagnostics(&shipment_id);
        assert_eq!(
            post.state,
            StoragePresenceState::ArchivedExpected,
            "diagnostics must transition to ArchivedExpected after archival"
        );
    }

    // ── Re-archiving an already-archived shipment is rejected ─────────────────

    /// Attempting to archive an already-archived shipment must fail —
    /// the persistent entry has been removed, so the contract cannot locate it.
    #[test]
    fn re_archiving_already_archived_shipment_is_rejected() {
        let (env, client, admin) = setup();
        let (shipment_id, _, _, _, _) = create_and_archive(&env, &client, &admin, 0x06);

        // Second archive call on the same (now archived) shipment must fail.
        env.mock_all_auths();
        let result = client.try_archive_shipment(&admin, &shipment_id);
        match result {
            Ok(Err(e)) => {
                let expected_error = soroban_sdk::Error::from_contract_error(NavinError::ShipmentNotFound as u32);
                let err_str = std::format!("{:?}", e);
                let expected_str = std::format!("{:?}", expected_error);
                assert!(err_str.contains(&expected_str) || err_str.contains("ShipmentNotFound"), "Expected ShipmentNotFound error, got {:?}", err_str);
            },
            Err(e) => {
                let err_str = std::format!("{:?}", e);
                assert!(err_str.contains("ShipmentNotFound") || err_str.contains("Code(4)"), "Expected ShipmentNotFound error in host error, got {:?}", err_str);
            },
            _ => panic!("Expected error but got success"),
        }
    }

    // ── Cancelled shipment archival consistency ────────────────────────────────

    /// A shipment cancelled before delivery can also be archived, and the
    /// archived copy must reflect the Cancelled status.
    #[test]
    fn cancelled_shipment_archives_with_correct_status() {
        let (env, client, admin) = setup();
        let company = Address::generate(&env);
        let receiver = Address::generate(&env);
        let carrier = Address::generate(&env);
        let data_hash = BytesN::from_array(&env, &[0x07u8; 32]);
        let deadline = test_utils::future_deadline(&env, 7200);

        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        let shipment_id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &Vec::new(&env),
            &deadline,
        );

        // Cancel without escrow — immediately finalizes.
        client.cancel_shipment(&company, &shipment_id, &data_hash);

        let pre_archive = client.get_shipment(&shipment_id);
        assert_eq!(pre_archive.status, ShipmentStatus::Cancelled);

        // Archive the cancelled (finalized) shipment.
        client.archive_shipment(&admin, &shipment_id);

        let archived = client.get_shipment(&shipment_id);
        assert_eq!(
            archived.status,
            ShipmentStatus::Cancelled,
            "archived copy must reflect the Cancelled terminal status"
        );
        assert_eq!(
            archived.sender, company,
            "sender must be preserved after archival"
        );
        assert_eq!(
            archived.carrier, carrier,
            "carrier must be preserved after archival"
        );
    }

    // ── Multiple shipments: each archives independently ────────────────────────

    /// Archiving one shipment must not corrupt the restore diagnostics or
    /// accessible data of a different shipment still in persistent storage.
    #[test]
    fn archiving_one_shipment_does_not_affect_another_active_shipment() {
        use crate::types::StoragePresenceState;

        let (env, client, admin) = setup();

        // Shipment A — will be archived.
        let (id_a, _, _, _, _) = create_and_archive(&env, &client, &admin, 0x0A);

        // Shipment B — stays active.
        let company_b = Address::generate(&env);
        let receiver_b = Address::generate(&env);
        let carrier_b = Address::generate(&env);
        let hash_b = BytesN::from_array(&env, &[0x0Bu8; 32]);
        let deadline_b = test_utils::future_deadline(&env, 7200);

        client.add_company(&admin, &company_b);
        client.add_carrier(&admin, &carrier_b);

        let id_b = client.create_shipment(
            &company_b,
            &receiver_b,
            &carrier_b,
            &hash_b,
            &Vec::new(&env),
            &deadline_b,
        );

        // Shipment A must be ArchivedExpected.
        let diag_a = client.get_restore_diagnostics(&id_a);
        assert_eq!(diag_a.state, StoragePresenceState::ArchivedExpected);

        // Shipment B must still be ActivePersistent.
        let diag_b = client.get_restore_diagnostics(&id_b);
        assert_eq!(
            diag_b.state,
            StoragePresenceState::ActivePersistent,
            "archiving shipment A must not disturb shipment B's active persistent state"
        );

        // Shipment B data must still be readable and correct.
        let shipment_b = client.get_shipment(&id_b);
        assert_eq!(shipment_b.sender, company_b);
        assert_eq!(shipment_b.carrier, carrier_b);
    }
}
