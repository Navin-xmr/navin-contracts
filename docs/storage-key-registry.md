# Storage Key Registry (Versioned)

This document is the registry of every storage key type and variant in the
shipment contract (`contracts/shipment/src`): `DataKey` plus the overflow key
enums that exist because `DataKey` is at the `#[contracttype]` case limit.

This is the single source of truth for the full list of storage keys.
For whether each key is on the *right* storage tier and why, see
[`docs/storage.md`](storage.md) — that document does not repeat this key list.
The tier column below records the tier the current code actually uses.

`test_storage_key_registry.rs` reads the key enums from the source and fails
if any variant is missing from this document.

## Registry Version

- **Version:** `v2`
- **Last Updated:** `2026-11-24`
- **Source of Truth:** `contracts/shipment/src/types.rs` (plus the private
  `SettlementProofKey` in `contracts/shipment/src/lib.rs`)

## How Soroban Encodes Keys

A `#[contracttype]` enum key is not stored by numeric discriminant. It is
encoded as an `ScVal::Vec` whose first element is the variant **name** as a
`Symbol`, followed by the variant's payload values. For example
`DataKey::Shipment(7)` is stored as `[Symbol("Shipment"), U64(7)]`.

So:

- **Renaming** a variant changes its on-chain key: existing entries become
  unreachable under the new name. This breaks storage.
- **Changing the payload type** of a variant (e.g. `u64` → `u32`, or adding a
  field) changes the encoded key as well. This also breaks storage.
- **Reordering** variants, or inserting a new one between existing ones, does
  not change any encoded key. This is harmless.
- Variant names in different key enums do not collide by type: the enum type
  itself is not part of the key, so two enums with an identically named
  variant and payload (e.g. `A::Log(u64)` and `B::Log(u64)`) would address the
  **same** entry. New variants must not reuse a name + payload shape already
  used by another key enum in the same storage tier.

## Compatibility Policy

- **Never rename a variant or change its payload type** once a deployed
  contract may have written it. Introduce a new variant and migrate instead.
- Order inside an enum carries no meaning; group variants for readability.
- A variant that is no longer read or written may be removed, but its
  name + payload shape stays retired: it must never be reused for different
  data, because old entries under that key may still exist on-chain.
- Every key addition or removal must update this document (the registry test
  enforces additions) and include an upgrade note when relevant.
- `DataKey` is at the `#[contracttype]` case limit; new keys go in an overflow
  enum (see `DisputeKey`, `SettlementKey`, `StatusHashKey`, `DeliveryProofKey`).

## `DataKey` (`contracts/shipment/src/types.rs`)

### Core Governance / Config

| Key | Tier |
|-----|------|
| `DataKey::Admin` | Instance |
| `DataKey::Version` | Instance |
| `DataKey::TokenContract` | Instance |
| `DataKey::ContractConfig` | Instance |
| `DataKey::ConfigChecksum` | Instance |
| `DataKey::IsPaused` | Instance |
| `DataKey::FeeConfig` | Instance |
| `DataKey::GlobalEventCount` | Instance |
| `DataKey::ReentrancyLock` | Instance |

### Role and Access Control

| Key | Tier |
|-----|------|
| `DataKey::UserRole(Address, Role)` | Persistent |
| `DataKey::RoleSuspended(Address, Role)` | Persistent |
| `DataKey::CompanySuspended(Address)` | Persistent |
| `DataKey::CarrierWhitelist(Address, Address)` | Persistent |

### Shipment and Escrow State

| Key | Tier |
|-----|------|
| `DataKey::Shipment(u64)` | Persistent |
| `DataKey::Escrow(u64)` | Persistent |
| `DataKey::ConfirmationHash(u64)` | Persistent |
| `DataKey::LastStatusUpdate(u64)` | Persistent |
| `DataKey::DeadlineWarningEmitted(u64)` | Persistent |
| `DataKey::EscrowFreezeReasonByShipment(u64)` | Persistent |
| `DataKey::ShipmentDependents(u64)` | Persistent |
| `DataKey::ArchivedShipment(u64)` | Temporary |

### Counters / Analytics / Limits

