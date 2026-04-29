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

## Upgrade Checklist for New Keys

1. Add the new key variant in `DataKey` with docs.
2. Update this registry (group + rationale).
3. Add/adjust tests for serialization/storage stability where applicable.
4. Update `scripts/release-check.sh` docs checks if new public APIs/errors are introduced.
