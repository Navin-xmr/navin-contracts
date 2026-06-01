//! Tests for issue #287 — per-method precondition guard helpers.
//!
//! Verifies that the centralised helpers (`require_admin`, `require_shipment`,
//! `require_carrier_for_shipment`, `require_initialized`, `require_not_paused`,
//! `require_role`, `require_active_company`, `require_active_carrier`) produce
//! the same errors as the inline checks they replaced, and that functional
//! behaviour is unchanged.

#[cfg(test)]
mod tests {
    use crate::{test_utils, NavinError, NavinShipment, NavinShipmentClient, ShipmentStatus};
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Ledger as _},
        Address, BytesN, Env, Vec,
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

    fn setup() -> (Env, NavinShipmentClient<'static>, Address, Address, Address) {
        let (env, admin) = test_utils::setup_env();
        let contract_id = env.register(NavinShipment, ());
        let client = NavinShipmentClient::new(&env, &contract_id);

        let token_id = env.register(MockToken, ());
        client.initialize(&admin, &token_id);

        let company = Address::generate(&env);
        let carrier = Address::generate(&env);
        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        (env, client, admin, company, carrier)
    }

    fn make_hash(env: &Env, seed: u8) -> BytesN<32> {
        BytesN::from_array(env, &[seed; 32])
    }

    fn future_deadline(env: &Env) -> u64 {
        env.ledger().timestamp() + 3600
    }

    // ── require_initialized ──────────────────────────────────────────────────

    #[test]
    fn require_initialized_blocks_uninitialized_contract() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(NavinShipment, ());
        let client = NavinShipmentClient::new(&env, &contract_id);

        // Any method that calls require_initialized should return NotInitialized.
        let result = client.try_get_admin();
        assert_eq!(result, Err(Ok(NavinError::NotInitialized)));
    }

    // ── require_not_paused ───────────────────────────────────────────────────

    #[test]
    fn require_not_paused_blocks_state_changes_when_paused() {
        let (env, client, admin, company, carrier) = setup();
        let hash = make_hash(&env, 1);
        let deadline = future_deadline(&env);

        // Pause the contract.
        client.pause(&admin);

        let result = client.try_create_shipment(
            &company,
            &Address::generate(&env),
            &carrier,
            &hash,
            &Vec::new(&env),
            &deadline,
        );
        assert_eq!(result, Err(Ok(NavinError::ContractPaused)));
    }

    // ── require_admin ────────────────────────────────────────────────────────

    #[test]
    fn require_admin_rejects_non_admin() {
        let (env, client, _admin, company, _carrier) = setup();

        // set_shipment_limit uses require_admin internally.
        let result = client.try_set_shipment_limit(&company, &10);
        assert_eq!(result, Err(Ok(NavinError::Unauthorized)));
    }

    #[test]
    fn require_admin_accepts_admin() {
        let (_env, client, admin, _company, _carrier) = setup();
        assert!(client.try_set_shipment_limit(&admin, &50).is_ok());
    }

    // ── require_role (Company) ───────────────────────────────────────────────

    #[test]
    fn require_role_company_rejects_carrier() {
        let (env, client, _admin, _company, carrier) = setup();
        let hash = make_hash(&env, 2);
        let deadline = future_deadline(&env);

        let result = client.try_create_shipment(
            &carrier,
            &Address::generate(&env),
            &carrier,
            &hash,
            &Vec::new(&env),
            &deadline,
        );
        assert_eq!(result, Err(Ok(NavinError::Unauthorized)));
    }

    // ── require_role (Carrier) ───────────────────────────────────────────────

    #[test]
    fn require_role_carrier_rejects_company_on_update_status() {
        let (env, client, _admin, company, carrier) = setup();
        let hash = make_hash(&env, 3);
        let deadline = future_deadline(&env);

        let id = client.create_shipment(
            &company,
            &Address::generate(&env),
            &carrier,
            &hash,
            &Vec::new(&env),
            &deadline,
        );

        let hash2 = make_hash(&env, 4);
        // company is not the carrier — should be Unauthorized.
        let result = client.try_update_status(&company, &id, &ShipmentStatus::InTransit, &hash2);
        assert_eq!(result, Err(Ok(NavinError::Unauthorized)));
    }

    // ── require_active_company ───────────────────────────────────────────────

    #[test]
    fn require_active_company_rejects_suspended_company() {
        let (env, client, admin, company, carrier) = setup();
        client.suspend_company(&admin, &company);

        let hash = make_hash(&env, 5);
        let deadline = future_deadline(&env);

        let result = client.try_create_shipment(
            &company,
            &Address::generate(&env),
            &carrier,
            &hash,
            &Vec::new(&env),
            &deadline,
        );
        assert_eq!(result, Err(Ok(NavinError::CompanySuspended)));
    }

    // ── require_active_carrier ───────────────────────────────────────────────

    #[test]
    fn require_active_carrier_rejects_suspended_carrier_on_update_status() {
        let (env, client, admin, company, carrier) = setup();
        let hash = make_hash(&env, 6);
        let deadline = future_deadline(&env);

        let id = client.create_shipment(
            &company,
            &Address::generate(&env),
            &carrier,
            &hash,
            &Vec::new(&env),
            &deadline,
        );

        client.suspend_carrier(&admin, &carrier);

        let hash2 = make_hash(&env, 7);
        let result = client.try_update_status(&carrier, &id, &ShipmentStatus::InTransit, &hash2);
        assert_eq!(result, Err(Ok(NavinError::CarrierSuspended)));
    }

    // ── require_shipment (via get_shipment) ──────────────────────────────────

    #[test]
    fn require_shipment_returns_not_found_for_missing_id() {
        let (_env, client, _admin, _company, _carrier) = setup();
        let result = client.try_get_shipment(&9999);
        assert!(matches!(result, Err(Ok(NavinError::ShipmentNotFound))));
    }

    // ── require_not_finalized ────────────────────────────────────────────────

    #[test]
    fn require_not_finalized_blocks_mutation_on_finalized_shipment() {
        let (env, client, _admin, company, carrier) = setup();
        let receiver = Address::generate(&env);
        let hash = make_hash(&env, 8);
        let deadline = future_deadline(&env);

        let id = client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &hash,
            &Vec::new(&env),
            &deadline,
        );

        // Transition to Delivered (finalized when escrow == 0).
        let h2 = make_hash(&env, 9);
        client.update_status(&carrier, &id, &ShipmentStatus::InTransit, &h2);
        test_utils::advance_past_rate_limit(&env);
        let h3 = make_hash(&env, 10);
        client.update_status(&carrier, &id, &ShipmentStatus::Delivered, &h3);

        // Shipment is now finalized (auto-finalized on Delivered with no escrow) — further mutations should fail.
        let h5 = make_hash(&env, 12);
        let result = client.try_cancel_shipment(&company, &id, &h5);
        assert_eq!(result, Err(Ok(NavinError::ShipmentFinalized)));
    }
}
