# Storage Layout — NavinShipment Contract

> **Audit Date:** 2026-02-26  
> **Contract:** `NavinShipment`  
> **File Audited:** `contracts/shipment/src/storage.rs`

---

## Overview

Stellar Soroban provides three storage tiers with distinct cost and durability profiles:

| Tier | Durability | Cost | Best For |
|------|------------|------|----------|
| **Instance** | Tied to contract lifetime | Cheapest per-read | Global config, counters, roles |
| **Persistent** | Survives indefinitely with rent | Medium, requires TTL extension | Per-shipment data that must not expire |
| **Temporary** | Short TTL, auto-deleted | Cheapest per-write | Ephemeral or post-lifecycle data |

---

## Full Storage Key Layout

### Instance Storage Keys

| Key | Type | Tier (Current) | Tier (Recommended) | Rationale |
|-----|------|---------------|-------------------|-----------|
| `Admin` | `Address` | Instance | ✅ Instance | Single global config value, lives with contract |
| `ProposedAdmin` | `Address` | Instance | ✅ Instance | Short-lived config; cleared after acceptance |
| `Version` | `u32` | Instance | ✅ Instance | Global contract metadata |
| `ShipmentCount` | `u64` | Instance | ✅ Instance | Global counter, always needed |
| `TokenContract` | `Address` | Instance | ✅ Instance | Immutable config after init |
| `AdminList` | `Vec<Address>` | Instance | ✅ Instance | Global multisig config |
| `MultiSigThreshold` | `u32` | Instance | ✅ Instance | Global multisig config |
| `ProposalCounter` | `u64` | Instance | ✅ Instance | Global counter |
| `ShipmentLimit` | `u32` | Instance | ✅ Instance | Global config value |
| `CarrierWhitelist(company, carrier)` | `bool` | Instance | ⚠️ **Persistent** | See note 1 |
| `UserRole(address, role)` | `bool` | Instance | ⚠️ **Persistent** | See note 2 |
| `Role(address)` | `Role` | Instance | ⚠️ **Persistent** | See note 2 |
| `TotalEscrowVolume` | `i128` | Instance | ✅ Instance | Global analytics counter |
| `TotalDisputes` | `u64` | Instance | ✅ Instance | Global analytics counter |
| `StatusCount(status)` | `u64` | Instance | ✅ Instance | Global analytics counter |
| `ActiveShipmentCount(company)` | `u32` | Instance | ⚠️ **Persistent** | See note 3 |

### Persistent Storage Keys

| Key | Type | Tier (Current) | Tier (Recommended) | Rationale |
|-----|------|---------------|-------------------|-----------|
| `Shipment(id)` | `Shipment` | Persistent | ✅ Persistent | Core shipment data must survive TTL |
| `Escrow(id)` | `i128` | Persistent | ✅ Persistent | Financial data, must not expire mid-shipment |
| `ConfirmationHash(id)` | `BytesN<32>` | Persistent | ✅ Persistent | Proof of delivery, must be permanently auditable |
| `LastStatusUpdate(id)` | `u64` | Persistent | ⚠️ **Temporary** | See note 4 |
| `Proposal(id)` | `Proposal` | Persistent | ✅ Persistent | Governance proposals need durability |
| `EventCount(id)` | `u32` | Persistent | ⚠️ **Temporary** | See note 5 |

### Temporary Storage Keys

| Key | Type | Tier (Current) | Tier (Recommended) | Rationale |
|-----|------|---------------|-------------------|-----------|
| `ArchivedShipment(id)` | `Shipment` | Temporary | ✅ Temporary | Post-lifecycle data, cheap ephemeral access |

---

## Issues Found & Recommendations

### ⚠️ Note 1 — `CarrierWhitelist` should move to Persistent

**Current:** `instance()`  
**Recommended:** `persistent()`  
**Reason:** Instance storage is a single ledger entry — every key stored there enlarges the one instance entry blob. With many company/carrier pairs, this inflates the instance entry size significantly, increasing the base cost of every single contract call (since instance storage is loaded on every invocation). Whitelist entries are per-relationship data, not global config.  
**Gas impact:** High. A contract with 100 carrier/company pairs stores all 100 booleans in the instance blob, paid on every call.

---

### ⚠️ Note 2 — `UserRole` and `Role` should move to Persistent

**Current:** `instance()`  
**Recommended:** `persistent()`  
**Reason:** Same issue as `CarrierWhitelist`. Role assignments are per-address data. As the number of companies and carriers grows, the instance storage blob grows with them, making every contract call more expensive. Role data is accessed only when the specific address is calling — it should live in persistent storage keyed by address.  
**Gas impact:** High. Each registered company and carrier adds two entries to the instance blob.

---

### ⚠️ Note 3 — `ActiveShipmentCount` should move to Persistent

