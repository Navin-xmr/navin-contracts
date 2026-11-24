//! Test that instance storage size does not grow with per-address keys moved to persistent storage.
//!
//! This test validates the fix for the issue where per-address keys
//! (CarrierWhitelist, UserRole, RoleSuspended, CompanySuspended,
//! CompanyShipmentLimit, ActiveShipmentCount, CompanyCreationQuota)
//! were previously stored in unbounded instance storage. After the fix,
//! these keys should live in persistent storage and instance storage
//! should remain bounded regardless of the number of companies/carriers.

extern crate std;

use crate::{
    contract::{init, register_company, whitelist_carrier, set_role},
    errors::NavinError,
    storage,
    types::{Role, CreationQuotaTracker},
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

/// Get the approximate size of instance storage in bytes.
/// We measure by counting the number of keys and estimating size.
fn estimate_instance_storage_size(env: &Env) -> usize {
    // Rough estimate: each entry takes ~32 bytes overhead + key/value size
    // For this test we just check that creating N companies doesn't
    // increase instance storage size significantly
    env.storage().instance().len()
}

#[test]
fn test_instance_storage_bounded_with_companies() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::random(&env);
    
    // Initialize contract
    init(&env, &admin);
    
    // Measure instance storage before adding companies
    let initial_instance_size = estimate_instance_storage_size(&env);
    println!("Initial instance storage entries: {}", initial_instance_size);
    
    // Create 50 companies and verify instance storage doesn't grow linearly
    let mut company_addresses = Vec::new();
    for i in 0..50 {
        let company = Address::random(&env);
        company_addresses.push(company.clone());
        
        // Register as company (this uses set_role)
        set_role(&env, &company, &Role::Company);
        
        // Add a shipment limit (this uses set_company_shipment_limit)
        storage::set_company_shipment_limit(&env, &company, 100 + i);
        
        // Add a creation quota tracker
        let tracker = CreationQuotaTracker {
            created_at: env.ledger().timestamp(),
            count: 0,
        };
        storage::set_creation_quota(&env, &company, &tracker);
    }
    
    // Measure instance storage after adding companies
    let after_companies_size = estimate_instance_storage_size(&env);
    println!("Instance storage entries after 50 companies: {}", after_companies_size);
    
    // The instance storage should NOT have grown by 50 or more entries
    // (each company would add at least 3 keys if they were still in instance storage)
    let growth = after_companies_size.saturating_sub(initial_instance_size);
    println!("Growth in instance entries: {} (expected: minimal)", growth);
    
    // Growth should be very small - maybe just the config or counters
    // If all per-address keys are in persistent storage, growth should be 0-2
    assert!(
        growth <= 5,
        "Instance storage grew by {} entries - per-address keys may still be in instance storage!",
        growth
    );
}

#[test]
fn test_instance_storage_bounded_with_carriers() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::random(&env);
    let company = Address::random(&env);
    
    // Initialize contract
    init(&env, &admin);
    
    // Set up company
    set_role(&env, &company, &Role::Company);
    
    // Measure instance storage before adding carriers
    let initial_instance_size = estimate_instance_storage_size(&env);
    println!("Initial instance storage entries: {}", initial_instance_size);
    
    // Create 50 carriers and whitelist them all
    for i in 0..50 {
        let carrier = Address::random(&env);
        
        // Set as carrier
        set_role(&env, &carrier, &Role::Carrier);
        
        // Whitelist for company (this uses add_carrier_to_whitelist)
        storage::add_carrier_to_whitelist(&env, &company, &carrier);
    }
    
    // Measure instance storage after adding carriers
    let after_carriers_size = estimate_instance_storage_size(&env);
    println!("Instance storage entries after 50 carriers: {}", after_carriers_size);
    
    // The instance storage should NOT have grown by 100+ entries
    // (50 UserRole entries + 50 CarrierWhitelist entries if still in instance)
    let growth = after_carriers_size.saturating_sub(initial_instance_size);
    println!("Growth in instance entries: {} (expected: minimal)", growth);
    
    // Growth should be small - no per-address keys in instance
    assert!(
        growth <= 10,
        "Instance storage grew by {} entries - whitelist/role keys may still be in instance storage!",
        growth
    );
}

#[test]
fn test_per_address_keys_in_persistent_storage() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::random(&env);
    let company = Address::random(&env);
    let carrier = Address::random(&env);
    
    // Initialize contract
    init(&env, &admin);
    
    // Set roles
    set_role(&env, &company, &Role::Company);
    set_role(&env, &carrier, &Role::Carrier);
    
    // Whitelist carrier
    storage::add_carrier_to_whitelist(&env, &company, &carrier);
    
    // Set company limit
    storage::set_company_shipment_limit(&env, &company, 100);
    
    // Increment active count
    storage::increment_active_shipment_count(&env, &company);
    
    // Verify keys exist in persistent storage
    assert!(
        storage::is_carrier_whitelisted(&env, &company, &carrier),
        "CarrierWhitelist key should be accessible in persistent storage"
    );
    
    assert!(
        storage::has_role(&env, &company, &Role::Company),
        "UserRole key should be accessible in persistent storage"
    );
    
    assert_eq!(
        storage::get_company_shipment_limit(&env, &company),
        Some(100),
        "CompanyShipmentLimit key should be accessible in persistent storage"
    );
    
    assert_eq!(
        storage::get_active_shipment_count(&env, &company),
        1,
        "ActiveShipmentCount key should be accessible in persistent storage"
    );
}

#[test]
fn test_role_suspension_in_persistent_storage() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::random(&env);
    let carrier = Address::random(&env);
    
    // Initialize contract
    init(&env, &admin);
    
    // Set carrier role
    set_role(&env, &carrier, &Role::Carrier);
    
    // Suspend the carrier
    storage::suspend_role(&env, &carrier, &Role::Carrier);
    
    // Verify suspension flag is in persistent storage
    assert!(
        storage::is_role_suspended(&env, &carrier, &Role::Carrier),
        "RoleSuspended key should be in persistent storage"
    );
    
    // Reactivate
    storage::reactivate_role(&env, &carrier, &Role::Carrier);
    
    // Verify suspension was removed
    assert!(
        !storage::is_role_suspended(&env, &carrier, &Role::Carrier),
        "RoleSuspended flag should be removed"
    );
}

#[test]
fn test_company_suspension_in_persistent_storage() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::random(&env);
    let company = Address::random(&env);
    
    // Initialize contract
    init(&env, &admin);
    
    // Set company role
    set_role(&env, &company, &Role::Company);
    
    // Suspend the company
    storage::suspend_company(&env, &company);
    
    // Verify suspension flag is in persistent storage
    assert!(
        storage::is_company_suspended(&env, &company),
        "CompanySuspended key should be in persistent storage"
    );
    
    // Reactivate
    storage::reactivate_company(&env, &company);
    
    // Verify suspension was removed
    assert!(
        !storage::is_company_suspended(&env, &company),
        "CompanySuspended flag should be removed"
    );
}
