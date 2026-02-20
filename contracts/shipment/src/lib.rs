#![no_std]

use soroban_sdk::{contract, contractimpl};

mod errors;
mod events;
mod storage;
mod test;
mod types;

pub use errors::*;
pub use types::*;

#[contract]
pub struct NavinShipment;

#[contractimpl]
impl NavinShipment {}
