//! Tests for issue #296 — shipment creation quota window.
//!
//! Verifies that the per-company creation quota is enforced within the active
//! window and resets correctly when the window expires.

#[cfg(test)]
mod tests {
    use crate::{test_utils, NavinError, NavinShipment, NavinShipmentClient};
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
        env.ledger().timestamp() + 7200
    }

    fn create_one(
        env: &Env,
        client: &NavinShipmentClient,
        company: &Address,
        carrier: &Address,
        seed: u8,
    ) -> Result<u64, crate::NavinError> {
        let hash = make_hash(env, seed);
        let deadline = future_deadline(env);
        match client.try_create_shipment(
            company,
            &Address::generate(env),
            carrier,
            &hash,
            &Vec::new(env),
            &deadline,
            &None,
        ) {
            Ok(Ok(id)) => Ok(id),
            Err(Ok(e)) => Err(e),
            _ => panic!("unexpected error in create_one"),
        }
    }

    // ── quota disabled by default ────────────────────────────────────────────

    #[test]
    fn quota_disabled_by_default_allows_unlimited_creation() {
        let (env, client, _admin, company, carrier) = setup();

        // With quota disabled (max=0), many shipments should succeed.
        for seed in 1u8..=10 {
            assert!(create_one(&env, &client, &company, &carrier, seed).is_ok());
            // Advance time to avoid idempotency window collision.
            env.ledger().with_mut(|l| l.timestamp += 400);
        }
    }

    // ── quota enforced within window ─────────────────────────────────────────

    #[test]
    fn quota_exceeded_within_window_returns_error() {
        let (env, client, admin, company, carrier) = setup();

        // Set quota: max 3 per 3600-second window.
        client.set_creation_quota(&admin, &3, &3600);

        for seed in 1u8..=3 {
            assert!(create_one(&env, &client, &company, &carrier, seed).is_ok());
            env.ledger().with_mut(|l| l.timestamp += 400);
        }

        // 4th attempt within the same window should fail.
        let result = create_one(&env, &client, &company, &carrier, 4);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));
    }

    // ── quota resets after window expires ────────────────────────────────────

    #[test]
    fn quota_resets_after_window_expires() {
        let (env, client, admin, company, carrier) = setup();

        client.set_creation_quota(&admin, &2, &3600);

        // Use up the quota.
        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        // Confirm quota is exhausted.
        let result = create_one(&env, &client, &company, &carrier, 3);
        assert_eq!(result, Err(NavinError::CreationQuotaExceeded));

        // Advance past the window (3600 seconds).
        env.ledger().with_mut(|l| l.timestamp += 3600);

        // Quota should have reset — new shipments allowed.
        assert!(create_one(&env, &client, &company, &carrier, 4).is_ok());
    }

    // ── get_creation_quota_status ────────────────────────────────────────────

    #[test]
    fn quota_status_returns_max_when_disabled() {
        let (_env, client, _admin, company, _carrier) = setup();

        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 0);
        assert_eq!(remaining, u32::MAX);
    }

    #[test]
    fn quota_status_tracks_usage_correctly() {
        let (env, client, admin, company, carrier) = setup();

        client.set_creation_quota(&admin, &5, &3600);

        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 0);
        assert_eq!(remaining, 5);

        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);

        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 1);
        assert_eq!(remaining, 4);
    }

    #[test]
    fn quota_status_resets_after_window() {
        let (env, client, admin, company, carrier) = setup();

        client.set_creation_quota(&admin, &3, &3600);

        assert!(create_one(&env, &client, &company, &carrier, 1).is_ok());
        env.ledger().with_mut(|l| l.timestamp += 400);
        assert!(create_one(&env, &client, &company, &carrier, 2).is_ok());

        // Advance past window.
        env.ledger().with_mut(|l| l.timestamp += 3600);

        let (used, remaining) = client.get_creation_quota_status(&company);
        assert_eq!(used, 0);
        assert_eq!(remaining, 3);
    }

    // ── set_creation_quota validation ────────────────────────────────────────

    #[test]
    fn set_creation_quota_rejects_zero_window_with_nonzero_max() {
        let (_env, client, admin, _company, _carrier) = setup();

        let result = client.try_set_creation_quota(&admin, &5, &0);
        assert_eq!(result, Err(Ok(NavinError::InvalidConfig)));
    }

    #[test]
    fn set_creation_quota_allows_zero_max_to_disable() {
        let (_env, client, admin, _company, _carrier) = setup();

        // max=0 disables quota regardless of window.
        assert!(client.try_set_creation_quota(&admin, &0, &0).is_ok());
    }

    #[test]
    fn set_creation_quota_rejects_non_admin() {
        let (env, client, _admin, company, _carrier) = setup();

        let result = client.try_set_creation_quota(&company, &5, &3600);
        assert_eq!(result, Err(Ok(NavinError::Unauthorized)));
    }

    // ── batch creation respects quota ────────────────────────────────────────

    #[test]
    fn batch_creation_respects_quota() {
        use crate::types::ShipmentInput;
        let (env, client, admin, company, carrier) = setup();

        // Allow max 2 per window.
        client.set_creation_quota(&admin, &2, &3600);

        let deadline = future_deadline(&env);
        let mut inputs = soroban_sdk::Vec::new(&env);
        for seed in 1u8..=3 {
            inputs.push_back(ShipmentInput {
                receiver: Address::generate(&env),
                carrier: carrier.clone(),
                data_hash: make_hash(&env, seed),
                payment_milestones: soroban_sdk::Vec::new(&env),
                deadline,
            depends_on: None,
        });
        }

        // Batch of 3 exceeds quota of 2.
        let result = client.try_create_shipments_batch(&company, &inputs);
        assert_eq!(result, Err(Ok(NavinError::CreationQuotaExceeded)));
    }
}