| Key | Tier |
|-----|------|
| `DataKey::ShipmentCount` | Instance |
| `DataKey::TotalEscrowVolume` | Instance |
| `DataKey::TotalDisputes` | Instance |
| `DataKey::StatusCount(ShipmentStatus)` | Instance |
| `DataKey::ShipmentLimit` | Instance |
| `DataKey::CompanyShipmentLimit(Address)` | Persistent |
| `DataKey::ActiveShipmentCount(Address)` | Persistent |
| `DataKey::EventCount(u64)` | Persistent |
| `DataKey::MilestoneEventCount(u64)` | Persistent |
| `DataKey::BreachEventCount(u64)` | Persistent |

### Governance Proposals

| Key | Tier |
|-----|------|
| `DataKey::ProposedAdmin` | Instance |
| `DataKey::AdminList` | Instance |
| `DataKey::MultiSigThreshold` | Instance |
| `DataKey::ProposalCounter` | Instance |
| `DataKey::Proposal(u64)` | Persistent |
| `DataKey::ProposalDigest(u64)` | Persistent |
| `DataKey::ProposalSalt(BytesN<32>)` | Instance |

### Audit Log

| Key | Tier |
|-----|------|
| `DataKey::AuditEntry(u64)` | Persistent |
| `DataKey::AuditEntryCount` | Persistent |

### Settlement Records

| Key | Tier |
|-----|------|
| `DataKey::SettlementCounter` | Instance |
| `DataKey::Settlement(u64)` | Persistent |
| `DataKey::ActiveSettlement(u64)` | Persistent |

### Idempotency / Circuit Protection / Quotas

| Key | Tier |
|-----|------|
| `DataKey::IdempotencyWindow(BytesN<32>)` | Temporary |
| `DataKey::ActorQuota(Address)` | Persistent |
| `DataKey::CircuitBreakerState` | Persistent |
| `DataKey::CircuitBreakerConfig` | Persistent |
| `DataKey::CompanyCreationQuota(Address)` | Persistent |
| `DataKey::CreationQuotaConfig` | Persistent |

### Reserved (declared, never read or written)

| Key | Notes |
|-----|-------|
| `DataKey::Company(Address)` | Legacy role slot; roles live in `UserRole(Address, Role)`. |
| `DataKey::Carrier(Address)` | Legacy role slot; roles live in `UserRole(Address, Role)`. |
| `DataKey::Treasury` | Treasury lives in `FeeConfig::treasury`. |
| `DataKey::RecoveryRecord(u64, u32)` | Recovery records are not persisted. |
| `DataKey::RecoveryRecordCount(u64)` | Recovery records are not persisted. |

### Retired (removed; name + payload must never be reused)

| Key | Notes |
|-----|-------|
| `NftContract` (unit variant, formerly in `DataKey`) | Never read or written. The settlement NFT address lives in `ContractConfig.settlement_nft_contract_id`. Removed to free a `DataKey` slot. |

## `DisputeKey` (`types.rs`)

| Key | Tier |
|-----|------|
| `DisputeKey::EvidenceCount(u64)` | Persistent |
| `DisputeKey::Evidence(u64, u32)` | Persistent |

## `SettlementKey` (`types.rs`)

| Key | Tier |
|-----|------|
| `SettlementKey::Latest(u64)` | Persistent |
| `SettlementKey::Ids(u64)` | Persistent |

## `StatusHashKey` (`types.rs`)

| Key | Tier |
|-----|------|
| `StatusHashKey::Log(u64)` | Persistent (read falls back to Temporary for shipments mid-migration) |

## `DeliveryProofKey` (`types.rs`)

| Key | Tier |
|-----|------|
| `DeliveryProofKey::PartialLog(u64)` | Persistent |

Append-only log of every `confirm_partial_delivery` hash for a shipment,
bounded by `MAX_PARTIAL_DELIVERY_PROOFS`.

## `SettlementProofKey` (private, `contracts/shipment/src/lib.rs`)

| Key | Tier |
|-----|------|
| `SettlementProofKey::Minted(u64)` | Persistent |

Marks that the settlement proof NFT for a shipment has been minted.

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

1. Add the new variant (to an overflow key enum while `DataKey` is at the case
   limit) with a doc comment.
2. Add it to this registry with its storage tier; `test_storage_key_registry`
   fails until you do.
3. Never rename or change the payload type of an existing variant.
4. Update `scripts/release-check.sh` docs checks if new public APIs/errors are introduced.
5. If adding a commonly used key, consider adding a wrapper helper function in `storage.rs`.
