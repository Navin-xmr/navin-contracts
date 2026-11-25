//! Storage helpers for the Navin shipment contract, split into logical
//! submodules by concern.  All public symbols are re-exported here so
//! callers continue to use `storage::fn_name()` without any path change.

mod config;
mod counters;
mod evidence;
mod multisig;
mod roles;
mod settlements;
mod shipments;

// ── Re-exports ────────────────────────────────────────────────────────────────

#[allow(unused_imports)]
pub use config::{
    clear_proposed_admin, get_admin, get_proposed_admin, get_token_contract, get_version,
    is_initialized, is_paused, is_reentrancy_locked, set_admin, set_paused, set_proposed_admin,
    set_reentrancy_lock, set_token_contract, set_version,
};

#[allow(unused_imports)]
pub use roles::{
    add_carrier_to_whitelist, get_role, has_carrier_role, has_company_role, has_role,
    is_carrier_suspended, is_carrier_whitelisted, is_company_suspended, is_role_suspended,
    reactivate_carrier, reactivate_company, reactivate_role, remove_carrier_from_whitelist,
    revoke_role, set_carrier_role, set_company_role, set_role, suspend_carrier, suspend_company,
    suspend_role,
};

#[allow(unused_imports)]
pub use shipments::{
    append_partial_delivery_proof, append_status_hash, archive_shipment, clamp_ttl,
    confirmation_hash_key, escrow_freeze_reason_key, escrow_key, extend_archived_shipment_ttl,
    extend_shipment_ttl, extend_status_hash_ttl, find_status_hash, get_archived_shipment,
    get_confirmation_hash, get_escrow, get_escrow_balance, get_escrow_freeze_reason,
    get_last_status_update, get_partial_delivery_proof, get_partial_delivery_proofs, get_shipment,
    get_shipment_dependents, get_shipment_metadata, get_status_hash_log,
    has_confirmation_hash_entry, has_escrow_entry, has_escrow_mismatch, has_event_count_entry,
    has_last_status_update_entry, has_persistent_shipment, is_shipment_archived,
    purge_status_hash_log, purge_status_hashes, remove_escrow, remove_escrow_balance,
    remove_shipment_dependents, set_confirmation_hash, set_escrow, set_escrow_freeze_reason,
    set_last_status_update, set_shipment, set_shipment_dependency, set_shipment_metadata,
    shipment_key, status_hash_history, status_hash_history_for,
};

#[allow(unused_imports)]
pub use counters::{
    add_total_escrow_volume, decrement_active_shipment_count, decrement_status_count,
    get_active_shipment_count, get_breach_event_count, get_company_shipment_limit,
    get_creation_quota, get_effective_shipment_limit, get_event_count, get_fee_config,
    get_global_event_count, get_milestone_event_count, get_shipment_count, get_shipment_counter,
    get_shipment_limit, get_status_count, get_total_disputes, get_total_escrow_volume,
    has_idempotency_window, increment_active_shipment_count, increment_breach_event_count,
    increment_global_event_count, increment_milestone_event_count, increment_status_count,
    increment_total_disputes, set_company_shipment_limit, set_creation_quota, set_fee_config,
    set_idempotency_window, set_shipment_counter, set_shipment_limit,
    try_increment_global_event_count,
};

#[allow(unused_imports)]
pub use multisig::{
    extend_proposal_ttl, get_admin_list, get_multisig_threshold, get_proposal,
    get_proposal_counter, get_proposal_digest, is_admin, is_proposal_salt_used, set_admin_list,
    set_multisig_threshold, set_proposal, set_proposal_counter, set_proposal_digest,
    set_proposal_salt_used,
};

#[allow(unused_imports)]
pub use settlements::{
    clear_active_settlement, get_active_settlement, get_latest_settlement, get_settlement,
    get_settlement_counter, increment_settlement_counter, set_active_settlement, set_settlement,
    set_settlement_counter,
};

#[allow(unused_imports)]
pub use evidence::{append_evidence, get_evidence, get_evidence_count};

