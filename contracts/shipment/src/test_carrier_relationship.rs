//! Tests for issue #295 — company/carrier relationship query APIs.
//!
//! Verifies `is_company_carrier_allowed` and `list_company_carriers` for
//! correctness, suspension semantics, pagination, and bounds.

#[cfg(test)]
mod tests {
    use crate::{test_utils, NavinShipment, NavinShipmentClient};
    use soroban_sdk::{
        contract, contractimpl, testutils::Address as _, testutils::Events as _, Address, BytesN,
        Env, Symbol, Vec,
    };

    #[contract]
    struct MockToken;

    #[contractimpl]
    impl MockToken {
        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}

        pub fn decimals(_env: Env) -> u32 {
            7
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

    fn add_company_and_carrier(
        env: &Env,
        client: &NavinShipmentClient,
        admin: &Address,
    ) -> (Address, Address) {
        let company = Address::generate(env);
        let carrier = Address::generate(env);
        client.add_company(admin, &company);
        client.add_carrier(admin, &carrier);
        (company, carrier)
    }

    // ── is_company_carrier_allowed ───────────────────────────────────────────

    #[test]
    fn create_shipment_rejects_unregistered_or_non_whitelisted_carrier() {
        let (env, client, admin) = setup();
        let company = Address::generate(&env);
        client.add_company(&admin, &company);

        let receiver = Address::generate(&env);
        let unregistered = Address::generate(&env);
        let data_hash = BytesN::from_array(&env, &[1u8; 32]);
        let deadline = env.ledger().timestamp() + 3600;

        let result = client.try_create_shipment(
            &company,
            &receiver,
            &unregistered,
            &data_hash,
            &Vec::new(&env),
            &deadline,
        );
        assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

        let registered_carrier = Address::generate(&env);
        client.add_carrier(&admin, &registered_carrier);
        let result = client.try_create_shipment(
            &company,
            &receiver,
            &registered_carrier,
            &data_hash,
            &Vec::new(&env),
            &deadline,
        );
        assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));
    }

    #[test]
    fn allowed_returns_false_when_not_whitelisted() {
        let (env, client, admin) = setup();
        let (company, carrier) = add_company_and_carrier(&env, &client, &admin);

        assert!(!client.is_company_carrier_allowed(&company, &carrier));
    }

    #[test]
    fn allowed_returns_true_after_whitelist() {
        let (env, client, admin) = setup();
        let (company, carrier) = add_company_and_carrier(&env, &client, &admin);

        client.add_carrier_to_whitelist(&company, &carrier);
        assert!(client.is_company_carrier_allowed(&company, &carrier));
    }

    #[test]
    fn allowed_returns_false_after_removal() {
        let (env, client, admin) = setup();
        let (company, carrier) = add_company_and_carrier(&env, &client, &admin);

        client.add_carrier_to_whitelist(&company, &carrier);
        client.remove_carrier_from_whitelist(&company, &carrier);
        assert!(!client.is_company_carrier_allowed(&company, &carrier));
    }

    #[test]
    fn allowed_returns_false_when_carrier_suspended() {
        let (env, client, admin) = setup();
        let (company, carrier) = add_company_and_carrier(&env, &client, &admin);

        client.add_carrier_to_whitelist(&company, &carrier);
        client.suspend_carrier(&admin, &carrier);

        // Whitelisted but carrier is suspended — should return false.
        assert!(!client.is_company_carrier_allowed(&company, &carrier));
    }

    #[test]
    fn allowed_returns_false_when_company_suspended() {
        let (env, client, admin) = setup();
        let (company, carrier) = add_company_and_carrier(&env, &client, &admin);

        client.add_carrier_to_whitelist(&company, &carrier);
        client.suspend_company(&admin, &company);

        assert!(!client.is_company_carrier_allowed(&company, &carrier));
    }

    #[test]
    fn allowed_returns_true_after_reactivation() {
        let (env, client, admin) = setup();
        let (company, carrier) = add_company_and_carrier(&env, &client, &admin);

        client.add_carrier_to_whitelist(&company, &carrier);
        client.suspend_carrier(&admin, &carrier);
        assert!(!client.is_company_carrier_allowed(&company, &carrier));

        client.reactivate_carrier(&admin, &carrier);
        assert!(client.is_company_carrier_allowed(&company, &carrier));
    }

    // ── list_company_carriers ────────────────────────────────────────────────

    #[test]
    fn list_returns_empty_when_no_whitelist() {
        let (env, client, admin) = setup();
        let (company, carrier) = add_company_and_carrier(&env, &client, &admin);

        let mut candidates = Vec::new(&env);
        candidates.push_back(carrier.clone());

        let page = client.list_company_carriers(&company, &candidates, &0, &10);
        assert_eq!(page.carriers.len(), 0);
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn list_returns_whitelisted_carriers() {
        let (env, client, admin) = setup();
        let company = Address::generate(&env);
        client.add_company(&admin, &company);

        let mut carriers = Vec::new(&env);
        for _ in 0..5u32 {
            let c = Address::generate(&env);
            client.add_carrier(&admin, &c);
            client.add_carrier_to_whitelist(&company, &c);
            carriers.push_back(c);
        }

        let page = client.list_company_carriers(&company, &carriers, &0, &10);
        assert_eq!(page.carriers.len(), 5);
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn list_pagination_returns_correct_pages() {
        let (env, client, admin) = setup();
        let company = Address::generate(&env);
        client.add_company(&admin, &company);

        let mut all_carriers = Vec::new(&env);
        for _ in 0..6u32 {
            let c = Address::generate(&env);
            client.add_carrier(&admin, &c);
            client.add_carrier_to_whitelist(&company, &c);
            all_carriers.push_back(c);
        }

        // Page 1: first 4.
        let page1 = client.list_company_carriers(&company, &all_carriers, &0, &4);
        assert_eq!(page1.carriers.len(), 4);
        assert!(page1.next_cursor.is_some());

        // Page 2: remaining 2.
        let cursor = page1.next_cursor.unwrap();
        let page2 = client.list_company_carriers(&company, &all_carriers, &cursor, &4);
        assert_eq!(page2.carriers.len(), 2);
        assert!(page2.next_cursor.is_none());
    }

    #[test]
    fn list_skips_non_whitelisted_candidates() {
        let (env, client, admin) = setup();
        let company = Address::generate(&env);
        client.add_company(&admin, &company);

        let whitelisted = Address::generate(&env);
        let not_whitelisted = Address::generate(&env);
        client.add_carrier(&admin, &whitelisted);
        client.add_carrier(&admin, &not_whitelisted);
        client.add_carrier_to_whitelist(&company, &whitelisted);

        let mut candidates = Vec::new(&env);
        candidates.push_back(not_whitelisted.clone());
        candidates.push_back(whitelisted.clone());
        candidates.push_back(not_whitelisted.clone());

        let page = client.list_company_carriers(&company, &candidates, &0, &10);
        assert_eq!(page.carriers.len(), 1);
        assert_eq!(page.carriers.get(0).unwrap(), whitelisted);
    }

    #[test]
    fn list_rejects_invalid_page_size() {
        use crate::NavinError;
        let (env, client, admin) = setup();
        let (company, _) = add_company_and_carrier(&env, &client, &admin);
        let candidates = Vec::new(&env);

        // page_size = 0
        let result = client.try_list_company_carriers(&company, &candidates, &0, &0);
        assert_eq!(result, Err(Ok(NavinError::InvalidConfig)));

        // page_size = 51
        let result = client.try_list_company_carriers(&company, &candidates, &0, &51);
        assert_eq!(result, Err(Ok(NavinError::InvalidConfig)));
    }

    #[test]
    fn list_cursor_past_end_returns_empty() {
        let (env, client, admin) = setup();
        let (company, carrier) = add_company_and_carrier(&env, &client, &admin);
        client.add_carrier_to_whitelist(&company, &carrier);

        let mut candidates = Vec::new(&env);
        candidates.push_back(carrier);

        // cursor = 1 is past the only element (index 0).
        let page = client.list_company_carriers(&company, &candidates, &1, &10);
        assert_eq!(page.carriers.len(), 0);
        assert!(page.next_cursor.is_none());
    }

    // ── is_carrier_whitelisted — unassigned role queries (issue #518) ───────────

    /// Query is_carrier_whitelisted for an unassigned carrier under a registered
    /// company. The function must return false gracefully without a storage panic.
    #[test]
    fn whitelist_returns_false_for_unassigned_carrier_under_registered_company() {
        let (env, client, admin) = setup();
        let (company, _) = add_company_and_carrier(&env, &client, &admin);

        // This address was never registered with any role.
        let unassigned = Address::generate(&env);

        assert!(
            !client.is_carrier_whitelisted(&company, &unassigned),
            "unassigned carrier must not appear whitelisted under a registered company"
        );
    }

    /// Query is_carrier_whitelisted for a registered carrier under an address
    /// that has no company role. Must return false without erroring.
    #[test]
    fn whitelist_returns_false_for_registered_carrier_under_unassigned_company() {
        let (env, client, admin) = setup();
        let (_company, carrier) = add_company_and_carrier(&env, &client, &admin);

        // An address that was never given the Company role.
        let unassigned_company = Address::generate(&env);

        assert!(
            !client.is_carrier_whitelisted(&unassigned_company, &carrier),
            "carrier must not appear whitelisted under an address with no company role"
        );
    }

    /// Query is_carrier_whitelisted when both addresses have no role assigned.
    /// Must return false without panicking or producing an invalid-key error.
    #[test]
    fn whitelist_returns_false_when_both_addresses_are_unassigned() {
        let (env, client, _admin) = setup();

        let unassigned_company = Address::generate(&env);
        let unassigned_carrier = Address::generate(&env);

        assert!(
            !client.is_carrier_whitelisted(&unassigned_company, &unassigned_carrier),
            "two fully unassigned addresses must yield false from is_carrier_whitelisted"
        );
    }

    #[test]
    fn list_deterministic_order_matches_candidates_order() {
        let (env, client, admin) = setup();
        let company = Address::generate(&env);
        client.add_company(&admin, &company);

        let c1 = Address::generate(&env);
        let c2 = Address::generate(&env);
        let c3 = Address::generate(&env);
        client.add_carrier(&admin, &c1);
        client.add_carrier(&admin, &c2);
        client.add_carrier(&admin, &c3);
        client.add_carrier_to_whitelist(&company, &c1);
        client.add_carrier_to_whitelist(&company, &c3);

        let mut candidates = Vec::new(&env);
        candidates.push_back(c1.clone());
        candidates.push_back(c2.clone()); // not whitelisted
        candidates.push_back(c3.clone());

        let page = client.list_company_carriers(&company, &candidates, &0, &10);
        assert_eq!(page.carriers.len(), 2);
        assert_eq!(page.carriers.get(0).unwrap(), c1);
        assert_eq!(page.carriers.get(1).unwrap(), c3);
    }

    // ════════════════════════════════════════════════════════════════════════
    // ISSUE #539 — duplicate carrier whitelist registration rejection
    // ════════════════════════════════════════════════════════════════════════

    /// Baseline: a fresh `add_carrier_to_whitelist` succeeds and the
    /// carrier is then visible via `is_carrier_whitelisted`. Anchors the
    /// duplicate-rejection tests below — without this we couldn't tell a
    /// duplicate-rejection failure apart from a generic add failure.
    #[test]
    fn issue_539_first_whitelist_add_succeeds() {
        let (env, client, admin) = setup();
        let (company, carrier) = add_company_and_carrier(&env, &client, &admin);

        let result = client.try_add_carrier_to_whitelist(&company, &carrier);
        assert!(
            result.is_ok(),
            "first whitelist add for a fresh carrier must succeed"
        );
        assert!(
            client.is_carrier_whitelisted(&company, &carrier),
            "carrier must be visible on the whitelist after a successful add"
        );
    }

    /// Re-adding an already-whitelisted carrier must return the typed
    /// `CarrierAlreadyWhitelisted` error instead of silently succeeding.
    /// This is the core acceptance criterion of #539.
    #[test]
    fn issue_539_duplicate_whitelist_add_returns_typed_error() {
        let (env, client, admin) = setup();
        let (company, carrier) = add_company_and_carrier(&env, &client, &admin);

        client.add_carrier_to_whitelist(&company, &carrier);

        let result = client.try_add_carrier_to_whitelist(&company, &carrier);
        assert_eq!(
            result,
            Err(Ok(crate::NavinError::CarrierAlreadyWhitelisted)),
            "duplicate add must surface the typed CarrierAlreadyWhitelisted error"
        );
    }

    /// Whitelist state must remain consistent after a rejected duplicate
    /// add. The first entry still exists; no second copy is recorded.
    /// Guards against a future refactor that returns the error AFTER
    /// mutating storage.
    #[test]
    fn issue_539_whitelist_state_unchanged_after_duplicate_rejection() {
        let (env, client, admin) = setup();
        let (company, carrier) = add_company_and_carrier(&env, &client, &admin);

        client.add_carrier_to_whitelist(&company, &carrier);
        let _ = client.try_add_carrier_to_whitelist(&company, &carrier);

        assert!(
            client.is_carrier_whitelisted(&company, &carrier),
            "whitelist entry must persist after a duplicate add is rejected"
        );
    }

    /// The duplicate-rejection rule is per (company, carrier) pair. The
    /// same carrier whitelisted by a DIFFERENT company is a fresh
    /// (company, carrier) tuple and must still be addable.
    #[test]
    fn issue_539_same_carrier_can_be_whitelisted_by_different_companies() {
        let (env, client, admin) = setup();
        let company_a = Address::generate(&env);
        let company_b = Address::generate(&env);
        let carrier = Address::generate(&env);
        client.add_company(&admin, &company_a);
        client.add_company(&admin, &company_b);
        client.add_carrier(&admin, &carrier);

        client.add_carrier_to_whitelist(&company_a, &carrier);
        let result = client.try_add_carrier_to_whitelist(&company_b, &carrier);
        assert!(
            result.is_ok(),
            "the duplicate-add rule is scoped per (company, carrier) and must not block a different company"
        );
        assert!(client.is_carrier_whitelisted(&company_a, &carrier));
        assert!(client.is_carrier_whitelisted(&company_b, &carrier));
    }

    // ── handoff_shipment notification emission (issue #690) ─────────────────

    /// Performing a carrier handoff must emit exactly four `notification`
    /// events — one each for the sender, receiver, old carrier, and new
    /// carrier — using the `CarrierHandoff` notification type.
    #[test]
    fn handoff_emits_notifications_for_all_four_parties() {
        use crate::types::NotificationType;
        use soroban_sdk::TryFromVal;

        let (env, client, admin) = setup();

        let company = Address::generate(&env);
        let receiver = Address::generate(&env);
        let old_carrier = Address::generate(&env);
        let new_carrier = Address::generate(&env);

        client.add_company(&admin, &company);
        client.add_carrier(&admin, &old_carrier);
        client.add_carrier(&admin, &new_carrier);

        let data_hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
        let handoff_hash = soroban_sdk::BytesN::from_array(&env, &[2u8; 32]);
        let deadline = env.ledger().timestamp() + 3600;

        let shipment_id = client.create_shipment(
            &company,
            &receiver,
            &old_carrier,
            &data_hash,
            &Vec::new(&env),
            &deadline,
        );

        // Clear events produced during setup so the count below is unambiguous.
        env.events().all();

        client.handoff_shipment(&old_carrier, &new_carrier, &shipment_id, &handoff_hash);

        let events = env.events().all();

        // Collect every notification event emitted during the handoff call.
        let mut notification_events = soroban_sdk::Vec::new(&env);
        for event in events.iter() {
            if let Some(raw) = event.1.get(0) {
                if Symbol::try_from_val(&env, &raw).ok() == Some(Symbol::new(&env, "notification")) {
                    notification_events.push_back(event);
                }
            }
        }

        assert_eq!(
            notification_events.len(),
            4,
            "handoff_shipment must emit exactly 4 notifications: sender, receiver, old carrier, new carrier"
        );

        // Every notification must carry the CarrierHandoff type.
        for (_contract, _topics, data) in notification_events.iter() {
            let (_recipient, notif_type, sid, _hash): (
                Address,
                NotificationType,
                u64,
                soroban_sdk::BytesN<32>,
            ) = soroban_sdk::TryFromVal::try_from_val(&env, &data)
                .expect("notification event data must deserialize");

            assert_eq!(
                notif_type,
                NotificationType::CarrierHandoff,
                "all handoff notifications must use NotificationType::CarrierHandoff"
            );
            assert_eq!(
                sid, shipment_id,
                "notification must reference the correct shipment id"
            );
        }

        // Verify each of the four parties received exactly one notification.
        let mut recipients = soroban_sdk::Vec::new(&env);
        for (_contract, _topics, data) in notification_events.iter() {
            let (recipient, _, _, _): (
                Address,
                NotificationType,
                u64,
                soroban_sdk::BytesN<32>,
            ) = soroban_sdk::TryFromVal::try_from_val(&env, &data)
                .expect("notification event data must deserialize");
            recipients.push_back(recipient);
        }

        assert!(
            recipients.first_index_of(&company).is_some(),
            "sender (company) must receive a CarrierHandoff notification"
        );
        assert!(
            recipients.first_index_of(&receiver).is_some(),
            "receiver must receive a CarrierHandoff notification"
        );
        assert!(
            recipients.first_index_of(&old_carrier).is_some(),
            "old carrier must receive a CarrierHandoff notification"
        );
        assert!(
            recipients.first_index_of(&new_carrier).is_some(),
            "new carrier must receive a CarrierHandoff notification"
        );
    }

    /// After remove + re-add the carrier must be addable again. The
    /// duplicate-rejection rule must only fire on *current* membership,
    /// not on historical presence.
    #[test]
    fn issue_539_can_readd_after_remove() {
        let (env, client, admin) = setup();
        let (company, carrier) = add_company_and_carrier(&env, &client, &admin);

        client.add_carrier_to_whitelist(&company, &carrier);
        client.remove_carrier_from_whitelist(&company, &carrier);
        assert!(!client.is_carrier_whitelisted(&company, &carrier));

        let result = client.try_add_carrier_to_whitelist(&company, &carrier);
        assert!(
            result.is_ok(),
            "carrier must be re-addable after being removed from the whitelist"
        );
    }
}
