//! Tests for issue #752 — the pause switch must gate the admin-transfer and
//! multi-sig proposal surface, not just ordinary role management.

#[cfg(test)]
mod tests {
    use crate::{test_utils, NavinError, NavinShipment, NavinShipmentClient};
    use soroban_sdk::{
        contract, contractimpl, testutils::Address as _, Address, BytesN, Env, Vec,
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

    /// Initialised contract with a two-admin, threshold-2 multi-sig.
    fn setup_multisig() -> (Env, NavinShipmentClient<'static>, Address, Address) {
        let (env, admin) = test_utils::setup_env();
        let contract_id = env.register(NavinShipment, ());
        let client = NavinShipmentClient::new(&env, &contract_id);
        let token_id = env.register(MockToken, ());
        client.initialize(&admin, &token_id);

        let admin2 = Address::generate(&env);
        let mut admins = Vec::new(&env);
        admins.push_back(admin.clone());
        admins.push_back(admin2.clone());
        client.init_multisig(&admin, &admins, &2);

        (env, client, admin, admin2)
    }

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
}
