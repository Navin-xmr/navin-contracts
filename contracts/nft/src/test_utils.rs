#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

pub fn setup_env() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    (env, admin)
}
