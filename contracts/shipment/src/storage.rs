use crate::{errors::NavinError, types::*};
use soroban_sdk::{Address, BytesN, Env};

/// Check if the contract has been initialized (admin set).
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `bool` - True if the contract is initialized.
///
/// # Examples
/// ```rust
/// // let initialized = storage::is_initialized(&env);
/// ```
pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

/// Returns the stored admin address. Panics if not set.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `Address` - The contract's admin address.
///
/// # Errors
/// Panics if the `Admin` key is not found in instance storage.
///
/// # Examples
/// ```rust
/// // let admin = storage::get_admin(&env);
/// ```
pub fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

/// Store the admin address in instance storage (config scope).
///
/// # Arguments
/// * `env` - The execution environment.
/// * `admin` - The address to be set as admin.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_admin(&env, &admin_address);
/// ```
pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

/// Returns the proposed admin address from instance storage, if set.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `Option<Address>` - The proposed admin address, or `None`.
///
/// # Examples
/// ```rust
/// // let proposed = storage::get_proposed_admin(&env);
/// ```
pub fn get_proposed_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::ProposedAdmin)
}

/// Store the proposed admin address in instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `admin` - The address being proposed as the new admin.
///
/// # Examples
/// ```rust
/// // storage::set_proposed_admin(&env, &new_admin_addr);
/// ```
pub fn set_proposed_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::ProposedAdmin, admin);
}

/// Clear the proposed admin address from instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Examples
/// ```rust
/// // storage::clear_proposed_admin(&env);
/// ```
pub fn clear_proposed_admin(env: &Env) {
    env.storage().instance().remove(&DataKey::ProposedAdmin);
}

/// Get the contract version number.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `u32` - The current contract version. Default is 1.
///
/// # Examples
/// ```rust
/// // let version = storage::get_version(&env);
/// ```
pub fn get_version(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::Version).unwrap_or(1)
}

/// Set the contract version number.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `version` - The version number to set.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_version(&env, 2);
/// ```
pub fn set_version(env: &Env, version: u32) {
    env.storage().instance().set(&DataKey::Version, &version);
}

/// Get the current shipment counter from instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `u64` - The number of shipments created so far. Defaults to 0.
///
/// # Examples
/// ```rust
/// // let counter = storage::get_shipment_counter(&env);
/// ```
pub fn get_shipment_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::ShipmentCount)
        .unwrap_or(0)
}

/// Set the shipment counter in instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `counter` - The new value for the shipment count.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_shipment_counter(&env, 10);
/// ```
pub fn set_shipment_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&DataKey::ShipmentCount, &counter);
}

/// Increment the shipment counter by 1 and return the new value.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `u64` - The incremented shipment count.
///
/// # Examples
/// ```rust
/// // let next_id = storage::increment_shipment_counter(&env);
/// ```
#[allow(dead_code)]
pub fn increment_shipment_counter(env: &Env) -> u64 {
    let cur = get_shipment_counter(env);
    let next = cur.checked_add(1).unwrap_or(cur);
    set_shipment_counter(env, next);
    next
}

/// Alternate name requested: returns the shipment count (wrapper).
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `u64` - The shipment count.
///
/// # Examples
/// ```rust
/// // let count = storage::get_shipment_count(&env);
/// ```
#[allow(dead_code)]
pub fn get_shipment_count(env: &Env) -> u64 {
    get_shipment_counter(env)
}

/// Alternate name requested: increment shipment count and return new value.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `u64` - The incremented shipment count.
///
/// # Examples
/// ```rust
/// // let next_id = storage::increment_shipment_count(&env);
/// ```
#[allow(dead_code)]
pub fn increment_shipment_count(env: &Env) -> u64 {
    increment_shipment_counter(env)
}

/// Add a carrier to a company's whitelist in instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `company` - The company's address.
/// * `carrier` - The carrier's address.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::add_carrier_to_whitelist(&env, &company_addr, &carrier_addr);
/// ```
pub fn add_carrier_to_whitelist(env: &Env, company: &Address, carrier: &Address) {
    let key = DataKey::CarrierWhitelist(company.clone(), carrier.clone());
    env.storage().instance().set(&key, &true);
}

/// Remove a carrier from a company's whitelist in instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `company` - The company's address.
/// * `carrier` - The carrier's address.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::remove_carrier_from_whitelist(&env, &company_addr, &carrier_addr);
/// ```
pub fn remove_carrier_from_whitelist(env: &Env, company: &Address, carrier: &Address) {
    let key = DataKey::CarrierWhitelist(company.clone(), carrier.clone());
    env.storage().instance().remove(&key);
}

/// Check whether a carrier is whitelisted for a given company.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `company` - The company's address.
/// * `carrier` - The carrier's address.
///
/// # Returns
/// * `bool` - True if the carrier is whitelisted for the company.
///
/// # Examples
/// ```rust
/// // let whitelisted = storage::is_carrier_whitelisted(&env, &company_addr, &carrier_addr);
/// ```
pub fn is_carrier_whitelisted(env: &Env, company: &Address, carrier: &Address) -> bool {
    let key = DataKey::CarrierWhitelist(company.clone(), carrier.clone());
    env.storage().instance().get(&key).unwrap_or(false)
}

