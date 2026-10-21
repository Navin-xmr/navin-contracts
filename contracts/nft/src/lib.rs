#![no_std]

extern crate alloc;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, vec, Address, Env, IntoVal,
    String, Symbol, Vec,
};

mod errors;
mod storage;

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
enum NftKey {
    Owner(u64),
    OwnerTokens(Address),
    BurnApproved(u64),
    TokenCount,
    Admin,
    Name,
    Symbol,
    ProposedAdmin,
    Initialized,
    Paused,
}

// ── Errors ──────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NftError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAdmin = 3,
    TokenDoesNotExist = 4,
    NotOwner = 5,
    TokenAlreadyMinted = 6,
    MintToContract = 7,
    ContractPaused = 8,
    InvalidNameOrSymbol = 9,
    BurnNotApproved = 10,
    TransferNotAllowed = 11,
    NoProposedAdmin = 12,
    SameAdmin = 13,
}

// ── TTL ─────────────────────────────────────────────────────────────────────

#[cfg(any(test, feature = "testutils"))]
pub mod test_utils;

#[contract]
pub struct NavinNft;

#[contractimpl]
impl NavinShipmentNft {
    /// Extend instance storage TTL to prevent premature contract expiration
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTENSION);
    }

    /// Extend persistent storage TTL for a specific owner's token list
    fn extend_persistent_ttl(env: &Env, owner: &Address) {
        let key = NftKey::OwnerTokens(owner.clone());
        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTENSION);
        }
    }

    pub fn initialize(
        env: Env,
        admin: Address,
        name: String,
        symbol: String,
    ) -> Result<(), NftError> {
        admin.require_auth();

        if env.storage().instance().has(&NftKey::Initialized) {
            return Err(NftError::AlreadyInitialized);
        }
        if name.is_empty() || symbol.is_empty() {
            return Err(NftError::InvalidNameOrSymbol);
        }
        env.storage().instance().set(&NftKey::Admin, &admin);
        env.storage().instance().set(&NftKey::Name, &name);
        env.storage().instance().set(&NftKey::Symbol, &symbol);
        env.storage().instance().set(&NftKey::TokenCount, &0_u64);
        env.storage().instance().set(&NftKey::Initialized, &true);
        env.events().publish(
            (event_topics::INIT, event_topics::EVENT_SCHEMA_VERSION),
            (admin, name, symbol, env.ledger().timestamp()),
        );
        Self::extend_instance_ttl(&env);
        Ok(())
    }

    /// Transfers a token and invokes the receiver interface for contract recipients.
    ///
    /// Re-minting after burn: `burn` removes the `Owner(token_id)` entry, so a
    /// burned `token_id` counts as unminted and may be minted again (by the
    /// admin). This is intentional — only a *currently owned* id is rejected
    /// with `TokenAlreadyMinted`.
    pub fn mint(env: Env, to: Address, token_id: u64) -> Result<u64, NftError> {
        Self::require_not_paused(&env)?;
        Self::require_admin(&env)?;

        if to == env.current_contract_address() {
            return Err(NftError::MintToContract);
        }

        if env.storage().persistent().has(&NftKey::Owner(token_id)) {
            return Err(NftError::TokenAlreadyMinted);
        }

        env.storage()
            .persistent()
            .set(&NftKey::Owner(token_id), &to);

        Self::add_token_to_owner(&env, &to, token_id);

        let count: u64 = env
            .storage()
            .instance()
            .get(&NftKey::TokenCount)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&NftKey::TokenCount, &(count + 1));

        env.events().publish(
            (event_topics::MINT, event_topics::EVENT_SCHEMA_VERSION),
            (token_id, to.clone(), env.ledger().timestamp()),
        );
        Self::extend_instance_ttl(&env);
        Self::extend_persistent_ttl(&env, &to);
        Ok(token_id)
    }

    /// Contract recipients must expose `can_receive_nft(token_id) -> bool`
    /// and return `true`; account recipients do not need a callback.
    pub fn transfer(env: Env, from: Address, to: Address, token_id: u64) -> Result<(), NftError> {
        Self::require_not_paused(&env)?;
        from.require_auth();

        let owner = Self::get_owner_inner(&env, token_id)?;
        if owner != from {
            return Err(NftError::NotOwner);
        }
        if admin != storage::get_admin(&env) {
            return Err(NftError::Unauthorized);
        }

        if Self::is_contract_address(&to) {
            let result = env.try_invoke_contract::<bool, soroban_sdk::Error>(
                &to,
                &Symbol::new(&env, "can_receive_nft"),
                vec![&env, token_id.into_val(&env)],
            );
            if !matches!(result, Ok(Ok(true))) {
                return Err(NftError::TransferNotAllowed);
            }
        }

        env.storage()
            .persistent()
            .set(&NftKey::Owner(token_id), &to);

        Self::remove_token_from_owner(&env, &from, token_id);
        Self::add_token_to_owner(&env, &to, token_id);

        if Self::is_contract_address(&to) {
            let supports_receiver = env.try_invoke_contract::<bool, soroban_sdk::Error>(
                &to,
                &Symbol::new(&env, "supports_nft_receiver"),
                Vec::new(&env),
            );
            if !matches!(supports_receiver, Ok(Ok(true))) {
                return Err(NftError::TransferRejected);
            }

            let callback_args: Vec<Val> = (from.clone(), token_id, data).into_val(&env);
            let callback_result = env.try_invoke_contract::<(), soroban_sdk::Error>(
                &to,
                &Symbol::new(&env, "on_nft_received"),
                callback_args,
            );
            if !matches!(callback_result, Ok(Ok(()))) {
                return Err(NftError::TransferRejected);
            }
        }

        env.events().publish(
            (event_topics::TRANSFER, event_topics::EVENT_SCHEMA_VERSION),
            (token_id, from.clone(), to.clone(), env.ledger().timestamp()),
        );

        Self::extend_instance_ttl(&env);
        Self::extend_persistent_ttl(&env, &from);
        Self::extend_persistent_ttl(&env, &to);
        Ok(())
    }

    fn is_contract_address(address: &Address) -> bool {
        // Soroban StrKeys use `C` for contracts and `G` for accounts.
        let mut strkey = [0_u8; 56];
        address.to_string().copy_into_slice(&mut strkey);
        strkey[0] == b'C'
    }

    pub fn burn(env: Env, caller: Address, token_id: u64) -> Result<(), NftError> {
        Self::require_not_paused(&env)?;
        caller.require_auth();

        let owner = Self::get_owner_inner(&env, token_id)?;
        let admin = Self::get_admin_inner(&env)?;

        if caller != owner && caller != admin {
            return Err(NftError::NotOwner);
        }
        if admin != storage::get_admin(&env) {
            return Err(NftError::Unauthorized);
        }
        if storage::get_owner(&env, token_id).is_some() {
            return Err(NftError::AlreadyMinted);
        }

        env.events().publish(
            (event_topics::BURN, event_topics::EVENT_SCHEMA_VERSION),
            (token_id, caller, env.ledger().timestamp()),
        );
        Self::extend_instance_ttl(&env);
        Ok(())
    }

    /// Nominate a new admin. The current admin must authorize the proposal.
    pub fn propose_admin_transfer(env: Env, new_admin: Address) -> Result<(), NftError> {
        let admin = Self::require_admin(&env)?;
        if admin == new_admin {
            return Err(NftError::SameAdmin);
        }

        env.storage()
            .instance()
            .set(&NftKey::ProposedAdmin, &new_admin);
        env.events().publish(
            (
                event_topics::ADMIN_PROPOSED,
                event_topics::EVENT_SCHEMA_VERSION,
            ),
            (admin, new_admin, env.ledger().timestamp()),
        );
        Self::extend_instance_ttl(&env);
        Ok(())
    }

    /// Accept the pending admin transfer. The proposed admin must authorize it.
    pub fn accept_admin_transfer(env: Env) -> Result<(), NftError> {
        let old_admin = Self::get_admin_inner(&env)?;
        let proposed_admin: Address = env
            .storage()
            .instance()
            .get(&NftKey::ProposedAdmin)
            .ok_or(NftError::NoProposedAdmin)?;
        proposed_admin.require_auth();

        env.storage()
            .instance()
            .set(&NftKey::Admin, &proposed_admin);
        env.storage().instance().remove(&NftKey::ProposedAdmin);
        env.events().publish(
            (
                event_topics::ADMIN_TRANSFER,
                event_topics::EVENT_SCHEMA_VERSION,
            ),
            (old_admin, proposed_admin, env.ledger().timestamp()),
        );
        Self::extend_instance_ttl(&env);
        Ok(())
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), NftError> {
        admin.require_auth();
        storage::set_owner(&env, token_id, &to);
        if let Some(uri) = metadata_uri {
            storage::set_metadata_uri(&env, token_id, &uri);
        }
        Ok(())
    }

    pub fn owner_of(env: Env, token_id: u64) -> Result<Address, NftError> {
        storage::get_owner(&env, token_id).ok_or(NftError::TokenNotFound)
    }

    pub fn get_token_uri(env: Env, token_id: u64) -> Result<String, NftError> {
        if storage::get_owner(&env, token_id).is_none() {
            return Err(NftError::TokenNotFound);
        }

        if let Some(uri) = storage::get_metadata_uri(&env, token_id) {
            return Ok(uri);
        }

        let base_uri = storage::get_base_uri(&env).ok_or(NftError::BaseUriNotSet)?;
        Ok(append_token_id(&env, &base_uri, token_id))
    }
}

    pub fn name(env: Env) -> Result<String, NftError> {
        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&NftKey::Name)
            .ok_or(NftError::NotInitialized)
    }

    pub fn nft_symbol(env: Env) -> Result<String, NftError> {
        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&NftKey::Symbol)
            .ok_or(NftError::NotInitialized)
    }

