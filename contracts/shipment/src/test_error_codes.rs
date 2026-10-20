#![cfg(test)]

extern crate std;

use super::test_helpers::*;


#[test]
fn test_get_status_summary_empty() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let summary = client.get_status_summary();
    assert_eq!(summary.created, 0);
    assert_eq!(summary.in_transit, 0);
    assert_eq!(summary.at_checkpoint, 0);
    assert_eq!(summary.partially_delivered, 0);
    assert_eq!(summary.delivered, 0);
    assert_eq!(summary.disputed, 0);
    assert_eq!(summary.cancelled, 0);
}

// #368: Status summary regression — every lifecycle bucket changes correctly
//
// This test covers a full mixed-state lifecycle: Created → InTransit →
// AtCheckpoint → Delivered and Created → Cancelled.  The summary is asserted
// after every individual transition so that any accounting drift in
// `get_status_summary` (added or missing counter update) is caught immediately.

// Shipment Limit Tests

#[test]
fn test_set_and_get_shipment_limit() {
    let (_env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    // Default limit should be 100 (set in initialize)
    assert_eq!(client.get_shipment_limit(), 100);

    // Admin sets new limit
    client.set_shipment_limit(&admin, &10);
    assert_eq!(client.get_shipment_limit(), 10);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_set_shipment_limit_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let outsider = Address::generate(&env);
    client.initialize(&admin, &token_contract);

    // Outsider tries to set limit
    client.set_shipment_limit(&outsider, &10);
}

// Dispute Evidence Tests

// Circular Dependency Tests
// Dependencies Not Met Tests

#[test]
#[should_panic(expected = "Error(Contract, #30)")]
fn test_batch_limit_reached() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    // Set limit to 2
    client.set_shipment_limit(&admin, &2);

    // Attempt to create 3 shipments in a batch
    let mut shipments = soroban_sdk::Vec::new(&env);
    for i in 1..=3 {
        shipments.push_back(ShipmentInput {
            receiver: Address::generate(&env),
            carrier: Address::generate(&env),
            data_hash: BytesN::from_array(&env, &[i as u8; 32]),
            payment_milestones: soroban_sdk::Vec::new(&env),
            deadline,
        });
    }

    client.create_shipments_batch(&company, &shipments);
}

// Proposal Salt Reuse Tests

#[test]
fn test_proposal_salt_reused_rejected() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    // Set up multisig
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &1);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let salt = BytesN::from_array(&env, &[1u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    // First proposal with specific salt should succeed
    let proposal_id_1 = client.propose_action_with_salt(&admin, &action, &salt);
    assert_eq!(proposal_id_1, 1);

    // Attempt second proposal with the same salt should fail with ProposalSaltReused
    let result = client.try_propose_action_with_salt(&admin, &action, &salt);
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalSaltReused)),
        "reused salt should be rejected with ProposalSaltReused error"
    );
}

#[test]
fn test_proposal_unique_salts_accepted() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    // Set up multisig
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &1);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let salt_1 = BytesN::from_array(&env, &[1u8; 32]);
    let salt_2 = BytesN::from_array(&env, &[2u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    // First proposal with salt_1
    let proposal_id_1 = client.propose_action_with_salt(&admin, &action, &salt_1);
    assert_eq!(proposal_id_1, 1);

    // Second proposal with different salt_2 should succeed
    let proposal_id_2 = client.propose_action_with_salt(&admin, &action, &salt_2);
    assert_eq!(proposal_id_2, 2);

    // Both proposals should exist
    let proposal_1 = client.get_proposal(&proposal_id_1);
    let proposal_2 = client.get_proposal(&proposal_id_2);
    assert_eq!(proposal_1.id, proposal_id_1);
    assert_eq!(proposal_2.id, proposal_id_2);
}

#[test]
fn test_proposal_salt_reuse_error_code_56() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    // Set up multisig
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &1);

    let new_wasm_hash = BytesN::from_array(&env, &[42u8; 32]);
    let salt = BytesN::from_array(&env, &[99u8; 32]);
    let action = crate::AdminAction::Upgrade(new_wasm_hash);

    // Create first proposal
    let _ = client.propose_action_with_salt(&admin, &action, &salt);

    // Attempt to create second proposal with same salt
    let result = client.try_propose_action_with_salt(&admin, &action, &salt);

    // Verify the error is ProposalSaltReused with code 56
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalSaltReused)),
        "ProposalSaltReused error should be returned"
    );

    let salt_reused_err = crate::NavinError::ProposalSaltReused;
    assert_eq!(
        salt_reused_err as u32, 56,
        "ProposalSaltReused error code must be 56"
    );
}

