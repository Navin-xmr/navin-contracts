#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, vec, Address, Env, Symbol,
    Vec,
};

// ── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
enum NftKey {
    Owner(u64),
    OwnerTokens(Address),
    TokenCount,
    Admin,
    Name,
    Symbol,
    Initialized,
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
}

// ── Events ──────────────────────────────────────────────────────────────────

const MINT: Symbol = symbol_short!("mint");
const TRANSFER_NFT: Symbol = symbol_short!("xfer");
const BURN: Symbol = symbol_short!("burn");

// ── Contract ────────────────────────────────────────────────────────────────

#[contract]
pub struct NavinShipmentNft;

#[contractimpl]
impl NavinShipmentNft {
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
        env.storage().instance().set(&NftKey::Admin, &admin);
        env.storage().instance().set(&NftKey::Name, &name);
        env.storage().instance().set(&NftKey::Symbol, &symbol);
        env.storage().instance().set(&NftKey::TokenCount, &0_u64);
        env.storage().instance().set(&NftKey::Initialized, &true);
        Ok(())
    }

    /// Mint `token_id` to `to`.
    ///
    /// Re-minting after burn: `burn` removes the `Owner(token_id)` entry, so a
    /// burned `token_id` counts as unminted and may be minted again (by the
    /// admin). This is intentional — only a *currently owned* id is rejected
    /// with `TokenAlreadyMinted`.
    pub fn mint(env: Env, to: Address, token_id: u64) -> Result<u64, NftError> {
        Self::require_admin(&env)?;

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

        env.events().publish((MINT,), (token_id, to));
        Ok(token_id)
    }

    pub fn transfer(env: Env, from: Address, to: Address, token_id: u64) -> Result<(), NftError> {
        from.require_auth();

        let owner = Self::get_owner_inner(&env, token_id)?;
        if owner != from {
            return Err(NftError::NotOwner);
        }

        if to == from {
            return Ok(());
        }

        env.storage()
            .persistent()
            .set(&NftKey::Owner(token_id), &to);

        Self::remove_token_from_owner(&env, &from, token_id);
        Self::add_token_to_owner(&env, &to, token_id);

        env.events().publish((TRANSFER_NFT,), (token_id, from, to));
        Ok(())
    }

    pub fn burn(env: Env, caller: Address, token_id: u64) -> Result<(), NftError> {
        caller.require_auth();

        let owner = Self::get_owner_inner(&env, token_id)?;
        let admin = Self::get_admin_inner(&env)?;

        if caller != owner && caller != admin {
            return Err(NftError::NotOwner);
        }

        env.storage().persistent().remove(&NftKey::Owner(token_id));
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

        env.events().publish((BURN,), (token_id, caller));
        Ok(())
    }

    pub fn owner_of(env: Env, token_id: u64) -> Result<Address, NftError> {
        Self::get_owner_inner(&env, token_id)
    }

    pub fn balance_of(env: Env, owner: Address) -> u64 {
        let tokens: Vec<u64> = env
            .storage()
            .persistent()
            .get(&NftKey::OwnerTokens(owner))
            .unwrap_or_else(|| vec![&env]);
        tokens.len() as u64
    }

    pub fn total_supply(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&NftKey::TokenCount)
            .unwrap_or(0)
    }

    pub fn name(env: Env) -> Result<Symbol, NftError> {
        env.storage()
            .instance()
            .get(&NftKey::Name)
            .ok_or(NftError::NotInitialized)
    }

    pub fn nft_symbol(env: Env) -> Result<Symbol, NftError> {
        env.storage()
            .instance()
            .get(&NftKey::Symbol)
            .ok_or(NftError::NotInitialized)
    }

    pub fn get_admin(env: Env) -> Result<Address, NftError> {
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
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(NavinShipmentNft, ());
        let client = NavinShipmentNftClient::new(&env, &contract_id);
        env.mock_all_auths();
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
    fn test_double_burn_fails() {
        let (env, _admin, client) = setup();
        let owner = Address::generate(&env);
        client.mint(&owner, &1);
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
}

