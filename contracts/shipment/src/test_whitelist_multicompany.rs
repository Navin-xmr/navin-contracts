//! Tests for issue #5 — carrier whitelist multi-company edge-case tests.
//!
//! Verifies that `DataKey::CarrierWhitelist(company, carrier)` is pair-scoped,
//! meaning a carrier shared across multiple companies retains independent
//! whitelist state for each company.  Removal from one company must never
//! affect another company's whitelist entry for the same carrier.

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

    // ── Add a carrier shared across two companies ─────────────────────────────

    /// A carrier whitelisted for company_a must NOT be automatically allowed
    /// for company_b — the whitelist is pair-specific.
    #[test]
    fn whitelist_add_is_scoped_to_company() {
        let (env, client, admin) = setup();

        let company_a = Address::generate(&env);
        let company_b = Address::generate(&env);
        let shared_carrier = Address::generate(&env);

        client.add_company(&admin, &company_a);
        client.add_company(&admin, &company_b);
        client.add_carrier(&admin, &shared_carrier);

        // Whitelist the carrier only for company_a.
        client.add_carrier_to_whitelist(&company_a, &shared_carrier);

        // company_a → allowed
        assert!(
            client.is_company_carrier_allowed(&company_a, &shared_carrier),
            "carrier must be allowed for company_a after explicit whitelist"
        );

        // company_b → NOT allowed (no separate whitelist entry)
        assert!(
            !client.is_company_carrier_allowed(&company_b, &shared_carrier),
            "carrier must NOT be allowed for company_b when only company_a whitelisted it"
        );
    }

    /// Both companies can independently whitelist the same carrier, and both
    /// lookups must return true.
    #[test]
    fn whitelist_same_carrier_in_two_companies_independently() {
        let (env, client, admin) = setup();

        let company_a = Address::generate(&env);
        let company_b = Address::generate(&env);
        let shared_carrier = Address::generate(&env);

        client.add_company(&admin, &company_a);
        client.add_company(&admin, &company_b);
        client.add_carrier(&admin, &shared_carrier);

        client.add_carrier_to_whitelist(&company_a, &shared_carrier);
        client.add_carrier_to_whitelist(&company_b, &shared_carrier);

        assert!(
            client.is_company_carrier_allowed(&company_a, &shared_carrier),
            "carrier must be allowed for company_a"
        );
        assert!(
            client.is_company_carrier_allowed(&company_b, &shared_carrier),
            "carrier must be allowed for company_b"
        );
    }

    // ── Remove scoped to the correct company ─────────────────────────────────

    /// Removing the carrier from company_a must NOT affect company_b's
    /// whitelist entry for the same carrier.
    #[test]
    fn whitelist_remove_from_one_company_does_not_affect_other() {
        let (env, client, admin) = setup();

        let company_a = Address::generate(&env);
        let company_b = Address::generate(&env);
        let shared_carrier = Address::generate(&env);

        client.add_company(&admin, &company_a);
        client.add_company(&admin, &company_b);
        client.add_carrier(&admin, &shared_carrier);

        // Both companies whitelist the same carrier.
        client.add_carrier_to_whitelist(&company_a, &shared_carrier);
        client.add_carrier_to_whitelist(&company_b, &shared_carrier);

        // Remove from company_a only.
        client.remove_carrier_from_whitelist(&company_a, &shared_carrier);

        // company_a → now NOT allowed
        assert!(
            !client.is_company_carrier_allowed(&company_a, &shared_carrier),
            "carrier must be disallowed for company_a after removal"
        );

        // company_b → still allowed (removal must be pair-scoped)
        assert!(
            client.is_company_carrier_allowed(&company_b, &shared_carrier),
            "carrier must still be allowed for company_b — removal from company_a must not cascade"
        );
    }

    /// Symmetric: removing from company_b must not affect company_a.
    #[test]
    fn whitelist_remove_from_second_company_does_not_affect_first() {
        let (env, client, admin) = setup();

        let company_a = Address::generate(&env);
        let company_b = Address::generate(&env);
        let shared_carrier = Address::generate(&env);

        client.add_company(&admin, &company_a);
        client.add_company(&admin, &company_b);
        client.add_carrier(&admin, &shared_carrier);

        client.add_carrier_to_whitelist(&company_a, &shared_carrier);
        client.add_carrier_to_whitelist(&company_b, &shared_carrier);

        // Remove from company_b only.
        client.remove_carrier_from_whitelist(&company_b, &shared_carrier);

        // company_a → still allowed
        assert!(
            client.is_company_carrier_allowed(&company_a, &shared_carrier),
            "carrier must still be allowed for company_a — removal from company_b must not cascade"
        );

        // company_b → NOT allowed
        assert!(
            !client.is_company_carrier_allowed(&company_b, &shared_carrier),
            "carrier must be disallowed for company_b after removal"
        );
    }

    // ── Lookup for the same carrier in two companies ──────────────────────────

    /// Querying the same carrier for two distinct companies returns independent
    /// boolean results based solely on each pair's whitelist state.
    #[test]
    fn whitelist_lookup_same_carrier_two_companies_independent_results() {
        let (env, client, admin) = setup();

        let company_a = Address::generate(&env);
        let company_b = Address::generate(&env);
        let company_c = Address::generate(&env);
        let shared_carrier = Address::generate(&env);

        client.add_company(&admin, &company_a);
        client.add_company(&admin, &company_b);
        client.add_company(&admin, &company_c);
        client.add_carrier(&admin, &shared_carrier);

        // Only company_a whitelists the carrier.
        client.add_carrier_to_whitelist(&company_a, &shared_carrier);

        // Explicit lookup results
        assert!(client.is_company_carrier_allowed(&company_a, &shared_carrier));
        assert!(!client.is_company_carrier_allowed(&company_b, &shared_carrier));
        assert!(!client.is_company_carrier_allowed(&company_c, &shared_carrier));
    }

    // ── list_company_carriers respects per-company scope ─────────────────────

    /// list_company_carriers for company_a must not include a carrier that is
    /// only whitelisted for company_b, even when that carrier is in the
    /// candidate list.
    #[test]
    fn list_company_carriers_does_not_cross_company_boundaries() {
        let (env, client, admin) = setup();

        let company_a = Address::generate(&env);
        let company_b = Address::generate(&env);
        let carrier_for_a = Address::generate(&env);
        let shared_carrier = Address::generate(&env);

        client.add_company(&admin, &company_a);
        client.add_company(&admin, &company_b);
        client.add_carrier(&admin, &carrier_for_a);
        client.add_carrier(&admin, &shared_carrier);

        // carrier_for_a → only company_a
        client.add_carrier_to_whitelist(&company_a, &carrier_for_a);
        // shared_carrier → only company_b
        client.add_carrier_to_whitelist(&company_b, &shared_carrier);

        let mut candidates = Vec::new(&env);
        candidates.push_back(carrier_for_a.clone());
        candidates.push_back(shared_carrier.clone());

        // company_a listing should see only carrier_for_a
        let page_a = client.list_company_carriers(&company_a, &candidates, &0, &10);
        assert_eq!(
            page_a.carriers.len(),
            1,
            "company_a must only list its own whitelisted carrier"
        );
        assert_eq!(page_a.carriers.get(0).unwrap(), carrier_for_a);

        // company_b listing should see only shared_carrier
        let page_b = client.list_company_carriers(&company_b, &candidates, &0, &10);
        assert_eq!(
            page_b.carriers.len(),
            1,
            "company_b must only list its own whitelisted carrier"
        );
        assert_eq!(page_b.carriers.get(0).unwrap(), shared_carrier);
    }

    // ── Re-add after removal is pair-scoped ──────────────────────────────────

    /// After removing a carrier from company_a, re-adding it restores only
    /// company_a's entry. company_b is unaffected throughout.
    #[test]
    fn whitelist_re_add_after_removal_is_pair_scoped() {
        let (env, client, admin) = setup();

        let company_a = Address::generate(&env);
        let company_b = Address::generate(&env);
        let shared_carrier = Address::generate(&env);

        client.add_company(&admin, &company_a);
        client.add_company(&admin, &company_b);
        client.add_carrier(&admin, &shared_carrier);

        client.add_carrier_to_whitelist(&company_a, &shared_carrier);
        client.add_carrier_to_whitelist(&company_b, &shared_carrier);

        // Remove from company_a
        client.remove_carrier_from_whitelist(&company_a, &shared_carrier);
        assert!(!client.is_company_carrier_allowed(&company_a, &shared_carrier));
        assert!(client.is_company_carrier_allowed(&company_b, &shared_carrier));

        // Re-add to company_a
        client.add_carrier_to_whitelist(&company_a, &shared_carrier);
        assert!(
            client.is_company_carrier_allowed(&company_a, &shared_carrier),
            "re-adding must restore company_a's whitelist entry"
        );
        assert!(
            client.is_company_carrier_allowed(&company_b, &shared_carrier),
            "company_b's whitelist entry must remain unaffected throughout"
        );
    }
}
