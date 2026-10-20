#![cfg(test)]

extern crate std;

use super::test_helpers::*;


#[test]
fn test_only_admin_can_assign_roles() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);

    // Admin can add company
    client.add_company(&admin, &company);
    // Admin can add carrier
    client.add_carrier(&admin, &carrier);

    // Non-admin cannot add company
    env.mock_all_auths();
    let result = client.try_add_company(&outsider, &Address::generate(&env));
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // Non-admin cannot add carrier
    let result = client.try_add_carrier(&outsider, &Address::generate(&env));
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));
}

#[test]
fn test_unassigned_addresses_are_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);

    // Unassigned cannot create shipment
    let result = client.try_create_shipment(
        &outsider,
        &Address::generate(&env),
        &Address::generate(&env),
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // Unassigned cannot add carrier to whitelist
    let result = client.try_add_carrier_to_whitelist(&outsider, &Address::generate(&env));
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));

    // Unassigned cannot report geofence event
    let result =
        client.try_report_geofence_event(&outsider, &1, &GeofenceEvent::ZoneEntry, &data_hash);
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));
}

// Admin Transfer Tests

#[test]
fn test_successful_admin_transfer() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let new_admin = Address::generate(&env);

    // 1. Current admin proposes new admin
    client.transfer_admin(&admin, &new_admin);

    // 2. New admin accepts the transfer
    client.accept_admin_transfer(&new_admin);

    // Verify ownership changed
    assert_eq!(client.get_admin(), new_admin);

    // Verify old admin lost privileges
    let company = Address::generate(&env);
    env.mock_all_auths();

    // Attempting to add a company with the old admin should now fail
    let result = client.try_add_company(&admin, &company);
    assert_eq!(result, Err(Ok(crate::NavinError::Unauthorized)));
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_unauthorized_admin_transfer() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let outsider = Address::generate(&env);
    let new_admin = Address::generate(&env);

    // Outsider tries to transfer admin - should fail
    client.transfer_admin(&outsider, &new_admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_unauthorized_admin_acceptance() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let new_admin = Address::generate(&env);
    let imposter = Address::generate(&env);

    // 1. Current admin proposes new admin
    client.transfer_admin(&admin, &new_admin);

    // 2. Imposter tries to accept the transfer - should fail
    client.accept_admin_transfer(&imposter);
}

// Multi-Signature Tests

#[test]
fn test_init_multisig_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    client.init_multisig(&admin, &admins, &2);

    let (stored_admins, threshold) = client.get_multisig_config();
    assert_eq!(stored_admins.len(), 3);
    assert_eq!(threshold, 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_init_multisig_invalid_threshold_too_high() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1);
    admins.push_back(admin2);

    // Threshold 3 > admin count 2
    client.init_multisig(&admin, &admins, &3);
}

#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_init_multisig_invalid_threshold_zero() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1);
    admins.push_back(admin2);

    // Threshold 0 is invalid
    client.init_multisig(&admin, &admins, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_init_multisig_too_few_admins() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1);

    // Only 1 admin, need at least 2
    client.init_multisig(&admin, &admins, &1);
}

#[test]
fn test_propose_action_upgrade() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    client.init_multisig(&admin, &admins, &2);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    let proposal_id = client.propose_action(&admin1, &action);
    assert_eq!(proposal_id, 1);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.id, 1);
    assert_eq!(proposal.proposer, admin1);
    assert_eq!(proposal.approvals.len(), 1);
    assert!(!proposal.executed);
}

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_propose_action_not_admin() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let outsider = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1);
    admins.push_back(admin2);

    client.init_multisig(&admin, &admins, &2);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    // Outsider tries to propose
    client.propose_action(&outsider, &action);
}

#[test]
fn test_approve_action_success() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    // Set threshold to 3 so it doesn't auto-execute on second approval
    client.init_multisig(&admin, &admins, &3);

    let new_admin = Address::generate(&env);
    let action = crate::AdminAction::TransferAdmin(new_admin);

    let proposal_id = client.propose_action(&admin1, &action);

    // Admin2 approves
    client.approve_action(&admin2, &proposal_id);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.approvals.len(), 2);
    assert!(!proposal.executed); // Should not be executed yet
}

