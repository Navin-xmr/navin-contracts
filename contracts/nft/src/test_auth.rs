//! # NFT Authorization Regression Tests
//!
//! The main test module calls `env.mock_all_auths()` in its setup, which
//! masks whether `require_admin()` is actually enforced. The tests here never
//! use `mock_all_auths()` for the calls under test: each authorization is
//! granted explicitly via `mock_auths`, so removing or weakening an auth
//! check in the contract makes these tests fail.

extern crate std;

use crate::{NavinShipmentNft, NavinShipmentNftClient};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal, Symbol,
};

/// Sets up an initialized NFT contract without enabling `mock_all_auths`.
/// Only the `initialize` call itself is authorized, and only for `admin`.
fn setup_strict() -> (Env, Address, NavinShipmentNftClient<'static>) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(NavinShipmentNft, ());
    let client = NavinShipmentNftClient::new(&env, &contract_id);

    let name = symbol_short!("NavinNFT");
    let symbol = symbol_short!("NNFT");
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (admin.clone(), name.clone(), symbol.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &name, &symbol);

    (env, admin, client)
}

fn authorize_mint(
    env: &Env,
    client: &NavinShipmentNftClient<'static>,
    signer: &Address,
    to: &Address,
    token_id: u64,
) {
    env.mock_auths(&[MockAuth {
        address: signer,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "mint",
            args: (to.clone(), token_id).into_val(env),
            sub_invokes: &[],
        },
    }]);
}

#[test]
fn test_mint_by_non_admin_fails() {
    let (env, _admin, client) = setup_strict();
    let non_admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    // The non-admin signs the mint invocation, but the contract requires the
    // stored admin's signature, so the call must be rejected.
    authorize_mint(&env, &client, &non_admin, &recipient, 1);
    let result = client.try_mint(&recipient, &1);
    assert!(result.is_err(), "non-admin must not be able to mint");

    // No state was mutated by the rejected call.
    assert_eq!(client.total_supply(), 0);
    assert_eq!(client.balance_of(&recipient), 0);
    assert!(client.try_owner_of(&1).is_err());
}

#[test]
fn test_non_admin_minting_to_self_fails() {
    let (env, _admin, client) = setup_strict();
    let attacker = Address::generate(&env);

    authorize_mint(&env, &client, &attacker, &attacker, 99);
    let result = client.try_mint(&attacker, &99);
    assert!(result.is_err(), "non-admin must not be able to self-mint");
    assert_eq!(client.balance_of(&attacker), 0);
}

#[test]
fn test_mint_requires_admin_auth() {
    let (env, admin, client) = setup_strict();
    let recipient = Address::generate(&env);

    // With no authorization at all the call must fail.
    env.mock_auths(&[]);
    let result = client.try_mint(&recipient, &1);
    assert!(result.is_err(), "unauthenticated mint must fail");
    assert_eq!(client.total_supply(), 0);

    // Positive control: the exact same call succeeds once the admin signs it.
    authorize_mint(&env, &client, &admin, &recipient, 1);
    assert_eq!(client.mint(&recipient, &1), 1);
    assert_eq!(client.owner_of(&1), recipient);
    assert_eq!(client.total_supply(), 1);
}

#[test]
fn test_mint_fails_for_former_admin_after_admin_transfer() {
    let (env, admin, client) = setup_strict();
    let new_admin = Address::generate(&env);
    let recipient = Address::generate(&env);

    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "transfer_admin",
            args: (admin.clone(), new_admin.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.transfer_admin(&admin, &new_admin);

    // The former admin's signature is no longer sufficient.
    authorize_mint(&env, &client, &admin, &recipient, 1);
    assert!(client.try_mint(&recipient, &1).is_err());

    // The new admin can mint.
    authorize_mint(&env, &client, &new_admin, &recipient, 1);
    client.mint(&recipient, &1);
    assert_eq!(client.owner_of(&1), recipient);
}

/// Audits `require_admin()`: a successful mint must record exactly one
/// authorization, from the stored admin, for the `mint` invocation.
#[test]
fn test_require_admin_is_enforced_on_mint() {
    let (env, admin, client) = setup_strict();
    let recipient = Address::generate(&env);

    authorize_mint(&env, &client, &admin, &recipient, 3);
    client.mint(&recipient, &3);

    assert_eq!(
        env.auths(),
        std::vec![(
            admin.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    client.address.clone(),
                    Symbol::new(&env, "mint"),
                    (recipient.clone(), 3_u64).into_val(&env),
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
}

#[test]
fn test_transfer_requires_owner_auth() {
    let (env, admin, client) = setup_strict();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    authorize_mint(&env, &client, &admin, &alice, 1);
    client.mint(&alice, &1);

    // Bob signing a transfer of Alice's token must fail.
    env.mock_auths(&[MockAuth {
        address: &bob,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "transfer",
            args: (alice.clone(), bob.clone(), 1_u64).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client.try_transfer(&alice, &bob, &1).is_err());
    assert_eq!(client.owner_of(&1), alice);

    // Alice signing succeeds.
    env.mock_auths(&[MockAuth {
        address: &alice,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "transfer",
            args: (alice.clone(), bob.clone(), 1_u64).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.transfer(&alice, &bob, &1);
    assert_eq!(client.owner_of(&1), bob);
}

#[test]
fn test_burn_requires_caller_auth() {
    let (env, admin, client) = setup_strict();
    let owner = Address::generate(&env);

    authorize_mint(&env, &client, &admin, &owner, 1);
    client.mint(&owner, &1);

    env.mock_auths(&[MockAuth {
        address: &owner,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "approve_burn",
            args: (1_u64,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.approve_burn(&1);

    // Without the owner's signature the burn must fail.
    env.mock_auths(&[]);
    assert!(client.try_burn(&owner, &1).is_err());
    assert_eq!(client.total_supply(), 1);

    env.mock_auths(&[MockAuth {
        address: &owner,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "burn",
            args: (owner.clone(), 1_u64).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.burn(&owner, &1);
    assert_eq!(client.total_supply(), 0);
}

#[test]
fn test_pause_by_non_admin_fails() {
    let (env, _admin, client) = setup_strict();
    let non_admin = Address::generate(&env);

    env.mock_auths(&[MockAuth {
        address: &non_admin,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "pause",
            args: (non_admin.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert_eq!(
        client.try_pause(&non_admin),
        Err(Ok(crate::NftError::NotAdmin))
    );
}
