#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Symbol, Vec};

mod errors;
mod event_topics;
mod storage;
mod test;

#[cfg(test)]
mod test_utils;

pub use errors::*;

/// Pass as `expiration_ledger` to `approve` for an allowance that
/// effectively never expires (issue #659).
pub const MAX_EXPIRATION_LEDGER: u32 = u32::MAX;

#[contract]
pub struct NavinToken;

/// Returns Err(TokenError::ContractPaused) if the contract is currently
/// paused (issue #657). Checked after initialization but before
/// require_auth, matching the shipment contract's guard ordering.
fn require_not_paused(env: &Env) -> Result<(), TokenError> {
    if storage::is_paused(env) {
        return Err(TokenError::ContractPaused);
    }
    Ok(())
}

#[contractimpl]
impl NavinToken {
    /// Initialize the token with admin, name, symbol, and total supply
    pub fn initialize(
        env: Env,
        admin: Address,
        name: String,
        symbol: String,
        total_supply: i128,
    ) -> Result<(), TokenError> {
        if storage::is_initialized(&env) {
            return Err(TokenError::AlreadyInitialized);
        }

        if name.is_empty() || symbol.is_empty() {
            return Err(TokenError::InvalidAmount);
        }

        if total_supply <= 0 {
            return Err(TokenError::InvalidAmount);
        }

        storage::set_admin(&env, &admin);
        storage::set_name(&env, &name);
        storage::set_symbol(&env, &symbol);
        storage::set_total_supply(&env, total_supply);
        storage::set_balance(&env, &admin, total_supply);

        env.events().publish(
            (
                Symbol::new(env, event_topics::INIT),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (admin.clone(), total_supply),
        );

        Ok(())
    }

    /// Get the token admin
    pub fn get_admin(env: Env) -> Result<Address, TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        Ok(storage::get_admin(&env))
    }

