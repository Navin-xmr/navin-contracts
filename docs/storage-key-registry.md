# Storage Key Registry (Versioned)

This document defines a migration-safe registry for `DataKey` usage in `contracts/shipment/src/types.rs`.

## Registry Version

- **Version:** `v1`
- **Last Updated:** `2026-04-28`
- **Source of Truth:** `contracts/shipment/src/types.rs`

## Policy

- Do not reorder or repurpose existing `DataKey` variants.
- New variants are append-only and must include a clear doc comment.
- Removed keys are marked as deprecated in docs; on-chain discriminants remain reserved.
- Every key addition must update this document and include an upgrade note when relevant.

## Reserved Evolution Ranges

- **`0-199`:** active shipment contract storage keys (current registry space)
- **`200-299`:** reserved for future shipment analytics/index keys
- **`300-399`:** reserved for migration and compatibility shims
- **`400+`:** reserved for emergency/operational extensions

These ranges are policy ranges for planning and review safety; they are not runtime-enforced.

## Active Key Groups

### Core Governance / Config

- `Admin`
- `Version`
- `TokenContract`
- `ContractConfig`
- `ConfigChecksum`
- `IsPaused`

### Role and Access Control

- `Company(Address)`
- `Carrier(Address)`
- `CarrierSuspended(Address)`
- `CompanySuspended(Address)`
- `CarrierWhitelist(Address, Address)`
- `UserRole(Address, Role)`
- `RoleSuspended(Address, Role)`
- `Role(Address)`

### Shipment and Escrow State

- `Shipment(u64)`
- `Escrow(u64)`
- `ConfirmationHash(u64)`
- `LastStatusUpdate(u64)`
- `ArchivedShipment(u64)`
- `EscrowFreezeReasonByShipment(u64)`
- `StatusHash(u64, ShipmentStatus)`
- `ReentrancyLock`

### Counters / Analytics

- `ShipmentCount`
- `TotalEscrowVolume`
- `TotalDisputes`
- `StatusCount(ShipmentStatus)`
- `ShipmentLimit`
- `CompanyShipmentLimit(Address)`
- `ActiveShipmentCount(Address)`
- `EventCount(u64)`
- `MilestoneEventCount(u64)`
- `BreachEventCount(u64)`
- `AuditEntryCount`
- `SettlementCounter`

### Governance Proposals

- `ProposedAdmin`
- `AdminList`
- `MultiSigThreshold`
- `ProposalCounter`
- `Proposal(u64)`

### Append-Only Audit / Evidence

- `ShipmentNote(u64, u32)`
- `ShipmentNoteCount(u64)`
- `DisputeEvidence(u64, u32)`
- `DisputeEvidenceCount(u64)`
- `AuditEntry(u64)`

### Settlement Records

- `Settlement(u64)`
- `ActiveSettlement(u64)`

### Idempotency / Circuit Protection

- `IdempotencyWindow(BytesN<32>)`
- `ActorQuota(Address)`
- `CircuitBreakerState`

## Storage Key Wrapper Helpers

The `storage` module provides convenience wrapper functions to simplify key construction and reduce the chance of errors when working with common keys. These helpers wrap the `DataKey` enum variants and make repeated key assembly easier to read and maintain.

### Available Helpers

#### `shipment_key(shipment_id: u64) -> DataKey`

Constructs a `DataKey::Shipment(shipment_id)` key for accessing shipment data.

```rust
// Instead of: env.storage().persistent().get(&DataKey::Shipment(123))
let key = storage::shipment_key(123);
env.storage().persistent().get(&key);
```

#### `escrow_key(shipment_id: u64) -> DataKey`

Constructs a `DataKey::Escrow(shipment_id)` key for accessing escrow amounts.

```rust
// Instead of: env.storage().persistent().get(&DataKey::Escrow(123))
let key = storage::escrow_key(123);
env.storage().persistent().get(&key);
```

#### `confirmation_hash_key(shipment_id: u64) -> DataKey`

Constructs a `DataKey::ConfirmationHash(shipment_id)` key for accessing confirmation hashes.

```rust
// Instead of: env.storage().persistent().get(&DataKey::ConfirmationHash(123))
let key = storage::confirmation_hash_key(123);
env.storage().persistent().get(&key);
```

### Benefits

- **Consistency**: Single source of truth for key construction patterns
- **Readability**: Clear intent when reading code
- **Maintainability**: Changes to key structure only need to be made in one place
- **Error Reduction**: Less chance of typos or incorrect key construction

## Upgrade Checklist for New Keys

1. Add the new key variant in `DataKey` with docs.
2. Update this registry (group + rationale).
3. Add/adjust tests for serialization/storage stability where applicable.
4. Update `scripts/release-check.sh` docs checks if new public APIs/errors are introduced.
5. If adding a commonly used key, consider adding a wrapper helper function in `storage.rs`.