**Current:** `instance()`  
**Recommended:** `persistent()`  
**Reason:** This is per-company data, not global config. Storing one counter per company in the instance blob has the same scaling problem as roles and whitelists — instance storage grows with user count and is loaded on every invocation.  
**Gas impact:** Medium. One entry per company.

---

### ⚠️ Note 4 — `LastStatusUpdate` should move to Temporary

**Current:** `persistent()`  
**Recommended:** `temporary()`  
**Reason:** `LastStatusUpdate` is only used to enforce the rate-limiting interval between status updates. Once a shipment is delivered or cancelled, this value is never read again. It only needs to survive as long as the shipment is active (days to weeks), not permanently. Temporary storage is significantly cheaper and auto-cleans itself.  
**Gas impact:** Medium. Eliminates ongoing rent payments for a value that has no post-lifecycle utility.

---

### ⚠️ Note 5 — `EventCount` should move to Temporary

**Current:** `persistent()`  
**Recommended:** `temporary()`  
**Reason:** The event count is an operational counter used during a shipment's active lifecycle. Once a shipment is archived or completed, the event count has no on-chain utility — event history is maintained off-chain by the indexer. Temporary storage is sufficient and avoids permanent rent on this counter.  
**Gas impact:** Low-medium. One persistent entry per shipment eliminated.

---

### 🐛 Bug Found — Double Set in `set_confirmation_hash`

```rust
pub fn set_confirmation_hash(env: &Env, shipment_id: u64, hash: &BytesN<32>) {
    let key = DataKey::ConfirmationHash(shipment_id);
    env.storage().persistent().set(&key, hash);
    env.storage().persistent().set(&key, hash); // ← redundant duplicate
}
```

The key is set twice identically. The second call is a wasted operation. Remove the duplicate line.

---

## Recommended Migration Summary

| Key | Change | Estimated Saving |
|-----|--------|-----------------|
| `CarrierWhitelist(company, carrier)` | Instance → Persistent | High — removes N entries from instance blob |
| `UserRole(address, role)` | Instance → Persistent | High — removes 2×N entries from instance blob |
| `Role(address)` | Instance → Persistent | High — removes N entries from instance blob |
| `ActiveShipmentCount(company)` | Instance → Persistent | Medium — removes N entries from instance blob |
| `LastStatusUpdate(id)` | Persistent → Temporary | Medium — eliminates rent per shipment |
| `EventCount(id)` | Persistent → Temporary | Low-medium — eliminates rent per shipment |
| Duplicate `.set()` in `set_confirmation_hash` | Remove duplicate line | Minor — one wasted storage write per delivery |

---

## TTL Extension Coverage

The `extend_shipment_ttl` function currently extends TTL for:
- `Shipment(id)` ✅
- `Escrow(id)` ✅  
- `ConfirmationHash(id)` ✅

After migration, it should **also** cover:
- `LastStatusUpdate(id)` — if kept in persistent (but recommended to move to temporary)
- `EventCount(id)` — if kept in persistent (but recommended to move to temporary)

After moving `LastStatusUpdate` and `EventCount` to temporary, no changes to `extend_shipment_ttl` are needed.

---

## Keys That Are Correctly Classified (No Change Needed)

| Key | Tier | Reason |
|-----|------|--------|
| `Admin` | Instance | Single global value, needed on every call |
| `Version` | Instance | Single global value |
| `ShipmentCount` | Instance | Global counter, needed frequently |
| `TokenContract` | Instance | Single global config |
| `AdminList` | Instance | Small fixed list, global config |
| `MultiSigThreshold` | Instance | Single global config value |
| `ProposalCounter` | Instance | Global counter |
| `ShipmentLimit` | Instance | Single global config value |
| `TotalEscrowVolume` | Instance | Global analytics, small |
| `TotalDisputes` | Instance | Global analytics, small |
| `StatusCount(status)` | Instance | Small fixed enum set (5 variants max) |
| `Shipment(id)` | Persistent | Core data, must survive long-term |
| `Escrow(id)` | Persistent | Financial data, must not expire |
| `ConfirmationHash(id)` | Persistent | Delivery proof, permanent audit trail |
| `Proposal(id)` | Persistent | Governance data needs durability |
| `ArchivedShipment(id)` | Temporary | Post-lifecycle, ephemeral is correct |

---

## Implementation Notes

When migrating `CarrierWhitelist`, `UserRole`, `Role`, and `ActiveShipmentCount` from instance to persistent storage, ensure:

1. TTL extension is called for these keys when a shipment is created or updated for that company/carrier, otherwise they may expire during a long-inactive period.
2. Existing data in instance storage will not be automatically migrated — a one-time migration function or re-registration of all roles will be needed on upgrade.
3. All 271 existing tests must be re-run after any migration to confirm no regression.