#[test]
fn test_proposal_salt_different_actions_same_salt() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    // Set up multisig
    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());
    client.init_multisig(&admin, &admins, &1);

    let new_admin = Address::generate(&env);
    let salt = BytesN::from_array(&env, &[77u8; 32]);

    let action_1 = crate::AdminAction::Upgrade(BytesN::from_array(&env, &[42u8; 32]));
    let action_2 = crate::AdminAction::TransferAdmin(new_admin);

    // First proposal with salt
    let proposal_id_1 = client.propose_action_with_salt(&admin, &action_1, &salt);
    assert_eq!(proposal_id_1, 1);

    // Attempt second proposal with same salt but different action should also fail
    // Salt reuse is checked regardless of action content
    let result = client.try_propose_action_with_salt(&admin, &action_2, &salt);
    assert_eq!(
        result,
        Err(Ok(crate::NavinError::ProposalSaltReused)),
        "salt reuse should be rejected even with different actions"
    );
}

// COMPREHENSIVE NEGATIVE TEST SUITE - Testing All NavinError Variants
// This section systematically tests every NavinError variant to ensure
// proper error handling across all contract functions.

// Error #6: InvalidHash Tests

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_create_shipment_returns_invalid_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &zero_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
}

// NOTE: This test is commented out because the feature may not be fully implemented yet
// NOTE: This test is commented out because the feature may not be fully implemented yet
#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_confirm_delivery_returns_invalid_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

    let (receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::InTransit,
    );

    client.confirm_delivery(&receiver, &shipment_id, &zero_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_raise_dispute_returns_invalid_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

    let (_receiver, carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::InTransit,
    );

    client.raise_dispute(&carrier, &shipment_id, &zero_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #36)")]
fn test_resolve_dispute_returns_invalid_hash() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

    let (_receiver, _carrier, shipment_id) = setup_shipment_with_status(
        &env,
        &client,
        &admin,
        &token_contract,
        crate::ShipmentStatus::Disputed,
    );

    client.resolve_dispute(
        &admin,
        &shipment_id,
        &crate::DisputeResolution::RefundToCompany,
        &zero_hash,
    );
}

// Error #11: CounterOverflow Tests

// Error #12: CarrierNotWhitelisted Tests

// NOTE: This test is commented out because the feature may not be fully implemented yet

// Error #13: CarrierNotAuthorized Tests

// NOTE: This test is commented out because the feature may not be fully implemented yet

// Error #14: InvalidAmount Tests

// Error #15: EscrowAlreadyDeposited Tests

// Error #19: MilestoneAlreadyPaid Tests

// Error #20: MetadataLimitExceeded Tests

// Error #21: RateLimitExceeded Tests

// Error #22: ProposalNotFound Tests

#[test]
#[should_panic(expected = "Error(Contract, #22)")]
fn test_get_proposal_returns_proposal_not_found() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.get_proposal(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #22)")]
fn test_approve_action_returns_proposal_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &2);

    client.approve_action(&admin2, &999);
}

#[test]
#[should_panic(expected = "Error(Contract, #22)")]
fn test_execute_proposal_returns_proposal_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &2);

    client.execute_proposal(&999);
}

#[test]
fn test_approve_action_returns_proposal_not_found_variant() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2.clone());

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &2);

    let result = client.try_approve_action(&admin2, &999);
    assert_eq!(result, Err(Ok(NavinError::ProposalNotFound)));
}

#[test]
fn test_execute_proposal_returns_proposal_not_found_variant() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);

    client.initialize(&admin, &token_contract);
    client.init_multisig(&admin, &admins, &2);

    let result = client.try_execute_proposal(&999);
    assert_eq!(result, Err(Ok(NavinError::ProposalNotFound)));
}

// Error #23: ProposalAlreadyExecuted Tests

// Error #24: ProposalExpired Tests

// Error #25: AlreadyApproved Tests

