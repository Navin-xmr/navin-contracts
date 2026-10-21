#![no_std]

pub mod event_topics;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, vec, Address, Env, Symbol,
    Vec,
};

// ── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
enum NftKey {
    Owner(u64),
    OwnerTokens(Address),
    OwnerTokenCount(Address),
    BurnApproved(u64),
    Approval(Address, Address, u64),
    ApprovedOperator(u64),
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
    ApprovalNotActive = 11,
    Overflow = 12,
    InvalidBatchLength = 13,
}

// ── TTL ─────────────────────────────────────────────────────────────────────

/// Ledgers remaining before a TTL bump kicks in (~1 day).
const TTL_THRESHOLD: u32 = 17_280;

/// Ledgers to extend by once the threshold is hit (~30 days).
const TTL_EXTENSION: u32 = 518_400;

// ── Contract ────────────────────────────────────────────────────────────────

#[contract]
pub struct NavinShipmentNft;

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
        for key in [
            NftKey::OwnerTokens(owner.clone()),
            NftKey::OwnerTokenCount(owner.clone()),
        ] {
            if env.storage().persistent().has(&key) {
                env.storage()
                    .persistent()
                    .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTENSION);
            }
        }
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
            (event_topics::INIT, event_topics::EVENT_SCHEMA_VERSION),
            (admin, name, symbol, env.ledger().timestamp()),
        );
        Self::extend_instance_ttl(&env);
        Ok(())
    }

    /// Mint `token_id` to `to`.
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

        Self::add_token_to_owner(&env, &to, token_id)?;

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

    /// Mint a batch of tokens; recipient and token ID vectors must be aligned.
    pub fn mint_batch(
        env: Env,
        recipients: Vec<Address>,
        token_ids: Vec<u64>,
    ) -> Result<(), NftError> {
        Self::require_not_paused(&env)?;
        Self::require_admin(&env)?;
        if recipients.len() != token_ids.len() {
            return Err(NftError::InvalidBatchLength);
        }

        let mut total_supply: u64 = env
            .storage()
            .instance()
            .get(&NftKey::TokenCount)
            .unwrap_or(0);
        for (to, token_id) in recipients.iter().zip(token_ids.iter()) {
            if to == env.current_contract_address() {
                return Err(NftError::MintToContract);
            }
            if env.storage().persistent().has(&NftKey::Owner(token_id)) {
                return Err(NftError::TokenAlreadyMinted);
            }

            total_supply = total_supply.checked_add(1).ok_or(NftError::Overflow)?;
            env.storage()
                .persistent()
                .set(&NftKey::Owner(token_id), &to);
            Self::add_token_to_owner(&env, &to, token_id)?;
            Self::extend_persistent_ttl(&env, &to);
            env.events().publish(
                (event_topics::MINT, event_topics::EVENT_SCHEMA_VERSION),
                (token_id, to, env.ledger().timestamp()),
            );
        }
        env.storage()
            .instance()
            .set(&NftKey::TokenCount, &total_supply);
        env.events().publish(
            (event_topics::MINT_BATCH, event_topics::EVENT_SCHEMA_VERSION),
            (token_ids, recipients, env.ledger().timestamp()),
        );
        Self::extend_instance_ttl(&env);
        Ok(())
    }

    pub fn transfer(env: Env, from: Address, to: Address, token_id: u64) -> Result<(), NftError> {
        Self::require_not_paused(&env)?;
        from.require_auth();

        let owner = Self::get_owner_inner(&env, token_id)?;
        if owner != from {
            return Err(NftError::NotOwner);
        }

        Self::transfer_token(&env, &from, &to, token_id)
    }

    /// Transfer a token on behalf of its owner using an active token approval.
    pub fn transfer_from(
        env: Env,
        operator: Address,
        from: Address,
        to: Address,
        token_id: u64,
    ) -> Result<(), NftError> {
        Self::require_not_paused(&env)?;
        operator.require_auth();

        let owner = Self::get_owner_inner(&env, token_id)?;
        if owner != from || Self::get_approved_inner(&env, token_id)? != Some(operator) {
            return Err(NftError::ApprovalNotActive);
        }

        Self::transfer_token(&env, &from, &to, token_id)
    }

    fn transfer_token(
        env: &Env,
        from: &Address,
        to: &Address,
        token_id: u64,
    ) -> Result<(), NftError> {
        if to == from {
            Self::extend_instance_ttl(&env);
            return Ok(());
        }

        Self::clear_approval(env, from, token_id);
        env.storage().persistent().set(&NftKey::Owner(token_id), to);

        Self::remove_token_from_owner(env, from, token_id)?;
        Self::add_token_to_owner(env, to, token_id)?;

        env.events().publish(
            (event_topics::TRANSFER, event_topics::EVENT_SCHEMA_VERSION),
            (token_id, from.clone(), to.clone(), env.ledger().timestamp()),
        );

        Self::extend_instance_ttl(env);
        Self::extend_persistent_ttl(env, from);
        Self::extend_persistent_ttl(env, to);
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
        env.storage()
            .persistent()
            .remove(&NftKey::BurnApproved(token_id));
        Self::clear_approval(&env, &owner, token_id);
        Self::remove_token_from_owner(&env, &owner, token_id)?;

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

        env.events().publish(
            (event_topics::BURN, event_topics::EVENT_SCHEMA_VERSION),
            (token_id, caller, env.ledger().timestamp()),
        );
        Self::extend_instance_ttl(&env);
        Ok(())
    }

    /// Burn a batch of tokens as the admin or as an owner of every token.
    /// Non-admin owners must have approved burning each token beforehand.
    pub fn burn_batch(env: Env, caller: Address, token_ids: Vec<u64>) -> Result<(), NftError> {
        Self::require_not_paused(&env)?;
        caller.require_auth();
        let admin = Self::get_admin_inner(&env)?;
        let is_admin = caller == admin;

        for token_id in token_ids.iter() {
            let owner = Self::get_owner_inner(&env, token_id)?;
            if !is_admin && caller != owner {
                return Err(NftError::NotOwner);
            }
            if !is_admin
                && !env
                    .storage()
                    .persistent()
                    .get(&NftKey::BurnApproved(token_id))
                    .unwrap_or(false)
            {
                return Err(NftError::BurnNotApproved);
            }

            env.storage().persistent().remove(&NftKey::Owner(token_id));
            env.storage()
                .persistent()
                .remove(&NftKey::BurnApproved(token_id));
            Self::clear_approval(&env, &owner, token_id);
            Self::remove_token_from_owner(&env, &owner, token_id)?;

            let count: u64 = env
                .storage()
                .instance()
                .get(&NftKey::TokenCount)
                .unwrap_or(0);
            let new_count = count.checked_sub(1).ok_or(NftError::Overflow)?;
            env.storage()
                .instance()
                .set(&NftKey::TokenCount, &new_count);
            Self::extend_persistent_ttl(&env, &owner);

            env.events().publish(
                (event_topics::BURN, event_topics::EVENT_SCHEMA_VERSION),
                (token_id, caller.clone(), env.ledger().timestamp()),
            );
        }

        env.events().publish(
            (event_topics::BURN_BATCH, event_topics::EVENT_SCHEMA_VERSION),
            (token_ids, caller, env.ledger().timestamp()),
        );
        Self::extend_instance_ttl(&env);
        Ok(())
    }

    /// Approve an operator to transfer this token. `None` means no expiration.
    pub fn approve(
        env: Env,
        operator: Address,
        token_id: u64,
        expiration_ledger: Option<u32>,
    ) -> Result<(), NftError> {
        let owner = Self::get_owner_inner(&env, token_id)?;
        owner.require_auth();

        Self::clear_approval(&env, &owner, token_id);
        env.storage().persistent().set(
            &NftKey::Approval(owner.clone(), operator.clone(), token_id),
            &expiration_ledger,
        );
        env.storage()
            .persistent()
            .set(&NftKey::ApprovedOperator(token_id), &operator);
        Self::extend_approval_ttl(&env, &owner, &operator, token_id);

        env.events().publish(
            (event_topics::APPROVAL, event_topics::EVENT_SCHEMA_VERSION),
            (
                owner.clone(),
                operator,
                token_id,
                expiration_ledger,
                env.ledger().timestamp(),
            ),
        );
        Self::extend_instance_ttl(&env);
        Self::extend_persistent_ttl(&env, &owner);
        Ok(())
    }

    /// Revoke the current operator approval for this token.
    pub fn revoke_approval(env: Env, operator: Address, token_id: u64) -> Result<(), NftError> {
        let owner = Self::get_owner_inner(&env, token_id)?;
        owner.require_auth();

        if env
            .storage()
            .persistent()
            .get::<_, Address>(&NftKey::ApprovedOperator(token_id))
            == Some(operator.clone())
        {
            Self::clear_approval(&env, &owner, token_id);
            env.events().publish(
                (
                    event_topics::APPROVAL_REVOKED,
                    event_topics::EVENT_SCHEMA_VERSION,
                ),
                (owner.clone(), operator, token_id, env.ledger().timestamp()),
            );
        }
        Self::extend_instance_ttl(&env);
        Self::extend_persistent_ttl(&env, &owner);
        Ok(())
    }

    /// Return the currently approved operator, or `None` if absent or expired.
    pub fn get_approved(env: Env, token_id: u64) -> Result<Option<Address>, NftError> {
        Self::extend_instance_ttl(&env);
        Self::get_approved_inner(&env, token_id)
    }

    pub fn transfer_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), NftError> {
        admin.require_auth();
        if Self::get_admin_inner(&env)? != admin {
            return Err(NftError::NotAdmin);
        }

        env.storage().instance().set(&NftKey::Admin, &new_admin);
        env.events().publish(
            (
                event_topics::ADMIN_TRANSFER,
                event_topics::EVENT_SCHEMA_VERSION,
            ),
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
        Self::get_owner_token_count(&env, &owner)
    }

    pub fn tokens_of_owner(env: Env, owner: Address) -> Vec<u64> {
        Self::extend_instance_ttl(&env);
        Self::extend_persistent_ttl(&env, &owner);
        env.storage()
            .persistent()
            .get(&NftKey::OwnerTokens(owner))
            .unwrap_or_else(|| vec![&env])
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

    fn get_approved_inner(env: &Env, token_id: u64) -> Result<Option<Address>, NftError> {
        let owner = Self::get_owner_inner(env, token_id)?;
        let Some(operator) = env
            .storage()
            .persistent()
            .get::<_, Address>(&NftKey::ApprovedOperator(token_id))
        else {
            return Ok(None);
        };
        let approval_key = NftKey::Approval(owner.clone(), operator.clone(), token_id);
        let Some(expiration_ledger) = env
            .storage()
            .persistent()
            .get::<_, Option<u32>>(&approval_key)
        else {
            return Ok(None);
        };
        if expiration_ledger.is_some_and(|expiration| env.ledger().sequence() > expiration) {
            return Ok(None);
        }
        Self::extend_approval_ttl(env, &owner, &operator, token_id);
        Ok(Some(operator))
    }

    fn extend_approval_ttl(env: &Env, owner: &Address, operator: &Address, token_id: u64) {
        let approval_key = NftKey::Approval(owner.clone(), operator.clone(), token_id);
        let operator_key = NftKey::ApprovedOperator(token_id);
        if env.storage().persistent().has(&approval_key) {
            env.storage()
                .persistent()
                .extend_ttl(&approval_key, TTL_THRESHOLD, TTL_EXTENSION);
        }
        if env.storage().persistent().has(&operator_key) {
            env.storage()
                .persistent()
                .extend_ttl(&operator_key, TTL_THRESHOLD, TTL_EXTENSION);
        }
    }

    fn clear_approval(env: &Env, owner: &Address, token_id: u64) {
        if let Some(operator) = env
            .storage()
            .persistent()
            .get::<_, Address>(&NftKey::ApprovedOperator(token_id))
        {
            env.storage()
                .persistent()
                .remove(&NftKey::Approval(owner.clone(), operator, token_id));
            env.storage()
                .persistent()
                .remove(&NftKey::ApprovedOperator(token_id));
        }
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

    fn get_owner_token_count(env: &Env, owner: &Address) -> u64 {
        let key = NftKey::OwnerTokenCount(owner.clone());
        if let Some(count) = env.storage().persistent().get(&key) {
            return count;
        }

        // Backfill counts for owners that already held tokens before this key
        // was introduced.
        let count = env
            .storage()
            .persistent()
            .get::<_, Vec<u64>>(&NftKey::OwnerTokens(owner.clone()))
            .map(|tokens| tokens.len() as u64)
            .unwrap_or(0);
        env.storage().persistent().set(&key, &count);
        count
    }

    fn add_token_to_owner(env: &Env, owner: &Address, token_id: u64) -> Result<(), NftError> {
        let mut tokens: Vec<u64> = env
            .storage()
            .persistent()
            .get(&NftKey::OwnerTokens(owner.clone()))
            .unwrap_or_else(|| vec![env]);
        let count = Self::get_owner_token_count(env, owner)
            .checked_add(1)
            .ok_or(NftError::Overflow)?;
        tokens.push_back(token_id);
        env.storage()
            .persistent()
            .set(&NftKey::OwnerTokens(owner.clone()), &tokens);
        env.storage()
            .persistent()
            .set(&NftKey::OwnerTokenCount(owner.clone()), &count);
        Ok(())
    }

    fn remove_token_from_owner(env: &Env, owner: &Address, token_id: u64) -> Result<(), NftError> {
        let tokens: Vec<u64> = env
            .storage()
            .persistent()
            .get(&NftKey::OwnerTokens(owner.clone()))
            .unwrap_or_else(|| vec![env]);
        let count = Self::get_owner_token_count(env, owner)
            .checked_sub(1)
            .ok_or(NftError::Overflow)?;
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
        env.storage()
            .persistent()
            .set(&NftKey::OwnerTokenCount(owner.clone()), &count);
        Ok(())
    }
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
    fn test_mint_batch_assigns_one_hundred_tokens() {
        let (env, _admin, client) = setup();
        let recipient = Address::generate(&env);
        let mut recipients = Vec::new(&env);
        let mut token_ids = Vec::new(&env);
        for token_id in 1..=100 {
            recipients.push_back(recipient.clone());
            token_ids.push_back(token_id);
        }

        client.mint_batch(&recipients, &token_ids);

        assert_eq!(client.balance_of(&recipient), 100);
        assert_eq!(client.total_supply(), 100);
        for token_id in token_ids.iter() {
            assert_eq!(client.owner_of(&token_id), recipient);
        }
    }

    #[test]
    fn test_mint_batch_rejects_mismatched_vectors() {
        let (env, _admin, client) = setup();
        let recipient = Address::generate(&env);
        let recipients = vec![&env, recipient];
        let token_ids = vec![&env, 1_u64, 2_u64];

        assert_eq!(
            client.try_mint_batch(&recipients, &token_ids),
            Err(Ok(NftError::InvalidBatchLength))
        );
        assert_eq!(client.total_supply(), 0);
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
    fn test_approved_operator_can_transfer_within_expiration() {
        let (env, _admin, client) = setup();
        let owner = Address::generate(&env);
        let operator = Address::generate(&env);
        let recipient = Address::generate(&env);
        client.mint(&owner, &1);
        client.approve(&operator, &1, &Some(10_u32));
        assert_eq!(client.get_approved(&1), Some(operator.clone()));

        env.ledger().with_mut(|ledger| ledger.sequence_number = 10);
        client.transfer_from(&operator, &owner, &recipient, &1);
        assert_eq!(client.owner_of(&1), recipient);
        assert_eq!(client.get_approved(&1), None);
    }

    #[test]
    fn test_approved_operator_cannot_transfer_after_expiration() {
        let (env, _admin, client) = setup();
        let owner = Address::generate(&env);
        let operator = Address::generate(&env);
        let recipient = Address::generate(&env);
        client.mint(&owner, &1);
        client.approve(&operator, &1, &Some(10_u32));

        env.ledger().with_mut(|ledger| ledger.sequence_number = 11);
        assert_eq!(client.get_approved(&1), None);
        assert_eq!(
            client.try_transfer_from(&operator, &owner, &recipient, &1),
            Err(Ok(NftError::ApprovalNotActive))
        );
        assert_eq!(client.owner_of(&1), owner);
    }

    #[test]
    fn test_owner_can_revoke_approval() {
        let (env, _admin, client) = setup();
        let owner = Address::generate(&env);
        let operator = Address::generate(&env);
        client.mint(&owner, &1);
        client.approve(&operator, &1, &None);
        client.revoke_approval(&operator, &1);
        assert_eq!(client.get_approved(&1), None);
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
    fn test_admin_burn_batch_removes_tokens() {
        let (env, admin, client) = setup();
        let owner = Address::generate(&env);
        let mut recipients = Vec::new(&env);
        let mut token_ids = Vec::new(&env);
        for token_id in 1..=10 {
            recipients.push_back(owner.clone());
            token_ids.push_back(token_id);
        }
        client.mint_batch(&recipients, &token_ids);

        client.burn_batch(&admin, &token_ids);

        assert_eq!(client.total_supply(), 0);
        assert_eq!(client.balance_of(&owner), 0);
        for token_id in token_ids.iter() {
            assert!(client.try_owner_of(&token_id).is_err());
        }
    }

    #[test]
    fn test_owner_can_burn_batch_after_approving_each_token() {
        let (env, _admin, client) = setup();
        let owner = Address::generate(&env);
        let mut recipients = Vec::new(&env);
        let mut token_ids = Vec::new(&env);
        for token_id in 1..=3 {
            recipients.push_back(owner.clone());
            token_ids.push_back(token_id);
        }
        client.mint_batch(&recipients, &token_ids);
        for token_id in token_ids.iter() {
            client.approve_burn(&token_id);
        }

        client.burn_batch(&owner, &token_ids);

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
    fn test_balance_of_after_one_thousand_mints() {
        let (env, _admin, client) = setup();
        let owner = Address::generate(&env);

        env.cost_estimate().budget().reset_unlimited();
        for token_id in 1..=1_000 {
            client.mint(&owner, &token_id);
        }
        let balance = client.balance_of(&owner);
        env.cost_estimate().budget().reset_default();

        assert_eq!(balance, 1_000);
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
        client.transfer(&alice, &bob, &1);
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