/// Assign a role to an address in instance storage.
///
/// Supports multiple roles per address via `UserRole(address, role)` keys
/// and also sets the legacy `Role(address)` key for backward compatibility.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `address` - The address to assign the role to.
/// * `role` - The role to assign.
///
/// # Examples
/// ```rust
/// // storage::set_role(&env, &user_addr, &Role::Company);
/// ```
pub fn set_role(env: &Env, address: &Address, role: &Role) {
    let key = DataKey::UserRole(address.clone(), role.clone());
    env.storage().instance().set(&key, &true);
    // also set legacy single-role slot for compatibility for the primary role
    env.storage()
        .instance()
        .set(&DataKey::Role(address.clone()), role);
}

/// Check if an address has a specific role in instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `address` - The address to check.
/// * `role` - The role to check for.
///
/// # Returns
/// * `bool` - True if the address has the specified role.
///
/// # Examples
/// ```rust
/// // let is_company = storage::has_role(&env, &user_addr, &Role::Company);
/// ```
pub fn has_role(env: &Env, address: &Address, role: &Role) -> bool {
    let key = DataKey::UserRole(address.clone(), role.clone());
    env.storage().instance().get(&key).unwrap_or(false)
}

/// Retrieve the primary role assigned to an address from instance storage.
///
/// Uses the legacy `Role(address)` key for backward compatibility.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `address` - The address to look up.
///
/// # Returns
/// * `Option<Role>` - The role if assigned, or `None`.
///
/// # Examples
/// ```rust
/// // let role = storage::get_role(&env, &user_addr);
/// ```
pub fn get_role(env: &Env, address: &Address) -> Option<Role> {
    env.storage()
        .instance()
        .get(&DataKey::Role(address.clone()))
}

/// Grant the `Company` role to an address.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `company` - The address to grant the Company role to.
///
/// # Examples
/// ```rust
/// // storage::set_company_role(&env, &company_addr);
/// ```
pub fn set_company_role(env: &Env, company: &Address) {
    set_role(env, company, &Role::Company);
}

/// Grant the `Carrier` role to an address.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `carrier` - The address to grant the Carrier role to.
///
/// # Examples
/// ```rust
/// // storage::set_carrier_role(&env, &carrier_addr);
/// ```
pub fn set_carrier_role(env: &Env, carrier: &Address) {
    set_role(env, carrier, &Role::Carrier);
}

/// Revoke a role from an address in instance storage.
///
/// Removes the `UserRole(address, role)` key and resets the legacy
/// `Role(address)` key to `Unassigned`.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `address` - The address whose role is being revoked.
/// * `role` - The role to revoke.
///
/// # Examples
/// ```rust
/// // storage::revoke_role(&env, &user_addr, &Role::Company);
/// ```
pub fn revoke_role(env: &Env, address: &Address, role: &Role) {
    let key = DataKey::UserRole(address.clone(), role.clone());
    env.storage().instance().remove(&key);
    // Reset legacy single-role slot to Unassigned
    env.storage()
        .instance()
        .set(&DataKey::Role(address.clone()), &Role::Unassigned);
}

/// Suspend a role temporarily. The role is retained but marked as suspended.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `address` - The address whose role is being suspended.
/// * `role` - The role to suspend.
///
/// # Examples
/// ```rust
/// // storage::suspend_role(&env, &user_addr, &Role::Company);
/// ```
pub fn suspend_role(env: &Env, address: &Address, role: &Role) {
    // Mark as suspended using a separate key
    let suspend_key = DataKey::RoleSuspended(address.clone(), role.clone());
    env.storage().instance().set(&suspend_key, &true);
}

/// Reactivate a suspended role.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `address` - The address whose role is being reactivated.
/// * `role` - The role to reactivate.
///
/// # Examples
/// ```rust
/// // storage::reactivate_role(&env, &user_addr, &Role::Company);
/// ```
pub fn reactivate_role(env: &Env, address: &Address, role: &Role) {
    // Remove suspension flag
    let suspend_key = DataKey::RoleSuspended(address.clone(), role.clone());
    env.storage().instance().remove(&suspend_key);
}

/// Check if a role is suspended
pub fn is_role_suspended(env: &Env, address: &Address, role: &Role) -> bool {
    let suspend_key = DataKey::RoleSuspended(address.clone(), role.clone());
    env.storage().instance().get(&suspend_key).unwrap_or(false)
}

/// Check whether an address has Company role (legacy compatibility)
#[allow(dead_code)]
pub fn has_company_role(env: &Env, address: &Address) -> bool {
    has_role(env, address, &Role::Company)
}

/// Check whether an address has the `Carrier` role.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `address` - The address to check.
///
/// # Returns
/// * `bool` - True if the address has the Carrier role.
///
/// # Examples
/// ```rust
/// // let is_carrier = storage::has_carrier_role(&env, &addr);
/// ```
#[allow(dead_code)]
pub fn has_carrier_role(env: &Env, address: &Address) -> bool {
    has_role(env, address, &Role::Carrier)
}

/// Mark a carrier as suspended in instance storage.
pub fn suspend_carrier(env: &Env, carrier: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::CarrierSuspended(carrier.clone()), &true);
}

/// Remove a carrier suspension flag from instance storage.
pub fn reactivate_carrier(env: &Env, carrier: &Address) {
    env.storage()
        .instance()
        .remove(&DataKey::CarrierSuspended(carrier.clone()));
}

/// Returns true when the carrier has an active suspension flag.
pub fn is_carrier_suspended(env: &Env, carrier: &Address) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::CarrierSuspended(carrier.clone()))
        .unwrap_or(false)
}

/// Mark a company as suspended in instance storage.
pub fn suspend_company(env: &Env, company: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::CompanySuspended(company.clone()), &true);
}

/// Remove a company suspension flag from instance storage.
pub fn reactivate_company(env: &Env, company: &Address) {
    env.storage()
        .instance()
        .remove(&DataKey::CompanySuspended(company.clone()));
}

