#![no_std]

mod errors;
mod events;
mod storage;
mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl};

#[contract]
pub struct NavinShipment;

#[contractimpl]
impl NavinShipment {}
