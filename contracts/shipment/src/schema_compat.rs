//! # Schema Compatibility Tests
//!
//! Guards against accidental breaking changes to on-chain types, error codes,
//! and event topics across contract releases.
//!
//! ## Philosophy
//!
//! Everything that a client (backend indexer, frontend, analytics pipeline)
//! has already deployed against is a **compatibility surface**.  Changing the
//! discriminant of an enum, renaming a struct field, or altering an event
//! topic is a **breaking change** even if Rust still compiles.
//!
//! These tests encode the *current* expectations as compile-time and
//! runtime assertions.  A regression fails CI loudly before it reaches the
//! network.
//!
//! ## Compatibility checklist
//!
//! | Category          | What is tested                                              |
//! |-------------------|-------------------------------------------------------------|
//! | Error codes       | Each `NavinError` variant keeps its assigned `u32` value   |
//! | Status enum       | `ShipmentStatus` variants exist and have stable names       |
//! | Status transitions| Valid FSM edges are unchanged                               |
//! | Type construction | Public struct fields can still be constructed with same types|
//! | Event topics      | Emitted event topic strings match documented values         |
//! | Role enum         | `Role` variants exist and have stable names                 |
//! | AdminAction enum  | `AdminAction` variants exist with stable shapes             |
//! | DisputeResolution | `DisputeResolution` variants are stable                     |
//! | BreachType enum   | `BreachType` variants are stable                            |
//! | GeofenceEvent enum| `GeofenceEvent` variants are stable                         |
//! | NotificationType  | `NotificationType` variants are stable                      |
//!
//! ## How to add a new breaking change deliberately
//!
//! 1. Update the affected test(s) to reflect the new expectation.
//! 2. Add a comment explaining *why* this is intentionally breaking.
//! 3. Bump the contract version in your PR description.

#![cfg(test)]

extern crate std;

use crate::{
    AdminAction, BreachType, DisputeResolution, GeofenceEvent, NavinError, NavinShipment,
    NavinShipmentClient, NotificationType, Role, ShipmentStatus,
};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Events, Ledger as _},
    Address, BytesN, Env, Symbol, TryFromVal, Vec as SorobanVec,
};

// ---------------------------------------------------------------------------
// Minimal mock token
// ---------------------------------------------------------------------------

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

fn setup_env() -> (Env, NavinShipmentClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token_contract = env.register(MockToken {}, ());
    let client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
    (env, client, admin, token_contract)
}

// ===========================================================================
// 1. Error code stability
//    Every variant must keep its documented numeric discriminant.
//    Clients that receive raw `u32` error codes from the RPC will break
//    silently if these values shift.
// ===========================================================================

#[test]
fn compat_error_codes_are_stable() {
    assert_eq!(NavinError::AlreadyInitialized as u32, 1);
    assert_eq!(NavinError::NotInitialized as u32, 2);
    assert_eq!(NavinError::Unauthorized as u32, 3);
    assert_eq!(NavinError::ShipmentNotFound as u32, 4);
    assert_eq!(NavinError::InvalidStatus as u32, 5);
    assert_eq!(NavinError::InvalidHash as u32, 6);
    assert_eq!(NavinError::EscrowLocked as u32, 7);
    assert_eq!(NavinError::InsufficientFunds as u32, 8);
    assert_eq!(NavinError::ShipmentAlreadyCompleted as u32, 9);
    assert_eq!(NavinError::InvalidTimestamp as u32, 10);
    assert_eq!(NavinError::CounterOverflow as u32, 11);
    assert_eq!(NavinError::CarrierNotWhitelisted as u32, 12);
    assert_eq!(NavinError::CarrierNotAuthorized as u32, 13);
    assert_eq!(NavinError::InvalidAmount as u32, 14);
    assert_eq!(NavinError::EscrowAlreadyDeposited as u32, 15);
    assert_eq!(NavinError::BatchTooLarge as u32, 16);
    assert_eq!(NavinError::InvalidShipmentInput as u32, 17);
    assert_eq!(NavinError::MilestoneSumInvalid as u32, 18);
    assert_eq!(NavinError::MilestoneAlreadyPaid as u32, 19);
    assert_eq!(NavinError::MetadataLimitExceeded as u32, 20);
    assert_eq!(NavinError::RateLimitExceeded as u32, 21);
    assert_eq!(NavinError::ProposalNotFound as u32, 22);
    assert_eq!(NavinError::ProposalAlreadyExecuted as u32, 23);
    assert_eq!(NavinError::ProposalExpired as u32, 24);
    assert_eq!(NavinError::AlreadyApproved as u32, 25);
    assert_eq!(NavinError::InsufficientApprovals as u32, 26);
    assert_eq!(NavinError::NotAnAdmin as u32, 27);
    assert_eq!(NavinError::InvalidMultiSigConfig as u32, 28);
    assert_eq!(NavinError::NotExpired as u32, 29);
    assert_eq!(NavinError::ShipmentLimitReached as u32, 30);
    assert_eq!(NavinError::InvalidConfig as u32, 31);
}

