# Checkpoint Symbol Fixture Helper

## Overview

The `checkpoint_symbol` helper function provides a deterministic, reusable way to construct Symbol instances for milestone and status checkpoint tests. This helper eliminates duplicated symbol construction code and ensures consistent naming patterns across the test suite.

## Location

The helper is defined in `src/test_utils.rs`:

```rust
pub fn checkpoint_symbol(env: &Env, name: &str) -> Symbol
```

## Purpose

- **Deterministic**: Same input always produces the same Symbol output
- **Reusable**: Single source of truth for checkpoint/milestone symbol creation
- **Self-documenting**: Makes test intent clear through descriptive names
- **Validated**: Enforces Stellar Symbol constraints (1-12 chars, alphanumeric + underscore)

## Naming Patterns

The helper supports common checkpoint and milestone naming conventions:

### Descriptive Names
Use meaningful names that describe the checkpoint location or stage:
- `"warehouse"` - Initial pickup or storage location
- `"port"` - Port of departure or arrival
- `"customs"` - Customs clearance checkpoint
- `"final"` - Final delivery destination

### Sequential Names
Use numbered identifiers for generic milestones:
- `"M1"`, `"M2"`, `"M3"` - Milestone 1, 2, 3
- `"checkpoint1"`, `"checkpoint2"` - Checkpoint 1, 2

### Short Codes
Use concise identifiers for common stages:
- `"pickup"` - Pickup from origin
- `"transit"` - In-transit status
- `"delivery"` - Delivery to destination

## Usage Examples

### Creating Milestone Schedules

```rust
// Descriptive milestone schedule
let mut milestones = Vec::new(&env);
milestones.push_back((checkpoint_symbol(&env, "warehouse"), 30));
milestones.push_back((checkpoint_symbol(&env, "port"), 30));
milestones.push_back((checkpoint_symbol(&env, "final"), 40));

let shipment_id = client.create_shipment(
    &company,
    &receiver,
    &carrier,
    &data_hash,
    &milestones,
    &deadline,
    &None,
);
```

## Constraints

The helper enforces Stellar Symbol constraints:

- **Length**: 1-12 characters (enforced by Stellar protocol)
- **Format**: Alphanumeric and underscore only (A-Z, a-z, 0-9, _)
- **Invalid**: Spaces, hyphens, special characters, unicode, null bytes

## Benefits

1. **Reduced Duplication**: Single helper replaces dozens of `Symbol::new(&env, "...")` calls
2. **Consistent Naming**: Encourages standardized checkpoint names across tests
3. **Self-Documenting**: Function name makes intent clear
4. **Easier Refactoring**: Changes to symbol construction logic only need to be made in one place
5. **Type Safety**: Compiler ensures correct usage through type system

## Testing

Run tests with:

```bash
cargo test checkpoint_symbol --lib
```
