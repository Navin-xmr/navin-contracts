//! Tests for issue #8 — suspension cascade tests for in-flight shipments.
//!
//! Verifies that suspending a carrier or company while a shipment is already
//! in-flight (status: InTransit) has the correct cascade effect:
//!
//! - The carrier can no longer call `update_status`.
//! - The company can no longer call `create_shipment` or `cancel_shipment`.
//! - The in-flight shipment itself is **not** auto-cancelled by the suspension;
//!   its status remains unchanged until an admin or another authorised action
//!   resolves it.
//! - Reactivating the suspended party restores access.

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

    /// Helper: create a shipment and advance it to InTransit status.
    /// Returns (company, receiver, carrier, shipment_id, data_hash).
    fn create_in_transit_shipment(
        env: &Env,
        client: &NavinShipmentClient<'static>,
        admin: &Address,
    ) -> (Address, Address, Address, u64, BytesN<32>) {
        let company = Address::generate(env);
        let receiver = Address::generate(env);
        let carrier = Address::generate(env);
        let data_hash = BytesN::from_array(env, &[0xABu8; 32]);
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

        // Advance past rate limit before first status update
        test_utils::advance_past_rate_limit(env);

        let transit_hash = BytesN::from_array(env, &[0xCDu8; 32]);
        client.update_status(&carrier, &shipment_id, &ShipmentStatus::InTransit, &transit_hash);

        (company, receiver, carrier, shipment_id, data_hash)
    }

    // ── Carrier suspension cascade ─────────────────────────────────────────────

    /// Suspending the carrier while a shipment is InTransit prevents the
    /// carrier from calling update_status on that in-flight shipment.
    #[test]
    fn suspend_carrier_blocks_update_status_on_in_flight_shipment() {
        let (env, client, admin) = setup();
        let (_, _, carrier, shipment_id, _) = create_in_transit_shipment(&env, &client, &admin);

        // Confirm the shipment is in-transit before suspension.
        let shipment = client.get_shipment(&shipment_id);
        assert_eq!(shipment.status, ShipmentStatus::InTransit);

        // Suspend the carrier.
        client.suspend_carrier(&admin, &carrier);

        // The carrier must no longer be able to update the shipment status.
        test_utils::advance_past_rate_limit(&env);
        let update_hash = BytesN::from_array(&env, &[0x11u8; 32]);
        let result = client.try_update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::AtCheckpoint,
            &update_hash,
        );
        assert!(
            result.is_err(),
            "suspended carrier must not update status on in-flight shipment"
        );
    }

    /// Suspending the carrier does NOT auto-cancel the in-flight shipment —
    /// the status must remain unchanged until an explicit action resolves it.
    #[test]
    fn suspend_carrier_does_not_auto_cancel_in_flight_shipment() {
        let (env, client, admin) = setup();
        let (_, _, carrier, shipment_id, _) = create_in_transit_shipment(&env, &client, &admin);

        // Suspend the carrier.
        client.suspend_carrier(&admin, &carrier);

        // The shipment must still be InTransit.
        let shipment = client.get_shipment(&shipment_id);
        assert_eq!(
            shipment.status,
            ShipmentStatus::InTransit,
            "carrier suspension must not auto-cancel the in-flight shipment"
        );
    }

    /// After reactivating the carrier, update_status on the same in-flight
    /// shipment must succeed again.
    #[test]
    fn reactivate_carrier_restores_update_status_on_in_flight_shipment() {
        let (env, client, admin) = setup();
        let (_, _, carrier, shipment_id, _) = create_in_transit_shipment(&env, &client, &admin);

        client.suspend_carrier(&admin, &carrier);

        // Confirm the suspension blocked the update.
        test_utils::advance_past_rate_limit(&env);
        let blocked_hash = BytesN::from_array(&env, &[0x22u8; 32]);
        let blocked = client.try_update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::AtCheckpoint,
            &blocked_hash,
        );
        assert!(blocked.is_err());

        // Reactivate and verify the update now succeeds.
        client.reactivate_carrier(&admin, &carrier);
        test_utils::advance_past_rate_limit(&env);
        let ok_hash = BytesN::from_array(&env, &[0x33u8; 32]);
        let result = client.try_update_status(
            &carrier,
            &shipment_id,
            &ShipmentStatus::AtCheckpoint,
            &ok_hash,
        );
        assert!(
            result.is_ok(),
            "reactivated carrier must be able to update status again"
        );
    }

    // ── Company suspension cascade ─────────────────────────────────────────────

    /// Suspending the company (sender) while a shipment is InTransit prevents
    /// the company from creating new shipments.
    #[test]
    fn suspend_company_blocks_new_shipment_creation_while_another_is_in_flight() {
        let (env, client, admin) = setup();
        let (company, _, carrier, shipment_id, _) =
            create_in_transit_shipment(&env, &client, &admin);

        // Confirm the shipment is in-transit.
        assert_eq!(
            client.get_shipment(&shipment_id).status,
            ShipmentStatus::InTransit
        );

        // Suspend the company.
        client.suspend_company(&admin, &company);

        // The company must not create additional shipments.
        let new_hash = BytesN::from_array(&env, &[0x44u8; 32]);
        let new_deadline = test_utils::future_deadline(&env, 3600);
        let new_receiver = Address::generate(&env);
        let result = client.try_create_shipment(
            &company,
            &new_receiver,
            &carrier,
            &new_hash,
            &Vec::new(&env),
            &new_deadline,
        );
        assert!(
            result.is_err(),
            "suspended company must not create new shipments while another is in-flight"
        );
    }

    /// Suspending the company does NOT auto-cancel an in-flight shipment.
    #[test]
    fn suspend_company_does_not_auto_cancel_in_flight_shipment() {
        let (env, client, admin) = setup();
        let (company, _, _, shipment_id, _) = create_in_transit_shipment(&env, &client, &admin);

        client.suspend_company(&admin, &company);

        let shipment = client.get_shipment(&shipment_id);
        assert_eq!(
            shipment.status,
            ShipmentStatus::InTransit,
            "company suspension must not auto-cancel the in-flight shipment"
        );
    }

    /// Suspending the company while a shipment is InTransit blocks the company
    /// from cancelling that same in-flight shipment.
    #[test]
    fn suspend_company_blocks_cancel_of_in_flight_shipment() {
        let (env, client, admin) = setup();
        let (company, _, _, shipment_id, _) = create_in_transit_shipment(&env, &client, &admin);

        client.suspend_company(&admin, &company);

        let cancel_hash = BytesN::from_array(&env, &[0x55u8; 32]);
        let result = client.try_cancel_shipment(&company, &shipment_id, &cancel_hash);
        assert!(
            result.is_err(),
            "suspended company must not cancel an in-flight shipment"
        );
    }

    /// Admin can still force-cancel an in-flight shipment even when the carrier
    /// is suspended — the force path bypasses role checks.
    #[test]
    fn admin_can_force_cancel_in_flight_shipment_after_carrier_suspension() {
        let (env, client, admin) = setup();
        let (_, _, carrier, shipment_id, _) = create_in_transit_shipment(&env, &client, &admin);

        client.suspend_carrier(&admin, &carrier);

        let reason_hash = BytesN::from_array(&env, &[0x66u8; 32]);
        let result = client.try_force_cancel_shipment(&admin, &shipment_id, &reason_hash);
        assert!(
            result.is_ok(),
            "admin must be able to force-cancel an in-flight shipment after carrier suspension"
        );

        let shipment = client.get_shipment(&shipment_id);
        assert_eq!(shipment.status, ShipmentStatus::Cancelled);
    }

    /// Reactivating the company restores the ability to create new shipments.
    #[test]
    fn reactivate_company_restores_create_shipment_after_suspension() {
        let (env, client, admin) = setup();
        let (company, _, carrier, _, _) = create_in_transit_shipment(&env, &client, &admin);

        client.suspend_company(&admin, &company);

        // Confirm blocked
        let new_hash = BytesN::from_array(&env, &[0x77u8; 32]);
        let deadline = test_utils::future_deadline(&env, 3600);
        let receiver = Address::generate(&env);
        assert!(client
            .try_create_shipment(
                &company,
                &receiver,
                &carrier,
                &new_hash,
                &Vec::new(&env),
                &deadline,
            )
            .is_err());

        // Reactivate and verify access is restored
        client.reactivate_company(&admin, &company);
        let new_hash2 = BytesN::from_array(&env, &[0x88u8; 32]);
        let deadline2 = test_utils::future_deadline(&env, 3600);
        let result = client.try_create_shipment(
            &company,
            &receiver,
            &carrier,
            &new_hash2,
            &Vec::new(&env),
            &deadline2,
        );
        assert!(
            result.is_ok(),
            "reactivated company must be able to create new shipments"
        );
    }
}
