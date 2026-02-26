use crate::types::*;
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

/// Returns the proposed admin address if set.
pub fn get_proposed_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::ProposedAdmin)
}

/// Store the proposed admin address in instance storage.
pub fn set_proposed_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::ProposedAdmin, admin);
}

/// Clear the proposed admin address from instance storage.
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

/// Assign a role to an address (supports multiple roles per address)
pub fn set_role(env: &Env, address: &Address, role: &Role) {
    let key = DataKey::UserRole(address.clone(), role.clone());
    env.storage().instance().set(&key, &true);
    // also set legacy single-role slot for compatibility for the primary role
    env.storage()
        .instance()
        .set(&DataKey::Role(address.clone()), role);
}

/// Check if an address has a specific role
pub fn has_role(env: &Env, address: &Address, role: &Role) -> bool {
    let key = DataKey::UserRole(address.clone(), role.clone());
    env.storage().instance().get(&key).unwrap_or(false)
}

/// Retrieve the role assigned to an address (legacy compatibility). Returns None if not set.
pub fn get_role(env: &Env, address: &Address) -> Option<Role> {
    env.storage()
        .instance()
        .get(&DataKey::Role(address.clone()))
}

/// Grant Company role to an address (legacy compatibility)
pub fn set_company_role(env: &Env, company: &Address) {
    set_role(env, company, &Role::Company);
}

/// Backwards-compatible: grant Carrier role to an address.
pub fn set_carrier_role(env: &Env, carrier: &Address) {
    set_role(env, carrier, &Role::Carrier);
}

/// Check whether an address has Company role (legacy compatibility)
#[allow(dead_code)]
pub fn has_company_role(env: &Env, address: &Address) -> bool {
    has_role(env, address, &Role::Company)
}

/// Check whether an address has Carrier role (legacy compatibility)
#[allow(dead_code)]
pub fn has_carrier_role(env: &Env, address: &Address) -> bool {
    has_role(env, address, &Role::Carrier)
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
    env.storage().persistent().set(&key, hash); // Redundant identical set, keeping original logic
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

/// Get total escrow volume processed by the contract.
pub fn get_total_escrow_volume(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalEscrowVolume)
        .unwrap_or(0)
}

/// Add an amount to the total escrow volume.
pub fn add_total_escrow_volume(env: &Env, amount: i128) {
    let current = get_total_escrow_volume(env);
    env.storage()
        .instance()
        .set(&DataKey::TotalEscrowVolume, &(current + amount));
}

/// Get the total number of disputes raised.
pub fn get_total_disputes(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::TotalDisputes)
        .unwrap_or(0)
}

/// Increment the total disputes counter by 1.
pub fn increment_total_disputes(env: &Env) {
    let current = get_total_disputes(env);
    env.storage()
        .instance()
        .set(&DataKey::TotalDisputes, &(current + 1));
}

// ============= Pause / Unpause Storage Functions =============

/// Check if the contract is paused.
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::IsPaused)
        .unwrap_or(false)
}

/// Set the paused state of the contract.
pub fn set_paused(env: &Env, paused: bool) {
    env.storage().instance().set(&DataKey::IsPaused, &paused);
}

/// Get the count of shipments with a specific status.
pub fn get_status_count(env: &Env, status: &ShipmentStatus) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::StatusCount(status.clone()))
        .unwrap_or(0)
}

/// Increment the count of shipments with a specific status.
pub fn increment_status_count(env: &Env, status: &ShipmentStatus) {
    let current = get_status_count(env, status);
    env.storage()
        .instance()
        .set(&DataKey::StatusCount(status.clone()), &(current + 1));
}

/// Decrement the count of shipments with a specific status.
pub fn decrement_status_count(env: &Env, status: &ShipmentStatus) {
    let current = get_status_count(env, status);
    if current > 0 {
        env.storage()
            .instance()
            .set(&DataKey::StatusCount(status.clone()), &(current - 1));
    }
}

// ============= Shipment Limit Storage Functions =============

/// Get the configurable limit on active shipments per company.
/// Defaults to 100 if not set.
pub fn get_shipment_limit(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::ShipmentLimit)
        .unwrap_or(100)
}

/// Set the configurable limit on active shipments.
pub fn set_shipment_limit(env: &Env, limit: u32) {
    env.storage()
        .instance()
        .set(&DataKey::ShipmentLimit, &limit);
}

/// Get the current active shipment count for a company.
pub fn get_active_shipment_count(env: &Env, company: &Address) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::ActiveShipmentCount(company.clone()))
        .unwrap_or(0)
}

/// Set the active shipment count for a company.
pub fn set_active_shipment_count(env: &Env, company: &Address, count: u32) {
    env.storage()
        .instance()
        .set(&DataKey::ActiveShipmentCount(company.clone()), &count);
}

/// Increment the active shipment count for a company.
pub fn increment_active_shipment_count(env: &Env, company: &Address) {
    let current = get_active_shipment_count(env, company);
    set_active_shipment_count(env, company, current.saturating_add(1));
}

/// Decrement the active shipment count for a company.
pub fn decrement_active_shipment_count(env: &Env, company: &Address) {
    let current = get_active_shipment_count(env, company);
    set_active_shipment_count(env, company, current.saturating_sub(1));
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