/// Returns true when the company has an active suspension flag.
pub fn is_company_suspended(env: &Env, company: &Address) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::CompanySuspended(company.clone()))
        .unwrap_or(false)
}

/// Get shipment by ID
pub fn get_shipment(env: &Env, shipment_id: u64) -> Option<Shipment> {
    // First check persistent storage
    if let Some(shipment) = env
        .storage()
        .persistent()
        .get(&DataKey::Shipment(shipment_id))
    {
        return Some(shipment);
    }

    // If not in persistent, check temporary (archived) storage
    env.storage()
        .temporary()
        .get(&DataKey::ArchivedShipment(shipment_id))
}

/// Check whether shipment payload exists in persistent storage.
pub fn has_persistent_shipment(env: &Env, shipment_id: u64) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Shipment(shipment_id))
}

/// Check whether escrow entry exists in persistent storage.
pub fn has_escrow_entry(env: &Env, shipment_id: u64) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Escrow(shipment_id))
}

/// Check whether confirmation hash exists in persistent storage.
pub fn has_confirmation_hash_entry(env: &Env, shipment_id: u64) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::ConfirmationHash(shipment_id))
}

/// Check whether last status update timestamp exists in persistent storage.
pub fn has_last_status_update_entry(env: &Env, shipment_id: u64) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::LastStatusUpdate(shipment_id))
}

/// Check whether event count entry exists in persistent storage.
pub fn has_event_count_entry(env: &Env, shipment_id: u64) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::EventCount(shipment_id))
}

/// Persist a shipment to persistent storage (survives TTL extension).
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment` - The shipment to save.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_shipment(&env, &my_shipment);
/// ```
pub fn set_shipment(env: &Env, shipment: &Shipment) {
    env.storage()
        .persistent()
        .set(&DataKey::Shipment(shipment.id), shipment);
}

/// Get escrow amount for a shipment from persistent storage. Returns 0 if unset.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// * `i128` - The escrow amount, or 0.
///
/// # Examples
/// ```rust
/// // let amt = storage::get_escrow(&env, 1);
/// ```
pub fn get_escrow(env: &Env, shipment_id: u64) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Escrow(shipment_id))
        .unwrap_or(0)
}

/// Set escrow amount for a shipment in persistent storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
/// * `amount` - Escrow amount to set.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_escrow(&env, 1, 1000);
/// ```
#[allow(dead_code)]
pub fn set_escrow(env: &Env, shipment_id: u64, amount: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::Escrow(shipment_id), &amount);
}

/// Get the latest escrow freeze reason code for a shipment.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// * `Option<EscrowFreezeReason>` - Structured freeze reason if present.
pub fn get_escrow_freeze_reason(env: &Env, shipment_id: u64) -> Option<EscrowFreezeReason> {
    env.storage()
        .persistent()
        .get(&DataKey::EscrowFreezeReasonByShipment(shipment_id))
}

/// Store the latest escrow freeze reason code for a shipment.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
/// * `reason` - Structured freeze reason.
pub fn set_escrow_freeze_reason(env: &Env, shipment_id: u64, reason: &EscrowFreezeReason) {
    env.storage()
        .persistent()
        .set(&DataKey::EscrowFreezeReasonByShipment(shipment_id), reason);
}

/// Remove escrow for a shipment from persistent storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment whose escrow is removed.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::remove_escrow(&env, 1);
/// ```
#[allow(dead_code)]
pub fn remove_escrow(env: &Env, shipment_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::Escrow(shipment_id));
}

/// Backwards-compatible name used by tests: set escrow balance.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
/// * `amount` - Escrow balance to set.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_escrow_balance(&env, 1, 1000);
/// ```
#[allow(dead_code)]
pub fn set_escrow_balance(env: &Env, shipment_id: u64, amount: i128) {
    set_escrow(env, shipment_id, amount);
}

/// Backwards-compatible name used by tests: remove escrow balance.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::remove_escrow_balance(&env, 1);
/// ```
#[allow(dead_code)]
pub fn remove_escrow_balance(env: &Env, shipment_id: u64) {
    remove_escrow(env, shipment_id);
}

/// Store confirmation hash for a shipment in persistent storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
/// * `hash` - The SHA-256 data hash to store.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_confirmation_hash(&env, 1, &hash);
/// ```
pub fn set_confirmation_hash(env: &Env, shipment_id: u64, hash: &BytesN<32>) {
    let key = DataKey::ConfirmationHash(shipment_id);
    env.storage().persistent().set(&key, hash);
}

/// Retrieve confirmation hash for a shipment from persistent storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// * `Option<BytesN<32>>` - The hash if it exists.
///
/// # Examples
/// ```rust
/// // let hash_opt = storage::get_confirmation_hash(&env, 1);
/// ```
#[allow(dead_code)]
pub fn get_confirmation_hash(env: &Env, shipment_id: u64) -> Option<BytesN<32>> {
    let key = DataKey::ConfirmationHash(shipment_id);
    env.storage().persistent().get(&key)
}