// ── Tests (kept in mod.rs to retain `use super::*` access) ───────────────────

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::errors::NavinError;
    use crate::test_utils;
    use crate::types::{DataKey, Role};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, BytesN, Env, TryFromVal, Vec};

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
            assert!(env.storage().persistent().has(&canonical_key));
            assert!(!env.storage().persistent().has(&reversed_key));
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
    fn has_entry_helpers_match_current_data_key_schema() {
        let (env, contract_id) = with_contract_env();
        let shipment_id = 7u64;
        let hash = BytesN::from_array(&env, &[0xABu8; 32]);

        env.as_contract(&contract_id, || {
            assert!(!has_event_count_entry(&env, shipment_id));
            assert!(!has_confirmation_hash_entry(&env, shipment_id));
            assert!(!has_last_status_update_entry(&env, shipment_id));

            env.storage()
                .persistent()
                .set(&DataKey::EventCount(shipment_id), &3u32);
            set_confirmation_hash(&env, shipment_id, &hash);
            set_last_status_update(&env, shipment_id, 42);

            assert!(has_event_count_entry(&env, shipment_id));
            assert!(env
                .storage()
                .persistent()
                .has(&DataKey::EventCount(shipment_id)));

            assert!(has_confirmation_hash_entry(&env, shipment_id));
            assert!(env
                .storage()
                .persistent()
                .has(&confirmation_hash_key(shipment_id)));
            assert_eq!(get_confirmation_hash(&env, shipment_id), Some(hash.clone()));

            assert!(has_last_status_update_entry(&env, shipment_id));
            assert!(env
                .storage()
                .persistent()
                .has(&DataKey::LastStatusUpdate(shipment_id)));
            assert_eq!(get_last_status_update(&env, shipment_id), Some(42));
        });
    }

    #[test]
    fn global_event_count_reads_legacy_u32_value() {
        let (env, contract_id) = with_contract_env();

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&DataKey::GlobalEventCount, &u32::MAX);

            assert_eq!(get_global_event_count(&env), u64::from(u32::MAX));
            // Crossing the old u32 limit no longer saturates.
            assert_eq!(increment_global_event_count(&env), u64::from(u32::MAX) + 1);
            assert_eq!(get_global_event_count(&env), u64::from(u32::MAX) + 1);
        });
    }

    #[test]
    fn global_event_count_overflow_surfaces_counter_overflow() {
        let (env, contract_id) = with_contract_env();

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&DataKey::GlobalEventCount, &(u64::MAX - 1));

            assert_eq!(try_increment_global_event_count(&env), Ok(u64::MAX));
            assert_eq!(
                try_increment_global_event_count(&env),
                Err(NavinError::CounterOverflow)
            );
            // A failed increment leaves the stored counter untouched.
            assert_eq!(get_global_event_count(&env), u64::MAX);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #11)")]
    fn global_event_count_panics_instead_of_saturating() {
        let (env, contract_id) = with_contract_env();

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&DataKey::GlobalEventCount, &u64::MAX);
            increment_global_event_count(&env);
        });
    }

    #[test]
    fn global_event_idempotency_keys_stay_unique_past_u32_max() {
        use soroban_sdk::testutils::Events as _;

        let (env, contract_id) = with_contract_env();
        let company = Address::generate(&env);
        let carrier = Address::generate(&env);

        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&DataKey::GlobalEventCount, &(u32::MAX - 1));
            for _ in 0..4 {
                crate::events::emit_whitelist_added(&env, &company, &carrier);
            }
        });

        let mut counters: Vec<u64> = Vec::new(&env);
        let mut keys: Vec<BytesN<32>> = Vec::new(&env);
        for (_, _, data) in env.events().all().iter() {
            let (_, _, _, counter, key): (Address, Address, u32, u64, BytesN<32>) =
                TryFromVal::try_from_val(&env, &data).unwrap();
            assert!(!counters.contains(&counter), "duplicate event counter");
            assert!(!keys.contains(&key), "duplicate idempotency key");
            counters.push_back(counter);
            keys.push_back(key);
        }
        assert_eq!(keys.len(), 4);
        assert_eq!(
            counters.get(3),
            Some(u64::from(u32::MAX) + 2),
            "counter must keep growing past u32::MAX"
        );
    }
}