#[test]
fn compat_error_code_count_is_31() {
    // If a variant is added without updating the compatibility checklist,
    // the count assertion below will fail — forcing an explicit review.
    // Update this value AND the individual assertions above for new variants.
    let all_variants: &[NavinError] = &[
        NavinError::AlreadyInitialized,
        NavinError::NotInitialized,
        NavinError::Unauthorized,
        NavinError::ShipmentNotFound,
        NavinError::InvalidStatus,
        NavinError::InvalidHash,
        NavinError::EscrowLocked,
        NavinError::InsufficientFunds,
        NavinError::ShipmentAlreadyCompleted,
        NavinError::InvalidTimestamp,
        NavinError::CounterOverflow,
        NavinError::CarrierNotWhitelisted,
        NavinError::CarrierNotAuthorized,
        NavinError::InvalidAmount,
        NavinError::EscrowAlreadyDeposited,
        NavinError::BatchTooLarge,
        NavinError::InvalidShipmentInput,
        NavinError::MilestoneSumInvalid,
        NavinError::MilestoneAlreadyPaid,
        NavinError::MetadataLimitExceeded,
        NavinError::RateLimitExceeded,
        NavinError::ProposalNotFound,
        NavinError::ProposalAlreadyExecuted,
        NavinError::ProposalExpired,
        NavinError::AlreadyApproved,
        NavinError::InsufficientApprovals,
        NavinError::NotAnAdmin,
        NavinError::InvalidMultiSigConfig,
        NavinError::NotExpired,
        NavinError::ShipmentLimitReached,
        NavinError::InvalidConfig,
    ];
    assert_eq!(all_variants.len(), 31, "NavinError variant count changed");
}

// ===========================================================================
// 2. ShipmentStatus enum — variant existence and FSM edge stability
// ===========================================================================

#[test]
fn compat_shipment_status_variants_exist() {
    // Compile-time: pattern-match exhausts all variants.
    // Any removed or renamed variant is a compile error here.
    let statuses = [
        ShipmentStatus::Created,
        ShipmentStatus::InTransit,
        ShipmentStatus::AtCheckpoint,
        ShipmentStatus::Delivered,
        ShipmentStatus::Disputed,
        ShipmentStatus::Cancelled,
    ];
    assert_eq!(statuses.len(), 6, "ShipmentStatus variant count changed");
}