/// Extend TTL for shipment data
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
/// * `threshold` - Minimum ledgers remaining before extension is triggered.
/// * `extend_to` - Ledgers to extend the TTL to.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::extend_shipment_ttl(&env, 1, 1000, 500000);
/// ```
pub fn extend_shipment_ttl(env: &Env, shipment_id: u64, threshold: u32, extend_to: u32) {
    let key = DataKey::Shipment(shipment_id);
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, threshold, extend_to);
    }

    let escrow_key = DataKey::Escrow(shipment_id);
    if env.storage().persistent().has(&escrow_key) {
        env.storage()
            .persistent()
            .extend_ttl(&escrow_key, threshold, extend_to);
    }

    let hash_key = DataKey::ConfirmationHash(shipment_id);
    if env.storage().persistent().has(&hash_key) {
        env.storage()
            .persistent()
            .extend_ttl(&hash_key, threshold, extend_to);
    }

    let freeze_reason_key = DataKey::EscrowFreezeReasonByShipment(shipment_id);
    if env.storage().persistent().has(&freeze_reason_key) {
        env.storage()
            .persistent()
            .extend_ttl(&freeze_reason_key, threshold, extend_to);
    }
}

/// Backwards-compatible wrapper used by existing contract code/tests.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// * `i128` - Escrow balance of the shipment.
///
/// # Examples
/// ```rust
/// // let balance = storage::get_escrow_balance(&env, 1);
/// ```
pub fn get_escrow_balance(env: &Env, shipment_id: u64) -> i128 {
    get_escrow(env, shipment_id)
}

/// Get the token contract address
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `Option<Address>` - The token contract address if set.
///
/// # Examples
/// ```rust
/// // let token_addr = storage::get_token_contract(&env);
/// ```
pub fn get_token_contract(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::TokenContract)
}

/// Set the token contract address
///
/// # Arguments
/// * `env` - The execution environment.
/// * `token_contract` - The address of the token contract.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_token_contract(&env, &token_addr);
/// ```
pub fn set_token_contract(env: &Env, token_contract: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::TokenContract, token_contract);
}

/// Retrieve the timestamp of the last status update for a shipment.
/// Returns None if no status update has been recorded yet.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// * `Option<u64>` - The timestamp of the last update if set.
///
/// # Examples
/// ```rust
/// // let last = storage::get_last_status_update(&env, 1);
/// ```
pub fn get_last_status_update(env: &Env, shipment_id: u64) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::LastStatusUpdate(shipment_id))
}

/// Persist the timestamp of the last status update for a shipment.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
/// * `timestamp` - The ledger timestamp to store.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_last_status_update(&env, 1, 1690000000);
/// ```
pub fn set_last_status_update(env: &Env, shipment_id: u64, timestamp: u64) {
    env.storage()
        .persistent()
        .set(&DataKey::LastStatusUpdate(shipment_id), &timestamp);
}

// ============= Multi-Signature Storage Functions =============

/// Get the list of admin addresses for multi-sig.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `Option<Vec<Address>>` - The list of admin addresses if set.
///
/// # Examples
/// ```rust
/// // let admins = storage::get_admin_list(&env);
/// ```
pub fn get_admin_list(env: &Env) -> Option<soroban_sdk::Vec<Address>> {
    env.storage().instance().get(&DataKey::AdminList)
}

/// Set the list of admin addresses for multi-sig.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `admins` - The list of admin addresses.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_admin_list(&env, &admins);
/// ```
pub fn set_admin_list(env: &Env, admins: &soroban_sdk::Vec<Address>) {
    env.storage().instance().set(&DataKey::AdminList, admins);
}

/// Get the multi-sig threshold (number of approvals required).
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `Option<u32>` - The threshold if set.
///
/// # Examples
/// ```rust
/// // let threshold = storage::get_multisig_threshold(&env);
/// ```
pub fn get_multisig_threshold(env: &Env) -> Option<u32> {
    env.storage().instance().get(&DataKey::MultiSigThreshold)
}

/// Set the multi-sig threshold.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `threshold` - The number of approvals required.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_multisig_threshold(&env, 2);
/// ```
pub fn set_multisig_threshold(env: &Env, threshold: u32) {
    env.storage()
        .instance()
        .set(&DataKey::MultiSigThreshold, &threshold);
}

/// Get the current proposal counter.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `u64` - The number of proposals created so far. Defaults to 0.
///
/// # Examples
/// ```rust
/// // let counter = storage::get_proposal_counter(&env);
/// ```
pub fn get_proposal_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::ProposalCounter)
        .unwrap_or(0)
}

/// Set the proposal counter.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `counter` - The new value for the proposal count.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_proposal_counter(&env, 10);
/// ```
pub fn set_proposal_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&DataKey::ProposalCounter, &counter);
}

/// Retrieve a proposal from persistent storage. Returns None if not found.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `proposal_id` - The ID of the proposal.
///
/// # Returns
/// * `Option<Proposal>` - The proposal data if it exists.
///
/// # Examples
/// ```rust
/// // let proposal = storage::get_proposal(&env, 1);
/// ```
pub fn get_proposal(env: &Env, proposal_id: u64) -> Option<crate::types::Proposal> {
    env.storage()
        .persistent()
        .get(&DataKey::Proposal(proposal_id))
}

/// Persist a proposal to persistent storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `proposal` - The proposal to save.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_proposal(&env, &my_proposal);
/// ```
pub fn set_proposal(env: &Env, proposal: &crate::types::Proposal) {
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(proposal.id), proposal);
}

/// Check if an address is in the admin list.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `address` - The address to check.
///
/// # Returns
/// * `bool` - True if the address is in the admin list.
///
/// # Examples
/// ```rust
/// // let is_admin = storage::is_admin(&env, &address);
/// ```
pub fn is_admin(env: &Env, address: &Address) -> bool {
    if let Some(admins) = get_admin_list(env) {
        for admin in admins.iter() {
            if admin == *address {
                return true;
            }
        }
    }
    false
}

