//! Tests for issue #753 — `init_multisig` must not reset the proposal counter
//! out from under a proposal that is still in flight.

#[cfg(test)]
mod tests {
    use crate::{test_utils, NavinError, NavinShipment, NavinShipmentClient};
    use soroban_sdk::{
        contract, contractimpl, testutils::Address as _, Address, Env, Vec,
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

    // ── #753: re-initialising multi-sig cannot orphan a live proposal ───────

    #[test]
    fn init_multisig_rejected_while_a_proposal_is_pending() {
        let (env, client, admin, admin2) = setup_multisig();
        let action = crate::types::AdminAction::TransferAdmin(Address::generate(&env));

        let proposal_id = client.propose_action(&admin, &action);
        let admins = admin_list(&env, &admin, &admin2);

        // Re-initialising here would reset the proposal counter and hand this
        // live proposal's id to some future, unrelated action.
        assert_eq!(
            client.try_init_multisig(&admin, &admins, &2),
            Err(Ok(NavinError::MultiSigProposalPending))
        );

        // The pending proposal survives the rejected re-init untouched.
        client.approve_action(&admin2, &proposal_id);
    }

    #[test]
    fn init_multisig_allowed_once_the_pending_proposal_is_executed() {
        let (env, client, admin, admin2) = setup_multisig();
        let successor = Address::generate(&env);
        let action = crate::types::AdminAction::TransferAdmin(successor.clone());

        let proposal_id = client.propose_action(&admin, &action);
        client.approve_action(&admin2, &proposal_id);

        // Threshold met and executed, so nothing is left to orphan.
        let admins = admin_list(&env, &successor, &admin2);
        client.init_multisig(&successor, &admins, &2);
    }
}
