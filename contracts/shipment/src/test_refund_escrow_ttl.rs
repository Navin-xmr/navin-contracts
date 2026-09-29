//! Regression tests for #759: refund_escrow must extend shipment TTL once.
//!
//! The refund path previously called `extend_shipment_ttl` twice in a row.
//! Settlement now owns the single remaining extension.

use crate::test::*;
use soroban_sdk::{
    testutils::{storage::Persistent, Address as _, Ledger},
    Address, BytesN,
};

/// Refund still zeros escrow, cancels the shipment, and refreshes TTL.
#[test]
fn test_refund_escrow_extends_ttl_once() {
    let (env, client, admin, _token_contract) = setup_initialized_shipment_env();
    let company = Address::generate(&env);
    let carrier = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.add_company(&admin, &company);
    client.add_carrier(&admin, &carrier);
    client.add_carrier_to_whitelist(&company, &carrier);

    let mut cfg = client.get_contract_config();
    cfg.shipment_ttl_threshold = 518_000;
    client.update_config(&admin, &cfg);

    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deadline = env.ledger().timestamp() + 3600;
    let shipment_id = client.create_shipment(
        &company,
        &receiver,
        &carrier,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
        &deadline,
    );
    let escrow_amount: i128 = 3000;
    client.deposit_escrow(&company, &shipment_id, &escrow_amount);

    env.ledger().with_mut(|l| {
        l.sequence_number += 1_000;
        l.timestamp += 61;
    });

    client.refund_escrow(&company, &shipment_id);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.escrow_amount, 0);
    assert_eq!(shipment.status, crate::ShipmentStatus::Cancelled);

    env.as_contract(&client.address, || {
        let key = crate::types::DataKey::Shipment(shipment_id);
        let ttl = env.storage().persistent().get_ttl(&key);
        assert!(
            ttl >= 518_400,
            "refund_escrow must preserve the remaining TTL extension"
        );
    });
}

/// Source-level lock: refund_escrow delegates TTL to settle_escrow exactly once.
#[test]
fn test_refund_escrow_invokes_extend_shipment_ttl_once() {
    let src = include_str!("lib.rs");
    let refund_body = rust_fn_body(src, "pub fn refund_escrow");
    let settle_body = rust_fn_body(src, "fn settle_escrow");

    let refund_direct_ttl = refund_body.matches("extend_shipment_ttl(").count();
    let settle_ttl = settle_body.matches("extend_shipment_ttl(").count();
    let refund_settle = refund_body.matches("settle_escrow(").count();

    assert_eq!(
        refund_settle, 1,
        "refund_escrow must settle through settle_escrow"
    );
    assert_eq!(
        refund_direct_ttl, 0,
        "refund_escrow must not call extend_shipment_ttl directly"
    );
    assert_eq!(
        settle_ttl, 1,
        "settle_escrow must call extend_shipment_ttl exactly once"
    );
}

fn rust_fn_body<'a>(src: &'a str, signature: &str) -> &'a str {
    let start = src
        .find(signature)
        .unwrap_or_else(|| panic!("{signature} not found in lib.rs"));
    let after_sig = &src[start..];
    let brace = after_sig
        .find('{')
        .unwrap_or_else(|| panic!("{signature} has no opening brace"));
    let body_start = start + brace;
    let bytes = src.as_bytes();
    let mut depth = 0i32;
    for (offset, &b) in bytes[body_start..].iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[body_start..=body_start + offset];
                }
            }
            _ => {}
        }
    }
    panic!("{signature} is unclosed");
}
