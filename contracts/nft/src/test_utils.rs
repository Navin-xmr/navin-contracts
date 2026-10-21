use soroban_sdk::{testutils::Address as _, Address, Env, String};

use crate::{NavinNft, NavinNftClient};

pub fn setup_nft_contract() -> (Env, NavinNftClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(NavinNft, ());
    let client = NavinNftClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.set_base_uri(&admin, &String::from_str(&env, "https://example.com/nft/"));

    (env, client, admin)
}

pub fn mint_test_nft(client: &NavinNftClient, admin: &Address, owner: &Address, token_id: u64) {
    client.mint(admin, owner, &token_id, &None);
}
