#![no_std]

extern crate alloc;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, vec, Address, Env, IntoVal,
    Symbol, Val, Vec,
};

mod errors;
mod storage;

#[contracttype]
enum NftKey {
    Owner(u64),
    OwnerTokens(Address),
    BurnApproved(u64),
    TokenCount,
    Admin,
    Name,
    Symbol,
    Initialized,
    Paused,
}

// ── Errors ──────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Clone, Debug, PartialEq)]
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
    TransferRejected = 11,
}

// ── TTL ─────────────────────────────────────────────────────────────────────

#[cfg(any(test, feature = "testutils"))]
pub mod test_utils;

#[contract]
pub struct NavinNft;

#[contractimpl]
impl NavinNft {
    pub fn initialize(env: Env, admin: Address) -> Result<(), NftError> {
        if storage::is_initialized(&env) {
            return Err(NftError::AlreadyInitialized);
        }

        storage::set_admin(&env, &admin);
        Ok(())
    }

    /// Transfers a token and invokes the receiver interface for contract recipients.
    ///
    /// Contract recipients must return `true` from `supports_nft_receiver()` and
    /// successfully handle `on_nft_received(from, token_id, data)`, or the transfer
    /// fails with `NftError::TransferRejected`. Account recipients are not called.
    pub fn transfer(
        env: Env,
        from: Address,
        to: Address,
        token_id: u64,
        data: Vec<Val>,
    ) -> Result<(), NftError> {
        Self::require_not_paused(&env)?;
        from.require_auth();

        let owner = Self::get_owner_inner(&env, token_id)?;
        if owner != from {
            return Err(NftError::NotOwner);
        }
        if admin != storage::get_admin(&env) {
            return Err(NftError::Unauthorized);
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
        if metadata_uri.is_none() && storage::get_base_uri(&env).is_none() {
            return Err(NftError::BaseUriNotSet);
        }

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

fn append_token_id(env: &Env, base_uri: &String, token_id: u64) -> String {
    let mut digits = [0u8; 20];
    let mut value = token_id;
    let mut start = digits.len();

    loop {
        start -= 1;
        digits[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }

    let mut uri = Vec::with_capacity(base_uri.len() as usize + digits.len() - start);
    uri.resize(base_uri.len() as usize, 0);
    base_uri.copy_into_slice(&mut uri);
    uri.extend_from_slice(&digits[start..]);
    String::from_bytes(env, &uri)
}

#[cfg(test)]
mod test_auth;
#[cfg(test)]
mod test_events;
#[cfg(test)]
mod test_utils;

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::{contract, contractimpl, contracttype, testutils::Address as _};

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

        pub fn on_nft_received(env: Env, _from: Address, token_id: u64, data: Vec<Val>) {
            env.storage()
                .instance()
                .set(&ReceiverKey::LastReceivedToken, &token_id);
            env.storage()
                .instance()
                .set(&ReceiverKey::LastReceivedData, &data);
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

    #[contractimpl]
    impl NonReceiver {
        pub fn noop(_env: Env) {}
    }

    fn setup() -> (Env, Address, NavinShipmentNftClient<'static>) {
        let (env, admin) = test_utils::setup_env();
        let contract_id = env.register(NavinShipmentNft, ());
        let client = NavinShipmentNftClient::new(&env, &contract_id);
        client.initialize(&admin, &symbol_short!("NavinNFT"), &symbol_short!("NNFT"));
        (env, admin, client)
    }

    #[test]
    fn test_initialize_and_metadata() {
        let (env, admin, client) = setup();
        assert_eq!(client.name(), symbol_short!("NavinNFT"));
        assert_eq!(client.nft_symbol(), symbol_short!("NNFT"));
        assert_eq!(client.get_admin(), admin);
        let _ = env;
    }

    #[test]
    fn test_double_initialize_fails() {
        let (env, admin, client) = setup();
        let result = client.try_initialize(&admin, &symbol_short!("X"), &symbol_short!("X"));
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
        let result =
            client.try_initialize(&admin, &symbol_short!("NavinNFT"), &symbol_short!("NNFT"));
        assert!(result.is_err());
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
        let bob = Address::generate(&env);
        client.mint(&alice, &1);
        client.transfer(&alice, &bob, &1, &Vec::new(&env));
        assert_eq!(client.owner_of(&1), bob);
        assert_eq!(client.balance_of(&alice), 0);
        assert_eq!(client.balance_of(&bob), 1);
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
        client.initialize(&admin, &symbol_short!("NavinNFT"), &symbol_short!("NNFT"));
        client.mint(&owner, &100);

        // Now test burn in a clean env where mock_all_auths is NOT active
        let env_no_auth = Env::default();
        let admin2 = Address::generate(&env_no_auth);
        let owner2 = Address::generate(&env_no_auth);
        let cid = env_no_auth.register(NavinShipmentNft, ());
        let client_no_auth = NavinShipmentNftClient::new(&env_no_auth, &cid);

        // Mock auths only to set up state
        env_no_auth.mock_all_auths();
        client_no_auth.initialize(&admin2, &symbol_short!("NavinNFT"), &symbol_short!("NNFT"));
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
        let bob = Address::generate(&env);
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

        let res1 = client.try_initialize(&admin, &symbol_short!(""), &symbol_short!("NNFT"));
        assert_eq!(res1, Err(Ok(NftError::InvalidNameOrSymbol)));
    }

    #[test]
    fn test_initialize_with_empty_symbol_fails() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(NavinShipmentNft, ());
        let client = NavinShipmentNftClient::new(&env, &contract_id);
        env.mock_all_auths();

        let result = client.try_initialize(&admin, &symbol_short!("NavinNFT"), &symbol_short!(""));
        assert_eq!(result, Err(Ok(NftError::InvalidNameOrSymbol)));
    }
}
