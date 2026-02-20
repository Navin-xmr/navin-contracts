#![cfg(test)]

extern crate std;

use crate::{NavinShipment, NavinShipmentClient};
use soroban_sdk::Env;

#[test]
fn test_scaffold() {
    let env = Env::default();
    let _client = NavinShipmentClient::new(&env, &env.register(NavinShipment, ()));
}