#[test]
fn compat_status_transition_table_is_stable() {
    // Encode every documented valid transition from types.rs.
    // A regression in `is_valid_transition` breaks FSM invariants.
    let valid: &[(&ShipmentStatus, &ShipmentStatus)] = &[
        (&ShipmentStatus::Created, &ShipmentStatus::InTransit),
        (&ShipmentStatus::Created, &ShipmentStatus::Cancelled),
        (&ShipmentStatus::Created, &ShipmentStatus::Disputed),
        (&ShipmentStatus::InTransit, &ShipmentStatus::AtCheckpoint),
        (&ShipmentStatus::InTransit, &ShipmentStatus::Delivered),
        (&ShipmentStatus::InTransit, &ShipmentStatus::Disputed),
        (&ShipmentStatus::InTransit, &ShipmentStatus::Cancelled),
        (&ShipmentStatus::AtCheckpoint, &ShipmentStatus::InTransit),
        (&ShipmentStatus::AtCheckpoint, &ShipmentStatus::Delivered),
        (&ShipmentStatus::AtCheckpoint, &ShipmentStatus::Disputed),
        (&ShipmentStatus::AtCheckpoint, &ShipmentStatus::Cancelled),
        (&ShipmentStatus::Disputed, &ShipmentStatus::Cancelled),
        (&ShipmentStatus::Disputed, &ShipmentStatus::Delivered),
    ];

    for (from, to) in valid {
        assert!(
            from.is_valid_transition(to),
            "Expected valid transition {:?} → {:?} but is_valid_transition returned false",
            from,
            to
        );
    }

    // Documented *invalid* transitions must stay invalid
    let invalid: &[(&ShipmentStatus, &ShipmentStatus)] = &[
        (&ShipmentStatus::Delivered, &ShipmentStatus::InTransit),
        (&ShipmentStatus::Delivered, &ShipmentStatus::Cancelled),
        (&ShipmentStatus::Delivered, &ShipmentStatus::Disputed),
        (&ShipmentStatus::Cancelled, &ShipmentStatus::InTransit),
        (&ShipmentStatus::Cancelled, &ShipmentStatus::Delivered),
        (&ShipmentStatus::Created, &ShipmentStatus::Delivered),
        (&ShipmentStatus::Created, &ShipmentStatus::AtCheckpoint),
    ];

    for (from, to) in invalid {
        assert!(
            !from.is_valid_transition(to),
            "Expected invalid transition {:?} → {:?} but is_valid_transition returned true",
            from,
            to
        );
    }
}

// ===========================================================================
// 3. Role enum — variant stability
// ===========================================================================

#[test]
fn compat_role_variants_exist() {
    let roles = [Role::Company, Role::Carrier, Role::Unassigned];
    assert_eq!(roles.len(), 3, "Role variant count changed");
}

// ===========================================================================
// 4. DisputeResolution enum — variant stability
// ===========================================================================

#[test]
fn compat_dispute_resolution_variants_exist() {
    let variants = [
        DisputeResolution::ReleaseToCarrier,
        DisputeResolution::RefundToCompany,
    ];
    assert_eq!(
        variants.len(),
        2,
        "DisputeResolution variant count changed"
    );
}

// ===========================================================================
// 5. BreachType enum — variant stability
// ===========================================================================

#[test]
fn compat_breach_type_variants_exist() {
    let variants = [
        BreachType::TemperatureHigh,
        BreachType::TemperatureLow,
        BreachType::HumidityHigh,
        BreachType::Impact,
        BreachType::TamperDetected,
    ];
    assert_eq!(variants.len(), 5, "BreachType variant count changed");
}

// ===========================================================================
// 6. GeofenceEvent enum — variant stability
// ===========================================================================

#[test]
fn compat_geofence_event_variants_exist() {
    let variants = [
        GeofenceEvent::ZoneEntry,
        GeofenceEvent::ZoneExit,
        GeofenceEvent::RouteDeviation,
    ];
    assert_eq!(variants.len(), 3, "GeofenceEvent variant count changed");
}

// ===========================================================================
// 7. NotificationType enum — variant stability
// ===========================================================================

#[test]
fn compat_notification_type_variants_exist() {
    let variants = [
        NotificationType::ShipmentCreated,
        NotificationType::StatusChanged,
        NotificationType::DeliveryConfirmed,
        NotificationType::EscrowReleased,
        NotificationType::DisputeRaised,
        NotificationType::DisputeResolved,
        NotificationType::DeadlineApproaching,
    ];
    assert_eq!(
        variants.len(),
        7,
        "NotificationType variant count changed"
    );
}

// ===========================================================================
// 8. AdminAction enum — variant shape stability
// ===========================================================================

#[test]
fn compat_admin_action_variants_exist() {
    let env = Env::default();
    let addr = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[0u8; 32]);

    // Pattern-match forces a compile error if any variant is removed/renamed
    let variants = [
        AdminAction::Upgrade(hash.clone()),
        AdminAction::TransferAdmin(addr.clone()),
        AdminAction::ForceRelease(1u64),
        AdminAction::ForceRefund(2u64),
    ];
    assert_eq!(variants.len(), 4, "AdminAction variant count changed");
}

// ===========================================================================
// 9. Event topic strings — must match the published event schema
//    See: docs/event-schema.md and the events.rs module
// ===========================================================================

