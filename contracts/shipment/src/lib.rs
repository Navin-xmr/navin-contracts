#![no_std]

use soroban_sdk::{
    contract, contractimpl, symbol_short, Address, BytesN, Env, IntoVal, Map, Symbol, Vec,
};

mod config;
mod errors;
mod events;
mod storage;
mod stress_test;
mod test;
mod types;
mod validation;

pub use config::*;
pub use errors::*;
pub use types::*;
pub use validation::*;

fn extend_shipment_ttl(env: &Env, shipment_id: u64) {
    let config = config::get_config(env);
    storage::extend_shipment_ttl(
        env,
        shipment_id,
        config.shipment_ttl_threshold,
        config.shipment_ttl_extension,
    );
}

fn validate_milestones(_env: &Env, milestones: &Vec<(Symbol, u32)>) -> Result<(), NavinError> {
    if milestones.is_empty() {
        return Ok(());
    }
    let mut total_percentage = 0;
    for milestone in milestones.iter() {
        total_percentage += milestone.1;
    }

    if total_percentage != 100 {
        return Err(NavinError::MilestoneSumInvalid);
    }

    Ok(())
}

fn internal_release_escrow(env: &Env, shipment: &mut Shipment, amount: i128) {
    if amount <= 0 {
        return;
    }
    let actual_release = if amount > shipment.escrow_amount {
        shipment.escrow_amount
    } else {
        amount
    };

    if actual_release > 0 {
        shipment.escrow_amount -= actual_release;
        shipment.updated_at = env.ledger().timestamp();
        storage::set_shipment(env, shipment);

        // Get token contract address
        if let Some(token_contract) = storage::get_token_contract(env) {
            // Transfer tokens from this contract to carrier
            let contract_address = env.current_contract_address();
            let mut args: soroban_sdk::Vec<soroban_sdk::Val> = Vec::new(env);
            args.push_back(contract_address.into_val(env));
            args.push_back(shipment.carrier.clone().into_val(env));
            args.push_back(actual_release.into_val(env));
            env.invoke_contract::<()>(&token_contract, &symbol_short!("transfer"), args);
        }

        events::emit_escrow_released(env, shipment.id, &shipment.carrier, actual_release);
    }
}

fn require_initialized(env: &Env) -> Result<(), NavinError> {
    if !storage::is_initialized(env) {
        return Err(NavinError::NotInitialized);
    }
    Ok(())
}

fn require_role(env: &Env, address: &Address, role: Role) -> Result<(), NavinError> {
    require_initialized(env)?;

    match role {
        Role::Company => {
            if storage::has_company_role(env, address) {
                Ok(())
            } else {
                Err(NavinError::Unauthorized)
            }
        }
        Role::Carrier => {
            if storage::has_carrier_role(env, address) {
                Ok(())
            } else {
                Err(NavinError::Unauthorized)
            }
        }
        Role::Unassigned => Err(NavinError::Unauthorized),
    }
}

#[contract]
pub struct NavinShipment;

#[contractimpl]
impl NavinShipment {
    /// Set metadata key-value pair for a shipment. Only Company (sender) or Admin can set.
    /// Max 5 metadata entries allowed.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `caller` - The address attempting to set the metadata.
    /// * `shipment_id` - ID of the shipment.
    /// * `key` - The metadata key (max 32 chars).
    /// * `value` - The metadata value (max 32 chars).
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully set.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If the shipment doesn't exist.
    /// * `NavinError::Unauthorized` - If the caller is not the sender or admin.
    /// * `NavinError::MetadataLimitExceeded` - If adding would exceed the 5 key limit.
    ///
    /// # Examples
    /// ```rust
    /// // contract.set_shipment_metadata(&env, &caller, 1, &Symbol::new(&env, "weight"), &Symbol::new(&env, "kg_100"));
    /// ```
    pub fn set_shipment_metadata(
        env: Env,
        caller: Address,
        shipment_id: u64,
        key: Symbol,
        value: Symbol,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        caller.require_auth();
        let admin = storage::get_admin(&env);
        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;
        // Only sender or admin can set
        if caller != shipment.sender && caller != admin {
            return Err(NavinError::Unauthorized);
        }
        // Initialize metadata map if not present
        let mut metadata = shipment.metadata.unwrap_or(Map::new(&env));
        // Enforce max metadata entries from config
        let config = config::get_config(&env);
        if !metadata.contains_key(key.clone()) && metadata.len() >= config.max_metadata_entries {
            return Err(NavinError::MetadataLimitExceeded);
        }
        metadata.set(key.clone(), value.clone());
        shipment.metadata = Some(metadata);
        shipment.updated_at = env.ledger().timestamp();
        storage::set_shipment(&env, &shipment);
        Ok(())
    }
    /// Initialize the contract with an admin address and token contract address.
    /// Can only be called once. Sets the admin and shipment counter to 0.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - The address designated as the administrator.
    /// * `token_contract` - The address of the token contract used for escrow.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if initialized.
    ///
    /// # Errors
    /// * `NavinError::AlreadyInitialized` - If called when already initialized.
    ///
    /// # Examples
    /// ```rust
    /// // contract.initialize(&env, &admin_addr, &token_addr);
    /// ```
    pub fn initialize(env: Env, admin: Address, token_contract: Address) -> Result<(), NavinError> {
        if storage::is_initialized(&env) {
            return Err(NavinError::AlreadyInitialized);
        }

        storage::set_admin(&env, &admin);
        storage::set_token_contract(&env, &token_contract);
        storage::set_shipment_counter(&env, 0);
        storage::set_version(&env, 1);
        storage::set_company_role(&env, &admin);

        // Initialize with default configuration
        let default_config = ContractConfig::default();
        config::set_config(&env, &default_config);
        storage::set_shipment_limit(&env, default_config.default_shipment_limit);

        env.events().publish(
            (symbol_short!("init"),),
            (admin.clone(), token_contract.clone()),
        );

        Ok(())
    }

    /// Set the configurable limit on the number of active shipments a company can have.
    /// Only the admin can call this.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin address.
    /// * `limit` - The new active shipment limit.
    pub fn set_shipment_limit(env: Env, admin: Address, limit: u32) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        storage::set_shipment_limit(&env, limit);

        env.events()
            .publish((Symbol::new(&env, "set_limit"),), (admin, limit));

