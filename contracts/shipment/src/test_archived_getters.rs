//! Regression tests for issue #1288: read-only getters must keep working after
//! a shipment has been archived (moved from persistent to temporary storage).

#![cfg(test)]

use super::test_helpers::*;

#[test]
fn test_all_getters_read_archived_shipment() {
    let (env, client, admin, _token) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let proof_hash = BytesN::from_array(&env, &[2u8; 32]);
    let deadline = env.ledger().timestamp() + 86_400;

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    client.cancel_shipment(&company, &shipment_id, &proof_hash);

    let before = client.get_shipment(&shipment_id);
    let escrow_balance = client.get_escrow_balance(&shipment_id);
    let freeze_reason = client.get_escrow_freeze_reason(&shipment_id);
    let event_count = client.get_event_count(&shipment_id);
    let reference = client.get_shipment_reference(&shipment_id);

    client.archive_shipment(&admin, &shipment_id);

    // Explicit archived accessor and the normal getter must agree.
    assert_eq!(client.get_archived_shipment(&shipment_id), before);
    assert_eq!(client.get_shipment(&shipment_id), before);

    // Every per-field getter sees the same archived record.
    assert_eq!(client.get_shipment_creator(&shipment_id), company);
    assert_eq!(client.get_shipment_sender(&shipment_id), company);
    assert_eq!(client.get_shipment_receiver(&shipment_id), receiver);
    assert_eq!(client.get_shipment_carrier(&shipment_id), carrier);
    assert_eq!(
        client.get_shipment_status(&shipment_id),
        ShipmentStatus::Cancelled
    );
    assert_eq!(
        client.get_shipment_created_at(&shipment_id),
        before.created_at
    );
    assert_eq!(
        client.get_shipment_updated_at(&shipment_id),
        before.updated_at
    );
    assert_eq!(
        client.get_integration_nonce(&shipment_id),
        before.integration_nonce
    );

    // Other read-only views keyed by shipment ID behave the same way.
    assert_eq!(client.get_escrow_balance(&shipment_id), escrow_balance);
    assert_eq!(client.get_escrow_freeze_reason(&shipment_id), freeze_reason);
    assert_eq!(client.get_event_count(&shipment_id), event_count);
    assert_eq!(client.get_shipment_reference(&shipment_id), reference);
    assert!(client.get_status_hash_history(&shipment_id).is_empty());
    assert!(!client.verify_delivery_proof(&shipment_id, &proof_hash));

    let ids = soroban_sdk::Vec::from_array(&env, [shipment_id]);
    let batch = client.get_shipments_batch(&ids);
    assert_eq!(batch.len(), 1);
    assert_eq!(batch.get(0), Some(Some(before)));
}
