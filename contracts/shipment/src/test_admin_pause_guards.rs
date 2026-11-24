//! Tests for issue #752 — the pause switch must gate the admin-transfer and
//! multi-sig proposal surface, not just ordinary role management.

#[cfg(test)]
mod tests {
    use crate::{test_utils::setup_multisig, NavinError};
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

    fn admin_list(env: &Env, a: &Address, b: &Address) -> Vec<Address> {
        let mut admins = Vec::new(env);
        admins.push_back(a.clone());
        admins.push_back(b.clone());
        admins
    }

    // ── #752: pause gates the admin and multi-sig surface ───────────────────

    #[test]
    fn paused_contract_rejects_transfer_admin() {
        let (env, client, admin, _admin2) = setup_multisig();
        let successor = Address::generate(&env);

        client.pause(&admin);

        assert_eq!(
            client.try_transfer_admin(&admin, &successor),
            Err(Ok(NavinError::ContractPaused))
        );
    }

    #[test]
    fn paused_contract_rejects_accept_admin_transfer() {
        let (env, client, admin, _admin2) = setup_multisig();
        let successor = Address::generate(&env);

        client.transfer_admin(&admin, &successor);
        client.pause(&admin);

        assert_eq!(
            client.try_accept_admin_transfer(&successor),
            Err(Ok(NavinError::ContractPaused))
        );
    }

    #[test]
    fn paused_contract_rejects_init_multisig() {
        let (env, client, admin, admin2) = setup_multisig();
        let admins = admin_list(&env, &admin, &admin2);

        client.pause(&admin);

        assert_eq!(
            client.try_init_multisig(&admin, &admins, &2),
            Err(Ok(NavinError::ContractPaused))
        );
    }

    #[test]
    fn paused_contract_rejects_propose_approve_and_execute() {
        let (env, client, admin, admin2) = setup_multisig();
        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));

        // A live proposal created before the pause must not advance during it.
        let proposal_id = client.propose_action(&admin, &action);

        client.pause(&admin);

        assert_eq!(
            client.try_propose_action(&admin, &action),
            Err(Ok(NavinError::ContractPaused))
        );
        assert_eq!(
            client.try_approve_action(&admin2, &proposal_id),
            Err(Ok(NavinError::ContractPaused))
        );
        assert_eq!(
            client.try_execute_proposal(&proposal_id),
            Err(Ok(NavinError::ContractPaused))
        );

        // Unpausing restores the flow — the pause is a hold, not a lockout.
        client.unpause(&admin);
        client.approve_action(&admin2, &proposal_id);
    }

    #[test]
    fn paused_contract_rejects_propose_action_with_salt() {
        let (env, client, admin, _admin2) = setup_multisig();
        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));
        let salt = BytesN::from_array(&env, &[7_u8; 32]);

        client.pause(&admin);

        assert_eq!(
            client.try_propose_action_with_salt(&admin, &action, &salt),
            Err(Ok(NavinError::ContractPaused))
        );
    }

    // ── #890 / #891: pause gates circuit-breaker admin surface ──────

    #[test]
    fn paused_contract_rejects_reset_circuit_breaker() {
        let (_env, client, admin, _admin2) = setup_multisig();
        client.pause(&admin);

        assert_eq!(
            client.try_reset_circuit_breaker(&admin),
            Err(Ok(NavinError::ContractPaused))
        );
    }

    #[test]
    fn paused_contract_rejects_set_circuit_breaker_config() {
        let (_env, client, admin, _admin2) = setup_multisig();
        client.pause(&admin);

        assert_eq!(
            client.try_set_circuit_breaker_config(
                &admin,
                &crate::circuit_breaker::CircuitBreakerPreset::Strict
            ),
            Err(Ok(NavinError::ContractPaused))
        );
    }
}
