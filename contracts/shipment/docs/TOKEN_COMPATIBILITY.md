# SAC vs Custom Token Compatibility

The `NavinShipment` contract is designed to be compatible with both the Stellar Asset Contract (SAC) and custom token contracts that implement the standard Soroban Token Interface (SEP-41).

## Integration Matrix

The following table summarizes the compatibility across different token implementations:

| Feature | Stellar Asset Contract (SAC) | Custom Token (NavinToken) |
|---------|-----------------------------|---------------------------|
| **Standard** | SEP-41 + Admin Extensions | SEP-41 |
| **Escrow Flow** | Supported | Supported |
| **Milestone Payments**| Supported | Supported |
| **Auth Requirement** | `require_auth` | `require_auth` |
| **Minting (Tests)** | `mint(to, amount)` | `mint(admin, to, amount)` |
| **Transfer Method** | `transfer(from, to, amount)`| `transfer(from, to, amount)`|

## Behavioral Assumptions

### 1. Interface Compliance
All supported tokens must implement the following methods from the SEP-41 standard:
- `transfer(from: Address, to: Address, amount: i128)`
- `balance(id: Address) -> i128`

The shipment contract uses `symbol_short!("transfer")` to invoke movements, ensuring compatibility with any contract exposing that symbol.

### 2. Authentication Flow
- **Deposits**: When a company calls `deposit_escrow`, the shipment contract invokes `transfer(company, contract, amount)`. This requires the company to have authorized the transaction.
- **Releases**: When the contract releases funds to a carrier, it invokes `transfer(contract, carrier, amount)`. Soroban automatically handles the contract's authorization to move its own funds.

### 3. Error Handling
The contract uses `env.try_invoke_contract` when calling token methods. Any failure in the token contract (e.g., insufficient balance, account frozen, internal error) is captured and mapped to `NavinError::TokenTransferFailed` (Code 39). This ensures the shipment contract's state remains consistent and doesn't crash on external failures.

### 4. Atomic Operations
All token interactions are performed within the same transaction as the shipment state updates. If a token transfer fails, the entire transaction (including status changes and event emissions) is rolled back by the Soroban host.

## Integration Testing

The compatibility suite is located in `contracts/shipment/src/test_token_compatibility.rs`. It runs the full lifecycle (Create -> Deposit -> Transit -> Deliver/Refund) against both token variants to ensure parity in behavior.

### Running Tests
To run the compatibility suite:
```bash
cargo test test_token_compatibility
```

*Note: On Windows systems, you may encounter `export ordinal too large` errors due to linker limits when multiple large contracts are included in the same test binary. If this happens, try temporarily removing `cdylib` from the `crate-type` in the token contract's `Cargo.toml`.*