// ============= Analytics Storage Functions =============

/// Get the total escrow volume processed by the contract from instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `i128` - The cumulative escrow volume. Defaults to 0.
///
/// # Examples
/// ```rust
/// // let volume = storage::get_total_escrow_volume(&env);
/// ```
pub fn get_total_escrow_volume(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalEscrowVolume)
        .unwrap_or(0)
}

/// Add an amount to the total escrow volume.
pub fn add_total_escrow_volume(env: &Env, amount: i128) -> Result<(), NavinError> {
    let current = get_total_escrow_volume(env);
    let updated = current
        .checked_add(amount)
        .ok_or(NavinError::ArithmeticError)?;
    env.storage()
        .instance()
        .set(&DataKey::TotalEscrowVolume, &updated);
    Ok(())
}

/// Get the total number of disputes raised from instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `u64` - The total dispute count. Defaults to 0.
///
/// # Examples
/// ```rust
/// // let disputes = storage::get_total_disputes(&env);
/// ```
pub fn get_total_disputes(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::TotalDisputes)
        .unwrap_or(0)
}

/// Increment the total disputes counter by 1 in instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Examples
/// ```rust
/// // storage::increment_total_disputes(&env);
/// ```
pub fn increment_total_disputes(env: &Env) {
    let current = get_total_disputes(env);
    env.storage()
        .instance()
        .set(&DataKey::TotalDisputes, &(current + 1));
}

/// Get the count of shipments with a specific status from instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `status` - The shipment status to query.
///
/// # Returns
/// * `u64` - The count of shipments with the given status. Defaults to 0.
///
/// # Examples
/// ```rust
/// // let created_count = storage::get_status_count(&env, &ShipmentStatus::Created);
/// ```
pub fn get_status_count(env: &Env, status: &ShipmentStatus) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::StatusCount(status.clone()))
        .unwrap_or(0)
}

/// Increment the count of shipments with a specific status in instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `status` - The shipment status to increment.
///
/// # Examples
/// ```rust
/// // storage::increment_status_count(&env, &ShipmentStatus::Created);
/// ```
pub fn increment_status_count(env: &Env, status: &ShipmentStatus) {
    let current = get_status_count(env, status);
    env.storage()
        .instance()
        .set(&DataKey::StatusCount(status.clone()), &(current + 1));
}

/// Decrement the count of shipments with a specific status in instance storage.
///
/// Saturates at 0 — will not underflow.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `status` - The shipment status to decrement.
///
/// # Examples
/// ```rust
/// // storage::decrement_status_count(&env, &ShipmentStatus::Delivered);
/// ```
pub fn decrement_status_count(env: &Env, status: &ShipmentStatus) {
    let current = get_status_count(env, status);
    if current > 0 {
        env.storage()
            .instance()
            .set(&DataKey::StatusCount(status.clone()), &(current - 1));
    }
}

// ============= Shipment Limit Storage Functions =============

/// Get the configurable limit on active shipments per company from instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `u32` - The maximum active shipments per company. Defaults to 100.
///
/// # Examples
/// ```rust
/// // let limit = storage::get_shipment_limit(&env);
/// ```
pub fn get_shipment_limit(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::ShipmentLimit)
        .unwrap_or(100)
}

/// Set the configurable limit on active shipments in instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `limit` - The maximum number of active shipments allowed per company.
///
/// # Examples
/// ```rust
/// // storage::set_shipment_limit(&env, 200);
/// ```
pub fn set_shipment_limit(env: &Env, limit: u32) {
    env.storage()
        .instance()
        .set(&DataKey::ShipmentLimit, &limit);
}

/// Get the company-specific shipment limit override, if set.
pub fn get_company_shipment_limit(env: &Env, company: &Address) -> Option<u32> {
    env.storage()
        .instance()
        .get(&DataKey::CompanyShipmentLimit(company.clone()))
}

/// Set the company-specific shipment limit override.
pub fn set_company_shipment_limit(env: &Env, company: &Address, limit: u32) {
    env.storage()
        .instance()
        .set(&DataKey::CompanyShipmentLimit(company.clone()), &limit);
}

/// Resolve effective shipment limit (company override first, then global).
pub fn get_effective_shipment_limit(env: &Env, company: &Address) -> u32 {
    get_company_shipment_limit(env, company).unwrap_or_else(|| get_shipment_limit(env))
}

/// Get the current active shipment count for a company from instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `company` - The company address to query.
///
/// # Returns
/// * `u32` - The number of active shipments for the company. Defaults to 0.
///
/// # Examples
/// ```rust
/// // let count = storage::get_active_shipment_count(&env, &company_addr);
/// ```
pub fn get_active_shipment_count(env: &Env, company: &Address) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::ActiveShipmentCount(company.clone()))
        .unwrap_or(0)
}

/// Set the active shipment count for a company in instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `company` - The company address.
/// * `count` - The new active shipment count.
///
/// # Examples
/// ```rust
/// // storage::set_active_shipment_count(&env, &company_addr, 5);
/// ```
pub fn set_active_shipment_count(env: &Env, company: &Address, count: u32) {
    env.storage()
        .instance()
        .set(&DataKey::ActiveShipmentCount(company.clone()), &count);
}

/// Increment the active shipment count for a company in instance storage.
///
/// Uses saturating addition to prevent overflow.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `company` - The company address.
///
/// # Examples
/// ```rust
/// // storage::increment_active_shipment_count(&env, &company_addr);
/// ```
pub fn increment_active_shipment_count(env: &Env, company: &Address) {
    let current = get_active_shipment_count(env, company);
    set_active_shipment_count(env, company, current.saturating_add(1));
}