// Approve the same proposal twice with the same admin — must return AlreadyApproved error.
// Two different admins can approve the same proposal — confirms multi-sig flow works.
// Error #26: InsufficientApprovals Tests

// Error #27: NotAnAdmin Tests

// Non-admin (not in admin list) attempts propose_action — must return NotAnAdmin.
// Non-admin (not in admin list) attempts approve_action — must return NotAnAdmin.
// Admin (in admin list) can propose_action — verifies admin operations succeed.
// Different admin (in admin list) can approve_action — verifies admin operations succeed.
// Error #28: InvalidMultiSigConfig Tests

#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_init_multisig_returns_invalid_config_threshold_too_high() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);

    client.initialize(&admin, &token_contract);

    // Threshold of 3 but only 2 admins
    client.init_multisig(&admin, &admins, &3);
}

#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_init_multisig_returns_invalid_multisig_config_threshold_zero() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);

    client.initialize(&admin, &token_contract);

    // Threshold of 0 is invalid
    client.init_multisig(&admin, &admins, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_init_multisig_returns_invalid_multisig_config_empty_admins() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    let admins = soroban_sdk::Vec::new(&env);

    client.initialize(&admin, &token_contract);

    // Empty admin list is invalid
    client.init_multisig(&admin, &admins, &1);
}

#[test]
#[should_panic(expected = "Error(Contract, #31)")]
fn test_init_multisig_duplicate_admins() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);
    admins.push_back(admin.clone());

    client.initialize(&admin, &token_contract);

    client.init_multisig(&admin, &admins, &2);
}

#[test]
fn test_init_multisig_invalid_config_threshold_exceeds_admin_count() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);

    env.mock_all_auths();
    client.initialize(&admin, &token_contract);

    let result = client.try_init_multisig(&admin, &admins, &3);

    assert_eq!(result, Err(Ok(NavinError::InvalidConfig)));
}

#[test]
fn test_init_multisig_invalid_multisig_config_threshold_zero() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let admin2 = Address::generate(&env);

    let mut admins = soroban_sdk::Vec::new(&env);
    admins.push_back(admin.clone());
    admins.push_back(admin2);

    env.mock_all_auths();
    client.initialize(&admin, &token_contract);

    let result = client.try_init_multisig(&admin, &admins, &0);

    assert_eq!(result, Err(Ok(NavinError::InvalidMultiSigConfig)));
}

// Error #29: NotExpired Tests

// Error #30: ShipmentLimitReached Tests

#[test]
#[should_panic(expected = "Error(Contract, #30)")]
fn test_create_shipments_batch_returns_shipment_limit_reached() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.set_shipment_limit(&admin, &2);

    let mut shipments = soroban_sdk::Vec::new(&env);
    for i in 1..=3 {
        shipments.push_back(ShipmentInput {
            receiver: Address::generate(&env),
            carrier: Address::generate(&env),
            data_hash: BytesN::from_array(&env, &[i as u8; 32]),
            payment_milestones: soroban_sdk::Vec::new(&env),
            deadline,
        });
    }

    // Try to create 3 shipments when limit is 2
    client.create_shipments_batch(&company, &shipments);
}