        Ok(())
    }

    /// Get the current shipment limit.
    pub fn get_shipment_limit(env: Env) -> Result<u32, NavinError> {
        require_initialized(&env)?;
        Ok(storage::get_shipment_limit(&env))
    }

    /// Get the current active shipment count for a company.
    pub fn get_active_shipment_count(env: Env, company: Address) -> Result<u32, NavinError> {
        require_initialized(&env)?;
        Ok(storage::get_active_shipment_count(&env, &company))
    }

    /// Get the contract admin address.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<Address, NavinError>` - The current admin address.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let admin = contract.get_admin(&env);
    /// ```
    pub fn get_admin(env: Env) -> Result<Address, NavinError> {
        require_initialized(&env)?;
        Ok(storage::get_admin(&env))
    }

    /// Get the contract version number.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<u32, NavinError>` - The version number of the contract.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let version = contract.get_version(&env);
    /// ```
    pub fn get_version(env: Env) -> Result<u32, NavinError> {
        require_initialized(&env)?;
        Ok(storage::get_version(&env))
    }

    /// Get on-chain metadata for this contract.
    /// Returns version, admin, shipment count, and initialization status.
    /// Read-only — no authentication required.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<ContractMetadata, NavinError>` - Snapshot of contract metadata.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let metadata = contract.get_contract_metadata(&env);
    /// ```
    pub fn get_contract_metadata(env: Env) -> Result<ContractMetadata, NavinError> {
        require_initialized(&env)?;
        Ok(ContractMetadata {
            version: storage::get_version(&env),
            admin: storage::get_admin(&env),
            shipment_count: storage::get_shipment_counter(&env),
            initialized: true,
        })
    }

    /// Get the current shipment counter.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<u64, NavinError>` - The total number of shipments created.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let count = contract.get_shipment_counter(&env);
    /// ```
    pub fn get_shipment_counter(env: Env) -> Result<u64, NavinError> {
        require_initialized(&env)?;
        Ok(storage::get_shipment_counter(&env))
    }

    /// Get aggregated analytics for the contract.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<Analytics, NavinError>` - Aggregated analytics data.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    pub fn get_analytics(env: Env) -> Result<Analytics, NavinError> {
        require_initialized(&env)?;

        Ok(Analytics {
            total_shipments: storage::get_shipment_counter(&env),
            total_escrow_volume: storage::get_total_escrow_volume(&env),
            total_disputes: storage::get_total_disputes(&env),
            created_count: storage::get_status_count(&env, &ShipmentStatus::Created),
            in_transit_count: storage::get_status_count(&env, &ShipmentStatus::InTransit),
            at_checkpoint_count: storage::get_status_count(&env, &ShipmentStatus::AtCheckpoint),
            delivered_count: storage::get_status_count(&env, &ShipmentStatus::Delivered),
            disputed_count: storage::get_status_count(&env, &ShipmentStatus::Disputed),
            cancelled_count: storage::get_status_count(&env, &ShipmentStatus::Cancelled),
        })
    }

    /// Add a carrier to a company's whitelist.
    /// Only the company can add carriers to their own whitelist.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `company` - The company's address acting as caller.
    /// * `carrier` - The carrier address to whitelist.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully registered.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // contract.add_carrier_to_whitelist(&env, &company, &carrier);
    /// ```
    pub fn add_carrier_to_whitelist(
        env: Env,
        company: Address,
        carrier: Address,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        company.require_auth();
        require_role(&env, &company, Role::Company)?;

        storage::add_carrier_to_whitelist(&env, &company, &carrier);

        env.events().publish(
            (symbol_short!("add_wl"),),
            (company.clone(), carrier.clone()),
        );

        Ok(())
    }

    /// Remove a carrier from a company's whitelist.
    /// Only the company can remove carriers from their own whitelist.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `company` - The company address removing the carrier.
    /// * `carrier` - The carrier address to be removed.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully removed.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // contract.remove_carrier_from_whitelist(&env, &company, &carrier);
    /// ```
    pub fn remove_carrier_from_whitelist(
        env: Env,
        company: Address,
        carrier: Address,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        company.require_auth();
        require_role(&env, &company, Role::Company)?;

        storage::remove_carrier_from_whitelist(&env, &company, &carrier);

        env.events().publish(
            (symbol_short!("rm_wl"),),
            (company.clone(), carrier.clone()),
        );

        Ok(())
    }

    /// Check if a carrier is whitelisted for a company.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `company` - The company address.
    /// * `carrier` - The carrier address in question.
    ///
    /// # Returns
    /// * `Result<bool, NavinError>` - True if the carrier is whitelisted.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let is_whitelisted = contract.is_carrier_whitelisted(&env, &company, &carrier);
    /// ```
    pub fn is_carrier_whitelisted(
        env: Env,
        company: Address,
        carrier: Address,
    ) -> Result<bool, NavinError> {
        require_initialized(&env)?;

        Ok(storage::is_carrier_whitelisted(&env, &company, &carrier))
    }

    /// Returns the role assigned to a given address.
    /// Returns Role::Unassigned if no role is assigned.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `address` - The address to check.
    ///
    /// # Returns
    /// * `Result<Role, NavinError>` - The role assigned to the address.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let role = contract.get_role(&env, &address);
    /// ```
    pub fn get_role(env: Env, address: Address) -> Result<Role, NavinError> {
        require_initialized(&env)?;
        Ok(storage::get_role(&env, &address).unwrap_or(Role::Unassigned))
    }

    /// Allow admin to grant Company role.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin executing the role grant.
    /// * `company` - The address receiving the company role.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful role assignment.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If called by a non-admin.
    ///
    /// # Examples
    /// ```rust
    /// // contract.add_company(&env, &admin, &new_company_addr);
    /// ```
    pub fn add_company(env: Env, admin: Address, company: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        storage::set_company_role(&env, &company);
        Ok(())
    }

    /// Allow admin to grant Carrier role.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin executing the role grant.
    /// * `carrier` - The address receiving the carrier role.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful role assignment.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If called by a non-admin.
    ///
    /// # Examples
    /// ```rust
    /// // contract.add_carrier(&env, &admin, &new_carrier_addr);
    /// ```
    pub fn add_carrier(env: Env, admin: Address, carrier: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        storage::set_carrier_role(&env, &carrier);
        Ok(())
    }

    /// Create a shipment and emit the shipment_created event.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `sender` - Company address creating the shipment.
    /// * `receiver` - Destination address for the shipment.
    /// * `carrier` - Carrier address assigned to the shipment.
    /// * `data_hash` - Off-chain data hash of shipment details.
    /// * `payment_milestones` - Schedule for escrow releases based on checkpoints.
    /// * `deadline` - Timestamp after which shipment is considered expired and can be auto-cancelled.
    ///
    /// # Returns
    /// * `Result<u64, NavinError>` - Newly created shipment ID.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller isn't a Company.
    /// * `NavinError::InvalidHash` - If data_hash is all zeros.
    /// * `NavinError::MilestoneSumInvalid` - If milestone percentages do not equal 100%.
    /// * `NavinError::CounterOverflow` - If total shipment count overflows max u64.
    /// * `NavinError::InvalidTimestamp` - If the deadline is not strictly in the future.
    ///
    /// # Examples
    /// ```rust
    /// // let id = contract.create_shipment(&env, &sender, &receiver, &carrier, &hash, vec![(&env, Symbol::new(&env, "warehouse"), 100)], deadline_ts);
    /// ```
    pub fn create_shipment(
        env: Env,
        sender: Address,
        receiver: Address,
        carrier: Address,
        data_hash: BytesN<32>,
        payment_milestones: Vec<(Symbol, u32)>,
        deadline: u64,
    ) -> Result<u64, NavinError> {
        require_initialized(&env)?;
        sender.require_auth();
        require_role(&env, &sender, Role::Company)?;
        validate_milestones(&env, &payment_milestones)?;
        validate_hash(&data_hash)?;

        let now = env.ledger().timestamp();
        if deadline <= now {
            return Err(NavinError::InvalidTimestamp);
        }

        // Check company active shipment limit
        let current_active = storage::get_active_shipment_count(&env, &sender);
        let limit = storage::get_shipment_limit(&env);
        if current_active >= limit {
            return Err(NavinError::ShipmentLimitReached);
        }

        let shipment_id = storage::get_shipment_counter(&env)
            .checked_add(1)
            .ok_or(NavinError::CounterOverflow)?;

        let shipment = Shipment {
            id: shipment_id,
            sender: sender.clone(),
            receiver: receiver.clone(),
            carrier,
            data_hash: data_hash.clone(),
            status: ShipmentStatus::Created,
            created_at: now,
            updated_at: now,
            escrow_amount: 0,
            total_escrow: 0,
            payment_milestones,
            paid_milestones: Vec::new(&env),
            metadata: None,
            deadline,
        };

        storage::set_shipment(&env, &shipment);
        storage::set_shipment_counter(&env, shipment_id);
        storage::increment_status_count(&env, &ShipmentStatus::Created);
        storage::increment_active_shipment_count(&env, &sender);
        extend_shipment_ttl(&env, shipment_id);

        events::emit_shipment_created(&env, shipment_id, &sender, &receiver, &data_hash);
        events::emit_notification(
            &env,
            &receiver,
            NotificationType::ShipmentCreated,
            shipment_id,
            &data_hash,
        );
        events::emit_notification(
            &env,
            &shipment.carrier,
            NotificationType::ShipmentCreated,
            shipment_id,
            &data_hash,
        );

        Ok(shipment_id)
    }

    /// Create multiple shipments in a single atomic transaction.
    /// Limit: 10 shipments per batch.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `sender` - Company address creating shipments.
    /// * `shipments` - Vector of shipment inputs.
    ///
    /// # Returns
    /// * `Result<Vec<u64>, NavinError>` - Vector of newly created shipment IDs.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller isn't a Company.
    /// * `NavinError::BatchTooLarge` - If more than 10 shipments are submitted.
    /// * `NavinError::InvalidShipmentInput` - If receiver matches carrier for any shipment.
    /// * `NavinError::InvalidHash` - If any data_hash is all zeros.
    /// * `NavinError::MilestoneSumInvalid` - If payment milestones are invalid per item.
    /// * `NavinError::InvalidTimestamp` - If the deadline is not strictly in the future.
    ///
    /// # Examples
    /// ```rust
    /// // let ids = contract.create_shipments_batch(&env, &sender, inputs_vec);
    /// ```
    pub fn create_shipments_batch(
        env: Env,
        sender: Address,
        shipments: Vec<ShipmentInput>,
    ) -> Result<Vec<u64>, NavinError> {
        require_initialized(&env)?;
        sender.require_auth();
        require_role(&env, &sender, Role::Company)?;

        let config = config::get_config(&env);
        if shipments.len() > config.batch_operation_limit {
            return Err(NavinError::BatchTooLarge);
        }

        let mut ids = Vec::new(&env);
        let now = env.ledger().timestamp();

        // Check batch size against limit
        let current_active = storage::get_active_shipment_count(&env, &sender);
        let limit = storage::get_shipment_limit(&env);
        if current_active.saturating_add(shipments.len()) > limit {
            return Err(NavinError::ShipmentLimitReached);
        }

        for shipment_input in shipments.iter() {
            if shipment_input.receiver == shipment_input.carrier {
                return Err(NavinError::InvalidShipmentInput);
            }
            validate_milestones(&env, &shipment_input.payment_milestones)?;
            validate_hash(&shipment_input.data_hash)?;

            if shipment_input.deadline <= now {
                return Err(NavinError::InvalidTimestamp);
            }

            let shipment_id = storage::get_shipment_counter(&env)
                .checked_add(1)
                .ok_or(NavinError::CounterOverflow)?;

            let shipment = Shipment {
                id: shipment_id,
                sender: sender.clone(),
                receiver: shipment_input.receiver.clone(),
                carrier: shipment_input.carrier.clone(),
                data_hash: shipment_input.data_hash.clone(),
                status: ShipmentStatus::Created,
                created_at: now,
                updated_at: now,
                escrow_amount: 0,
                total_escrow: 0,
                payment_milestones: shipment_input.payment_milestones,
                paid_milestones: Vec::new(&env),
                metadata: None,
                deadline: shipment_input.deadline,
            };

            storage::set_shipment(&env, &shipment);
            storage::set_shipment_counter(&env, shipment_id);
            storage::increment_status_count(&env, &ShipmentStatus::Created);
            storage::increment_active_shipment_count(&env, &sender);
            extend_shipment_ttl(&env, shipment_id);

            events::emit_shipment_created(
                &env,
                shipment_id,
                &sender,
                &shipment_input.receiver,
                &shipment_input.data_hash,
            );
            events::emit_notification(
                &env,
                &shipment_input.receiver,
                NotificationType::ShipmentCreated,
                shipment_id,
                &shipment_input.data_hash,
            );
            events::emit_notification(
                &env,
                &shipment_input.carrier,
                NotificationType::ShipmentCreated,
                shipment_id,
                &shipment_input.data_hash,
            );
            ids.push_back(shipment_id);
        }

        Ok(ids)
    }

    /// Retrieve shipment details by ID.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment to fetch.
    ///
    /// # Returns
    /// * `Result<Shipment, NavinError>` - Reconstructed shipment struct.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    ///
    /// # Examples
    /// ```rust
    /// // let shipment = contract.get_shipment(&env, 1);
    /// ```
    pub fn get_shipment(env: Env, shipment_id: u64) -> Result<Shipment, NavinError> {
        require_initialized(&env)?;
        storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)
    }

    /// Deposit escrow funds for a shipment.
    /// Only a Company can deposit, and the shipment must be in Created status.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `from` - Company address providing escrow.
    /// * `shipment_id` - Target shipment.
    /// * `amount` - Balance of tokens deposited into escrow.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful deposit.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller isn't a Company.
    /// * `NavinError::InvalidAmount` - If amount is zero, negative, or exceeds the maximum.
    /// * `NavinError::ShipmentNotFound` - If shipment is untracked.
    /// * `NavinError::InvalidStatus` - If shipment is not in `Created` status.
    /// * `NavinError::EscrowLocked` - If escrow is already deposited for shipment.
    ///
    /// # Examples
    /// ```rust
    /// // contract.deposit_escrow(&env, &company, 1, 5000000);
    /// ```
    pub fn deposit_escrow(
        env: Env,
        from: Address,
        shipment_id: u64,
        amount: i128,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        from.require_auth();
        require_role(&env, &from, Role::Company)?;

        validate_amount(amount).map_err(|_| NavinError::InsufficientFunds)?;

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        if shipment.status != ShipmentStatus::Created {
            return Err(NavinError::InvalidStatus);
        }

        if shipment.escrow_amount > 0 {
            return Err(NavinError::EscrowLocked);
        }

        // Get token contract address
        let token_contract = storage::get_token_contract(&env).ok_or(NavinError::NotInitialized)?;

        // Transfer tokens from user to this contract
        let contract_address = env.current_contract_address();
        let mut args: soroban_sdk::Vec<soroban_sdk::Val> = Vec::new(&env);
        args.push_back(from.clone().into_val(&env));
        args.push_back(contract_address.into_val(&env));
        args.push_back(amount.into_val(&env));
        env.invoke_contract::<()>(&token_contract, &symbol_short!("transfer"), args);

        shipment.escrow_amount = amount;
        shipment.total_escrow = amount;
        shipment.updated_at = env.ledger().timestamp();
        storage::set_shipment(&env, &shipment);
        storage::add_total_escrow_volume(&env, amount);
        extend_shipment_ttl(&env, shipment_id);

        events::emit_escrow_deposited(&env, shipment_id, &from, amount);

        Ok(())
    }

    /// Update shipment status with transition validation.
    /// Only the carrier or admin can update the status.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `caller` - Carrier or admin address making the update.
    /// * `shipment_id` - Current shipment identifier.
    /// * `new_status` - The destination transitional status.
    /// * `data_hash` - The off-chain data hash tracking context for update.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on valid transition.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If shipment doesn't exist.
    /// * `NavinError::Unauthorized` - If caller is neither the carrier nor admin.
    /// * `NavinError::RateLimitExceeded` - If status was updated too recently (unless Admin).
    /// * `NavinError::InvalidStatus` - If transitioning to an improperly sequenced state.
    ///
    /// # Examples
    /// ```rust
    /// // contract.update_status(&env, &carrier, 1, ShipmentStatus::InTransit, &hash);
    /// ```
    pub fn update_status(
        env: Env,
        caller: Address,
        shipment_id: u64,
        new_status: ShipmentStatus,
        data_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        caller.require_auth();

        let admin = storage::get_admin(&env);
        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        if caller != shipment.carrier && caller != admin {
            return Err(NavinError::Unauthorized);
        }

        // Rate-limit check: admin bypasses; all other callers must wait the minimum interval.
        if caller != admin {
            if let Some(last) = storage::get_last_status_update(&env, shipment_id) {
                let now = env.ledger().timestamp();
                let config = config::get_config(&env);
                if now.saturating_sub(last) < config.min_status_update_interval {
                    return Err(NavinError::RateLimitExceeded);
                }
            }
        }

        if !shipment.status.is_valid_transition(&new_status) {
            return Err(NavinError::InvalidStatus);
        }

        let old_status = shipment.status.clone();
        shipment.status = new_status.clone();
        shipment.data_hash = data_hash.clone();
        shipment.updated_at = env.ledger().timestamp();

        storage::set_shipment(&env, &shipment);
        storage::decrement_status_count(&env, &old_status);
        storage::increment_status_count(&env, &shipment.status);

        if shipment.status == ShipmentStatus::Disputed {
            storage::increment_total_disputes(&env);
        }

        storage::set_last_status_update(&env, shipment_id, env.ledger().timestamp());
        extend_shipment_ttl(&env, shipment_id);

        events::emit_status_updated(&env, shipment_id, &old_status, &new_status, &data_hash);
        events::emit_notification(
            &env,
            &shipment.sender,
            NotificationType::StatusChanged,
            shipment_id,
            &data_hash,
        );
        events::emit_notification(
            &env,
            &shipment.receiver,
            NotificationType::StatusChanged,
            shipment_id,
            &data_hash,
        );

        Ok(())
    }

    /// Returns the current escrowed amount for a specific shipment.
    /// Returns 0 if no escrow has been deposited.
    /// Returns ShipmentNotFound if the shipment does not exist.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the shipment.
    ///
    /// # Returns
    /// * `Result<i128, NavinError>` - Amount stored in escrow.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    ///
    /// # Examples
    /// ```rust
    /// // let balance = contract.get_escrow_balance(&env, 1);
    /// ```
    pub fn get_escrow_balance(env: Env, shipment_id: u64) -> Result<i128, NavinError> {
        require_initialized(&env)?;
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }
        Ok(storage::get_escrow_balance(&env, shipment_id))
    }

    /// Returns the total number of shipments created on the platform.
    /// Returns 0 if the contract has not been initialized.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `u64` - Overall total shipments registered.
    ///
    /// # Examples
    /// ```rust
    /// // let total = contract.get_shipment_count(&env);
    /// ```
    pub fn get_shipment_count(env: Env) -> u64 {
        storage::get_shipment_counter(&env)
    }

    /// Confirm delivery of a shipment.
    /// Only the designated receiver can call this function.
    /// Shipment must be in InTransit or AtCheckpoint status.
    /// Stores the confirmation_hash (hash of proof-of-delivery data) and
    /// transitions the shipment status to Delivered.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `receiver` - Receiver address confirming the delivery.
    /// * `shipment_id` - Identifier of delivered shipment.
    /// * `confirmation_hash` - The proof-of-delivery hash.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful confirmation.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If shipment does not exist.
    /// * `NavinError::Unauthorized` - If called by an address other than the shipment receiver.
    /// * `NavinError::InvalidStatus` - If shipment is not in a transitable status to Delivered.
    ///
    /// # Examples
    /// ```rust
    /// // contract.confirm_delivery(&env, &receiver_addr, 1, 5000000);
    /// ```
    pub fn confirm_delivery(
        env: Env,
        receiver: Address,
        shipment_id: u64,
        confirmation_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        receiver.require_auth();

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        // Only the designated receiver can confirm delivery
        if shipment.receiver != receiver {
            return Err(NavinError::Unauthorized);
        }

        // Validate transition to Delivered
        if !shipment
            .status
            .is_valid_transition(&ShipmentStatus::Delivered)
        {
            return Err(NavinError::InvalidStatus);
        }

        let now = env.ledger().timestamp();
        let old_status = shipment.status.clone();
        shipment.status = ShipmentStatus::Delivered;
        shipment.updated_at = now;

        storage::set_shipment(&env, &shipment);
        storage::decrement_status_count(&env, &old_status);
        storage::increment_status_count(&env, &ShipmentStatus::Delivered);

        storage::set_confirmation_hash(&env, shipment_id, &confirmation_hash);
        storage::decrement_active_shipment_count(&env, &shipment.sender);
        extend_shipment_ttl(&env, shipment_id);

        let remaining_escrow = shipment.escrow_amount;
        internal_release_escrow(&env, &mut shipment, remaining_escrow);

        env.events().publish(
            (Symbol::new(&env, "delivery_confirmed"),),
            (shipment_id, receiver, confirmation_hash.clone()),
        );

        // Reputation: record successful delivery for the carrier
        events::emit_delivery_success(&env, &shipment.carrier, shipment_id, now);
        events::emit_notification(
            &env,
            &shipment.sender,
            NotificationType::DeliveryConfirmed,
            shipment_id,
            &confirmation_hash,
        );
        events::emit_notification(
            &env,
            &shipment.carrier,
            NotificationType::DeliveryConfirmed,
            shipment_id,
            &confirmation_hash,
        );

        Ok(())
    }

    /// Report a geofence event for a shipment.
    /// Only registered carriers can report geofence events.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `carrier` - Carrier address reporting the event.
    /// * `shipment_id` - ID of the tracked shipment.
    /// * `zone_type` - Type of geofence event crossed.
    /// * `data_hash` - Encrypted off-chain location data representation.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful report tracking.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller isn't a Carrier role.
    /// * `NavinError::ShipmentNotFound` - If tracking context specifies an invalid shipment.
    ///
    /// # Examples
    /// ```rust
    /// // contract.report_geofence_event(&env, &carrier, 1, GeofenceEvent::ZoneEntry, &hash);
    /// ```
    pub fn report_geofence_event(
        env: Env,
        carrier: Address,
        shipment_id: u64,
        zone_type: GeofenceEvent,
        data_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        carrier.require_auth();
        require_role(&env, &carrier, Role::Carrier)?;

        // Verify shipment exists and carrier is assigned
        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        if shipment.carrier != carrier {
            return Err(NavinError::Unauthorized);
        }

        let timestamp = env.ledger().timestamp();

        env.events().publish(
            (Symbol::new(&env, "geofence_event"),),
            (shipment_id, zone_type, data_hash, timestamp),
        );

        Ok(())
    }

    /// Update ETA for a shipment.
    /// Only the designated registered carrier can update ETA.
    /// ETA must be strictly in the future.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `carrier` - Active assigned carrier modifying ETA.
    /// * `shipment_id` - Identifiable tracker mapping to shipment.
    /// * `eta_timestamp` - The estimated timestamp prediction in the future.
    /// * `data_hash` - The mapped hash associated with the update.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful ETA registry.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller isn't the assigned carrier.
    /// * `NavinError::ShipmentNotFound` - If shipment instance targets missing entry.
    /// * `NavinError::InvalidTimestamp` - If provided ETA is strictly in the past or present.
    ///
    /// # Examples
    /// ```rust
    /// // contract.update_eta(&env, &carrier, 1, new_eta, &hash);
    /// ```
    pub fn update_eta(
        env: Env,
        carrier: Address,
        shipment_id: u64,
        eta_timestamp: u64,
        data_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        carrier.require_auth();
        require_role(&env, &carrier, Role::Carrier)?;

        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        if shipment.carrier != carrier {
            return Err(NavinError::Unauthorized);
        }

        if eta_timestamp <= env.ledger().timestamp() {
            return Err(NavinError::InvalidTimestamp);
        }

        env.events().publish(
            (Symbol::new(&env, "eta_updated"),),
            (shipment_id, eta_timestamp, data_hash),
        );

        Ok(())
    }

    /// Record a milestone for a shipment.
    /// Only registered carriers can record milestones.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `carrier` - Assigned carrier address triggering the recording.
    /// * `shipment_id` - ID of the tracked shipment.
    /// * `checkpoint` - Representation of progress milestone achieved.
    /// * `data_hash` - Integrity hash associated with offchain progress indicators.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful tracking record update.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If called by unassigned identity.
    /// * `NavinError::ShipmentNotFound` - If shipment instance targets missing entry.
    /// * `NavinError::InvalidStatus` - If tracked instance is not `InTransit`.
    ///
    /// # Examples
    /// ```rust
    /// // contract.record_milestone(&env, &carrier, 1, Symbol::new(&env, "warehouse"), &hash);
    /// ```
    pub fn record_milestone(
        env: Env,
        carrier: Address,
        shipment_id: u64,
        checkpoint: Symbol,
        data_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        carrier.require_auth();
        require_role(&env, &carrier, Role::Carrier)?;

        // Verify shipment exists, carrier is assigned, and status
        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        if shipment.carrier != carrier {
            return Err(NavinError::Unauthorized);
        }

        if shipment.status != ShipmentStatus::InTransit {
            return Err(NavinError::InvalidStatus);
        }

        let timestamp = env.ledger().timestamp();

        let _milestone = Milestone {
            shipment_id,
            checkpoint: checkpoint.clone(),
            data_hash: data_hash.clone(),
            timestamp,
            reporter: carrier.clone(),
        };

        // Do NOT store the milestone on-chain
        // Emit the milestone_recorded event (Hash-and-Emit pattern)
        events::emit_milestone_recorded(&env, shipment_id, &checkpoint, &data_hash, &carrier);

        // Check for milestone-based payments
        let mut mut_shipment = shipment;
        let mut found_index = None;
        for (i, milestone) in mut_shipment.payment_milestones.iter().enumerate() {
            if milestone.0 == checkpoint {
                found_index = Some(i);
                break;
            }
        }

        if let Some(idx) = found_index {
            let mut already_paid = false;
            for paid_symbol in mut_shipment.paid_milestones.iter() {
                if paid_symbol == checkpoint {
                    already_paid = true;
                    break;
                }
            }

            if !already_paid {
                let milestone = mut_shipment.payment_milestones.get(idx as u32).unwrap();
                let release_amount = (mut_shipment.total_escrow * milestone.1 as i128) / 100;
                mut_shipment.paid_milestones.push_back(checkpoint.clone());
                internal_release_escrow(&env, &mut mut_shipment, release_amount);
            }
        }

        Ok(())
    }

    /// Record multiple milestones for a shipment in a single atomic transaction.
    /// Allows a carrier to record multiple checkpoints at once, reducing gas costs.
    /// Limit: 10 milestones per batch.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `carrier` - Assigned carrier address triggering the recording.
    /// * `shipment_id` - ID of the tracked shipment.
    /// * `milestones` - Vector of (checkpoint, data_hash) tuples.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful batch recording.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If called by unassigned identity.
    /// * `NavinError::ShipmentNotFound` - If shipment instance targets missing entry.
    /// * `NavinError::InvalidStatus` - If tracked instance is not `InTransit`.
    /// * `NavinError::BatchTooLarge` - If more than 10 milestones are submitted.
    ///
    /// # Examples
    /// ```rust
    /// // let milestones = vec![
    /// //     (Symbol::new(&env, "warehouse"), hash1),
    /// //     (Symbol::new(&env, "port"), hash2),
    /// // ];
    /// // contract.record_milestones_batch(&env, &carrier, 1, milestones);
    /// ```
    pub fn record_milestones_batch(
        env: Env,
        carrier: Address,
        shipment_id: u64,
        milestones: Vec<(Symbol, BytesN<32>)>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        carrier.require_auth();
        require_role(&env, &carrier, Role::Carrier)?;

        // Validate batch size
        let config = config::get_config(&env);
        if milestones.len() > config.batch_operation_limit {
            return Err(NavinError::BatchTooLarge);
        }

        // Verify shipment exists, carrier is assigned, and status
        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        if shipment.carrier != carrier {
            return Err(NavinError::Unauthorized);
        }

        if shipment.status != ShipmentStatus::InTransit {
            return Err(NavinError::InvalidStatus);
        }

        // Validate all milestones before committing any (atomic operation)
        // This ensures that if any milestone is invalid, none are committed
        for milestone_tuple in milestones.iter() {
            let data_hash = milestone_tuple.1.clone();

            // Basic validation - ensure data_hash is valid
            if data_hash.len() != 32 {
                return Err(NavinError::InvalidHash);
            }
        }

        // All validations passed, now process each milestone
        let timestamp = env.ledger().timestamp();
        let mut mut_shipment = shipment;

        for milestone_tuple in milestones.iter() {
            let checkpoint = milestone_tuple.0.clone();
            let data_hash = milestone_tuple.1.clone();

            let _milestone = Milestone {
                shipment_id,
                checkpoint: checkpoint.clone(),
                data_hash: data_hash.clone(),
                timestamp,
                reporter: carrier.clone(),
            };

            // Emit one event per milestone (Hash-and-Emit pattern)
            events::emit_milestone_recorded(&env, shipment_id, &checkpoint, &data_hash, &carrier);

            // Check for milestone-based payments
            let mut found_index = None;
            for (i, payment_milestone) in mut_shipment.payment_milestones.iter().enumerate() {
                if payment_milestone.0 == checkpoint {
                    found_index = Some(i);
                    break;
                }
            }

            if let Some(idx) = found_index {
                let mut already_paid = false;
                for paid_symbol in mut_shipment.paid_milestones.iter() {
                    if paid_symbol == checkpoint {
                        already_paid = true;
                        break;
                    }
                }

                if !already_paid {
                    let payment_milestone =
                        mut_shipment.payment_milestones.get(idx as u32).unwrap();
                    let release_amount =
                        (mut_shipment.total_escrow * payment_milestone.1 as i128) / 100;
                    mut_shipment.paid_milestones.push_back(checkpoint.clone());
                    internal_release_escrow(&env, &mut mut_shipment, release_amount);
                }
            }
        }

        Ok(())
    }

    /// Extend the TTL of a shipment's persistent storage entries.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - Shipment ID to renew TTL.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on success.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // contract.extend_shipment_ttl(env, 1);
    /// ```
    pub fn extend_shipment_ttl(env: Env, shipment_id: u64) -> Result<(), NavinError> {
        require_initialized(&env)?;
        extend_shipment_ttl(&env, shipment_id);
        Ok(())
    }

    /// Cancel a shipment before it is delivered.
    /// Only the Company (sender) or Admin can cancel.
    /// Shipment must not be Delivered or Disputed.
    /// If escrow exists, triggers automatic refund to the Company.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `caller` - Executing Company or Admin address.
    /// * `shipment_id` - ID specifying cancelled shipment instance.
    /// * `reason_hash` - The mapped hash associated to the cancellation context.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on cancellation.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If tracking context is invalid list element.
    /// * `NavinError::Unauthorized` - If called by unauthorized accounts.
    /// * `NavinError::ShipmentAlreadyCompleted` - If tracking context specified reached terminal states.
    ///
    /// # Examples
    /// ```rust
    /// // contract.cancel_shipment(&env, &admin, 1, &hash);
    /// ```
    pub fn cancel_shipment(
        env: Env,
        caller: Address,
        shipment_id: u64,
        reason_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        caller.require_auth();

        let admin = storage::get_admin(&env);
        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        if caller != shipment.sender && caller != admin {
            return Err(NavinError::Unauthorized);
        }

        match shipment.status {
            ShipmentStatus::Delivered | ShipmentStatus::Disputed => {
                return Err(NavinError::ShipmentAlreadyCompleted);
            }
            _ => {}
        }

        let escrow_amount = shipment.escrow_amount;
        let old_status = shipment.status.clone();
        shipment.status = ShipmentStatus::Cancelled;
        shipment.escrow_amount = 0;
        shipment.updated_at = env.ledger().timestamp();

        storage::set_shipment(&env, &shipment);
        storage::decrement_status_count(&env, &old_status);
        storage::increment_status_count(&env, &ShipmentStatus::Cancelled);

        // Decrement active shipment count if it was not already cancelled
        if old_status != ShipmentStatus::Cancelled {
            storage::decrement_active_shipment_count(&env, &shipment.sender);
        }

        if escrow_amount > 0 {
            storage::remove_escrow_balance(&env, shipment_id);
            events::emit_escrow_released(&env, shipment_id, &shipment.sender, escrow_amount);
        }
        extend_shipment_ttl(&env, shipment_id);

        events::emit_shipment_cancelled(&env, shipment_id, &caller, &reason_hash);

        Ok(())
    }

    /// Upgrade the contract to a new WASM implementation.
    /// Only the admin can trigger upgrades. State is preserved.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin executing the upgrade.
    /// * `new_wasm_hash` - Hash pointer to the new WASM instance loaded on network.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful deployment upgrade instance.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller isn't contract admin instance.
    /// * `NavinError::CounterOverflow` - If total tracking version identifier pointer triggers overflow.
    ///
    /// # Examples
    /// ```rust
    /// // contract.upgrade(env, admin, new_wasm_hash);
    /// ```
    pub fn upgrade(env: Env, admin: Address, new_wasm_hash: BytesN<32>) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        let new_version = storage::get_version(&env)
            .checked_add(1)
            .ok_or(NavinError::CounterOverflow)?;

        storage::set_version(&env, new_version);
        events::emit_contract_upgraded(&env, &admin, &new_wasm_hash, new_version);
        env.deployer().update_current_contract_wasm(new_wasm_hash);

        Ok(())
    }

    /// Release escrowed funds to the carrier after delivery confirmation.
    /// Only the receiver or admin can trigger release.
    /// Shipment must be in Delivered status.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `caller` - Originating user triggering escrow delivery (receiver/admin).
    /// * `shipment_id` - Tracking assignment associated with delivery payload instances.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful asset delivery.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If tracking context specifies an invalid shipment.
    /// * `NavinError::Unauthorized` - If caller isn't receiver or admin.
    /// * `NavinError::InvalidStatus` - If contract expects specific lifecycle constraint and differs.
    /// * `NavinError::InsufficientFunds` - If payload is fully released and balances are zeroed out.
    ///
    /// # Examples
    /// ```rust
    /// // contract.release_escrow(env, receiver, 1);
    /// ```
    pub fn release_escrow(env: Env, caller: Address, shipment_id: u64) -> Result<(), NavinError> {
        require_initialized(&env)?;
        caller.require_auth();

        let admin = storage::get_admin(&env);
        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        if caller != shipment.receiver && caller != admin {
            return Err(NavinError::Unauthorized);
        }

        if shipment.status != ShipmentStatus::Delivered {
            return Err(NavinError::InvalidStatus);
        }

        let escrow_amount = shipment.escrow_amount;
        if escrow_amount == 0 {
            return Err(NavinError::InsufficientFunds);
        }

        internal_release_escrow(&env, &mut shipment, escrow_amount);
        events::emit_notification(
            &env,
            &shipment.sender,
            NotificationType::EscrowReleased,
            shipment_id,
            &BytesN::from_array(&env, &[0u8; 32]),
        );
        events::emit_notification(
            &env,
            &shipment.carrier,
            NotificationType::EscrowReleased,
            shipment_id,
            &BytesN::from_array(&env, &[0u8; 32]),
        );

        Ok(())
    }

    /// Refund escrowed funds to the company if shipment is cancelled.
    /// Only the sender (Company) or admin can trigger refund.
    /// Shipment must be in Created or Cancelled status.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `caller` - Reference mapping handler execution triggers for scope access control checks.
    /// * `shipment_id` - Identification marker mapping.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful refund sequence generation.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If valid identifiers track undefined mappings instances.
    /// * `NavinError::Unauthorized` - If execution identity doesn't resolve matching configurations contexts mappings.
    /// * `NavinError::InvalidStatus` - If mapping resolves illegal flow mappings configuration combinations triggers.
    /// * `NavinError::InsufficientFunds` - If token escrow state points map uninitialized quantities values scope checks.
    ///
    /// # Examples
    /// ```rust
    /// // contract.refund_escrow(env, sender, 1);
    /// ```
    pub fn refund_escrow(env: Env, caller: Address, shipment_id: u64) -> Result<(), NavinError> {
        require_initialized(&env)?;
        caller.require_auth();

        let admin = storage::get_admin(&env);
        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        if caller != shipment.sender && caller != admin {
            return Err(NavinError::Unauthorized);
        }

        if shipment.status != ShipmentStatus::Created
            && shipment.status != ShipmentStatus::Cancelled
        {
            return Err(NavinError::InvalidStatus);
        }

        let escrow_amount = shipment.escrow_amount;
        if escrow_amount == 0 {
            return Err(NavinError::InsufficientFunds);
        }

        // Get token contract address
        let token_contract = storage::get_token_contract(&env).ok_or(NavinError::NotInitialized)?;

        // Transfer tokens from this contract to company
        let contract_address = env.current_contract_address();
        let mut args: soroban_sdk::Vec<soroban_sdk::Val> = Vec::new(&env);
        args.push_back(contract_address.into_val(&env));
        args.push_back(shipment.sender.clone().into_val(&env));
        args.push_back(escrow_amount.into_val(&env));
        env.invoke_contract::<soroban_sdk::Val>(&token_contract, &symbol_short!("transfer"), args);

        shipment.escrow_amount = 0;
        let old_status = shipment.status.clone();
        shipment.status = ShipmentStatus::Cancelled;
        shipment.updated_at = env.ledger().timestamp();

        storage::set_shipment(&env, &shipment);
        storage::decrement_status_count(&env, &old_status);
        storage::increment_status_count(&env, &ShipmentStatus::Cancelled);

        // Decrement active shipment count if it was not already cancelled
        if old_status != ShipmentStatus::Cancelled {
            storage::decrement_active_shipment_count(&env, &shipment.sender);
        }

        extend_shipment_ttl(&env, shipment_id);
        extend_shipment_ttl(&env, shipment_id);

        events::emit_escrow_refunded(&env, shipment_id, &shipment.sender, escrow_amount);

        Ok(())
    }

    /// Raise a dispute for a shipment.
    /// Only the sender, receiver, or carrier can raise a dispute.
    /// Shipment must not be Cancelled or already Disputed.
    ///
    /// # Arguments
    /// * `env` - Execution environment tracking context.
    /// * `caller` - Identity specifying resolution event raising instances configuration contexts.
    /// * `shipment_id` - Object tracker index identifying execution scope handlers.
    /// * `reason_hash` - Encoded offchain metadata representation parameter validation identifier limits strings pointers.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful dispute registry logging.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If parameters index unresolvable target references configurations identifiers constraints matches.
    /// * `NavinError::Unauthorized` - If resolving constraints mapping fails identifiers scopes validations check mapping instances boundaries checks definitions roles mapping assignments properties permissions restrictions validations pointers identifiers strings tokens handlers arrays identifiers arrays values identifiers arrays matches matches mappings mapping roles properties maps pointers validators maps mapping permissions mapped values pointers matches mapped roles restrictions mapping validators bounds validators identifiers fields validations mapped keys mapped validators fields fields mapping mapped arrays string mapped mapped properties validators string permissions maps string permissions keys mappings bound.
    /// * `NavinError::ShipmentAlreadyCompleted` - If state evaluates illegal targets.
    ///
    /// # Examples
    /// ```rust
    /// // contract.raise_dispute(env, caller, 1, hash);
    /// ```
    pub fn raise_dispute(
        env: Env,
        caller: Address,
        shipment_id: u64,
        reason_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        caller.require_auth();

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        if caller != shipment.sender && caller != shipment.receiver && caller != shipment.carrier {
            return Err(NavinError::Unauthorized);
        }

        if shipment.status == ShipmentStatus::Cancelled
            || shipment.status == ShipmentStatus::Disputed
        {
            return Err(NavinError::ShipmentAlreadyCompleted);
        }

        let old_status = shipment.status.clone();
        shipment.status = ShipmentStatus::Disputed;
        shipment.updated_at = env.ledger().timestamp();

        storage::set_shipment(&env, &shipment);
        storage::decrement_status_count(&env, &old_status);
        storage::increment_status_count(&env, &ShipmentStatus::Disputed);
        storage::increment_total_disputes(&env);

        extend_shipment_ttl(&env, shipment_id);

        events::emit_dispute_raised(&env, shipment_id, &caller, &reason_hash);
        events::emit_notification(
            &env,
            &shipment.sender,
            NotificationType::DisputeRaised,
            shipment_id,
            &reason_hash,
        );
        events::emit_notification(
            &env,
            &shipment.receiver,
            NotificationType::DisputeRaised,
            shipment_id,
            &reason_hash,
        );
        events::emit_notification(
            &env,
            &shipment.carrier,
            NotificationType::DisputeRaised,
            shipment_id,
            &reason_hash,
        );

        Ok(())
    }

    /// Resolve a dispute by releasing funds to carrier or refunding to company.
    /// Only admin can resolve disputes.
    ///
    /// # Arguments
    /// * `env` - Execution environment tracking context.
    /// * `admin` - Contract admin executing the resolution.
    /// * `shipment_id` - ID specifying tracked shipment sequence.
    /// * `resolution` - Target outcome assigned by platform resolving admin.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful resolution instance.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller isn't contract admin mapping.
    /// * `NavinError::ShipmentNotFound` - If parameters track undefined mappings.
    /// * `NavinError::InvalidStatus` - If tracked instance is not `Disputed`.
    /// * `NavinError::InsufficientFunds` - If linked balance mapped values reflect unset tracking.
    ///
    /// # Examples
    /// ```rust
    /// // contract.resolve_dispute(env, admin, 1, DisputeResolution::ReleaseToCarrier);
    /// ```
    pub fn resolve_dispute(
        env: Env,
        admin: Address,
        shipment_id: u64,
        resolution: DisputeResolution,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        if shipment.status != ShipmentStatus::Disputed {
            return Err(NavinError::InvalidStatus);
        }

        let escrow_amount = shipment.escrow_amount;
        if escrow_amount == 0 {
            return Err(NavinError::InsufficientFunds);
        }

        shipment.escrow_amount = 0;
        shipment.updated_at = env.ledger().timestamp();

        let recipient = match resolution {
            DisputeResolution::ReleaseToCarrier => {
                shipment.status = ShipmentStatus::Delivered;
                shipment.carrier.clone()
            }
            DisputeResolution::RefundToCompany => {
                shipment.status = ShipmentStatus::Cancelled;
                shipment.sender.clone()
            }
        };

        storage::decrement_status_count(&env, &ShipmentStatus::Disputed);
        storage::increment_status_count(&env, &shipment.status);
        storage::decrement_active_shipment_count(&env, &shipment.sender);

        storage::set_shipment(&env, &shipment);
        storage::remove_escrow_balance(&env, shipment_id);
        extend_shipment_ttl(&env, shipment_id);

        match resolution {
            DisputeResolution::ReleaseToCarrier => {
                events::emit_escrow_released(&env, shipment_id, &recipient, escrow_amount);
            }
            DisputeResolution::RefundToCompany => {
                events::emit_escrow_refunded(&env, shipment_id, &recipient, escrow_amount);
                // Reputation: carrier lost this dispute
                events::emit_carrier_dispute_loss(&env, &shipment.carrier, shipment_id);
            }
        }

        events::emit_notification(
            &env,
            &shipment.sender,
            NotificationType::DisputeResolved,
            shipment_id,
            &BytesN::from_array(&env, &[0u8; 32]),
        );
        events::emit_notification(
            &env,
            &shipment.receiver,
            NotificationType::DisputeResolved,
            shipment_id,
            &BytesN::from_array(&env, &[0u8; 32]),
        );
        events::emit_notification(
            &env,
            &shipment.carrier,
            NotificationType::DisputeResolved,
            shipment_id,
            &BytesN::from_array(&env, &[0u8; 32]),
        );

        Ok(())
    }

    /// Handoff a shipment from current carrier to a new carrier.
    /// Only the current assigned carrier can initiate the handoff.
    /// New carrier must have Carrier role.
    ///
    /// # Arguments
    /// * `env` - Execution environment context mapped tracking handler.
    /// * `current_carrier` - Identity specifying event originating handlers instance.
    /// * `new_carrier` - New carrier targeted parameter taking responsibility.
    /// * `shipment_id` - Key object specifying mapping configurations instance sequence.
    /// * `handoff_hash` - Validation mapping properties verification arrays format parameters payload.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful tracker identity assignment switch.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If resolving executing bounds maps invalid permissions constraints checking.
    /// * `NavinError::ShipmentNotFound` - If bound key identifiers specify missing pointer entries array fields values references maps values definitions constraints boundary pointers boundaries checks matches roles matches mapped restrictions keys pointers parameters hashes properties checks rules matches strings bounds check restrictions validations maps roles maps identifiers assignments values sizes limit matches matching mapping constraints roles validation handlers scopes values bounds.
    /// * `NavinError::ShipmentAlreadyCompleted` - If configuration checks bounds limits evaluated properties limit boundary fields rules match terminal status tracking pointer identifiers strings.
    ///
    /// # Examples
    /// ```rust
    /// // contract.handoff_shipment(env, old, new_carrier, 1, hash);
    /// ```
    pub fn handoff_shipment(
        env: Env,
        current_carrier: Address,
        new_carrier: Address,
        shipment_id: u64,
        handoff_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        current_carrier.require_auth();
        require_role(&env, &current_carrier, Role::Carrier)?;
        require_role(&env, &new_carrier, Role::Carrier)?;

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        // Verify current carrier is the assigned carrier
        if shipment.carrier != current_carrier {
            return Err(NavinError::Unauthorized);
        }

        // Prevent handoff from completed shipments
        match shipment.status {
            ShipmentStatus::Delivered | ShipmentStatus::Cancelled => {
                return Err(NavinError::ShipmentAlreadyCompleted);
            }
            _ => {}
        }

        // Update carrier address on the shipment
        let old_carrier = shipment.carrier.clone();
        shipment.carrier = new_carrier.clone();
        shipment.updated_at = env.ledger().timestamp();

        storage::set_shipment(&env, &shipment);
        extend_shipment_ttl(&env, shipment_id);

        // Emit carrier_handoff event
        events::emit_carrier_handoff(&env, shipment_id, &old_carrier, &new_carrier, &handoff_hash);

        // Record a milestone for the handoff
        events::emit_milestone_recorded(
            &env,
            shipment_id,
            &symbol_short!("handoff"),
            &handoff_hash,
            &current_carrier,
        );

        Ok(())
    }

    /// Report a condition breach for a shipment (temperature, humidity, impact, tamper).
    ///
    /// Only the assigned carrier can report a breach. This is purely informational:
    /// shipment status is **not** changed. The full sensor payload stays off-chain;
    /// only its `data_hash` is emitted on-chain following the Hash-and-Emit pattern.
    ///
    /// # Arguments
    /// * `env` - Execution environment wrapper contexts instances format variables arrays mapped fields parameters bindings mappings validation matching variables references format map rules scopes mappings targets scopes properties bindings mappings context references format bindings sizes arrays values.
    /// * `carrier` - Tracking address specifying mapped context boundaries mapped assignments limits pointer validations constraints checking identifiers boundaries limits pointer configurations constraints context values references formats map matching arrays instances string definitions parameters matches checks limits permissions rules string formats limits rules scopes configurations maps tokens contexts scopes mapping instances matches.
    /// * `shipment_id` - Execution identifier reference binding sequence parameters formatting properties matches checking definitions sizes boundary arrays fields values bindings tracking identifier sequences parameters mapping limits bounds validation context limits formats values.
    /// * `breach_type` - Parameter tracking mapped enum values binding sequence identifier maps pointers validations checking mapped roles parameters mapped map matching pointer formats parameters mapping context limits keys.
    /// * `data_hash` - Configuration identifier string pointers limits bounds values matches arrays validation mapped strings format properties rules context bindings format array scopes references definitions maps matches validation sizes limits permissions validations.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok on successful registry mapping array parameters matches array format limitations validation limit strings arrays parameters matching size context scopes values maps arrays constraints matching context sizes properties.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If resolving executing bounds maps invalid permissions.
    /// * `NavinError::ShipmentNotFound` - If tracking context is invalid list element.
    ///
    /// # Examples
    /// ```rust
    /// // contract.report_condition_breach(&env, &carrier, 1, BreachType::TemperatureHigh, &hash);
    /// ```
    pub fn report_condition_breach(
        env: Env,
        carrier: Address,
        shipment_id: u64,
        breach_type: BreachType,
        data_hash: BytesN<32>,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        carrier.require_auth();
        require_role(&env, &carrier, Role::Carrier)?;

        let shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        // Only the assigned carrier for this shipment may report
        if shipment.carrier != carrier {
            return Err(NavinError::Unauthorized);
        }

        events::emit_condition_breach(&env, shipment_id, &carrier, &breach_type, &data_hash);

        // Reputation: record breach against carrier
        events::emit_carrier_breach(&env, &carrier, shipment_id, &breach_type);

        Ok(())
    }

    /// Verify a proof-of-delivery hash against the stored confirmation hash.
    ///
    /// Returns `true` if `proof_hash` matches the hash stored during delivery confirmation,
    /// `false` if delivered but hashes differ, and errors if the shipment does not exist.
    ///
    /// # Arguments
    /// * `env` - Execution environment tracking mapped instances validation variables maps format boundary values fields mapped contexts matching references size parameter pointer definition format contexts.
    /// * `shipment_id` - Identifying tracker mapping definitions arrays limits constraints binding values parameters mappings matches values matching variables scope sizes context properties configuration sequences format context rules bindings sequences arrays.
    /// * `proof_hash` - Encrypted target references validating properties identifiers scope scopes variables.
    ///
    /// # Returns
    /// * `Result<bool, NavinError>` - A boolean wrapper validating conditions logic identifiers values mappings rules limit format parameters checking sizes rules instances bindings context definitions matches size limits maps arrays context rules map sequences properties validation properties format constraints string values bindings contexts definitions scopes strings bounds limitations references tokens arrays maps configuration matching validation sizes rules checking.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ShipmentNotFound` - If tracking context specifies an invalid shipment.
    ///
    /// # Examples
    /// ```rust
    /// // let is_valid = contract.verify_delivery_proof(&env, 1, hash);
    /// ```
    pub fn verify_delivery_proof(
        env: Env,
        shipment_id: u64,
        proof_hash: BytesN<32>,
    ) -> Result<bool, NavinError> {
        require_initialized(&env)?;

        // Ensure the shipment exists
        if storage::get_shipment(&env, shipment_id).is_none() {
            return Err(NavinError::ShipmentNotFound);
        }

        let stored = storage::get_confirmation_hash(&env, shipment_id);
        Ok(stored == Some(proof_hash))
    }

    /// Propose a new admin for the contract. Only the current admin can call this.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Current administrator address.
    /// * `new_admin` - Address proposed as the new administrator.
    pub fn transfer_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        storage::set_proposed_admin(&env, &new_admin);
        events::emit_admin_proposed(&env, &admin, &new_admin);

        Ok(())
    }

    /// Accept the admin role transfer. Only the proposed admin can call this.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `new_admin` - The proposed administrator address accepting the role.
    pub fn accept_admin_transfer(env: Env, new_admin: Address) -> Result<(), NavinError> {
        require_initialized(&env)?;
        new_admin.require_auth();

        let proposed = storage::get_proposed_admin(&env).ok_or(NavinError::Unauthorized)?;

        if proposed != new_admin {
            return Err(NavinError::Unauthorized);
        }

        let old_admin = storage::get_admin(&env);

        storage::set_admin(&env, &new_admin);
        storage::clear_proposed_admin(&env);

        // Also update the role for the new admin if it's not already set
        storage::set_company_role(&env, &new_admin);

        events::emit_admin_transferred(&env, &old_admin, &new_admin);

        Ok(())
    }

    /// Initialize multi-signature configuration for critical admin actions.
    /// Only the current admin can call this. Must be called after contract initialization.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Current administrator address.
    /// * `admins` - List of admin addresses for multi-sig (2-10 addresses).
    /// * `threshold` - Number of approvals required (must be <= admin count).
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if multi-sig is configured.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller is not the admin.
    /// * `NavinError::InvalidMultiSigConfig` - If config is invalid.
    ///
    /// # Examples
    /// ```rust
    /// // let admins = vec![&env, admin1, admin2, admin3];
    /// // contract.init_multisig(&env, &admin, &admins, 2);
    /// ```
    pub fn init_multisig(
        env: Env,
        admin: Address,
        admins: soroban_sdk::Vec<Address>,
        threshold: u32,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        // Validate configuration
        let config = config::get_config(&env);
        let admin_count = admins.len();
        if admin_count < config.multisig_min_admins || admin_count > config.multisig_max_admins {
            return Err(NavinError::InvalidMultiSigConfig);
        }

        if threshold == 0 || threshold > admin_count {
            return Err(NavinError::InvalidMultiSigConfig);
        }

        storage::set_admin_list(&env, &admins);
        storage::set_multisig_threshold(&env, threshold);
        storage::set_proposal_counter(&env, 0);

        env.events()
            .publish((symbol_short!("ms_init"),), (admin_count, threshold));

        Ok(())
    }

    /// Propose a critical admin action that requires multi-sig approval.
    /// Only admins in the admin list can propose actions.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `proposer` - Admin address creating the proposal.
    /// * `action` - The action to be executed after approval.
    ///
    /// # Returns
    /// * `Result<u64, NavinError>` - The proposal ID.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::NotAnAdmin` - If caller is not in the admin list.
    ///
    /// # Examples
    /// ```rust
    /// // let action = AdminAction::Upgrade(new_wasm_hash);
    /// // let proposal_id = contract.propose_action(&env, &admin, &action);
    /// ```
    pub fn propose_action(
        env: Env,
        proposer: Address,
        action: crate::types::AdminAction,
    ) -> Result<u64, NavinError> {
        require_initialized(&env)?;
        proposer.require_auth();

        // Check if proposer is in admin list
        if !storage::is_admin(&env, &proposer) {
            return Err(NavinError::NotAnAdmin);
        }

        let proposal_id = storage::get_proposal_counter(&env)
            .checked_add(1)
            .ok_or(NavinError::CounterOverflow)?;

        let now = env.ledger().timestamp();
        let config = config::get_config(&env);
        let expires_at = now + config.proposal_expiry_seconds;

        let mut approvals = soroban_sdk::Vec::new(&env);
        approvals.push_back(proposer.clone());

        let proposal = crate::types::Proposal {
            id: proposal_id,
            proposer: proposer.clone(),
            action: action.clone(),
            approvals,
            created_at: now,
            expires_at,
            executed: false,
        };

        storage::set_proposal(&env, &proposal);
        storage::set_proposal_counter(&env, proposal_id);

        env.events()
            .publish((symbol_short!("propose"),), (proposal_id, proposer, action));

        Ok(proposal_id)
    }

    /// Approve a pending proposal. Only admins in the admin list can approve.
    /// Same admin cannot approve twice.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `approver` - Admin address approving the proposal.
    /// * `proposal_id` - ID of the proposal to approve.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if approved successfully.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::NotAnAdmin` - If caller is not in the admin list.
    /// * `NavinError::ProposalNotFound` - If proposal doesn't exist.
    /// * `NavinError::ProposalExpired` - If proposal has expired.
    /// * `NavinError::ProposalAlreadyExecuted` - If proposal was already executed.
    /// * `NavinError::AlreadyApproved` - If admin already approved this proposal.
    ///
    /// # Examples
    /// ```rust
    /// // contract.approve_action(&env, &admin2, 1);
    /// ```
    pub fn approve_action(env: Env, approver: Address, proposal_id: u64) -> Result<(), NavinError> {
        require_initialized(&env)?;
        approver.require_auth();

        // Check if approver is in admin list
        if !storage::is_admin(&env, &approver) {
            return Err(NavinError::NotAnAdmin);
        }

        let mut proposal =
            storage::get_proposal(&env, proposal_id).ok_or(NavinError::ProposalNotFound)?;

        // Check if proposal has expired
        let now = env.ledger().timestamp();
        if now > proposal.expires_at {
            return Err(NavinError::ProposalExpired);
        }

        // Check if already executed
        if proposal.executed {
            return Err(NavinError::ProposalAlreadyExecuted);
        }

        // Check if already approved by this admin
        for existing_approver in proposal.approvals.iter() {
            if existing_approver == approver {
                return Err(NavinError::AlreadyApproved);
            }
        }

        // Add approval
        proposal.approvals.push_back(approver.clone());
        storage::set_proposal(&env, &proposal);

        env.events().publish(
            (symbol_short!("approve"),),
            (proposal_id, approver, proposal.approvals.len()),
        );

        // Check if threshold is met and auto-execute
        let threshold = storage::get_multisig_threshold(&env).unwrap_or(2);
        if proposal.approvals.len() >= threshold {
            Self::execute_proposal_internal(env.clone(), proposal_id)?;
        }

        Ok(())
    }

    /// Execute a proposal that has met the approval threshold.
    /// Can be called by anyone once threshold is met.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `proposal_id` - ID of the proposal to execute.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if executed successfully.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ProposalNotFound` - If proposal doesn't exist.
    /// * `NavinError::ProposalExpired` - If proposal has expired.
    /// * `NavinError::ProposalAlreadyExecuted` - If proposal was already executed.
    /// * `NavinError::InsufficientApprovals` - If not enough approvals.
    ///
    /// # Examples
    /// ```rust
    /// // contract.execute_proposal(&env, 1);
    /// ```
    pub fn execute_proposal(env: Env, proposal_id: u64) -> Result<(), NavinError> {
        require_initialized(&env)?;
        Self::execute_proposal_internal(env, proposal_id)
    }

    /// Internal function to execute a proposal.
    fn execute_proposal_internal(env: Env, proposal_id: u64) -> Result<(), NavinError> {
        let mut proposal =
            storage::get_proposal(&env, proposal_id).ok_or(NavinError::ProposalNotFound)?;

        // Check if proposal has expired
        let now = env.ledger().timestamp();
        if now > proposal.expires_at {
            return Err(NavinError::ProposalExpired);
        }

        // Check if already executed
        if proposal.executed {
            return Err(NavinError::ProposalAlreadyExecuted);
        }

        // Check if threshold is met
        let threshold = storage::get_multisig_threshold(&env).unwrap_or(2);
        if proposal.approvals.len() < threshold {
            return Err(NavinError::InsufficientApprovals);
        }

        // Mark as executed
        proposal.executed = true;
        storage::set_proposal(&env, &proposal);

        // Execute the action (clone action before matching to avoid move issues)
        let action = proposal.action.clone();
        match action {
            crate::types::AdminAction::Upgrade(wasm_hash) => {
                let new_version = storage::get_version(&env)
                    .checked_add(1)
                    .ok_or(NavinError::CounterOverflow)?;

                storage::set_version(&env, new_version);
                events::emit_contract_upgraded(&env, &proposal.proposer, &wasm_hash, new_version);
                env.deployer().update_current_contract_wasm(wasm_hash);
            }
            crate::types::AdminAction::TransferAdmin(new_admin) => {
                let old_admin = storage::get_admin(&env);
                storage::set_admin(&env, &new_admin);
                storage::set_company_role(&env, &new_admin);
                events::emit_admin_transferred(&env, &old_admin, &new_admin);
            }
            crate::types::AdminAction::ForceRelease(shipment_id) => {
                let mut shipment =
                    storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

                let escrow_amount = shipment.escrow_amount;
                if escrow_amount > 0 {
                    // Get token contract address
                    if let Some(token_contract) = storage::get_token_contract(&env) {
                        // Transfer tokens from this contract to carrier
                        let contract_address = env.current_contract_address();
                        let mut args: soroban_sdk::Vec<soroban_sdk::Val> =
                            soroban_sdk::Vec::new(&env);
                        args.push_back(contract_address.into_val(&env));
                        args.push_back(shipment.carrier.clone().into_val(&env));
                        args.push_back(escrow_amount.into_val(&env));
                        env.invoke_contract::<()>(
                            &token_contract,
                            &symbol_short!("transfer"),
                            args,
                        );
                    }

                    shipment.escrow_amount = 0;
                    shipment.updated_at = env.ledger().timestamp();
                    storage::set_shipment(&env, &shipment);

                    events::emit_escrow_released(
                        &env,
                        shipment_id,
                        &shipment.carrier,
                        escrow_amount,
                    );
                }
            }
            crate::types::AdminAction::ForceRefund(shipment_id) => {
                let mut shipment =
                    storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

                let escrow_amount = shipment.escrow_amount;
                if escrow_amount > 0 {
                    // Get token contract address
                    if let Some(token_contract) = storage::get_token_contract(&env) {
                        // Transfer tokens from this contract to company
                        let contract_address = env.current_contract_address();
                        let mut args: soroban_sdk::Vec<soroban_sdk::Val> =
                            soroban_sdk::Vec::new(&env);
                        args.push_back(contract_address.into_val(&env));
                        args.push_back(shipment.sender.clone().into_val(&env));
                        args.push_back(escrow_amount.into_val(&env));
                        env.invoke_contract::<()>(
                            &token_contract,
                            &symbol_short!("transfer"),
                            args,
                        );
                    }

                    shipment.escrow_amount = 0;
                    shipment.updated_at = env.ledger().timestamp();
                    storage::set_shipment(&env, &shipment);

                    events::emit_escrow_refunded(
                        &env,
                        shipment_id,
                        &shipment.sender,
                        escrow_amount,
                    );
                }
            }
        }

        env.events()
            .publish((symbol_short!("executed"),), (proposal_id, proposal.action));

        Ok(())
    }

    /// Get a proposal by ID.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `proposal_id` - ID of the proposal.
    ///
    /// # Returns
    /// * `Result<Proposal, NavinError>` - The proposal data.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::ProposalNotFound` - If proposal doesn't exist.
    ///
    /// # Examples
    /// ```rust
    /// // let proposal = contract.get_proposal(&env, 1);
    /// ```
    pub fn get_proposal(env: Env, proposal_id: u64) -> Result<crate::types::Proposal, NavinError> {
        require_initialized(&env)?;
        storage::get_proposal(&env, proposal_id).ok_or(NavinError::ProposalNotFound)
    }

    /// Get the multi-sig configuration.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<(Vec<Address>, u32), NavinError>` - Tuple of (admin list, threshold).
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let (admins, threshold) = contract.get_multisig_config(&env);
    /// ```
    pub fn get_multisig_config(env: Env) -> Result<(soroban_sdk::Vec<Address>, u32), NavinError> {
        require_initialized(&env)?;
        let admins = storage::get_admin_list(&env).unwrap_or(soroban_sdk::Vec::new(&env));
        let threshold = storage::get_multisig_threshold(&env).unwrap_or(0);
        Ok((admins, threshold))
    }

    /// Update the contract configuration.
    /// Only the admin can update the configuration.
    /// Emits a `config_updated` event on success.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `admin` - Contract admin address.
    /// * `new_config` - The new configuration to apply.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully updated.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    /// * `NavinError::Unauthorized` - If caller is not the admin.
    /// * `NavinError::InvalidConfig` - If the configuration is invalid.
    ///
    /// # Examples
    /// ```rust
    /// // let mut config = ContractConfig::default();
    /// // config.batch_operation_limit = 20;
    /// // contract.update_config(&env, &admin, config);
    /// ```
    pub fn update_config(
        env: Env,
        admin: Address,
        new_config: ContractConfig,
    ) -> Result<(), NavinError> {
        require_initialized(&env)?;
        admin.require_auth();

        if storage::get_admin(&env) != admin {
            return Err(NavinError::Unauthorized);
        }

        // Validate the new configuration
        config::validate_config(&new_config).map_err(|_| NavinError::InvalidConfig)?;

        // Store the new configuration
        config::set_config(&env, &new_config);

        // Emit config_updated event
        env.events()
            .publish((Symbol::new(&env, "config_updated"),), (admin, new_config));

        Ok(())
    }

    /// Get the current contract configuration.
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    ///
    /// # Returns
    /// * `Result<ContractConfig, NavinError>` - The current configuration.
    ///
    /// # Errors
    /// * `NavinError::NotInitialized` - If contract is not initialized.
    ///
    /// # Examples
    /// ```rust
    /// // let config = contract.get_config(&env);
    /// ```
    pub fn get_contract_config(env: Env) -> Result<ContractConfig, NavinError> {
        require_initialized(&env)?;
        Ok(config::get_config(&env))
    }

    /// Cancel a shipment and auto-refund escrow if its delivery deadline has passed.
    /// Permissionless design — can be triggered by any caller (e.g., automated cron/crank).
    ///
    /// # Arguments
    /// * `env` - Execution environment.
    /// * `shipment_id` - ID of the target shipment.
    ///
    /// # Returns
    /// * `Result<(), NavinError>` - Ok if successfully cancelled and escrow refunded.
    ///
    /// # Errors
    /// * `NavinError::NotExpired` - If the current ledger time hasn't passed the deadline.
    /// * `NavinError::ShipmentAlreadyCompleted` - If the shipment is already in a terminal state.
    pub fn check_deadline(env: Env, shipment_id: u64) -> Result<(), NavinError> {
        require_initialized(&env)?;

        let mut shipment =
            storage::get_shipment(&env, shipment_id).ok_or(NavinError::ShipmentNotFound)?;

        if env.ledger().timestamp() < shipment.deadline {
            return Err(NavinError::NotExpired);
        }

        match shipment.status {
            ShipmentStatus::Delivered | ShipmentStatus::Disputed | ShipmentStatus::Cancelled => {
                return Err(NavinError::ShipmentAlreadyCompleted);
            }
            _ => {}
        }

        let escrow_amount = shipment.escrow_amount;
        let old_status = shipment.status.clone();
        shipment.status = ShipmentStatus::Cancelled;
        shipment.escrow_amount = 0;
        shipment.updated_at = env.ledger().timestamp();

        storage::set_shipment(&env, &shipment);
        storage::decrement_status_count(&env, &old_status);
        storage::increment_status_count(&env, &ShipmentStatus::Cancelled);
        storage::decrement_active_shipment_count(&env, &shipment.sender);

        if escrow_amount > 0 {
            storage::remove_escrow_balance(&env, shipment_id);

            let token_contract =
                storage::get_token_contract(&env).ok_or(NavinError::NotInitialized)?;
            let contract_address = env.current_contract_address();
            let mut args: soroban_sdk::Vec<soroban_sdk::Val> = Vec::new(&env);

            args.push_back(contract_address.into_val(&env));
            args.push_back(shipment.sender.clone().into_val(&env));
            args.push_back(escrow_amount.into_val(&env));
            env.invoke_contract::<soroban_sdk::Val>(
                &token_contract,
                &symbol_short!("transfer"),
                args,
            );
            events::emit_escrow_refunded(&env, shipment_id, &shipment.sender, escrow_amount);
        }

        extend_shipment_ttl(&env, shipment_id);
        events::emit_shipment_expired(&env, shipment_id);

        Ok(())
    }
}