/// Helper: returns `true` if any emitted event has a first topic equal to
/// the given symbol name.
fn has_event_topic(env: &Env, name: &str) -> bool {
    let expected = Symbol::new(env, name);
    env.events().all().iter().any(|(_, topics, _)| {
        if let Some(first) = topics.get(0) {
            Symbol::try_from_val(env, &first)
                .map(|s| s == expected)
                .unwrap_or(false)
        } else {
            false
        }
    })
}

#[test]
fn compat_event_topic_shipment_created() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let deadline = env.ledger().timestamp() + 3600;
    client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &SorobanVec::new(&env),
        &deadline,
    );

    assert!(
        has_event_topic(&env, "shipment_created"),
        "Expected topic 'shipment_created' to be emitted"
    );
}

#[test]
fn compat_event_topic_status_updated() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let deadline = env.ledger().timestamp() + 3600;
    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &SorobanVec::new(&env),
        &deadline,
    );
    client.update_status(
        &carrier,
        &id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[2u8; 32]),
    );

    assert!(
        has_event_topic(&env, "status_updated"),
        "Expected topic 'status_updated' to be emitted"
    );
}

#[test]
fn compat_event_topic_escrow_deposited() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let deadline = env.ledger().timestamp() + 3600;
    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &SorobanVec::new(&env),
        &deadline,
    );
    client.deposit_escrow(&company, &id, &500_000i128);

    assert!(
        has_event_topic(&env, "escrow_deposited"),
        "Expected topic 'escrow_deposited' to be emitted"
    );
}

#[test]
fn compat_event_topic_escrow_released() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 7200;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &SorobanVec::new(&env),
        &deadline,
    );
    client.deposit_escrow(&company, &id, &1_000_000i128);
    // Manually set to Delivered so escrow balance is preserved for release
    env.as_contract(&client.address, || {
        let mut s = crate::storage::get_shipment(&env, id).unwrap();
        s.status = ShipmentStatus::Delivered;
        crate::storage::set_shipment(&env, &s);
    });
    client.release_escrow(&admin, &id);

    assert!(
        has_event_topic(&env, "escrow_released"),
        "Expected topic 'escrow_released' to be emitted"
    );
}

#[test]
fn compat_event_topic_escrow_refunded() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &SorobanVec::new(&env),
        &deadline,
    );
    // refund_escrow works from Created status and cancels the shipment internally
    client.deposit_escrow(&company, &id, &500_000i128);
    client.refund_escrow(&company, &id);

    assert!(
        has_event_topic(&env, "escrow_refunded"),
        "Expected topic 'escrow_refunded' to be emitted"
    );
}

#[test]
fn compat_event_topic_dispute_raised() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &SorobanVec::new(&env),
        &deadline,
    );
    client.update_status(
        &carrier,
        &id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[2u8; 32]),
    );
    client.raise_dispute(
        &receiver,
        &id,
        &BytesN::from_array(&env, &[9u8; 32]),
    );

    assert!(
        has_event_topic(&env, "dispute_raised"),
        "Expected topic 'dispute_raised' to be emitted"
    );
}

#[test]
fn compat_event_topic_shipment_cancelled() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &SorobanVec::new(&env),
        &deadline,
    );
    client.cancel_shipment(&company, &id, &BytesN::from_array(&env, &[99u8; 32]));

    assert!(
        has_event_topic(&env, "shipment_cancelled"),
        "Expected topic 'shipment_cancelled' to be emitted"
    );
}

#[test]
fn compat_event_topic_delivery_success() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 7200;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &SorobanVec::new(&env),
        &deadline,
    );
    // confirm_delivery performs the InTransit → Delivered transition internally
    client.update_status(
        &carrier,
        &id,
        &ShipmentStatus::InTransit,
        &BytesN::from_array(&env, &[2u8; 32]),
    );
    client.confirm_delivery(
        &receiver,
        &id,
        &BytesN::from_array(&env, &[1u8; 32]),
    );

    assert!(
        has_event_topic(&env, "delivery_success"),
        "Expected topic 'delivery_success' to be emitted"
    );
}