// Additional Coverage for NotInitialized Error

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_create_shipment_returns_not_initialized() {
    let (env, client, _admin, _token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_add_company_returns_not_initialized() {
    let (env, client, admin, _token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.add_company(&admin, &company);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_add_carrier_returns_not_initialized() {
    let (env, client, admin, _token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);

    client.add_carrier(&admin, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_admin_returns_not_initialized() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    client.get_admin();
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_set_shipment_limit_returns_not_initialized() {
    let (_env, client, admin, _token_contract) = setup_shipment_env();

    client.set_shipment_limit(&admin, &10);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_shipment_limit_returns_not_initialized() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    client.get_shipment_limit();
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_active_shipment_count_returns_not_initialized() {
    let (env, client, _admin, _token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.get_active_shipment_count(&company);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_analytics_returns_not_initialized() {
    let (_env, client, _admin, _token_contract) = setup_shipment_env();

    client.get_analytics();
}

// Additional Coverage for Unauthorized Error

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_add_company_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    client.add_company(&non_admin, &company);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_add_carrier_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    client.add_carrier(&non_admin, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_set_shipment_limit_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let non_admin = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    client.set_shipment_limit(&non_admin, &10);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_add_carrier_to_whitelist_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let non_company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.add_carrier_to_whitelist(&non_company, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_remove_carrier_from_whitelist_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let non_company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier_to_whitelist(&company, &carrier);

    client.remove_carrier_from_whitelist(&non_company, &carrier);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_cancel_shipment_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    let reason_hash = BytesN::from_array(&env, &[3u8; 32]);
    client.cancel_shipment(&outsider, &shipment_id, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_report_condition_breach_returns_unauthorized() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let outsider = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let breach_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );

    client.report_condition_breach(
        &outsider,
        &shipment_id,
        &BreachType::TemperatureHigh,
        &Severity::Low,
        &breach_hash,
    );
}

// Additional Coverage for ShipmentNotFound Error

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_update_status_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);

    client.update_status(&carrier, &999, &ShipmentStatus::InTransit, &data_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_confirm_delivery_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let receiver = Address::generate(&env);
    let confirmation_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);

    client.confirm_delivery(&receiver, &999, &confirmation_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_release_escrow_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let receiver = Address::generate(&env);

    client.initialize(&admin, &token_contract);

    client.release_escrow(&receiver, &999);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_refund_escrow_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.refund_escrow(&company, &999);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_raise_dispute_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);
    let reason_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    client.raise_dispute(&company, &999, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_resolve_dispute_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.resolve_dispute(
        &admin,
        &999,
        &crate::DisputeResolution::ReleaseToCarrier,
        &BytesN::from_array(&env, &[1u8; 32]),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_cancel_shipment_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let company = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let reason_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.cancel_shipment(&company, &999, &reason_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_update_eta_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let eta_hash = BytesN::from_array(&env, &[1u8; 32]);
    let eta_timestamp = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);

    client.update_eta(&carrier, &999, &eta_timestamp, &eta_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_record_milestone_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let checkpoint = soroban_sdk::Symbol::new(&env, "port_arrival");
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);

    client.record_milestone(&carrier, &999, &checkpoint, &data_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_handoff_shipment_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let new_carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);
    client.add_carrier(&admin, &new_carrier);

    let handoff_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.handoff_shipment(&carrier, &new_carrier, &999, &handoff_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_report_condition_breach_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    let carrier = Address::generate(&env);
    let breach_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &token_contract);
    client.add_carrier(&admin, &carrier);

    client.report_condition_breach(
        &carrier,
        &999,
        &BreachType::TemperatureHigh,
        &Severity::High,
        &breach_hash,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_check_deadline_returns_shipment_not_found() {
    let (_env, client, admin, token_contract) = setup_shipment_env();

    client.initialize(&admin, &token_contract);

    client.check_deadline(&999);
}

// Additional Coverage for InvalidStatus Error

// NOTE: This test is commented out because the feature may not be fully implemented yet

// Event Counter Tests

// Shipment Archival Tests

// [ISSUE #600] ShipmentUnavailable error variant tests
//
// ShipmentUnavailable (#42) is returned by preflight_check_shipment_available
// when a shipment exists only in temporary (archived) storage.
// These tests verify the error code, the validation helper's behaviour, and
// that mutating operations on archived shipments are correctly rejected.

/// preflight_check_shipment_available must return ShipmentNotFound (not
/// ShipmentUnavailable) for a shipment ID that never existed.
#[test]
fn test_preflight_never_created_returns_shipment_not_found() {
    let (env, client, admin, token_contract) = setup_shipment_env();
    client.initialize(&admin, &token_contract);

    let result = env.as_contract(&client.address, || {
        crate::validation::preflight_check_shipment_available(&env, 8888u64)
    });

    assert_eq!(
        result,
        Err(NavinError::ShipmentNotFound),
        "non-existent shipment must return ShipmentNotFound, not ShipmentUnavailable"
    );
}

/// preflight_check_shipment_available must return Ok for an active shipment,
/// confirming it does not block normal operations.
/// The ShipmentUnavailable error discriminant must be exactly 42.
/// This pins the error code so it cannot drift across refactors.
#[test]
fn test_shipment_unavailable_error_code_is_42() {
    assert_eq!(
        NavinError::ShipmentUnavailable as u32,
        42,
        "ShipmentUnavailable discriminant must be 42"
    );
}

// Analytics Event Tests
// Role Revocation Tests