/// Decrement the active shipment count for a company in instance storage.
///
/// Uses saturating subtraction to prevent underflow.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `company` - The company address.
///
/// # Examples
/// ```rust
/// // storage::decrement_active_shipment_count(&env, &company_addr);
/// ```
pub fn decrement_active_shipment_count(env: &Env, company: &Address) {
    let current = get_active_shipment_count(env, company);
    set_active_shipment_count(env, company, current.saturating_sub(1));
}

// ============= Milestone Event Counter Storage Functions =============

/// Get the milestone event count for a shipment.
/// Returns 0 if no milestone events have been emitted yet.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// * `u32` - The number of milestone events emitted for this shipment.
///
/// # Examples
/// ```rust
/// // let count = storage::get_milestone_event_count(&env, 1);
/// ```
pub fn get_milestone_event_count(env: &Env, shipment_id: u64) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::MilestoneEventCount(shipment_id))
        .unwrap_or(0)
}

/// Increment the milestone event count for a shipment.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::increment_milestone_event_count(&env, 1);
/// ```
pub fn increment_milestone_event_count(env: &Env, shipment_id: u64) {
    let current = get_milestone_event_count(env, shipment_id);
    env.storage().persistent().set(
        &DataKey::MilestoneEventCount(shipment_id),
        &current.saturating_add(1),
    );
}

/// Get the condition breach event count for a shipment.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// * `u32` - The number of condition breach events emitted for this shipment.
pub fn get_breach_event_count(env: &Env, shipment_id: u64) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::BreachEventCount(shipment_id))
        .unwrap_or(0)
}

/// Increment the condition breach event count for a shipment.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
pub fn increment_breach_event_count(env: &Env, shipment_id: u64) {
    let current = get_breach_event_count(env, shipment_id);
    env.storage().persistent().set(
        &DataKey::BreachEventCount(shipment_id),
        &current.saturating_add(1),
    );
}

// ============= Event Counter Storage Functions =============

/// Get the event count for a shipment.
/// Returns 0 if no events have been emitted yet.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// * `u32` - The number of events emitted for this shipment.
///
/// # Examples
/// ```rust
/// // let count = storage::get_event_count(&env, 1);
/// ```
pub fn get_event_count(env: &Env, shipment_id: u64) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::EventCount(shipment_id))
        .unwrap_or(0)
}

/// Increment the event count for a shipment.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::increment_event_count(&env, 1);
/// ```
pub fn increment_event_count(env: &Env, shipment_id: u64) {
    let current = get_event_count(env, shipment_id);
    env.storage().persistent().set(
        &DataKey::EventCount(shipment_id),
        &current.saturating_add(1),
    );
}

// ============= Shipment Archival Storage Functions =============

/// Archive a shipment by moving it from persistent to temporary storage.
/// This reduces state rent costs for completed shipments.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment to archive.
/// * `shipment` - The shipment data to archive.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::archive_shipment(&env, 1, &shipment);
/// ```
pub fn archive_shipment(env: &Env, shipment_id: u64, shipment: &Shipment) {
    // Store in temporary storage (cheaper, shorter TTL)
    env.storage()
        .temporary()
        .set(&DataKey::ArchivedShipment(shipment_id), shipment);

    // Remove from persistent storage
    env.storage()
        .persistent()
        .remove(&DataKey::Shipment(shipment_id));
}

/// Get an archived shipment from temporary storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the archived shipment.
///
/// # Returns
/// * `Option<Shipment>` - The archived shipment if it exists.
///
/// # Examples
/// ```rust
/// // let shipment = storage::get_archived_shipment(&env, 1);
/// ```
#[allow(dead_code)]
pub fn get_archived_shipment(env: &Env, shipment_id: u64) -> Option<Shipment> {
    env.storage()
        .temporary()
        .get(&DataKey::ArchivedShipment(shipment_id))
}

/// Check if a shipment is archived.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// * `bool` - True if the shipment is archived.
///
/// # Examples
/// ```rust
/// // let is_archived = storage::is_shipment_archived(&env, 1);
/// ```
#[allow(dead_code)]
pub fn is_shipment_archived(env: &Env, shipment_id: u64) -> bool {
    env.storage()
        .temporary()
        .has(&DataKey::ArchivedShipment(shipment_id))
}

// ============= Shipment Note Storage Functions =============

/// Get the total number of notes appended to a shipment.
pub fn get_note_count(env: &Env, shipment_id: u64) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::ShipmentNoteCount(shipment_id))
        .unwrap_or(0)
}

/// Increment the note count for a shipment and return the new index.
pub fn increment_note_count(env: &Env, shipment_id: u64) -> u32 {
    let current = get_note_count(env, shipment_id);
    let next = current.checked_add(1).expect("Note count overflow");
    env.storage()
        .persistent()
        .set(&DataKey::ShipmentNoteCount(shipment_id), &next);
    current // Return 0-based index for storage
}

/// Store a note hash for a shipment at a specific index.
pub fn set_note_hash(env: &Env, shipment_id: u64, index: u32, hash: &BytesN<32>) {
    env.storage()
        .persistent()
        .set(&DataKey::ShipmentNote(shipment_id, index), hash);
}

/// Retrieve a note hash for a shipment by its index.
#[allow(dead_code)]
pub fn get_note_hash(env: &Env, shipment_id: u64, index: u32) -> Option<BytesN<32>> {
    env.storage()
        .persistent()
        .get(&DataKey::ShipmentNote(shipment_id, index))
}

