//! # NFT Event Regression Tests
//!
//! Asserts the exact topics and payloads published by every state-mutating
//! NFT entry point so that refactors of the event emission logic cannot
//! silently corrupt the data consumed by indexers and block explorers.
//!
//! Every event is expected to carry `(EVENT_NAME, EVENT_SCHEMA_VERSION)` as
//! its topic tuple (see [`crate::event_topics`]).

extern crate std;

use crate::event_topics::{self, EVENT_SCHEMA_VERSION};
use crate::{NavinShipmentNft, NavinShipmentNftClient};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    vec, Address, Env, IntoVal, Symbol, Val, Vec,
};

const TIMESTAMP: u64 = 1_700_000_000;

fn setup() -> (Env, Address, NavinShipmentNftClient<'static>) {
    let env = Env::default();
    env.ledger().set_timestamp(TIMESTAMP);
    let admin = Address::generate(&env);
    let contract_id = env.register(NavinShipmentNft, ());
    let client = NavinShipmentNftClient::new(&env, &contract_id);
    env.mock_all_auths();
    client.initialize(&admin, &symbol_short!("NavinNFT"), &symbol_short!("NNFT"));
    (env, admin, client)
}

fn topics(env: &Env, name: Symbol) -> Vec<Val> {
    (name, EVENT_SCHEMA_VERSION).into_val(env)
}

/// Returns the most recent event published by `contract`.
fn last_event(env: &Env, contract: &Address) -> (Address, Vec<Val>, Val) {
    let mut last = None;
    for event in env.events().all().iter() {
        if &event.0 == contract {
            last = Some(event);
        }
    }
    last.expect("expected the NFT contract to publish an event")
}

/// Counts events published by `contract` whose topics match `name`.
fn count_events(env: &Env, contract: &Address, name: Symbol) -> u32 {
    let expected = topics(env, name);
    let mut count = 0;
    for event in env.events().all().iter() {
        if &event.0 == contract && event.1 == expected {
            count += 1;
        }
    }
    count
}

#[test]
fn test_schema_version_constants_match() {
    let env = Env::default();
    assert_eq!(
        EVENT_SCHEMA_VERSION,
        Symbol::new(&env, event_topics::EVENT_SCHEMA_VERSION_STR)
    );
}

#[test]
fn test_initialize_emits_versioned_event() {
    let (env, admin, client) = setup();
    let event = last_event(&env, &client.address);

    assert_eq!(
        vec![&env, event],
        vec![
            &env,
            (
                client.address.clone(),
                topics(&env, event_topics::INIT),
                (
                    admin,
                    symbol_short!("NavinNFT"),
                    symbol_short!("NNFT"),
                    TIMESTAMP
                )
                    .into_val(&env),
            ),
        ]
    );
}

#[test]
fn test_mint_emits_schema_versioned_topics() {
    let (env, _admin, client) = setup();
    let recipient = Address::generate(&env);
    client.mint(&recipient, &7);

    let event = last_event(&env, &client.address);
    let expected: Vec<Val> = vec![
        &env,
        symbol_short!("mint").into_val(&env),
        symbol_short!("v1").into_val(&env),
    ];
    assert_eq!(event.1, expected);
}

#[test]
fn test_mint_emits_event() {
    let (env, _admin, client) = setup();
    let recipient = Address::generate(&env);
    client.mint(&recipient, &42);

    let event = last_event(&env, &client.address);
    assert_eq!(
        vec![&env, event],
        vec![
            &env,
            (
                client.address.clone(),
                topics(&env, event_topics::MINT),
                (42_u64, recipient, TIMESTAMP).into_val(&env),
            ),
        ]
    );
    assert_eq!(count_events(&env, &client.address, event_topics::MINT), 1);
}

#[test]
fn test_transfer_emits_event() {
    let (env, _admin, client) = setup();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint(&alice, &1);
    client.transfer(&alice, &bob, &1);

    let event = last_event(&env, &client.address);
    assert_eq!(
        vec![&env, event],
        vec![
            &env,
            (
                client.address.clone(),
                topics(&env, event_topics::TRANSFER),
                (1_u64, alice, bob, TIMESTAMP).into_val(&env),
            ),
        ]
    );
    assert_eq!(
        count_events(&env, &client.address, event_topics::TRANSFER),
        1
    );
}

#[test]
fn test_burn_emits_event() {
    let (env, _admin, client) = setup();
    let owner = Address::generate(&env);
    client.mint(&owner, &5);
    client.approve_burn(&5);
    client.burn(&owner, &5);

    let event = last_event(&env, &client.address);
    assert_eq!(
        vec![&env, event],
        vec![
            &env,
            (
                client.address.clone(),
                topics(&env, event_topics::BURN),
                (5_u64, owner, TIMESTAMP).into_val(&env),
            ),
        ]
    );
    assert_eq!(count_events(&env, &client.address, event_topics::BURN), 1);
}

#[test]
fn test_burn_by_admin_emits_event_with_admin_as_caller() {
    let (env, admin, client) = setup();
    let owner = Address::generate(&env);
    client.mint(&owner, &9);
    client.burn(&admin, &9);

    let event = last_event(&env, &client.address);
    assert_eq!(
        vec![&env, event],
        vec![
            &env,
            (
                client.address.clone(),
                topics(&env, event_topics::BURN),
                (9_u64, admin, TIMESTAMP).into_val(&env),
            ),
        ]
    );
}

#[test]
fn test_self_transfer_emits_noop_event() {
    let (env, _admin, client) = setup();
    let alice = Address::generate(&env);
    client.mint(&alice, &1);
    client.transfer(&alice, &alice, &1);

    // Self-transfer is a no-op and, matching token contract semantics,
    // must NOT publish a transfer event.
    assert_eq!(
        count_events(&env, &client.address, event_topics::TRANSFER),
        0
    );
    assert_eq!(client.owner_of(&1), alice);
    assert_eq!(client.balance_of(&alice), 1);
}

#[test]
fn test_transfer_admin_emits_event() {
    let (env, admin, client) = setup();
    let new_admin = Address::generate(&env);
    client.transfer_admin(&admin, &new_admin);

    let event = last_event(&env, &client.address);
    assert_eq!(
        vec![&env, event],
        vec![
            &env,
            (
                client.address.clone(),
                topics(&env, event_topics::ADMIN_TRANSFER),
                (admin, new_admin, TIMESTAMP).into_val(&env),
            ),
        ]
    );
}

#[test]
fn test_failed_mint_emits_no_event() {
    let (env, _admin, client) = setup();
    let recipient = Address::generate(&env);
    client.mint(&recipient, &1);
    let before = count_events(&env, &client.address, event_topics::MINT);

    let result = client.try_mint(&recipient, &1);
    assert!(result.is_err());

    // A rejected mint must not add a mint event on top of what was recorded.
    let after = count_events(&env, &client.address, event_topics::MINT);
    assert!(after <= before);
    assert_eq!(client.total_supply(), 1);
}
