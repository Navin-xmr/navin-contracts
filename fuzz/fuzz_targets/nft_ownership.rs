#![no_main]

use libfuzzer_sys::fuzz_target;
use navin_nft::{test_utils::account_address, NavinShipmentNft, NavinShipmentNftClient, NftKey};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Vec};

/// Small bounded state space: eight token ids and three plain account holders.
/// Account addresses are used instead of `Address::generate` (which yields
/// contract addresses) so transfers are not subject to the receiver opt-in.
const TOKEN_IDS: u64 = 8;
const HOLDERS: usize = 3;
const MAX_OPS: usize = 32;

/// Check the ownership invariants that must hold after *every* operation:
/// total supply equals the number of ids in owners' lists, each owner's
/// balance matches its list length, and every listed id reports that owner.
fn assert_ownership_invariants(client: &NavinShipmentNftClient<'_>, holders: &[Address]) {
    let total_supply = client.total_supply();
    let mut owned = 0_u64;

    for holder in holders {
        let tokens = client.tokens_of_owner(holder);
        assert_eq!(
            client.balance_of(holder),
            tokens.len() as u64,
            "balance_of must equal the length of tokens_of_owner"
        );
        for token_id in tokens.iter() {
            assert_eq!(
                client.owner_of(&token_id),
                holder.clone(),
                "owner_of must agree with tokens_of_owner"
            );
        }
        owned += tokens.len() as u64;
    }

    assert_eq!(
        total_supply, owned,
        "total_supply must equal the total number of owned ids"
    );
}

/// Approvals must not survive a transfer or a burn. `get_approved` errors for
/// burned ids (their owner record is gone), so inspect the approval pointer
/// directly for those.
fn assert_no_approval(env: &Env, client: &NavinShipmentNftClient<'_>, token_id: u64) {
    let has_approval = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .has(&NftKey::ApprovedOperator(token_id))
    });
    assert!(
        !has_approval,
        "approval for token {token_id} must be cleared on transfer/burn"
    );
}

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(
        NavinShipmentNft,
        (
            admin.clone(),
            symbol_short!("NavinNFT"),
            symbol_short!("NNFT"),
        ),
    );
    let client = NavinShipmentNftClient::new(&env, &contract_id);

    let holders = [
        account_address(&env, 0),
        account_address(&env, 1),
        account_address(&env, 2),
    ];
    // Only the admin (or the current owner) may burn; other callers are
    // legitimate fuzz inputs whose calls simply fail.
    let callers = [
        admin,
        holders[0].clone(),
        holders[1].clone(),
        holders[2].clone(),
    ];

    for chunk in data.chunks(4).take(MAX_OPS) {
        let op = chunk[0] % 8;
        let a = chunk.get(1).copied().unwrap_or(0) as usize;
        let b = chunk.get(2).copied().unwrap_or(0) as usize;
        let c = chunk.get(3).copied().unwrap_or(0) as usize;

        let token_id = (b as u64) % TOKEN_IDS;
        let from = &holders[a % HOLDERS];
        let to = &holders[c % HOLDERS];
        let operator = &holders[(a + c) % HOLDERS];
        let caller = &callers[(a + b + c) % callers.len()];

        match op {
            0 => {
                // A burned id is unminted again, so a re-mint is a valid
                // success here; duplicate mints of a live id simply fail.
                let _ = client.try_mint(to, &token_id);
            }
            1 => {
                let mut recipients = Vec::new(&env);
                let mut token_ids = Vec::new(&env);
                for k in 0..3_u64 {
                    recipients.push_back(holders[(a + k as usize) % HOLDERS].clone());
                    token_ids.push_back((token_id + k) % TOKEN_IDS);
                }
                let _ = client.try_mint_batch(&recipients, &token_ids);
            }
            2 => {
                if matches!(
                    client.try_transfer(from, to, &token_id, &Vec::new(&env)),
                    Ok(Ok(()))
                ) && from != to
                {
                    // A self-transfer is an intentional no-op that leaves
                    // approvals untouched.
                    assert_no_approval(&env, &client, token_id);
                }
            }
            3 => {
                if matches!(
                    client.try_transfer_from(operator, from, to, &token_id),
                    Ok(Ok(()))
                ) && from != to
                {
                    assert_no_approval(&env, &client, token_id);
                }
            }
            4 => {
                // `None` means no expiration; the ledger never advances here.
                let _ = client.try_approve(operator, &token_id, &None);
            }
            5 => {
                let _ = client.try_revoke_approval(operator, &token_id);
            }
            6 => {
                if matches!(client.try_burn(caller, &token_id), Ok(Ok(()))) {
                    assert_no_approval(&env, &client, token_id);
                }
            }
            7 => {
                let mut token_ids = Vec::new(&env);
                for k in 0..3_u64 {
                    token_ids.push_back((token_id + k) % TOKEN_IDS);
                }
                if matches!(client.try_burn_batch(caller, &token_ids), Ok(Ok(()))) {
                    for id in token_ids.iter() {
                        assert_no_approval(&env, &client, id);
                    }
                }
            }
            _ => unreachable!(),
        }

        assert_ownership_invariants(&client, &holders);
    }
});
