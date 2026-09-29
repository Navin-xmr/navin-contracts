#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Symbol, Map, BytesN};

mod errors;
mod storage;
mod test;
mod test_cross_contract;

#[cfg(test)]
mod test_utils;

pub use errors::*;

#[contract]
pub struct NavinShipmentNft;

#[contractimpl]
impl NavinShipmentNft {
    /// Initialize the NFT contract with admin and collection metadata
    pub fn initialize(
        env: Env,
        admin: Address,
        name: String,
        symbol: String,
    ) -> Result<(), NftError> {
        if storage::is_initialized(&env) {
            return Err(NftError::AlreadyInitialized);
        }

        if name.is_empty() || symbol.is_empty() {
            return Err(NftError::InvalidInput);
        }

        storage::set_admin(&env, &admin);
        storage::set_name(&env, &name);
        storage::set_symbol(&env, &symbol);
        storage::set_next_token_id(&env, 1);

        env.events()
            .publish((symbol_short!("init"),), (admin.clone(), name, symbol));

        Ok(())
    }

    /// Mint a new NFT representing a shipment
    pub fn mint_shipment_nft(
        env: Env,
        to: Address,
        shipment_id: u64,
        data_hash: BytesN<32>,
        metadata: Map<Symbol, String>,
    ) -> Result<u64, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }

        // For now, anyone can mint. In future versions, this might be restricted to authorized contracts
        to.require_auth();

        let token_id = storage::get_next_token_id(&env);
        
        // Store NFT data
        storage::set_owner(&env, token_id, &to);
        storage::set_shipment_id(&env, token_id, shipment_id);
        storage::set_data_hash(&env, token_id, &data_hash);
        storage::set_metadata(&env, token_id, &metadata);
        
        // Update next token ID
        storage::set_next_token_id(&env, token_id + 1);

        env.events()
            .publish((symbol_short!("mint"),), (to, token_id, shipment_id));

        Ok(token_id)
    }

    /// Get the owner of a token
    pub fn owner_of(env: Env, token_id: u64) -> Result<Address, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }

        storage::get_owner(&env, token_id).ok_or(NftError::TokenNotFound)
    }

    /// Transfer a token from one address to another
    pub fn transfer(
        env: Env,
        from: Address,
        to: Address,
        token_id: u64,
    ) -> Result<(), NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }

        from.require_auth();

        let current_owner = storage::get_owner(&env, token_id).ok_or(NftError::TokenNotFound)?;
        
        if current_owner != from {
            return Err(NftError::Unauthorized);
        }

        storage::set_owner(&env, token_id, &to);

        env.events()
            .publish((symbol_short!("transfer"),), (from, to, token_id));

        Ok(())
    }

    /// Get shipment ID associated with a token
    pub fn get_shipment_id(env: Env, token_id: u64) -> Result<u64, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }

        storage::get_shipment_id(&env, token_id).ok_or(NftError::TokenNotFound)
    }

    /// Get data hash associated with a token
    pub fn get_data_hash(env: Env, token_id: u64) -> Result<BytesN<32>, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }

        storage::get_data_hash(&env, token_id).ok_or(NftError::TokenNotFound)
    }

    /// Get metadata for a token
    pub fn get_metadata(env: Env, token_id: u64) -> Result<Map<Symbol, String>, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }

        storage::get_metadata(&env, token_id).ok_or(NftError::TokenNotFound)
    }

    /// Get total supply of tokens
    pub fn total_supply(env: Env) -> Result<u64, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }

        Ok(storage::get_next_token_id(&env) - 1)
    }

    /// Get collection name
    pub fn name(env: Env) -> Result<String, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }
        Ok(storage::get_name(&env))
    }

    /// Get collection symbol
    pub fn symbol(env: Env) -> Result<String, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }
        Ok(storage::get_symbol(&env))
    }

    /// Get admin address
    pub fn get_admin(env: Env) -> Result<Address, NftError> {
        if !storage::is_initialized(&env) {
            return Err(NftError::NotInitialized);
        }
        Ok(storage::get_admin(&env))
    }

    /// Check if contract is initialized
    pub fn is_initialized(env: Env) -> bool {
        storage::is_initialized(&env)
    }
}
    /// Extend instance storage TTL to prevent premature contract expiration
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTENSION);
    }

    /// Extend persistent storage TTL for a specific owner's token list
    fn extend_persistent_ttl(env: &Env, owner: &Address) {
        env.storage()
            .persistent()
            .extend_ttl(&NftKey::OwnerTokens(owner.clone()), TTL_THRESHOLD, TTL_EXTENSION);
    }

    pub fn initialize(
        env: Env,
        admin: Address,
        name: Symbol,
        symbol: Symbol,
    ) -> Result<(), NftError> {
        admin.require_auth();

        if env.storage().instance().has(&NftKey::Initialized) {
            return Err(NftError::AlreadyInitialized);
        }
        if name == symbol_short!("") || symbol == symbol_short!("") {
            return Err(NftError::InvalidNameOrSymbol);
        }
        env.storage().instance().set(&NftKey::Admin, &admin);
        env.storage().instance().set(&NftKey::Name, &name);
        env.storage().instance().set(&NftKey::Symbol, &symbol);
        env.storage().instance().set(&NftKey::TokenCount, &0_u64);
        env.storage().instance().set(&NftKey::Initialized, &true);
        env.events().publish(
            (INIT,),
            (admin, name, symbol, env.ledger().timestamp()),
        );
        Self::extend_instance_ttl(&env);
        Ok(())
    }

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

        env.events()
            .publish((MINT,), (token_id, to.clone(), env.ledger().timestamp()));
        
        Self::extend_instance_ttl(&env);
        Self::extend_persistent_ttl(&env, &to);
        Ok(token_id)
    }

    pub fn transfer(env: Env, from: Address, to: Address, token_id: u64) -> Result<(), NftError> {
        Self::require_not_paused(&env)?;
        from.require_auth();

        let owner = Self::get_owner_inner(&env, token_id)?;
        if owner != from {
            return Err(NftError::NotOwner);
        }

        if to == from {
            Self::extend_instance_ttl(&env);
            return Ok(());
        }

        env.storage()
            .persistent()
            .set(&NftKey::Owner(token_id), &to);

        Self::remove_token_from_owner(&env, &from, token_id);
        Self::add_token_to_owner(&env, &to, token_id);

        env.events().publish(
            (TRANSFER_NFT,),
            (token_id, from.clone(), to.clone(), env.ledger().timestamp()),
        );
        
        Self::extend_instance_ttl(&env);
        Self::extend_persistent_ttl(&env, &from);
        Self::extend_persistent_ttl(&env, &to);
        Ok(())
    }

    pub fn burn(env: Env, caller: Address, token_id: u64) -> Result<(), NftError> {
        Self::require_not_paused(&env)?;
        caller.require_auth();

        let owner = Self::get_owner_inner(&env, token_id)?;
        let admin = Self::get_admin_inner(&env)?;

        if caller != owner && caller != admin {
            return Err(NftError::NotOwner);
        }

        // Check if burn is approved (unless caller is admin)
        if caller != admin {
            let burn_approved = env
                .storage()
                .persistent()
                .get(&NftKey::BurnApproved(token_id))
                .unwrap_or(false);
            if !burn_approved {
                return Err(NftError::BurnNotApproved);
            }
        }

        env.storage().persistent().remove(&NftKey::Owner(token_id));
        env.storage().persistent().remove(&NftKey::BurnApproved(token_id));
        Self::remove_token_from_owner(&env, &owner, token_id);

        let count: u64 = env
            .storage()
            .instance()
            .get(&NftKey::TokenCount)
            .unwrap_or(0);
        if count > 0 {
            env.storage()
                .instance()
                .set(&NftKey::TokenCount, &(count - 1));
        }

        env.events()
            .publish((BURN,), (token_id, caller, env.ledger().timestamp()));
        
        Self::extend_instance_ttl(&env);
        Self::extend_persistent_ttl(&env, &owner);
        Ok(())
    }

    pub fn transfer_admin(
        env: Env,
        admin: Address,
        new_admin: Address,
    ) -> Result<(), NftError> {
        admin.require_auth();
        if Self::get_admin_inner(&env)? != admin {
            return Err(NftError::NotAdmin);
        }

        env.storage().instance().set(&NftKey::Admin, &new_admin);
        env.events().publish(
            (ADMIN_TRANSFER,),
            (admin, new_admin, env.ledger().timestamp()),
        );
        Self::extend_instance_ttl(&env);
        Ok(())
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), NftError> {
        admin.require_auth();
        if Self::get_admin_inner(&env)? != admin {
            return Err(NftError::NotAdmin);
        }
        env.storage().instance().set(&NftKey::Paused, &true);
        Self::extend_instance_ttl(&env);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), NftError> {
        admin.require_auth();
        if Self::get_admin_inner(&env)? != admin {
            return Err(NftError::NotAdmin);
        }
        env.storage().instance().set(&NftKey::Paused, &false);
        Self::extend_instance_ttl(&env);
        Ok(())
    }

    pub fn approve_burn(env: Env, token_id: u64) -> Result<(), NftError> {
        let owner = Self::get_owner_inner(&env, token_id)?;
        owner.require_auth();

        env.storage()
            .persistent()
            .set(&NftKey::BurnApproved(token_id), &true);
        Self::extend_instance_ttl(&env);
        Self::extend_persistent_ttl(&env, &owner);
        Ok(())
    }

    pub fn is_burn_approved(env: Env, token_id: u64) -> bool {
        Self::extend_instance_ttl(&env);
        env.storage()
            .persistent()
            .get(&NftKey::BurnApproved(token_id))
            .unwrap_or(false)
    }

    pub fn owner_of(env: Env, token_id: u64) -> Result<Address, NftError> {
        Self::extend_instance_ttl(&env);
        Self::get_owner_inner(&env, token_id)
    }

    pub fn balance_of(env: Env, owner: Address) -> u64 {
        Self::extend_instance_ttl(&env);
        Self::extend_persistent_ttl(&env, &owner);
        let tokens: Vec<u64> = env
            .storage()
            .persistent()
            .get(&NftKey::OwnerTokens(owner))
            .unwrap_or_else(|| vec![&env]);
        tokens.len() as u64
    }

    pub fn total_supply(env: Env) -> u64 {
        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&NftKey::TokenCount)
            .unwrap_or(0)
    }

    pub fn name(env: Env) -> Result<Symbol, NftError> {
        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&NftKey::Name)
            .ok_or(NftError::NotInitialized)
    }

    pub fn nft_symbol(env: Env) -> Result<Symbol, NftError> {
        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&NftKey::Symbol)
            .ok_or(NftError::NotInitialized)
    }

    pub fn get_admin(env: Env) -> Result<Address, NftError> {
        Self::extend_instance_ttl(&env);
        Self::get_admin_inner(&env)
    }

    fn get_owner_inner(env: &Env, token_id: u64) -> Result<Address, NftError> {
        env.storage()
            .persistent()
            .get(&NftKey::Owner(token_id))
            .ok_or(NftError::TokenDoesNotExist)
    }

    fn get_admin_inner(env: &Env) -> Result<Address, NftError> {
        env.storage()
            .instance()
            .get(&NftKey::Admin)
            .ok_or(NftError::NotInitialized)
    }

    fn require_admin(env: &Env) -> Result<Address, NftError> {
        let admin = Self::get_admin_inner(env)?;
        admin.require_auth();
        Ok(admin)
    }

    fn require_not_paused(env: &Env) -> Result<(), NftError> {
        if env
            .storage()
            .instance()
            .get(&NftKey::Paused)
            .unwrap_or(false)
        {
            return Err(NftError::ContractPaused);
        }
        Ok(())
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
        if new_tokens.is_empty() {
            env.storage()
                .persistent()
                .remove(&NftKey::OwnerTokens(owner.clone()));
        } else {
            env.storage()
                .persistent()
                .set(&NftKey::OwnerTokens(owner.clone()), &new_tokens);
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::testutils::Address as _;

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
        let result = client.try_initialize(&admin, &symbol_short!("NavinNFT"), &symbol_short!("NNFT"));
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
        client.transfer(&alice, &bob, &1);
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
        let result = client.try_transfer(&bob, &alice, &1);
        assert!(result.is_err());
    }

    #[test]
    fn test_burn_by_owner() {
        let (env, _admin, client) = setup();
        let owner = Address::generate(&env);
        client.mint(&owner, &1);
        assert_eq!(client.total_supply(), 1);
        assert_eq!(client.balance_of(&owner), 1);
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
        client.transfer(&alice, &alice, &1);
        assert_eq!(client.owner_of(&1), alice);
    }

    #[test]
    fn test_initialize_empty_name_or_symbol_fails() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(NavinShipmentNft, ());
        let client = NavinShipmentNftClient::new(&env, &contract_id);
        env.mock_all_auths();

        let res1 = client.try_initialize(&admin, &symbol_short!(""), &symbol_short!("NNFT"));
        assert_eq!(res1, Err(Ok(NftError::InvalidNameOrSymbol)));

        let res2 = client.try_initialize(&admin, &symbol_short!("NavinNFT"), &symbol_short!(""));
        assert_eq!(res2, Err(Ok(NftError::InvalidNameOrSymbol)));
    }
}