// ============= Dispute Evidence Storage Functions =============

/// Get the total number of evidence hashes appended to a shipment dispute.
pub fn get_evidence_count(env: &Env, shipment_id: u64) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::DisputeEvidenceCount(shipment_id))
        .unwrap_or(0)
}

/// Increment the evidence count for a shipment dispute and return the new index.
pub fn increment_evidence_count(env: &Env, shipment_id: u64) -> u32 {
    let current = get_evidence_count(env, shipment_id);
    let next = current.checked_add(1).expect("Evidence count overflow");
    env.storage()
        .persistent()
        .set(&DataKey::DisputeEvidenceCount(shipment_id), &next);
    current // Return 0-based index for storage
}

/// Store an evidence hash for a shipment dispute at a specific index.
pub fn set_evidence_hash(env: &Env, shipment_id: u64, index: u32, hash: &BytesN<32>) {
    env.storage()
        .persistent()
        .set(&DataKey::DisputeEvidence(shipment_id, index), hash);
}

/// Retrieve an evidence hash for a shipment dispute by its index.
pub fn get_evidence_hash(env: &Env, shipment_id: u64, index: u32) -> Option<BytesN<32>> {
    env.storage()
        .persistent()
        .get(&DataKey::DisputeEvidence(shipment_id, index))
}

// ============= Milestone Event Counter Storage Functions =============

// ============= Idempotency Window Storage Functions =============

/// Returns true if the action hash is already within an active idempotency window.
pub fn has_idempotency_window(env: &Env, action_hash: &BytesN<32>) -> bool {
    env.storage()
        .temporary()
        .has(&DataKey::IdempotencyWindow(action_hash.clone()))
}

/// Record an action hash in temporary storage for `window_seconds`.
/// The key expires naturally when the ledger TTL elapses - no cleanup needed.
pub fn set_idempotency_window(env: &Env, action_hash: &BytesN<32>, window_seconds: u64) {
    let key = DataKey::IdempotencyWindow(action_hash.clone());
    // Soroban temporary storage TTL is in ledgers. At ~5 s/ledger we convert
    // seconds to ledgers (rounding up, minimum 1).
    let ledgers = window_seconds.div_ceil(5).max(1) as u32;
    env.storage().temporary().set(&key, &true);
    env.storage().temporary().extend_ttl(&key, 0, ledgers);
}

// ============= Pause/Unpause Storage Functions =============

/// Check if the contract is paused.
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::IsPaused)
        .unwrap_or(false)
}

/// Set the contract pause state.
pub fn set_paused(env: &Env, paused: bool) {
    env.storage().instance().set(&DataKey::IsPaused, &paused);
}

// ============= IoT Hash Verification Storage Functions =============

/// Store the data hash for a specific shipment status transition.
pub fn set_status_hash(env: &Env, shipment_id: u64, status: &ShipmentStatus, hash: &BytesN<32>) {
    env.storage()
        .persistent()
        .set(&DataKey::StatusHash(shipment_id, status.clone()), hash);
}

/// Retrieve the data hash for a specific shipment status transition.
pub fn get_status_hash(env: &Env, shipment_id: u64, status: &ShipmentStatus) -> Option<BytesN<32>> {
    env.storage()
        .persistent()
        .get(&DataKey::StatusHash(shipment_id, status.clone()))
}

// ============= TTL Health Monitoring Functions =============

/// Check if a shipment exists in persistent storage.
///
/// This is used for TTL health monitoring to determine which shipments
/// are still active in persistent storage vs archived.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// * `bool` - True if the shipment exists in persistent storage.
///
/// # Examples
/// ```rust
/// // let exists = storage::shipment_exists_in_persistent(&env, 1);
/// ```
#[allow(dead_code)]
pub fn shipment_exists_in_persistent(env: &Env, shipment_id: u64) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Shipment(shipment_id))
}

// ============= Settlement Tracking Functions =============

/// Get the settlement counter value from instance storage.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `u64` - The number of settlements created so far. Defaults to 0.
///
/// # Examples
/// ```rust
/// // let counter = storage::get_settlement_counter(&env);
/// ```
#[allow(dead_code)]
pub fn get_settlement_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::SettlementCounter)
        .unwrap_or(0)
}

/// Set the settlement counter.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `counter` - The new value for the settlement count.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_settlement_counter(&env, 10);
/// ```
#[allow(dead_code)]
pub fn set_settlement_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&DataKey::SettlementCounter, &counter);
}

/// Increment the settlement counter by 1 and return the new value.
///
/// # Arguments
/// * `env` - The execution environment.
///
/// # Returns
/// * `u64` - The incremented settlement count.
///
/// # Examples
/// ```rust
/// // let next_id = storage::increment_settlement_counter(&env);
/// ```
#[allow(dead_code)]
pub fn increment_settlement_counter(env: &Env) -> u64 {
    let cur = get_settlement_counter(env);
    let next = cur.checked_add(1).unwrap_or(cur);
    set_settlement_counter(env, next);
    next
}

/// Get a settlement record by ID from persistent storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `settlement_id` - The ID of the settlement.
///
/// # Returns
/// * `Option<SettlementRecord>` - The settlement record if it exists.
///
/// # Examples
/// ```rust
/// // let settlement = storage::get_settlement(&env, 1);
/// ```
#[allow(dead_code)]
pub fn get_settlement(env: &Env, settlement_id: u64) -> Option<crate::types::SettlementRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::Settlement(settlement_id))
}