#[test]
#[should_panic(expected = "Error(Contract, #25)")]
fn test_approve_action_already_approved() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    let proposal_id = client.propose_action(&admin1, &action);

    // Admin1 tries to approve again (already approved when proposing)
    client.approve_action(&admin1, &proposal_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_approve_action_not_admin() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let outsider = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    let proposal_id = client.propose_action(&admin1, &action);

    // Outsider tries to approve
    client.approve_action(&outsider, &proposal_id);
}

#[test]
fn test_execute_proposal_auto_on_threshold() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let wasm: &[u8] = crate::test_utils::UPGRADE_WASM;
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    client.init_multisig(&admin, &admins, &2);

    let action = crate::AdminAction::Upgrade(new_wasm_hash);
    let proposal_id = client.propose_action(&admin1, &action);

    // Admin2 approves - this should auto-execute since threshold is met
    client.approve_action(&admin2, &proposal_id);

    // Verify version was incremented (check before trying to get proposal)
    let version: u32 = env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::Version)
            .unwrap()
    });
    assert_eq!(version, 2);

    // Note: After upgrade, the WASM is replaced, so we can't call get_proposal
    // on the upgraded contract. The execution happened successfully.
}

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn test_execute_proposal_already_executed() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let new_admin = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    // Use TransferAdmin action instead of Upgrade
    let action = crate::AdminAction::TransferAdmin(new_admin);
    let proposal_id = client.propose_action(&admin1, &action);

    client.approve_action(&admin2, &proposal_id);

    // Try to execute again
    client.execute_proposal(&proposal_id);
}

#[test]
fn test_proposal_expiration() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    let proposal_id = client.propose_action(&admin1, &action);

    // Fast forward time beyond expiration (7 days + 1 second)
    super::test_utils::advance_past_multisig_expiry(&env);

    // Try to approve expired proposal - should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.approve_action(&admin2, &proposal_id);
    }));

    assert!(result.is_err());
}

#[test]
fn test_transfer_admin_action() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let new_admin = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    client.init_multisig(&admin, &admins, &2);

    // Propose admin transfer
    let action = crate::AdminAction::TransferAdmin(new_admin.clone());
    let proposal_id = client.propose_action(&admin1, &action);

    // Approve and execute
    client.approve_action(&admin2, &proposal_id);

    // Verify admin was transferred
    let current_admin = client.get_admin();
    assert_eq!(current_admin, new_admin);
}

#[test]
fn test_three_of_five_multisig() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let wasm: &[u8] = crate::test_utils::UPGRADE_WASM;
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);
    let admin4 = Address::generate(&env);
    let admin5 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());
    admins.push_back(admin4.clone());
    admins.push_back(admin5.clone());

    client.init_multisig(&admin, &admins, &3);

    let action = crate::AdminAction::Upgrade(new_wasm_hash);
    let proposal_id = client.propose_action(&admin1, &action);

    // First approval (proposer)
    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.approvals.len(), 1);
    assert!(!proposal.executed);

    // Second approval
    client.approve_action(&admin2, &proposal_id);
    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.approvals.len(), 2);
    assert!(!proposal.executed);

    // Third approval - should auto-execute
    client.approve_action(&admin3, &proposal_id);

    // Verify version was incremented (check directly from storage)
    let version: u32 = env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::Version)
            .unwrap()
    });
    assert_eq!(version, 2);

    // Note: After upgrade, the WASM is replaced, so we can't call get_proposal
    // on the upgraded contract. The execution happened successfully.
}

#[test]
#[should_panic(expected = "Error(Contract, #26)")]
fn test_execute_proposal_insufficient_approvals() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    client.init_multisig(&admin, &admins, &3);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    let proposal_id = client.propose_action(&admin1, &action);

    // Only 1 approval, need 3
    client.execute_proposal(&proposal_id);
}

// Deadline Tests

// Notification Event Tests

// Analytics Tests