    /// Get token name
    pub fn name(env: Env) -> Result<String, TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        Ok(storage::get_name(&env))
    }

    /// Get token decimals
    pub fn decimals(env: Env) -> Result<u32, TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        Ok(7)
    }

    /// Get token symbol
    pub fn symbol(env: Env) -> Result<String, TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        Ok(storage::get_symbol(&env))
    }

    /// Get total supply
    pub fn total_supply(env: Env) -> Result<i128, TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        Ok(storage::get_total_supply(&env))
    }

    /// Get balance of an address
    pub fn balance(env: Env, address: Address) -> Result<i128, TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        Ok(storage::get_balance(&env, &address))
    }

    /// Transfer tokens from caller to recipient.
    ///
    /// Self-transfers (`from == to`) are permitted and treated as a harmless
    /// no-op to match the standard SEP-41/Soroban token interface semantics.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        require_not_paused(&env)?;

        from.require_auth();

        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }

        let from_balance = storage::get_balance(&env, &from);
        if from_balance < amount {
            return Err(TokenError::InsufficientBalance);
        }

        // Update balances with checked arithmetic
        let new_from_balance = from_balance
            .checked_sub(amount)
            .ok_or(TokenError::Overflow)?;
        let to_balance = storage::get_balance(&env, &to);
        let new_to_balance = to_balance.checked_add(amount).ok_or(TokenError::Overflow)?;
        storage::set_balance(&env, &from, new_from_balance);
        storage::set_balance(&env, &to, new_to_balance);

        // Extend TTL for affected balances
        storage::extend_balance_ttl_for(&env, &[from.clone(), to.clone()], 1000, 500000);

        env.events().publish(
            (
                Symbol::new(env, event_topics::TRANSFER),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (from, to, amount),
        );

        Ok(())
    }

    /// Transfer tokens from one address to another with approval.
    ///
    /// Self-transfers (`from == to`) are permitted and treated as a harmless
    /// no-op to match the standard SEP-41/Soroban token interface semantics.
    pub fn transfer_from(
        env: Env,
        spender: Address,
        from: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        require_not_paused(&env)?;

        spender.require_auth();

        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }

        let allowance = storage::get_allowance(&env, &from, &spender);
        if allowance < amount {
            return Err(TokenError::InsufficientAllowance);
        }

        let from_balance = storage::get_balance(&env, &from);
        if from_balance < amount {
            return Err(TokenError::InsufficientBalance);
        }

        // Update balances and allowance. Preserve the existing
        // expiration_ledger (issue #659) — spending down an allowance
        // doesn't reset how long it's valid for.
        let expiration_ledger = storage::get_allowance_raw(&env, &from, &spender)
            .map(|v| v.expiration_ledger)
            .unwrap_or(0);
        storage::set_balance(&env, &from, from_balance - amount);
        storage::set_balance(&env, &to, storage::get_balance(&env, &to) + amount);
        storage::set_allowance(&env, &from, &spender, allowance - amount, expiration_ledger);

        storage::extend_balance_ttl_for(&env, &[from.clone(), to.clone()], 1000, 500000);
        storage::extend_allowance_ttl(&env, &from, &spender, 1000, 500000);

        env.events()
            .publish((symbol_short!("tr_from"),), (from, to, spender, amount));

        Ok(())
    }

    /// Approve `spender` to transfer up to `amount` of `from`'s tokens,
    /// until `expiration_ledger` (issue #659) — matches the standard
    /// Soroban token interface's `approve(from, spender, amount,
    /// expiration_ledger)` shape. Pass `MAX_EXPIRATION_LEDGER` for an
    /// allowance that effectively never expires. `amount == 0` clears the
    /// allowance regardless of `expiration_ledger`.
    ///
    /// Self-approval (`from == spender`) is permitted and treated as a
    /// harmless no-op, matching the standard SEP-41/Soroban token interface.
    pub fn approve(
        env: Env,
        from: Address,
        spender: Address,
        amount: i128,
        expiration_ledger: u32,
    ) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        require_not_paused(&env)?;

        from.require_auth();

        if amount < 0 {
            return Err(TokenError::InvalidAmount);
        }

        if amount > 0 && expiration_ledger < env.ledger().sequence() {
            return Err(TokenError::InvalidExpirationLedger);
        }

        storage::set_allowance(&env, &from, &spender, amount, expiration_ledger);
        // Persistent entries must have their TTL maintained on write, or the
        // allowance is archived after the ledger passes its live-until
        // (issue #659).
        storage::extend_allowance_ttl(&env, &from, &spender, 1000, 500000);

        env.events().publish(
            (
                Symbol::new(env, event_topics::APPROVE),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (from, spender, amount, expiration_ledger),
        );

        Ok(())
    }

    /// Get the current allowance of `spender` for `from`'s tokens. Reads
    /// back as 0 once `expiration_ledger` has passed (issue #659).
    pub fn allowance(env: Env, from: Address, spender: Address) -> Result<i128, TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        Ok(storage::get_allowance(&env, &from, &spender))
    }

    /// Increase the allowance for a spender by a delta.
    /// This avoids the classic ERC-20 race condition present in `approve`.
    /// Self-approval (`owner == spender`) is allowed as a no-op to match the
    /// standard SEP-41/Soroban token interface semantics.
    pub fn increase_allowance(
        env: Env,
        owner: Address,
        spender: Address,
        delta: i128,
    ) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        require_not_paused(&env)?;

        owner.require_auth();

        if delta <= 0 {
            return Err(TokenError::InvalidAmount);
        }

        let current = storage::get_allowance(&env, &owner, &spender);
        let new_allowance = current.checked_add(delta).ok_or(TokenError::Overflow)?;
        // Preserve the existing expiration_ledger (issue #659) — raising an
        // allowance doesn't reset how long it's valid for.
        let expiration_ledger = storage::get_allowance_raw(&env, &owner, &spender)
            .map(|v| v.expiration_ledger)
            .unwrap_or(0);
        storage::set_allowance(&env, &owner, &spender, new_allowance, expiration_ledger);
        storage::extend_allowance_ttl(&env, &owner, &spender, 1000, 500000);

        env.events().publish(
            (
                Symbol::new(env, event_topics::ALLOWANCE_INCREASED),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (owner, spender, delta, new_allowance),
        );

        Ok(())
    }

    /// Decrease the allowance for a spender by a delta.
    /// Returns `InsufficientAllowance` if the delta exceeds the current allowance.
    /// Self-approval (`owner == spender`) is allowed as a no-op to match the
    /// standard SEP-41/Soroban token interface semantics.
    pub fn decrease_allowance(
        env: Env,
        owner: Address,
        spender: Address,
        delta: i128,
    ) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        require_not_paused(&env)?;

        owner.require_auth();

        if delta <= 0 {
            return Err(TokenError::InvalidAmount);
        }

        let current = storage::get_allowance(&env, &owner, &spender);
        if delta > current {
            return Err(TokenError::InsufficientAllowance);
        }
        let new_allowance = current.checked_sub(delta).ok_or(TokenError::Overflow)?;
        // Preserve the existing expiration_ledger (issue #659) — lowering an
        // allowance doesn't reset how long it's valid for.
        let expiration_ledger = storage::get_allowance_raw(&env, &owner, &spender)
            .map(|v| v.expiration_ledger)
            .unwrap_or(0);
        storage::set_allowance(&env, &owner, &spender, new_allowance, expiration_ledger);
        storage::extend_allowance_ttl(&env, &owner, &spender, 1000, 500000);

        env.events().publish(
            (
                Symbol::new(env, event_topics::ALLOWANCE_DECREASED),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (owner, spender, delta, new_allowance),
        );

        Ok(())
    }

    /// Propose a new admin address. The new admin must accept the transfer
    /// before the contract's admin is updated.
    pub fn transfer_admin(
        env: Env,
        current_admin: Address,
        new_admin: Address,
    ) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }

        current_admin.require_auth();

        if storage::get_admin(&env) != current_admin {
            return Err(TokenError::Unauthorized);
        }

        if new_admin == current_admin {
            return Err(TokenError::SameAccount);
        }

        storage::set_pending_admin(&env, &new_admin);

        env.events()
            .publish((symbol_short!("admin_prop"),), (current_admin, new_admin));

        Ok(())
    }

    /// Accept a previously proposed admin transfer. The accepting address must
    /// be the one previously nominated by the current admin.
    pub fn accept_admin_transfer(env: Env, new_admin: Address) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }

        new_admin.require_auth();

        let pending_admin = storage::get_pending_admin(&env).ok_or(TokenError::Unauthorized)?;
        if pending_admin != new_admin {
            return Err(TokenError::Unauthorized);
        }

        let old_admin = storage::get_admin(&env);
        storage::set_admin(&env, &new_admin);
        storage::clear_pending_admin(&env);

        env.events()
            .publish((symbol_short!("admin_tr"),), (old_admin, new_admin));

        Ok(())
    }

    /// Mint new tokens (admin only)
    pub fn mint(env: Env, admin: Address, to: Address, amount: i128) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        require_not_paused(&env)?;

        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(TokenError::Unauthorized);
        }

        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }

        let current_supply = storage::get_total_supply(&env);
        let new_supply = current_supply
            .checked_add(amount)
            .ok_or(TokenError::Overflow)?;
        let to_balance = storage::get_balance(&env, &to);
        let new_to_balance = to_balance.checked_add(amount).ok_or(TokenError::Overflow)?;
        storage::set_total_supply(&env, new_supply);
        storage::set_balance(&env, &to, new_to_balance);

        // Extend TTL for the recipient's balance
        storage::extend_balance_ttl(&env, &to, 1000, 500000);

        env.events().publish(
            (
                Symbol::new(env, event_topics::MINT),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (to, amount),
        );

        Ok(())
    }

    /// Admin clawback burn: burns tokens from an arbitrary `from` address,
    /// authorized by the admin rather than the holder (issue #658). Kept
    /// under this distinct name so it can't be confused with the
    /// holder-authorized `burn` below, which requires `from`'s own auth.
    pub fn admin_burn(
        env: Env,
        admin: Address,
        from: Address,
        amount: i128,
    ) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        require_not_paused(&env)?;

        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(TokenError::Unauthorized);
        }

        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }

        let from_balance = storage::get_balance(&env, &from);
        if from_balance < amount {
            return Err(TokenError::InsufficientBalance);
        }

        let current_supply = storage::get_total_supply(&env);
        let new_supply = current_supply
            .checked_sub(amount)
            .ok_or(TokenError::Overflow)?;
        let new_from_balance = from_balance
            .checked_sub(amount)
            .ok_or(TokenError::Overflow)?;
        storage::set_total_supply(&env, new_supply);
        storage::set_balance(&env, &from, new_from_balance);

        // Extend TTL for the source's balance
        storage::extend_balance_ttl(&env, &from, 1000, 500000);

        env.events().publish(
            (
                Symbol::new(env, event_topics::ADMIN_BURN),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (from, amount),
        );

        Ok(())
    }

    /// Holder self-service burn: `from` burns their own tokens, requiring
    /// only their own auth — no admin involvement (issue #658).
    pub fn burn(env: Env, from: Address, amount: i128) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        require_not_paused(&env)?;

        from.require_auth();

        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }

        let from_balance = storage::get_balance(&env, &from);
        if from_balance < amount {
            return Err(TokenError::InsufficientBalance);
        }

        let current_supply = storage::get_total_supply(&env);
        let new_supply = current_supply.checked_sub(amount).ok_or(TokenError::Overflow)?;
        let new_from_balance = from_balance.checked_sub(amount).ok_or(TokenError::Overflow)?;
        storage::set_total_supply(&env, new_supply);
        storage::set_balance(&env, &from, new_from_balance);
        storage::extend_balance_ttl(&env, &from, 1000, 500000);

        env.events().publish(
            (
                Symbol::new(env, event_topics::BURN),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (from, amount),
        );

        Ok(())
    }

    /// Allowance-based burn: `spender` burns `amount` of `from`'s tokens,
    /// consuming an existing allowance — mirrors `transfer_from` but
    /// destroys the tokens instead of moving them (issue #658).
    pub fn burn_from(
        env: Env,
        spender: Address,
        from: Address,
        amount: i128,
    ) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        require_not_paused(&env)?;

        spender.require_auth();

        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }

        let allowance = storage::get_allowance(&env, &from, &spender);
        if allowance < amount {
            return Err(TokenError::InsufficientAllowance);
        }

        let from_balance = storage::get_balance(&env, &from);
        if from_balance < amount {
            return Err(TokenError::InsufficientBalance);
        }

        let expiration_ledger = storage::get_allowance_raw(&env, &from, &spender)
            .map(|v| v.expiration_ledger)
            .unwrap_or(0);
        let current_supply = storage::get_total_supply(&env);
        let new_supply = current_supply.checked_sub(amount).ok_or(TokenError::Overflow)?;
        let new_from_balance = from_balance.checked_sub(amount).ok_or(TokenError::Overflow)?;
        let new_allowance = allowance.checked_sub(amount).ok_or(TokenError::Overflow)?;
        storage::set_total_supply(&env, new_supply);
        storage::set_balance(&env, &from, new_from_balance);
        storage::set_allowance(&env, &from, &spender, new_allowance, expiration_ledger);
        storage::extend_balance_ttl(&env, &from, 1000, 500000);
        storage::extend_allowance_ttl(&env, &from, &spender, 1000, 500000);

        env.events().publish(
            (
                Symbol::new(env, event_topics::BURN_FROM),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (from, spender, amount),
        );

        Ok(())
    }

    /// Pause the contract, blocking transfer/transfer_from/mint/burn/
    /// burn_from/admin_burn/batch_transfer until unpause() is called
    /// (issue #657). Admin only.
    pub fn pause(env: Env, admin: Address) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }

        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(TokenError::Unauthorized);
        }

        storage::set_paused(&env, true);
        env.events().publish(
            (
                Symbol::new(env, event_topics::PAUSED),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (admin,),
        );

        Ok(())
    }

    /// Unpause the contract, re-enabling fund-moving operations
    /// (issue #657). Admin only.
    pub fn unpause(env: Env, admin: Address) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }

        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(TokenError::Unauthorized);
        }

        storage::set_paused(&env, false);
        env.events().publish(
            (
                Symbol::new(env, event_topics::UNPAUSED),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (admin,),
        );

        Ok(())
    }

    /// Check whether the contract is currently paused. Read-only, no auth
    /// required (issue #657).
    pub fn is_paused(env: Env) -> Result<bool, TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        Ok(storage::is_paused(&env))
    }

    /// Transfer to multiple recipients in a single call (issue #656). The
    /// whole batch is validated up front — if any leg would fail (a
    /// non-positive amount or insufficient total balance), the entire call
    /// returns Err and Soroban reverts every storage change made during this
    /// invocation, so no partial transfer can ever be observed. An empty
    /// `recipients` list is rejected as InvalidAmount, mirroring how a
    /// non-positive amount is rejected everywhere else in this contract.
    ///
    /// Self-transfers within the batch are permitted and treated as harmless
    /// no-ops, matching the standard SEP-41/Soroban token interface.
    pub fn batch_transfer(
        env: Env,
        from: Address,
        recipients: Vec<(Address, i128)>,
    ) -> Result<(), TokenError> {
        if !storage::is_initialized(&env) {
            return Err(TokenError::NotInitialized);
        }
        require_not_paused(&env)?;

        from.require_auth();

        if recipients.is_empty() {
            return Err(TokenError::InvalidAmount);
        }

        let mut total: i128 = 0;
        for (to, amount) in recipients.iter() {
            if amount <= 0 {
                return Err(TokenError::InvalidAmount);
            }
            if to == from {
                // Self-transfer is a no-op in the standard token interface.
                continue;
            }
            total = total.checked_add(amount).ok_or(TokenError::Overflow)?;
        }

        let from_balance = storage::get_balance(&env, &from);
        if from_balance < total {
            return Err(TokenError::InsufficientBalance);
        }

        let mut touched = Vec::new(&env);
        touched.push_back(from.clone());

        storage::set_balance(
            &env,
            &from,
            from_balance.checked_sub(total).ok_or(TokenError::Overflow)?,
        );
        for (to, amount) in recipients.iter() {
            let recipient_balance = storage::get_balance(&env, &to);
            let new_recipient_balance = recipient_balance
                .checked_add(amount)
                .ok_or(TokenError::Overflow)?;
            storage::set_balance(&env, &to, new_recipient_balance);
            touched.push_back(to.clone());
        }

        for address in touched.iter() {
            storage::extend_balance_ttl(&env, &address, 1000, 500000);
        }

        // Emit per-leg detail so off-chain observers can reconstruct exactly who
        // received how much from a batch transfer using events alone: one
        // `batch_leg` event (from, to, amount) per recipient — mirroring the
        // shape of `transfer`'s event — followed by a `batch_tr` summary
        // carrying the full recipient/amount list and the leg count.
        for (to, amount) in recipients.iter() {
            env.events()
                .publish((symbol_short!("batch_leg"),), (from.clone(), to, amount));
        }
        env.events().publish(
            (symbol_short!("batch_tr"),),
            (from, recipients.clone(), recipients.len()),
        );

        Ok(())
    }

    // ========================================================================
    // Metadata Allowlist Management (Admin Only)
    // ========================================================================

    /// Add a metadata key to the admin-registered allowlist.
    /// Only admin can register allowed keys.
    ///
    /// # Arguments
    /// * `admin` - Admin address authorizing the operation.
    /// * `key` - The metadata key to allow (e.g., "website", "twitter").
    ///
    /// # Errors
    /// * `MetadataError::NotInitialized` - If contract is not initialized.
    /// * `MetadataError::Unauthorized` - If caller is not admin.
    /// * `MetadataError::InvalidKey` - If key is empty.
    /// * `MetadataError::KeyAlreadyExists` - If key is already allowed.
    pub fn add_allowed_metadata_key(
        env: Env,
        admin: Address,
        key: Symbol,
    ) -> Result<(), MetadataError> {
        if !storage::is_initialized(&env) {
            return Err(MetadataError::NotInitialized);
        }

        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(MetadataError::Unauthorized);
        }

        // Validate key is not empty (compare with empty symbol)
        let empty_key = Symbol::new(&env, "");
        if key == empty_key {
            return Err(MetadataError::InvalidKey);
        }

        // Check if key is already allowed
        if storage::is_metadata_key_allowed(&env, &key) {
            return Err(MetadataError::KeyAlreadyExists);
        }

        storage::add_allowed_metadata_key(&env, &key);

        env.events().publish(
            (
                Symbol::new(env, event_topics::METADATA_ADDED),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (admin, key),
        );

        Ok(())
    }

    /// Remove a metadata key from the admin-registered allowlist.
    /// Only admin can remove allowed keys.
    ///
    /// # Arguments
    /// * `admin` - Admin address authorizing the operation.
    /// * `key` - The metadata key to remove from allowlist.
    ///
    /// # Errors
    /// * `MetadataError::NotInitialized` - If contract is not initialized.
    /// * `MetadataError::Unauthorized` - If caller is not admin.
    /// * `MetadataError::KeyNotFound` - If key is not in allowlist.
    pub fn remove_allowed_metadata_key(
        env: Env,
        admin: Address,
        key: Symbol,
    ) -> Result<(), MetadataError> {
        if !storage::is_initialized(&env) {
            return Err(MetadataError::NotInitialized);
        }

        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(MetadataError::Unauthorized);
        }

        // Check if key exists in allowlist
        if !storage::is_metadata_key_allowed(&env, &key) {
            return Err(MetadataError::KeyNotFound);
        }

        storage::remove_allowed_metadata_key(&env, &key);

        env.events().publish(
            (
                Symbol::new(env, event_topics::METADATA_REMOVED),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (admin, key),
        );

        Ok(())
    }

    /// Check if a metadata key is in the admin-registered allowlist.
    ///
    /// # Arguments
    /// * `key` - The metadata key to check.
    ///
    /// # Returns
    /// * `bool` - True if the key is allowed, false otherwise.
    pub fn is_metadata_key_allowed(env: Env, key: Symbol) -> Result<bool, MetadataError> {
        if !storage::is_initialized(&env) {
            return Err(MetadataError::NotInitialized);
        }

        Ok(storage::is_metadata_key_allowed(&env, &key))
    }

    // ========================================================================
    // Token Metadata Management (Admin Only)
    // ========================================================================

    /// Set a metadata key-value pair for the token.
    /// Only admin can set metadata, and only for allowed keys.
    ///
    /// # Arguments
    /// * `admin` - Admin address authorizing the operation.
    /// * `key` - The metadata key (must be in allowlist).
    /// * `value` - The metadata value.
    ///
    /// # Errors
    /// * `MetadataError::NotInitialized` - If contract is not initialized.
    /// * `MetadataError::Unauthorized` - If caller is not admin.
    /// * `MetadataError::KeyNotAllowed` - If key is not in allowlist.
    /// * `MetadataError::InvalidValue` - If value is empty.
    pub fn set_metadata(
        env: Env,
        admin: Address,
        key: Symbol,
        value: String,
    ) -> Result<(), MetadataError> {
        if !storage::is_initialized(&env) {
            return Err(MetadataError::NotInitialized);
        }

        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(MetadataError::Unauthorized);
        }

        // Validate key is in allowlist
        if !storage::is_metadata_key_allowed(&env, &key) {
            return Err(MetadataError::KeyNotAllowed);
        }

        // Validate value is not empty
        if value.is_empty() {
            return Err(MetadataError::InvalidValue);
        }

        storage::set_metadata(&env, &key, &value);

        env.events().publish(
            (
                Symbol::new(env, event_topics::METADATA_SET),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (admin, key, value),
        );

        Ok(())
    }

    /// Get a metadata value by key.
    ///
    /// # Arguments
    /// * `key` - The metadata key to retrieve.
    ///
    /// # Returns
    /// * `Option<String>` - The metadata value if exists, None otherwise.
    ///
    /// # Errors
    /// * `MetadataError::NotInitialized` - If contract is not initialized.
    pub fn get_metadata(env: Env, key: Symbol) -> Result<Option<String>, MetadataError> {
        if !storage::is_initialized(&env) {
            return Err(MetadataError::NotInitialized);
        }

        Ok(storage::get_metadata(&env, &key))
    }

    /// Remove a metadata key-value pair.
    /// Only admin can remove metadata.
    ///
    /// # Arguments
    /// * `admin` - Admin address authorizing the operation.
    /// * `key` - The metadata key to remove.
    ///
    /// # Errors
    /// * `MetadataError::NotInitialized` - If contract is not initialized.
    /// * `MetadataError::Unauthorized` - If caller is not admin.
    /// * `MetadataError::KeyNotFound` - If key does not exist.
    pub fn remove_metadata(env: Env, admin: Address, key: Symbol) -> Result<(), MetadataError> {
        if !storage::is_initialized(&env) {
            return Err(MetadataError::NotInitialized);
        }

        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(MetadataError::Unauthorized);
        }

        // Check if metadata exists
        if !storage::has_metadata(&env, &key) {
            return Err(MetadataError::KeyNotFound);
        }

        storage::remove_metadata(&env, &key);

        env.events().publish(
            (
                Symbol::new(env, event_topics::METADATA_DELETED),
                Symbol::new(env, event_topics::EVENT_SCHEMA_VERSION_STR),
            ),
            (admin, key),
        );

        Ok(())
    }
}
