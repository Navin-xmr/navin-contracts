use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, String};

use crate::{NavinShipmentNft, NavinShipmentNftClient};

/// Stellar account strkeys used to model plain token holders.
///
/// `Address::generate` always yields *contract* addresses in the Soroban test
/// environment, but only contract recipients have to opt in to receiving
/// NFTs: accounts accept every token. Plain holders are therefore modelled
/// with real account strkeys.
const ACCOUNT_STRKEYS: [&str; 4] = [
    "GAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHV4",
    "GABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABVCX",
    "GABQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABEQO",
    "GACAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAYMQ",
];

/// A plain holder: a Stellar account address.
///
/// `index` picks a distinct account so that a test can use more than one
/// plain holder.
pub fn account_address(env: &Env, index: usize) -> Address {
    Address::from_str(env, ACCOUNT_STRKEYS[index % ACCOUNT_STRKEYS.len()])
}

/// Creates an environment with all authorizations mocked and an admin
/// address to use for privileged calls.
pub fn setup_env() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    (env, admin)
}

/// Registers an initialized NFT contract with a base URI configured.
pub fn setup_nft_contract() -> (Env, NavinShipmentNftClient<'static>, Address) {
    let (env, admin) = setup_env();
    let contract_id = env.register(NavinShipmentNft, ());
    let client = NavinShipmentNftClient::new(&env, &contract_id);

    client.initialize(&admin, &symbol_short!("NavinNFT"), &symbol_short!("NNFT"));
    client.set_base_uri(&admin, &String::from_str(&env, "https://example.com/nft/"));

    (env, client, admin)
}

/// Mints `token_id` to `owner`.
///
/// The collection admin is the signer of every mint, so `admin` is only kept
/// for call-site readability.
pub fn mint_test_nft(
    client: &NavinShipmentNftClient,
    _admin: &Address,
    owner: &Address,
    token_id: u64,
) {
    client.mint(owner, &token_id);
}
