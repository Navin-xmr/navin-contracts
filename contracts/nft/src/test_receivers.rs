//! # Test-only NFT receiver contracts
//!
//! The NFT contract only transfers a token to a contract recipient that opts
//! in explicitly: the recipient must expose `can_receive_nft(token_id)` and
//! answer `true`. `AcceptingReceiver` implements that interface and, in
//! addition, the optional `on_nft_received(from, token_id, data)` callback
//! that records the forwarded transfer payload so tests can assert it.

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Val, Vec};

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiverKey {
    LastReceivedToken,
    LastReceivedData,
}

#[contract]
pub struct AcceptingReceiver;

#[contractimpl]
impl AcceptingReceiver {
    /// Gate consulted by the NFT contract before transferring to a contract.
    pub fn can_receive_nft(_env: Env, _token_id: u64) -> bool {
        true
    }

    /// Optional callback; records what the NFT contract forwarded.
    pub fn on_nft_received(env: Env, _from: Address, token_id: u64, data: Vec<Val>) {
        env.storage()
            .instance()
            .set(&ReceiverKey::LastReceivedToken, &token_id);
        env.storage()
            .instance()
            .set(&ReceiverKey::LastReceivedData, &data);
    }

    /// The token ID of the last received NFT.
    pub fn last_received_token(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&ReceiverKey::LastReceivedToken)
            .unwrap_or(0)
    }

    /// The payload of the last received NFT.
    pub fn last_received_data(env: Env) -> Vec<Val> {
        env.storage()
            .instance()
            .get(&ReceiverKey::LastReceivedData)
            .unwrap_or_else(|| Vec::new(&env))
    }
}
