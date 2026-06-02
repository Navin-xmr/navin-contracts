# Client Examples

Short, practical snippets for common shipment contract calls. Copy the call shape and adapt it to your needs.

## Table of Contents

1. [Setup](#setup)
2. [Initialization](#initialization)
3. [Shipment Management](#shipment-management)
4. [Status Updates](#status-updates)
5. [Escrow Operations](#escrow-operations)
6. [Role Management](#role-management)
7. [Queries](#queries)

## Setup

```rust
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Symbol, Vec};
use shipment::{NavinShipment, NavinShipmentClient};

let env = Env::default();
let contract_id = env.register(NavinShipment, ());
let client = NavinShipmentClient::new(&env, &contract_id);

let admin = Address::generate(&env);
let company = Address::generate(&env);
let carrier = Address::generate(&env);
let receiver = Address::generate(&env);
```

## Initialization

### Initialize the contract

Set the admin and token contract address:

```rust
let token_contract = Address::generate(&env);
client.initialize(&admin, &token_contract);
```

## Shipment Management

### Create a shipment

```rust
let data_hash = BytesN::from_array(&env, &[1u8; 32]);
let deadline = env.ledger().timestamp() + 3600; // 1 hour from now

let shipment_id = client.create_shipment(
    &company,              // sender (Company)
    &receiver,             // recipient
    &carrier,              // assigned carrier
    &data_hash,            // SHA-256 hash of shipment data
    &Vec::new(&env),       // payment milestones (empty for no milestones)
    &deadline,             // deadline timestamp
);
```

### Get shipment details

```rust
let shipment = client.get_shipment(&shipment_id);
println!("Status: {:?}", shipment.status);
println!("Escrow: {}", shipment.escrow_amount);
```

### Cancel a shipment

Only the sender (Company) can cancel if the shipment is still in Created state:

```rust
let data_hash = BytesN::from_array(&env, &[1u8; 32]);
client.cancel_shipment(&company, &shipment_id, &data_hash);
```

## Status Updates

### Update shipment status

Move a shipment through its lifecycle (Created → InTransit → Delivered):

```rust
let data_hash = BytesN::from_array(&env, &[1u8; 32]);

// Mark as in transit (carrier updates status)
client.update_status(
    &carrier,
    &shipment_id,
    &ShipmentStatus::InTransit,
    &data_hash,
);

// Confirm delivery (receiver confirms arrival)
client.confirm_delivery(&receiver, &shipment_id, &data_hash);
```

## Escrow Operations

### Deposit escrow

The company deposits funds before shipment begins:

```rust
let amount = 1000i128; // Must be > 0
client.deposit_escrow(&company, &shipment_id, &amount);
```

### Release escrow

After delivery, the receiver (or admin) releases funds to the carrier:

```rust
client.release_escrow(&receiver, &shipment_id);
```

### Refund escrow

If the shipment is cancelled, the company can get funds back:

```rust
client.refund_escrow(&company, &shipment_id);
```

## Role Management

### Add a company

Admin grants Company role to an address:

```rust
client.add_company(&admin, &company);
```

### Add a carrier

Admin grants Carrier role to an address:

```rust
client.add_carrier(&admin, &carrier);
```

### Remove a company

Admin revokes Company role:

```rust
client.remove_company(&admin, &company);
```

### Suspend a carrier

Admin temporarily suspends a carrier (escrow frozen):

```rust
client.suspend_carrier(&admin, &carrier);
```

### Reactivate a carrier

Admin restores a suspended carrier:

```rust
client.reactivate_carrier(&admin, &carrier);
```

## Queries

### Get shipment count

Total number of shipments created:

```rust
let count = client.get_shipment_counter().unwrap();
println!("Total shipments: {}", count);
```

### Get contract status

Retrieve overall contract health and configuration:

```rust
let status = client.get_contract_status().unwrap();
println!("Paused: {}", status.is_paused);
println!("Total shipments: {}", status.shipment_count);
```

### Get active shipment count for company

How many active shipments a company has:

```rust
let active = client.get_active_shipment_count(&company).unwrap();
println!("Active shipments: {}", active);
```

## Error Handling

All contract calls that return `Result<T, NavinError>` can fail. Handle errors gracefully:

```rust
match client.try_deposit_escrow(&company, &shipment_id, &0) {
    Ok(()) => println!("Deposited"),
    Err(e) => println!("Deposit failed: {:?}", e),
}
```

Common errors:
- `InvalidAmount`: Zero or negative amount
- `ShipmentNotFound`: Shipment ID doesn't exist
- `InvalidStatus`: Operation not allowed in current status
- `Unauthorized`: Caller lacks permission
- `EscrowLocked`: Escrow already has funds