#[cfg(test)]
mod test_auth;
#[cfg(test)]
mod test_events;
#[cfg(test)]
mod test_utils;

    pub fn get_proposed_admin(env: Env) -> Result<Address, NftError> {
        Self::extend_instance_ttl(&env);
        Self::get_admin_inner(&env)?;
        env.storage()
            .instance()
            .get(&NftKey::ProposedAdmin)
            .ok_or(NftError::NoProposedAdmin)
    }

    fn get_owner_inner(env: &Env, token_id: u64) -> Result<Address, NftError> {
        env.storage()
            .persistent()
            .get(&NftKey::Owner(token_id))
            .ok_or(NftError::TokenDoesNotExist)
    }

    #[contract]
    struct NftReceiver;

    #[contracttype]
    enum ReceiverKey {
        LastReceivedToken,
        LastReceivedData,
    }

    #[contractimpl]
    impl NftReceiver {
        pub fn supports_nft_receiver(_env: Env) -> bool {
            true
        }

    fn is_contract_address(address: &Address) -> bool {
        let strkey = address.to_string();
        let mut bytes = [0_u8; 56];
        strkey.copy_into_slice(&mut bytes);
        // StrKeys use C for contract addresses and G for account addresses.
        bytes[0] == b'C'
    }

    fn add_token_to_owner(env: &Env, owner: &Address, token_id: u64) {
        let mut tokens: Vec<u64> = env
            .storage()
            .persistent()
            .get(&NftKey::OwnerTokens(owner.clone()))
            .unwrap_or_else(|| vec![env]);
        tokens.push_back(token_id);
        env.storage()
            .persistent()
            .set(&NftKey::OwnerTokens(owner.clone()), &tokens);
    }

    fn remove_token_from_owner(env: &Env, owner: &Address, token_id: u64) {
        let tokens: Vec<u64> = env
            .storage()
            .persistent()
            .get(&NftKey::OwnerTokens(owner.clone()))
            .unwrap_or_else(|| vec![env]);
        let mut new_tokens: Vec<u64> = vec![env];
        for id in tokens.iter() {
            if id != token_id {
                new_tokens.push_back(id);
            }
        }

        pub fn last_received_token(env: Env) -> u64 {
            env.storage()
                .instance()
                .get(&ReceiverKey::LastReceivedToken)
                .unwrap_or(0)
        }

        pub fn last_received_data(env: Env) -> Vec<Val> {
            env.storage()
                .instance()
                .get(&ReceiverKey::LastReceivedData)
                .unwrap_or_else(|| Vec::new(&env))
        }
    }

    #[contract]
    struct NonReceiver;

#[cfg(test)]
pub(crate) mod test_receivers {
    use soroban_sdk::{contract, contractimpl, Env};

    #[contract]
    pub struct AcceptingReceiver;

    #[contractimpl]
    impl AcceptingReceiver {
        pub fn can_receive_nft(_env: Env, _token_id: u64) -> bool {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::test_receivers::AcceptingReceiver;
    use soroban_sdk::testutils::Address as _;

    #[contract]
    struct ReceiverWithoutCallback;

    #[contractimpl]
    impl ReceiverWithoutCallback {
        pub fn unrelated(_env: Env) {}
    }

    fn setup() -> (Env, Address, NavinShipmentNftClient<'static>) {
        let (env, admin) = test_utils::setup_env();
        let contract_id = env.register(NavinShipmentNft, ());
        let client = NavinShipmentNftClient::new(&env, &contract_id);
        client.initialize(
            &admin,
            &String::from_str(&env, "NavinNFT"),
            &String::from_str(&env, "NNFT"),
        );
        (env, admin, client)
    }

    #[test]
    fn test_initialize_and_metadata() {
        let (env, admin, client) = setup();
        assert_eq!(client.name(), String::from_str(&env, "NavinNFT"));
        assert_eq!(client.nft_symbol(), String::from_str(&env, "NNFT"));
        assert_eq!(client.get_admin(), admin);
    }

    #[test]
    fn test_nft_error_equality() {
        assert_eq!(NftError::NotOwner, NftError::NotOwner);
        assert_ne!(NftError::NotOwner, NftError::NotAdmin);
    }

    #[test]
    fn test_admin_transfer_requires_proposal_and_acceptance() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);

        client.propose_admin_transfer(&new_admin);
        assert_eq!(client.get_admin(), admin);
        assert_eq!(client.get_proposed_admin(), new_admin);

        client.accept_admin_transfer();
        assert_eq!(client.get_admin(), new_admin);
        assert_eq!(
            client.try_get_proposed_admin(),
            Err(Ok(NftError::NoProposedAdmin))
        );
    }

    #[test]
    fn test_long_metadata_is_preserved() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(NavinShipmentNft, ());
        let client = NavinShipmentNftClient::new(&env, &contract_id);
        env.mock_all_auths();
        let name = String::from_str(&env, "A collection name longer than twenty-eight bytes");
        let symbol = String::from_str(&env, "A-symbol-longer-than-28-bytes");

        client.initialize(&admin, &name, &symbol);

        assert_eq!(client.name(), name);
        assert_eq!(client.nft_symbol(), symbol);
    }

    #[test]
    fn test_double_initialize_fails() {
        let (env, admin, client) = setup();
        let result = client.try_initialize(
            &admin,
            &String::from_str(&env, "X"),
            &String::from_str(&env, "X"),
        );
        assert!(result.is_err());
        let _ = env;
    }

    #[test]
    fn test_initialize_unauthenticated_fails() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(NavinShipmentNft, ());
        let client = NavinShipmentNftClient::new(&env, &contract_id);
        // Do not mock auths - should fail auth requirement
        let result = client.try_initialize(
            &admin,
            &String::from_str(&env, "NavinNFT"),
            &String::from_str(&env, "NNFT"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_legacy_symbol_metadata_arguments_are_rejected() {
        use soroban_sdk::IntoVal;

        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(NavinShipmentNft, ());
        env.mock_all_auths();
        let args = vec![
            &env,
            admin.into_val(&env),
            symbol_short!("NavinNFT").into_val(&env),
            symbol_short!("NNFT").into_val(&env),
        ];

        let result = env.try_invoke_contract::<(), soroban_sdk::Error>(
            &contract_id,
            &Symbol::new(&env, "initialize"),
            args,
        );

        assert!(!matches!(result, Ok(Ok(()))));
    }

    #[test]
    fn test_mint_and_owner() {
        let (env, _admin, client) = setup();
        let recipient = Address::generate(&env);
        let token_id = client.mint(&recipient, &42);
        assert_eq!(token_id, 42);
        assert_eq!(client.owner_of(&42), recipient);
        assert_eq!(client.total_supply(), 1);
    }

    #[test]
    fn test_mint_duplicate_fails() {
        let (env, _admin, client) = setup();
        let recipient = Address::generate(&env);
        client.mint(&recipient, &1);
        let result = client.try_mint(&recipient, &1);
        assert_eq!(result, Err(Ok(NftError::TokenAlreadyMinted)));
    }

    #[test]
    fn test_transfer() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        let bob = env.register(AcceptingReceiver, ());
        client.mint(&alice, &1);
        client.transfer(&alice, &bob, &1, &Vec::new(&env));
        assert_eq!(client.owner_of(&1), bob);
        assert_eq!(client.balance_of(&alice), 0);
        assert_eq!(client.balance_of(&bob), 1);
    }

    #[test]
    fn test_transfer_to_contract_without_receiver_callback_fails() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        let receiver = env.register(ReceiverWithoutCallback, ());
        client.mint(&alice, &1);

        let result = client.try_transfer(&alice, &receiver, &1);

        assert_eq!(result, Err(Ok(NftError::TransferNotAllowed)));
        assert_eq!(client.owner_of(&1), alice);
        assert_eq!(client.balance_of(&receiver), 0);
    }

    #[test]
    fn test_transfer_not_owner_fails() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.mint(&alice, &1);
        let result = client.try_transfer(&bob, &alice, &1, &Vec::new(&env));
        assert!(result.is_err());
    }

    #[test]
    fn test_transfer_to_contract_with_callback() {
        let (env, _admin, client) = setup();
        let owner = Address::generate(&env);
        let receiver_id = env.register(NftReceiver, ());
        let receiver = NftReceiverClient::new(&env, &receiver_id);
        client.mint(&owner, &1);

        let data = vec![&env, 7_u32.into_val(&env)];
        client.transfer(&owner, &receiver_id, &1, &data);

        assert_eq!(client.owner_of(&1), receiver_id);
        assert_eq!(receiver.last_received_token(), 1);
        assert_eq!(receiver.last_received_data(), data);
    }

    #[test]
    fn test_transfer_to_contract_without_callback() {
        let (env, _admin, client) = setup();
        let owner = Address::generate(&env);
        let receiver_id = env.register(NonReceiver, ());
        client.mint(&owner, &1);

        let result = client.try_transfer(&owner, &receiver_id, &1, &Vec::new(&env));
        assert_eq!(result, Err(Ok(NftError::TransferRejected)));
        assert_eq!(client.owner_of(&1), owner);
    }

    #[test]
    fn test_burn_by_owner() {
        let (env, _admin, client) = setup();
        let owner = Address::generate(&env);
        client.mint(&owner, &1);
        assert_eq!(client.total_supply(), 1);
        assert_eq!(client.balance_of(&owner), 1);
        client.approve_burn(&1);
        client.burn(&owner, &1);
        assert_eq!(client.total_supply(), 0);
        assert_eq!(client.balance_of(&owner), 0);
        assert!(client.try_owner_of(&1).is_err());
    }

    #[test]
    fn test_burn_by_admin() {
        let (env, admin, client) = setup();
        let owner = Address::generate(&env);
        client.mint(&owner, &1);
        client.burn(&admin, &1);
        assert_eq!(client.total_supply(), 0);
        assert_eq!(client.balance_of(&owner), 0);
    }

    #[test]
    fn test_burn_unauthenticated_fails() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let contract_id = env.register(NavinShipmentNft, ());
        let client = NavinShipmentNftClient::new(&env, &contract_id);

        // Set up contract under mock auths
        env.mock_all_auths();
        client.initialize(
            &admin,
            &String::from_str(&env, "NavinNFT"),
            &String::from_str(&env, "NNFT"),
        );
        client.mint(&owner, &100);

        // Now test burn in a clean env where mock_all_auths is NOT active
        let env_no_auth = Env::default();
        let admin2 = Address::generate(&env_no_auth);
        let owner2 = Address::generate(&env_no_auth);
        let cid = env_no_auth.register(NavinShipmentNft, ());
        let client_no_auth = NavinShipmentNftClient::new(&env_no_auth, &cid);

        // Mock auths only to set up state
        env_no_auth.mock_all_auths();
        client_no_auth.initialize(
            &admin2,
            &String::from_str(&env_no_auth, "NavinNFT"),
            &String::from_str(&env_no_auth, "NNFT"),
        );
        client_no_auth.mint(&owner2, &100);

        // Disable/mock auths selectively using mock_auths or test without mock_all_auths
        // Create a new env without mock_all_auths and call try_burn
        let env_strict = Env::default();
        let cid_strict = env_strict.register(NavinShipmentNft, ());
        let client_strict = NavinShipmentNftClient::new(&env_strict, &cid_strict);

        // Since auth is required as the first line of burn, calling try_burn without mock_all_auths fails auth
        let caller = Address::generate(&env_strict);
        let res = client_strict.try_burn(&caller, &100);
        assert!(res.is_err());
    }

    #[test]
    fn test_double_burn_fails() {
        let (env, _admin, client) = setup();
        let owner = Address::generate(&env);
        client.mint(&owner, &1);
        client.approve_burn(&1);
        client.burn(&owner, &1);
        let result = client.try_burn(&owner, &1);
        assert_eq!(result, Err(Ok(NftError::TokenDoesNotExist)));
        assert_eq!(client.total_supply(), 0);
    }

    #[test]
    fn test_remint_after_burn_succeeds() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.mint(&alice, &1);
        client.approve_burn(&1);
        client.burn(&alice, &1);

        // A burned id is free again (see the doc comment on `mint`).
        assert_eq!(client.mint(&bob, &1), 1);
        assert_eq!(client.owner_of(&1), bob);
        assert_eq!(client.balance_of(&alice), 0);
        assert_eq!(client.balance_of(&bob), 1);
        assert_eq!(client.total_supply(), 1);
    }

    #[test]
    fn test_burn_by_unrelated_caller_fails() {
        let (env, _admin, client) = setup();
        let owner = Address::generate(&env);
        let stranger = Address::generate(&env);
        client.mint(&owner, &1);
        let result = client.try_burn(&stranger, &1);
        assert_eq!(result, Err(Ok(NftError::NotOwner)));
        assert_eq!(client.owner_of(&1), owner);
        assert_eq!(client.total_supply(), 1);
    }

    #[test]
    fn test_balance_of() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.mint(&alice, &1);
        client.mint(&alice, &2);
        client.mint(&bob, &3);
        assert_eq!(client.balance_of(&alice), 2);
        assert_eq!(client.balance_of(&bob), 1);
    }

    #[test]
    fn test_balance_of_non_sequential_ids() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        client.mint(&alice, &42);
        client.mint(&alice, &9999);
        assert_eq!(client.balance_of(&alice), 2);
        assert_eq!(client.total_supply(), 2);
    }

    #[test]
    fn test_tokens_of_owner_tracks_mint() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        assert_eq!(client.tokens_of_owner(&alice), vec![&env]);
        client.mint(&alice, &7);
        client.mint(&alice, &42);
        assert_eq!(client.tokens_of_owner(&alice), vec![&env, 7_u64, 42_u64]);
    }

    #[test]
    fn test_tokens_of_owner_moves_on_transfer() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        let bob = env.register(AcceptingReceiver, ());
        client.mint(&alice, &1);
        client.mint(&alice, &2);
        client.transfer(&alice, &bob, &1, &Vec::new(&env));
        assert_eq!(client.tokens_of_owner(&alice), vec![&env, 2_u64]);
        assert_eq!(client.tokens_of_owner(&bob), vec![&env, 1_u64]);
    }

    #[test]
    fn test_tokens_of_owner_pruned_on_burn() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        client.mint(&alice, &1);
        client.mint(&alice, &2);
        client.approve_burn(&1);
        client.burn(&alice, &1);
        assert_eq!(client.tokens_of_owner(&alice), vec![&env, 2_u64]);
        client.approve_burn(&2);
        client.burn(&alice, &2);
        assert_eq!(client.tokens_of_owner(&alice), vec![&env]);
        assert_eq!(client.balance_of(&alice), 0);
    }

    #[test]
    fn test_token_not_found() {
        let (env, _, client) = setup();
        assert!(client.try_owner_of(&999).is_err());
        let _ = env;
    }

    #[test]
    fn test_self_transfer_noop() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        client.mint(&alice, &1);
        client.transfer(&alice, &alice, &1, &Vec::new(&env));
        assert_eq!(client.owner_of(&1), alice);
    }

    #[test]
    fn test_initialize_with_empty_name_fails() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(NavinShipmentNft, ());
        let client = NavinShipmentNftClient::new(&env, &contract_id);
        env.mock_all_auths();

        let res1 = client.try_initialize(
            &admin,
            &String::from_str(&env, ""),
            &String::from_str(&env, "NNFT"),
        );
        assert_eq!(res1, Err(Ok(NftError::InvalidNameOrSymbol)));
    }

    #[test]
    fn test_initialize_with_empty_symbol_fails() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(NavinShipmentNft, ());
        let client = NavinShipmentNftClient::new(&env, &contract_id);
        env.mock_all_auths();

        let res2 = client.try_initialize(
            &admin,
            &String::from_str(&env, "NavinNFT"),
            &String::from_str(&env, ""),
        );
        assert_eq!(res2, Err(Ok(NftError::InvalidNameOrSymbol)));
    }
}
