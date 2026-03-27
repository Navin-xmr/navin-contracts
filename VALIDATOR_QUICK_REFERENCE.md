# Validator Quick Reference Guide

## Overview

This guide provides a quick reference for the new validation helpers added to the Navin shipment contract.

## Validators

### 1. Symbol Validators

#### `validate_symbol(env: &Env, symbol: &Symbol) -> Result<(), NavinError>`

Validates a single Symbol for bounded usage.

**When to use**: Before storing any Symbol in metadata, milestones, or events.

**Error**: `InvalidShipmentInput` (code 17)

**Example**:
```rust
let key = Symbol::new(&env, "weight");
validation::validate_symbol(&env, &key)?;
```

---

#### `validate_milestone_symbols(env: &Env, milestones: &Vec<(Symbol, u32)>) -> Result<(), NavinError>`

Validates all milestone symbols and checks for duplicates.

**When to use**: Before storing milestone payment schedules.

**Error**: `InvalidShipmentInput` (code 17)

**Checks**:
- Each symbol is valid (bounded length)
- No duplicate milestone names

**Example**:
```rust
let mut milestones = Vec::new(&env);
milestones.push_back((Symbol::new(&env, "warehouse"), 50_u32));
milestones.push_back((Symbol::new(&env, "delivery"), 50_u32));
validation::validate_milestone_symbols(&env, &milestones)?;
```

---

#### `validate_metadata_symbols(env: &Env, key: &Symbol, value: &Symbol) -> Result<(), NavinError>`

Validates metadata key-value pair symbols.

**When to use**: Before storing metadata in a shipment.

**Error**: `InvalidShipmentInput` (code 17)

**Example**:
```rust
let key = Symbol::new(&env, "priority");
let value = Symbol::new(&env, "high");
validation::validate_metadata_symbols(&env, &key, &value)?;
```

---

### 2. Hash Validators

#### `validate_hash(hash: &BytesN<32>) -> Result<(), NavinError>`

Validates that a hash is not the all-zeros sentinel value.

**When to use**: Before storing any external hash (data_hash, reason_hash, note_hash, evidence_hash).

**Error**: `InvalidHash` (code 6)

**Checks**:
- Hash is not all zeros (0x00...00)

**Example**:
```rust
let data_hash = BytesN::from_array(&env, &[7u8; 32]);
validation::validate_hash(&data_hash)?;
```

---

## Integration Points

### Write Paths with Validators

| Method | Validator | Error |
|--------|-----------|-------|
| `create_shipment()` | `validate_milestone_symbols()` | InvalidShipmentInput |
| `set_shipment_metadata()` | `validate_metadata_symbols()` | InvalidShipmentInput |
| `append_note_hash()` | `validate_hash()` | InvalidHash |
| `add_dispute_evidence_hash()` | `validate_hash()` | InvalidHash |

---

## Common Patterns

### Pattern 1: Creating a Shipment with Milestones

```rust
let mut milestones = Vec::new(&env);
milestones.push_back((Symbol::new(&env, "warehouse"), 30_u32));
milestones.push_back((Symbol::new(&env, "port"), 30_u32));
milestones.push_back((Symbol::new(&env, "final"), 40_u32));

// Validators are called automatically in create_shipment()
let shipment_id = client.create_shipment(
    &company,
    &receiver,
    &carrier,
    &data_hash,
    &milestones,
    &deadline,
)?;
```

### Pattern 2: Setting Metadata

```rust
// Validators are called automatically in set_shipment_metadata()
client.set_shipment_metadata(
    &company,
    &shipment_id,
    &Symbol::new(&env, "weight"),
    &Symbol::new(&env, "kg_100"),
)?;
```

### Pattern 3: Appending Notes

```rust
let note_hash = BytesN::from_array(&env, &[8u8; 32]);

// Validator is called automatically in append_note_hash()
client.append_note_hash(&company, &shipment_id, &note_hash)?;
```

### Pattern 4: Adding Dispute Evidence

```rust
let evidence_hash = BytesN::from_array(&env, &[10u8; 32]);

// Validator is called automatically in add_dispute_evidence_hash()
client.add_dispute_evidence_hash(&company, &shipment_id, &evidence_hash)?;
```

---

## Error Handling

All validators return `Result<(), NavinError>`. Use the `?` operator to propagate errors:

```rust
pub fn my_function(env: &Env, symbol: &Symbol) -> Result<(), NavinError> {
    // Validator will return error if symbol is invalid
    validation::validate_symbol(env, symbol)?;
    
    // Continue with rest of logic
    Ok(())
}
```

---

## Validation Constraints

### Symbol Constraints

- **Max Length**: 40 bytes (XDR-encoded)
- **Min Length**: 1 byte (implicit, enforced by Soroban)
- **Allowed Characters**: Any valid Soroban Symbol characters
- **Uniqueness**: Required for milestone names within a shipment

### Hash Constraints

- **Length**: Exactly 32 bytes (enforced by type system)
- **Value**: Must not be all zeros (0x00...00)
- **Format**: SHA-256 hash of off-chain data

---

## Testing

### Running Validator Tests

```bash
# Run all validator unit tests
cargo test --lib validation

# Run specific validator test
cargo test --lib test_validate_symbol_valid_short_passes

# Run integration tests
cargo test --lib test_create_shipment_with_valid_milestone_symbols
```

### Test Coverage

- **Unit Tests**: 25 tests covering all validators
- **Integration Tests**: 9 tests covering write path integration
- **Edge Cases**: Empty, single, long, duplicates, all-zeros, all-ones

---

## Performance

| Validator | Complexity | Time |
|-----------|-----------|------|
| `validate_symbol()` | O(1) | ~1-2 µs |
| `validate_milestone_symbols()` | O(n²) | ~10-50 µs (n ≤ 10) |
| `validate_metadata_symbols()` | O(1) | ~2-4 µs |
| `validate_hash()` | O(32) | ~1 µs |

All validators are lightweight and suitable for on-chain execution.

---

## Security Notes

1. **Symbol Injection Prevention**: Length bounds prevent oversized symbols
2. **Duplicate Prevention**: Milestone validation prevents ambiguous payment tracking
3. **Zero Hash Prevention**: Rejects sentinel values that could bypass logic
4. **Defense in Depth**: Validation at multiple layers (input, storage, event)

---

## FAQ

**Q: What happens if I pass an invalid symbol?**
A: The function returns `InvalidShipmentInput` error (code 17), and the operation is rejected before any storage.

**Q: Can I have duplicate milestone names?**
A: No, `validate_milestone_symbols()` rejects duplicates to ensure payment tracking clarity.

**Q: What's the maximum symbol length?**
A: 40 bytes (XDR-encoded). This is a conservative bound with safety margin.

**Q: Why are zero hashes rejected?**
A: Zero hashes are commonly used as sentinels for "no data". Rejecting them prevents accidental or malicious use.

**Q: Are validators called automatically?**
A: Yes, all validators are integrated into the write paths and called automatically.

**Q: Can I bypass validators?**
A: No, validators are called before any storage or event emission operations.

---

## Related Documentation

- [VALIDATION_IMPLEMENTATION.md](./VALIDATION_IMPLEMENTATION.md) - Detailed implementation guide
- [contracts/shipment/src/validation.rs](./contracts/shipment/src/validation.rs) - Validator source code
- [contracts/shipment/src/lib.rs](./contracts/shipment/src/lib.rs) - Integration points
