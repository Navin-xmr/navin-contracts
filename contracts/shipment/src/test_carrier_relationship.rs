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

    // ── Removal side-effect tests (issue #463) ──────────────────────────────

    /// After removing a whitelisted carrier, `list_company_carriers` must no
    /// longer include it while retaining unaffected carriers.
    #[test]
    fn list_after_removal_omits_removed_carrier() {
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
        client.add_carrier_to_whitelist(&company, &c2);
        client.add_carrier_to_whitelist(&company, &c3);

        // Remove the middle carrier.
        client.remove_carrier_from_whitelist(&company, &c2);

        let mut candidates = Vec::new(&env);
        candidates.push_back(c1.clone());
        candidates.push_back(c2.clone());
        candidates.push_back(c3.clone());

        let page = client.list_company_carriers(&company, &candidates, &0, &10);
        assert_eq!(page.carriers.len(), 2);
        assert_eq!(page.carriers.get(0).unwrap(), c1);
        assert_eq!(
            page.carriers.get(1).unwrap(),
            c3,
            "removed carrier must not appear in the result"
        );
    }

    /// Pagination cursor remains consistent after a carrier is removed from
    /// the whitelist — the cursor is a plain index into the candidate vector,
    /// not affected by whitelist status.
    #[test]
    fn pagination_after_removal_remains_consistent() {
        let (env, client, admin) = setup();
        let company = Address::generate(&env);
        client.add_company(&admin, &company);

        let mut all = Vec::new(&env);
        for _i in 0..6u32 {
            let c = Address::generate(&env);
            client.add_carrier(&admin, &c);
            client.add_carrier_to_whitelist(&company, &c);
            all.push_back(c);
        }

        // Remove index 1 and 3.
        client.remove_carrier_from_whitelist(&company, &all.get(1).unwrap());
        client.remove_carrier_from_whitelist(&company, &all.get(3).unwrap());

        // Page 1 (size=2): should pick the first 2 whitelisted entries:
        // indices 0 and 2 (skipping removed 1).
        let p1 = client.list_company_carriers(&company, &all, &0, &2);
        assert_eq!(p1.carriers.len(), 2);
        assert_eq!(p1.carriers.get(0).unwrap(), all.get(0).unwrap());
        assert_eq!(p1.carriers.get(1).unwrap(), all.get(2).unwrap());
        assert!(p1.next_cursor.is_some());

        // Page 2 (size=2): continues at cursor, picks index 4 and 5
        // (skipping removed 3).
        let p2 = client.list_company_carriers(&company, &all, &p1.next_cursor.unwrap(), &2);
        assert_eq!(p2.carriers.len(), 2);
        assert_eq!(p2.carriers.get(0).unwrap(), all.get(4).unwrap());
        assert_eq!(p2.carriers.get(1).unwrap(), all.get(5).unwrap());
        assert!(p2.next_cursor.is_none());
    }

    /// Removal of one carrier does not affect the whitelist status or listing
    /// of other carriers in the same company.
    #[test]
    fn removal_preserves_other_carrier_entries() {
        let (env, client, admin) = setup();
        let (company, carrier_a) = add_company_and_carrier(&env, &client, &admin);
        let carrier_b = Address::generate(&env);
        let carrier_c = Address::generate(&env);
        client.add_carrier(&admin, &carrier_b);
        client.add_carrier(&admin, &carrier_c);

        client.add_carrier_to_whitelist(&company, &carrier_a);
        client.add_carrier_to_whitelist(&company, &carrier_b);
        client.add_carrier_to_whitelist(&company, &carrier_c);

        // Remove carrier_b.
        client.remove_carrier_from_whitelist(&company, &carrier_b);

        // carrier_a and carrier_c remain whitelisted.
        assert!(client.is_company_carrier_allowed(&company, &carrier_a));
        assert!(client.is_company_carrier_allowed(&company, &carrier_c));
        assert!(!client.is_company_carrier_allowed(&company, &carrier_b));
    }

    /// Multiple add/remove cycles on the same carrier leave storage clean and
    /// consistent at every step.
    #[test]
    fn multi_cycle_add_remove_is_consistent() {
        let (env, client, admin) = setup();
        let (company, carrier) = add_company_and_carrier(&env, &client, &admin);

        // Cycle 1: add → allowed.
        client.add_carrier_to_whitelist(&company, &carrier);
        assert!(client.is_company_carrier_allowed(&company, &carrier));
        assert!(client.is_carrier_whitelisted(&company, &carrier));

        // Cycle 1: remove → disallowed.
        client.remove_carrier_from_whitelist(&company, &carrier);
        assert!(!client.is_company_carrier_allowed(&company, &carrier));
        assert!(!client.is_carrier_whitelisted(&company, &carrier));

        // Cycle 2: re-add → re-allowed.
        client.add_carrier_to_whitelist(&company, &carrier);
        assert!(client.is_company_carrier_allowed(&company, &carrier));
        assert!(client.is_carrier_whitelisted(&company, &carrier));

        // Cycle 2: re-remove → disallowed again.
        client.remove_carrier_from_whitelist(&company, &carrier);
        assert!(!client.is_company_carrier_allowed(&company, &carrier));
        assert!(!client.is_carrier_whitelisted(&company, &carrier));

        // Cycle 3: final re-add proves storage is clean.
        client.add_carrier_to_whitelist(&company, &carrier);
        assert!(client.is_company_carrier_allowed(&company, &carrier));
        assert!(client.is_carrier_whitelisted(&company, &carrier));
    }

    /// Removing a carrier that was never whitelisted is a benign no-op — no
    /// error is returned and other whitelist entries are unaffected.
    #[test]
    fn remove_nonexistent_whitelist_entry_is_benign() {
        let (env, client, admin) = setup();
        let (company, whitelisted) = add_company_and_carrier(&env, &client, &admin);
        let never_added = Address::generate(&env);
        client.add_carrier(&admin, &never_added);

        // Whitelist only one carrier.
        client.add_carrier_to_whitelist(&company, &whitelisted);

        // Removing a carrier that was never whitelisted should succeed.
        let result = client.try_remove_carrier_from_whitelist(&company, &never_added);
        assert!(
            result.is_ok(),
            "removing non-existent whitelist entry must succeed"
        );

        // The other whitelist entry is intact.
        assert!(client.is_company_carrier_allowed(&company, &whitelisted));

        // The never-added carrier remains un-whitelisted.
        assert!(!client.is_company_carrier_allowed(&company, &never_added));
    }

    /// Direct `is_carrier_whitelisted` (the storage-level query) must also
    /// reflect removal immediately, not just the combined
    /// `is_company_carrier_allowed` gate.
    #[test]
    fn direct_lookup_reflects_removal() {
        let (env, client, admin) = setup();
        let (company, carrier) = add_company_and_carrier(&env, &client, &admin);

        client.add_carrier_to_whitelist(&company, &carrier);
        assert!(client.is_carrier_whitelisted(&company, &carrier));

        client.remove_carrier_from_whitelist(&company, &carrier);
        assert!(!client.is_carrier_whitelisted(&company, &carrier));
    }

    /// Removal from one company must not affect another company's whitelist
    /// entry for the same carrier (pair-scoped isolation).
    #[test]
    fn removal_is_pair_scoped() {
        let (env, client, admin) = setup();
        let company_a = Address::generate(&env);
        let company_b = Address::generate(&env);
        let shared = Address::generate(&env);
        client.add_company(&admin, &company_a);
        client.add_company(&admin, &company_b);
        client.add_carrier(&admin, &shared);

        client.add_carrier_to_whitelist(&company_a, &shared);
        client.add_carrier_to_whitelist(&company_b, &shared);

        // Remove from company_a.
        client.remove_carrier_from_whitelist(&company_a, &shared);
        assert!(!client.is_carrier_whitelisted(&company_a, &shared));
        assert!(
            client.is_carrier_whitelisted(&company_b, &shared),
            "company_b's whitelist entry must survive removal from company_a"
        );

        // Remove from company_b.
        client.remove_carrier_from_whitelist(&company_b, &shared);
        assert!(!client.is_carrier_whitelisted(&company_b, &shared));
    }
}