/// Persist a settlement record to persistent storage.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `settlement` - The settlement record to save.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_settlement(&env, &settlement);
/// ```
#[allow(dead_code)]
pub fn set_settlement(env: &Env, settlement: &crate::types::SettlementRecord) {
    env.storage()
        .persistent()
        .set(&DataKey::Settlement(settlement.settlement_id), settlement);
}

/// Get the active settlement ID for a shipment.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// * `Option<u64>` - The active settlement ID if one exists.
///
/// # Examples
/// ```rust
/// // let active_id = storage::get_active_settlement(&env, 1);
/// ```
#[allow(dead_code)]
pub fn get_active_settlement(env: &Env, shipment_id: u64) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::ActiveSettlement(shipment_id))
}

/// Set the active settlement ID for a shipment.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
/// * `settlement_id` - The settlement ID to mark as active.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::set_active_settlement(&env, 1, 100);
/// ```
#[allow(dead_code)]
pub fn set_active_settlement(env: &Env, shipment_id: u64, settlement_id: u64) {
    env.storage()
        .persistent()
        .set(&DataKey::ActiveSettlement(shipment_id), &settlement_id);
}

/// Clear the active settlement for a shipment.
///
/// # Arguments
/// * `env` - The execution environment.
/// * `shipment_id` - The ID of the shipment.
///
/// # Returns
/// No return value.
///
/// # Examples
/// ```rust
/// // storage::clear_active_settlement(&env, 1);
/// ```
#[allow(dead_code)]
pub fn clear_active_settlement(env: &Env, shipment_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::ActiveSettlement(shipment_id));
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::test_utils;
    use soroban_sdk::testutils::Address as _;

    fn with_contract_env() -> (Env, Address) {
        let (env, _) = test_utils::setup_env();
        let contract_id = env.register(crate::NavinShipment, ());
        (env, contract_id)
    }

    #[test]
    fn carrier_whitelist_tuple_key_round_trip_and_order_regression() {
        let (env, contract_id) = with_contract_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);

        env.as_contract(&contract_id, || {
            add_carrier_to_whitelist(&env, &company, &carrier);

            assert!(is_carrier_whitelisted(&env, &company, &carrier));
            assert!(!is_carrier_whitelisted(&env, &carrier, &company));

            let canonical_key = DataKey::CarrierWhitelist(company.clone(), carrier.clone());
            let reversed_key = DataKey::CarrierWhitelist(carrier.clone(), company.clone());
            assert!(env.storage().instance().has(&canonical_key));
            assert!(!env.storage().instance().has(&reversed_key));
        });
    }

    #[test]
    fn user_role_tuple_key_round_trip() {
        let (env, contract_id) = with_contract_env();
        let user = Address::generate(&env);

        env.as_contract(&contract_id, || {
            set_role(&env, &user, &Role::Carrier);

            assert!(has_role(&env, &user, &Role::Carrier));
            assert!(!has_role(&env, &user, &Role::Company));
        });
    }

    #[test]
    fn role_suspended_tuple_key_round_trip() {
        let (env, contract_id) = with_contract_env();
        let user = Address::generate(&env);

        env.as_contract(&contract_id, || {
            suspend_role(&env, &user, &Role::Company);
            assert!(is_role_suspended(&env, &user, &Role::Company));

            reactivate_role(&env, &user, &Role::Company);
            assert!(!is_role_suspended(&env, &user, &Role::Company));
        });
    }

    #[test]
    fn shipment_note_tuple_key_round_trip_and_component_regression() {
        let (env, contract_id) = with_contract_env();
        let shipment_id = 77_u64;
        let note_idx_0 = 0_u32;
        let note_idx_1 = 1_u32;
        let note_0 = BytesN::from_array(&env, &[0x11; 32]);
        let note_1 = BytesN::from_array(&env, &[0x22; 32]);

        env.as_contract(&contract_id, || {
            set_note_hash(&env, shipment_id, note_idx_0, &note_0);
            set_note_hash(&env, shipment_id, note_idx_1, &note_1);

            assert_eq!(get_note_hash(&env, shipment_id, note_idx_0), Some(note_0));
            assert_eq!(get_note_hash(&env, shipment_id, note_idx_1), Some(note_1));
            assert_eq!(get_note_hash(&env, shipment_id + 1, note_idx_0), None);
            assert_eq!(get_note_hash(&env, shipment_id, note_idx_1 + 1), None);
        });
    }

    #[test]
    fn dispute_evidence_tuple_key_round_trip_and_component_regression() {
        let (env, contract_id) = with_contract_env();
        let shipment_id = 900_u64;
        let evidence_idx_0 = 0_u32;
        let evidence_idx_1 = 1_u32;
        let evidence_0 = BytesN::from_array(&env, &[0x33; 32]);
        let evidence_1 = BytesN::from_array(&env, &[0x44; 32]);

        env.as_contract(&contract_id, || {
            set_evidence_hash(&env, shipment_id, evidence_idx_0, &evidence_0);
            set_evidence_hash(&env, shipment_id, evidence_idx_1, &evidence_1);

            assert_eq!(
                get_evidence_hash(&env, shipment_id, evidence_idx_0),
                Some(evidence_0)
            );
            assert_eq!(
                get_evidence_hash(&env, shipment_id, evidence_idx_1),
                Some(evidence_1)
            );
            assert_eq!(
                get_evidence_hash(&env, shipment_id + 1, evidence_idx_0),
                None
            );
            assert_eq!(
                get_evidence_hash(&env, shipment_id, evidence_idx_1 + 1),
                None
            );
        });
    }
}

// ============= Settlement State Storage Functions =============
