//! Tests for issue #295 — company/carrier relationship query APIs.
//!
//! Verifies `is_company_carrier_allowed` and `list_company_carriers` for
//! correctness, suspension semantics, pagination, and bounds.

#[cfg(test)]
mod tests {
    use crate::{test_utils, NavinShipment, NavinShipmentClient};
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env, Vec};

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

    // ── carrier handoff event flow ───────────────────────────────────────────────

    #[test]
    fn test_handoff_event_flow_with_valid_carrier_transition() {
        let (env, client, admin) = setup();
        let company = Address::generate(&env);
        client.add_company(&admin, &company);

        let carrier_a = Address::generate(&env);
        let carrier_b = Address::generate(&env);
        client.add_carrier(&admin, &carrier_a);
        client.add_carrier(&admin, &carrier_b);

        // Add both carriers to whitelist
        client.add_carrier_to_whitelist(&company, &carrier_a);
        client.add_carrier_to_whitelist(&company, &carrier_b);

        // Create shipment with carrier_a
        let receiver = Address::generate(&env);
        let data_hash = BytesN::from_array(&env, &[1u8; 32]);
        let deadline = env.ledger().timestamp() + 3600;
        let id = client.create_shipment(
            &company,
            &receiver,
            &carrier_a,
            &data_hash,
            &Vec::new(&env),
            &deadline,
            &None,
        );

        // Verify initial state
        let shipment = client.get_shipment(&id);
        assert_eq!(shipment.carrier, carrier_a);

        // Perform handoff to carrier_b
        client.handoff_shipment(&carrier_a, &id, &carrier_b, &data_hash);

        // Verify carrier transition
        let after = client.get_shipment(&id);
        assert_eq!(after.carrier, carrier_b);
        
        // Verify handoff event was emitted
        let events = env.events().all();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.topics.get(0).unwrap(), Symbol::new(&env, "handoff"));
        
        // Check from/to in payload
        let from_carrier = event.data.get(0).unwrap().to_address().unwrap();
        let to_carrier = event.data.get(1).unwrap().to_address().unwrap();
        assert_eq!(from_carrier, carrier_a);
        assert_eq!(to_carrier, carrier_b);
    }

    #[test]
    fn test_handoff_event_flow_rejects_invalid_transitions() {
        let (env, client, admin) = setup();
        let company = Address::generate(&env);
        client.add_company(&admin, &company);

        let carrier_a = Address::generate(&env);
        let carrier_b = Address::generate(&env);
        client.add_carrier(&admin, &carrier_a);
        client.add_carrier(&admin, &carrier_b);

        // Add only carrier_a to whitelist
        client.add_carrier_to_whitelist(&company, &carrier_a);

        // Create shipment with carrier_a
        let receiver = Address::generate(&env);
        let data_hash = BytesN::from_array(&env, &[1u8; 32]);
        let deadline = env.ledger().timestamp() + 3600;
        let id = client.create_shipment(
            &company,
            &receiver,
            &carrier_a,
            &data_hash,
            &Vec::new(&env),
            &deadline,
            &None,
        );

        // Try to handoff to carrier_b who is not whitelisted
        let result = client.try_handoff_shipment(&carrier_a, &id, &carrier_b, &data_hash);
        
        assert!(
            result.is_err(),
            "handoff should fail when target carrier is not whitelisted"
        );
        
        // Verify it returns appropriate error instead of panicking
        match result {
            Ok(_) => panic!("expected error but got success"),
            Err(e) => {
                assert_eq!(e, Err(Ok(crate::NavinError::CarrierNotWhitelisted)));
            }
        }
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

    // ── Additional edge case and integration tests ────────────────────────────

    #[test]
    fn allowed_with_multiple_companies_independent_whitelists() {
        let (env, client, admin) = setup();
        let company1 = Address::generate(&env);
        let company2 = Address::generate(&env);
        let carrier = Address::generate(&env);

        client.add_company(&admin, &company1);
        client.add_company(&admin, &company2);
        client.add_carrier(&admin, &carrier);

        // Whitelist carrier only for company1
        client.add_carrier_to_whitelist(&company1, &carrier);

        // Verify company1 sees carrier as allowed
        assert!(client.is_company_carrier_allowed(&company1, &carrier));

        // Verify company2 does not see carrier as allowed
        assert!(!client.is_company_carrier_allowed(&company2, &carrier));

        // Whitelist carrier for company2
        client.add_carrier_to_whitelist(&company2, &carrier);
        assert!(client.is_company_carrier_allowed(&company2, &carrier));

        // Remove from company1 whitelist
        client.remove_carrier_from_whitelist(&company1, &carrier);
        assert!(!client.is_company_carrier_allowed(&company1, &carrier));
        assert!(client.is_company_carrier_allowed(&company2, &carrier));
    }

    #[test]
    fn list_with_suspended_carriers_in_candidates() {
        let (env, client, admin) = setup();
        let company = Address::generate(&env);
        client.add_company(&admin, &company);

        let carrier1 = Address::generate(&env);
        let carrier2 = Address::generate(&env);
        let carrier3 = Address::generate(&env);

        client.add_carrier(&admin, &carrier1);
        client.add_carrier(&admin, &carrier2);
        client.add_carrier(&admin, &carrier3);

        // Whitelist all three
        client.add_carrier_to_whitelist(&company, &carrier1);
        client.add_carrier_to_whitelist(&company, &carrier2);
        client.add_carrier_to_whitelist(&company, &carrier3);

        // Suspend carrier2
        client.suspend_carrier(&admin, &carrier2);

        let mut candidates = Vec::new(&env);
        candidates.push_back(carrier1.clone());
        candidates.push_back(carrier2.clone());
        candidates.push_back(carrier3.clone());

        // List should exclude suspended carrier2
        let page = client.list_company_carriers(&company, &candidates, &0, &10);
        assert_eq!(page.carriers.len(), 2);
        assert_eq!(page.carriers.get(0).unwrap(), carrier1);
        assert_eq!(page.carriers.get(1).unwrap(), carrier3);
    }

    #[test]
    fn list_pagination_with_exact_page_boundary() {
        let (env, client, admin) = setup();
        let company = Address::generate(&env);
        client.add_company(&admin, &company);

        let mut carriers = Vec::new(&env);
        for _ in 0..10u32 {
            let c = Address::generate(&env);
            client.add_carrier(&admin, &c);
            client.add_carrier_to_whitelist(&company, &c);
            carriers.push_back(c);
        }

        // Request exactly 10 items with page_size 10
        let page = client.list_company_carriers(&company, &carriers, &0, &10);
        assert_eq!(page.carriers.len(), 10);
        assert!(page.next_cursor.is_none());

        // Request with page_size 5 should have next_cursor
        let page1 = client.list_company_carriers(&company, &carriers, &0, &5);
        assert_eq!(page1.carriers.len(), 5);
        assert!(page1.next_cursor.is_some());

        let page2 = client.list_company_carriers(&company, &carriers, &5, &5);
        assert_eq!(page2.carriers.len(), 5);
        assert!(page2.next_cursor.is_none());
    }

    #[test]
    fn handoff_preserves_shipment_state_except_carrier() {
        let (env, client, admin) = setup();
        let company = Address::generate(&env);
        client.add_company(&admin, &company);

        let carrier_a = Address::generate(&env);
        let carrier_b = Address::generate(&env);
        client.add_carrier(&admin, &carrier_a);
        client.add_carrier(&admin, &carrier_b);

        client.add_carrier_to_whitelist(&company, &carrier_a);
        client.add_carrier_to_whitelist(&company, &carrier_b);

        let receiver = Address::generate(&env);
        let data_hash = BytesN::from_array(&env, &[42u8; 32]);
        let deadline = env.ledger().timestamp() + 7200;
        let id = client.create_shipment(
            &company,
            &receiver,
            &carrier_a,
            &data_hash,
            &Vec::new(&env),
            &deadline,
            &None,
        );

        let before = client.get_shipment(&id);
        
        // Perform handoff
        client.handoff_shipment(&carrier_a, &id, &carrier_b, &data_hash);

        let after = client.get_shipment(&id);

        // Verify carrier changed
        assert_eq!(after.carrier, carrier_b);

        // Verify other fields remain unchanged
        assert_eq!(after.company, before.company);
        assert_eq!(after.receiver, before.receiver);
        assert_eq!(after.data_hash, before.data_hash);
        assert_eq!(after.deadline, before.deadline);
    }
}
