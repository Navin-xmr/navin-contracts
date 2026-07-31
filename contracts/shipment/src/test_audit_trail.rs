//! Integration tests proving that role and permission mutations (issue #633)
//! write to the audit trail declared in `audit.rs`, and that the trail is
//! reachable through the contract's public `query_audit_history*` interface.

#[cfg(test)]
mod tests {
    use crate::audit::AuditEventType;
    use crate::test_utils::setup_env;
    use crate::{NavinShipment, NavinShipmentClient};
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env};

    #[contract]
    struct MockToken;

    #[contractimpl]
    impl MockToken {
        pub fn decimals(_env: Env) -> u32 {
            7
        }

        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    }

    fn setup_test_env() -> (Env, NavinShipmentClient<'static>, Address) {
        let (env, admin) = setup_env();
        let token = env.register(MockToken {}, ());
        let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
        client.initialize(&admin, &token);
        (env, client, admin)
    }

    // ── Assign ───────────────────────────────────────────────────────────────

    #[test]
    fn test_add_company_writes_audit_entry() {
        let (env, client, admin) = setup_test_env();
        let company = Address::generate(&env);

        client.add_company(&admin, &company);

        let entries = client.query_audit_history_for_target(&company);
        assert_eq!(entries.len(), 1);
        let entry = entries.get(0).unwrap();
        assert_eq!(entry.event_type, AuditEventType::RoleAssigned);
        assert_eq!(entry.actor, admin);
        assert_eq!(entry.target, company);
    }

    #[test]
    fn test_add_carrier_writes_audit_entry() {
        let (env, client, admin) = setup_test_env();
        let carrier = Address::generate(&env);

        client.add_carrier(&admin, &carrier);

        let entries = client.query_audit_history_for_target(&carrier);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries.get(0).unwrap().event_type, AuditEventType::RoleAssigned);
    }

    // ── Revoke ───────────────────────────────────────────────────────────────

    #[test]
    fn test_revoke_role_writes_audit_entry() {
        let (env, client, admin) = setup_test_env();
        let company = Address::generate(&env);
        client.add_company(&admin, &company);

        client.revoke_role(&admin, &company);

        let entries = client.query_audit_history_for_target(&company);
        // One entry for the assignment, one for the revocation.
        assert_eq!(entries.len(), 2);
        assert_eq!(entries.get(0).unwrap().event_type, AuditEventType::RoleAssigned);
        assert_eq!(entries.get(1).unwrap().event_type, AuditEventType::RoleRevoked);
        assert_eq!(entries.get(1).unwrap().actor, admin);
        assert_eq!(entries.get(1).unwrap().target, company);
    }

    // ── Suspend ──────────────────────────────────────────────────────────────

    #[test]
    fn test_suspend_role_writes_audit_entry() {
        let (env, client, admin) = setup_test_env();
        let carrier = Address::generate(&env);
        client.add_carrier(&admin, &carrier);

        client.suspend_role(&admin, &carrier);

        let entries = client.query_audit_history_for_target(&carrier);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries.get(1).unwrap().event_type, AuditEventType::RoleSuspended);
        assert_eq!(entries.get(1).unwrap().actor, admin);
        assert_eq!(entries.get(1).unwrap().target, carrier);
    }

    // ── Reactivate ───────────────────────────────────────────────────────────

    #[test]
    fn test_reactivate_role_writes_audit_entry() {
        let (env, client, admin) = setup_test_env();
        let carrier = Address::generate(&env);
        client.add_carrier(&admin, &carrier);
        client.suspend_role(&admin, &carrier);

        client.reactivate_role(&admin, &carrier);

        let entries = client.query_audit_history_for_target(&carrier);
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries.get(2).unwrap().event_type,
            AuditEventType::RoleReactivated
        );
        assert_eq!(entries.get(2).unwrap().actor, admin);
        assert_eq!(entries.get(2).unwrap().target, carrier);
    }

    // ── Admin transfer ───────────────────────────────────────────────────────

    #[test]
    fn test_accept_admin_transfer_writes_audit_entry() {
        let (env, client, admin) = setup_test_env();
        let new_admin = Address::generate(&env);

        client.transfer_admin(&admin, &new_admin);
        // Proposing must not log a transfer yet — it hasn't happened.
        assert_eq!(client.query_audit_history_for_target(&new_admin).len(), 0);

        client.accept_admin_transfer(&new_admin);

        let entries = client.query_audit_history_for_target(&new_admin);
        assert_eq!(entries.len(), 1);
        let entry = entries.get(0).unwrap();
        assert_eq!(entry.event_type, AuditEventType::AdminTransferred);
        assert_eq!(entry.actor, admin);
        assert_eq!(entry.target, new_admin);
    }

    // ── Carrier whitelist ────────────────────────────────────────────────────

    #[test]
    fn test_add_carrier_to_whitelist_writes_audit_entry() {
        let (env, client, admin) = setup_test_env();
        let carrier = Address::generate(&env);
        // `admin` is registered with the Company role at initialize() time,
        // and add_carrier_to_whitelist requires the caller to hold that role.
        client.add_carrier(&admin, &carrier);

        client.add_carrier_to_whitelist(&admin, &carrier);

        let entries = client.query_audit_history_for_target(&carrier);
        // add_carrier assignment + the whitelist entry.
        assert_eq!(entries.len(), 2);
        let entry = entries.get(1).unwrap();
        assert_eq!(entry.event_type, AuditEventType::CarrierWhitelisted);
        assert_eq!(entry.actor, admin);
        assert_eq!(entry.target, carrier);
    }

    // ── Query surface ────────────────────────────────────────────────────────

    #[test]
    fn test_query_audit_history_by_actor_filters_correctly() {
        let (env, client, admin) = setup_test_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);

        client.add_company(&admin, &company);
        client.add_carrier(&admin, &carrier);

        let entries = client.query_audit_history_by_actor(&admin);
        assert_eq!(entries.len(), 2);
        for e in entries.iter() {
            assert_eq!(e.actor, admin);
        }
    }

    #[test]
    fn test_query_audit_history_time_range_filters_correctly() {
        let (env, client, admin) = setup_test_env();
        let company = Address::generate(&env);

        let before = env.ledger().timestamp();
        client.add_company(&admin, &company);
        let after = env.ledger().timestamp();

        let in_range = client.query_audit_history(&before, &after);
        assert_eq!(in_range.len(), 1);

        let out_of_range = client.query_audit_history(&0, &(before - 1));
        assert_eq!(out_of_range.len(), 0);
    }

    #[test]
    fn test_role_never_assigned_has_no_audit_history() {
        let (env, client, _admin) = setup_test_env();
        let untouched = Address::generate(&env);

        assert_eq!(client.query_audit_history_for_target(&untouched).len(), 0);
    }
}
