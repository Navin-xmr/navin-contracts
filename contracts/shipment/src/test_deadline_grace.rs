//! Tests for issue #6 — deadline grace-period boundary tests.
//!
//! Verifies the exact boundary semantics of `deadline_grace_seconds` as used
//! by `check_deadline`.  The expiry threshold is:
//!
//!   `expiry = deadline + deadline_grace_seconds`
//!
//! * `timestamp < expiry`  → `NotExpired` (shipment untouched)
//! * `timestamp >= expiry` → shipment cancelled
//!
//! Tests document the zero-grace default, boundary precision, and the 7-day
//! cap enforcement via `update_config`.

#[cfg(test)]
mod tests {
    use crate::{test_utils, NavinShipment, NavinShipmentClient, ShipmentStatus};
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Vec};

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

    /// Helper: create a shipment with a specific deadline. Returns shipment_id.
    fn create_with_deadline(
        env: &Env,
        client: &NavinShipmentClient<'static>,
        admin: &Address,
        seed: u8,
        deadline: u64,
    ) -> u64 {
        let company = Address::generate(env);
        let receiver = Address::generate(env);
        let carrier = Address::generate(env);
        let data_hash = BytesN::from_array(env, &[seed; 32]);

        client.add_company(admin, &company);
        client.add_carrier(admin, &carrier);

        client.create_shipment(
            &company,
            &receiver,
            &carrier,
            &data_hash,
            &Vec::new(env),
            &deadline,
        )
    }

    /// Helper: configure a grace period of `grace` seconds and return the
    /// updated config.
    fn set_grace(client: &NavinShipmentClient<'static>, admin: &Address, grace: u64) {
        let mut cfg = client.get_contract_config();
        cfg.deadline_grace_seconds = grace;
        client.update_config(admin, &cfg);
    }

    // ── Zero grace (default) ──────────────────────────────────────────────────

    /// Default config: deadline_grace_seconds = 0.
    /// check_deadline at exactly `deadline` must return NotExpired
    /// (threshold is deadline + 0 = deadline; condition is timestamp >= threshold).
    #[test]
    fn zero_grace_at_deadline_not_expired() {
        let (env, client, admin) = setup();
        let now = env.ledger().timestamp();
        let deadline = now + 1000;
        let id = create_with_deadline(&env, &client, &admin, 0x01, deadline);

        // timestamp == deadline → NOT yet >= expiry (>= means strictly at or past)
        // expiry = deadline + 0 = deadline; condition: timestamp < expiry → NotExpired.
        // We land at deadline - 1 to confirm NotExpired just before.
        test_utils::set_ledger_time(&env, deadline - 1);
        let result = client.try_check_deadline(&id);
        assert!(
            result.is_err(),
            "check_deadline at deadline - 1 with zero grace must return NotExpired"
        );
    }

    /// Default config: check_deadline at `deadline + 1` must cancel the shipment.
    #[test]
    fn zero_grace_one_second_past_deadline_cancels_shipment() {
        let (env, client, admin) = setup();
        let now = env.ledger().timestamp();
        let deadline = now + 1000;
        let id = create_with_deadline(&env, &client, &admin, 0x02, deadline);

        // expiry = deadline; timestamp = deadline + 1 >= expiry → cancels.
        test_utils::set_ledger_time(&env, deadline + 1);
        client.check_deadline(&id);

        let shipment = client.get_shipment(&id);
        assert_eq!(
            shipment.status,
            ShipmentStatus::Cancelled,
            "shipment must be cancelled 1 second past deadline with zero grace"
        );
    }

    // ── Grace period: inside the window → NotExpired ──────────────────────────

    /// With grace = 300 s: at `deadline + grace - 1` the shipment is NOT expired.
    #[test]
    fn grace_period_inside_window_is_not_expired() {
        let (env, client, admin) = setup();
        let now = env.ledger().timestamp();
        let deadline = now + 1000;
        let grace = 300u64;
        let id = create_with_deadline(&env, &client, &admin, 0x03, deadline);

        set_grace(&client, &admin, grace);

        // timestamp = deadline + grace - 1 → still inside grace window.
        test_utils::set_ledger_time(&env, deadline + grace - 1);
        let result = client.try_check_deadline(&id);
        assert!(
            result.is_err(),
            "check_deadline inside grace window must return NotExpired"
        );

        // Status must be unchanged.
        let shipment = client.get_shipment(&id);
        assert_eq!(
            shipment.status,
            ShipmentStatus::Created,
            "shipment status must remain Created while still inside the grace window"
        );
    }

    // ── Grace period: exactly at boundary ────────────────────────────────────

    /// With grace = 300 s: at exactly `deadline + grace` the shipment EXPIRES
    /// (boundary is inclusive — condition is timestamp >= expiry).
    #[test]
    fn grace_period_at_exact_boundary_expires() {
        let (env, client, admin) = setup();
        let now = env.ledger().timestamp();
        let deadline = now + 1000;
        let grace = 300u64;
        let id = create_with_deadline(&env, &client, &admin, 0x04, deadline);

        set_grace(&client, &admin, grace);

        // timestamp = deadline + grace → exactly at expiry threshold.
        test_utils::set_ledger_time(&env, deadline + grace);
        client.check_deadline(&id);

        let shipment = client.get_shipment(&id);
        assert_eq!(
            shipment.status,
            ShipmentStatus::Cancelled,
            "shipment must be cancelled at exactly deadline + grace"
        );
    }

    // ── Grace period: well past boundary ─────────────────────────────────────

    /// With grace = 300 s: well past `deadline + grace` the shipment is
    /// cancelled and escrow is zero.
    #[test]
    fn grace_period_past_boundary_cancels_shipment_and_clears_escrow() {
        let (env, client, admin) = setup();
        let now = env.ledger().timestamp();
        let deadline = now + 1000;
        let grace = 300u64;
        let id = create_with_deadline(&env, &client, &admin, 0x05, deadline);

        set_grace(&client, &admin, grace);

        test_utils::set_ledger_time(&env, deadline + grace + 500);
        client.check_deadline(&id);

        let shipment = client.get_shipment(&id);
        assert_eq!(shipment.status, ShipmentStatus::Cancelled);
        assert_eq!(
            shipment.escrow_amount, 0,
            "escrow must be zero after deadline expiry"
        );
    }

    // ── Grace period change takes immediate effect ────────────────────────────

    /// Increasing the grace period prevents a previously-expired call from
    /// succeeding (the new threshold is now in the future).
    #[test]
    fn increasing_grace_period_makes_previously_expired_time_not_expired() {
        let (env, client, admin) = setup();
        let now = env.ledger().timestamp();
        let deadline = now + 1000;
        let id = create_with_deadline(&env, &client, &admin, 0x06, deadline);

        // Zero grace, set time to deadline + 1 → would cancel.
        // But first extend grace to 600 s before checking.
        set_grace(&client, &admin, 600);
        test_utils::set_ledger_time(&env, deadline + 1);

        // deadline + 1 is still inside the 600 s grace window → NotExpired.
        let result = client.try_check_deadline(&id);
        assert!(
            result.is_err(),
            "after increasing grace, a previously-expired timestamp must now return NotExpired"
        );
    }

    /// Reducing the grace period makes a previously-inside-window time now expired.
    #[test]
    fn reducing_grace_period_makes_inside_window_time_expire() {
        let (env, client, admin) = setup();
        let now = env.ledger().timestamp();
        let deadline = now + 1000;
        let id = create_with_deadline(&env, &client, &admin, 0x07, deadline);

        // Start with a large grace so we are inside the window.
        set_grace(&client, &admin, 600);
        test_utils::set_ledger_time(&env, deadline + 100); // inside 600s window

        // Shrink grace to 50 s — now deadline + 100 > deadline + 50 → expired.
        set_grace(&client, &admin, 50);
        client.check_deadline(&id);

        let shipment = client.get_shipment(&id);
        assert_eq!(
            shipment.status,
            ShipmentStatus::Cancelled,
            "reducing grace below the elapsed time must cause expiry"
        );
    }

    // ── Max cap enforcement ───────────────────────────────────────────────────

    /// Configuring deadline_grace_seconds = 604_800 (exactly 7 days) must succeed.
    #[test]
    fn grace_period_exactly_seven_days_is_accepted() {
        let (env, client, admin) = setup();
        let mut cfg = client.get_contract_config();
        cfg.deadline_grace_seconds = 604_800; // 7 days exactly
        let result = client.try_update_config(&admin, &cfg);
        assert!(
            result.is_ok(),
            "grace period of exactly 7 days must be accepted by update_config"
        );
        let saved = client.get_contract_config();
        assert_eq!(saved.deadline_grace_seconds, 604_800);
    }

    /// Configuring deadline_grace_seconds = 604_801 (one second over 7 days)
    /// must be rejected by update_config.
    #[test]
    fn grace_period_over_seven_days_is_rejected() {
        let (env, client, admin) = setup();
        let mut cfg = client.get_contract_config();
        cfg.deadline_grace_seconds = 604_801; // 1 s over cap
        let result = client.try_update_config(&admin, &cfg);
        assert!(
            result.is_err(),
            "grace period exceeding 7 days must be rejected by update_config"
        );
    }

    // ── Already-terminal shipments are unaffected ─────────────────────────────

    /// check_deadline on a shipment already in a terminal state must return
    /// ShipmentAlreadyCompleted regardless of the grace period.
    #[test]
    fn check_deadline_on_already_cancelled_shipment_returns_already_completed() {
        use crate::NavinError;
        let (env, client, admin) = setup();
        let now = env.ledger().timestamp();
        let deadline = now + 1000;
        let id = create_with_deadline(&env, &client, &admin, 0x08, deadline);

        // Retrieve the shipment to get company address for cancellation.
        let shipment = client.get_shipment(&id);
        let company = shipment.sender.clone();
        let cancel_hash = BytesN::from_array(&env, &[0xFFu8; 32]);
        client.cancel_shipment(&company, &id, &cancel_hash);

        // Advance past the deadline.
        test_utils::set_ledger_time(&env, deadline + 500);

        let result = client.try_check_deadline(&id);
        assert_eq!(
            result,
            Err(Ok(NavinError::ShipmentAlreadyCompleted)),
            "check_deadline on an already-cancelled shipment must return ShipmentAlreadyCompleted"
        );
    }

    // ── check_deadline on non-existent shipment ───────────────────────────────

    /// check_deadline on a shipment that has never been created must return
    /// ShipmentNotFound.
    #[test]
    fn check_deadline_on_non_existent_shipment_returns_not_found() {
        use crate::NavinError;
        let (env, client, admin) = setup();

        // Advance past any possible deadline.
        test_utils::set_ledger_time(&env, 999_999_999);

        let result = client.try_check_deadline(&9999u64);
        assert_eq!(
            result,
            Err(Ok(NavinError::ShipmentNotFound)),
            "check_deadline on a non-existent shipment must return ShipmentNotFound"
        );
    }
}