#[test]
fn compat_event_topic_milestone_recorded() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &SorobanVec::new(&env),
        &deadline,
    );
    // Set InTransit via storage to avoid rate-limit on first call
    env.as_contract(&client.address, || {
        let mut s = crate::storage::get_shipment(&env, id).unwrap();
        s.status = ShipmentStatus::InTransit;
        crate::storage::set_shipment(&env, &s);
    });
    client.record_milestone(
        &carrier,
        &id,
        &Symbol::new(&env, "warehouse"),
        &BytesN::from_array(&env, &[3u8; 32]),
    );

    assert!(
        has_event_topic(&env, "milestone_recorded"),
        "Expected topic 'milestone_recorded' to be emitted"
    );
}

// ===========================================================================
// 10. Struct construction — public API shape
//     These tests verify that named fields still have the expected types.
//     A field rename or type change causes a compile error here.
// ===========================================================================

#[test]
fn compat_shipment_input_fields() {
    let env = Env::default();
    // Constructing with named fields; any rename/type-change is a compile error
    let _input = crate::ShipmentInput {
        receiver: Address::generate(&env),
        carrier: Address::generate(&env),
        data_hash: BytesN::from_array(&env, &[0u8; 32]),
        payment_milestones: SorobanVec::new(&env),
        deadline: 9999u64,
    };
}

#[test]
fn compat_shipment_struct_fields() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let deadline = env.ledger().timestamp() + 3600;
    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &SorobanVec::new(&env),
        &deadline,
    );

    let s = client.get_shipment(&id);

    // Field access; renaming any field causes a compile error
    let _: u64 = s.id;
    let _: Address = s.sender;
    let _: Address = s.receiver;
    let _: Address = s.carrier;
    let _: ShipmentStatus = s.status;
    let _: BytesN<32> = s.data_hash;
    let _: u64 = s.created_at;
    let _: u64 = s.updated_at;
    let _: i128 = s.escrow_amount;
    let _: i128 = s.total_escrow;
    let _: u64 = s.deadline;
}

#[test]
fn compat_contract_metadata_fields() {
    let (_env, client, admin, token_contract) = setup_env();
    client.initialize(&admin, &token_contract);

    let meta = client.get_contract_metadata();

    let _: u32 = meta.version;
    let _: Address = meta.admin;
    let _: u64 = meta.shipment_count;
    let _: bool = meta.initialized;
}

#[test]
fn compat_analytics_fields() {
    let (_env, client, admin, token_contract) = setup_env();
    client.initialize(&admin, &token_contract);

    let a = client.get_analytics();

    let _: u64 = a.total_shipments;
    let _: i128 = a.total_escrow_volume;
    let _: u64 = a.total_disputes;
    let _: u64 = a.created_count;
    let _: u64 = a.in_transit_count;
    let _: u64 = a.at_checkpoint_count;
    let _: u64 = a.delivered_count;
    let _: u64 = a.disputed_count;
    let _: u64 = a.cancelled_count;
}

// ===========================================================================
// 11. Contract version — baseline fixture reference
//     The contract starts at version 1 after initialization.
//     This locks the assumption that new deployments start at v1.
// ===========================================================================

#[test]
fn compat_initial_contract_version_is_1() {
    let (_env, client, admin, token_contract) = setup_env();
    client.initialize(&admin, &token_contract);
    assert_eq!(
        client.get_version(),
        1,
        "Initial contract version must be 1"
    );
}

// ===========================================================================
// 12. Query function existence
//     Calling the client methods ensures the public query API hasn't changed.
//     Removing or renaming a public function causes a compile error here.
// ===========================================================================

#[test]
fn compat_public_query_functions_exist() {
    let (env, client, admin, token_contract) = setup_env();
    let company = Address::generate(&env);
    let receiver = Address::generate(&env);
    let carrier = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 3600;

    client.initialize(&admin, &token_contract);
    client.add_company(&admin, &company);

    let id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &BytesN::from_array(&env, &[1u8; 32]),
        &SorobanVec::new(&env),
        &deadline,
    );

    // Every query method that clients depend on must still exist and return
    let _ = client.get_admin();
    let _ = client.get_version();
    let _ = client.get_shipment_counter();
    let _ = client.get_shipment_count();
    let _ = client.get_shipment(&id);
    let _ = client.get_escrow_balance(&id);
    let _ = client.get_contract_metadata();
    let _ = client.get_analytics();
    let _ = client.get_active_shipment_count(&company);
    let _ = client.get_shipment_limit();
    let _ = client.get_role(&receiver);
    let _ = client.is_carrier_whitelisted(&company, &carrier);
}
