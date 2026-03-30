# Soroban Budget Baseline Report

Contract: **shipment**
SDK version: **soroban-sdk 22.0.0**
Captured: **2026-03-30**
Branch: `issue/197-195-budget-benchmark-schema-compat`

---

## How to regenerate this report

```sh
cargo test --package shipment budget_bench -- --nocapture 2>&1 \
  | grep '^\[budget\]' \
  | column -t
```

The test suite writes one `[budget]` line per operation to stdout.
Redirect output and paste the table below.

---

## Baseline figures

> **Note:** Run `cargo test --package shipment budget_bench -- --nocapture`
> and replace the placeholder values below with the actual output on first
> commit of this file.  Values will vary slightly between machines; what
> matters is the *delta* between releases on the same machine.

| Operation | CPU instructions | Memory bytes |
|-----------|-----------------|--------------|
| `initialize` | 88,650 | 9,505 |
| `create_shipment` (single) | 283,674 | 41,465 |
| `create_shipments_batch` (10 items) | 1,715,052 | 224,021 |
| `update_status` (Created → InTransit) | 333,039 | 51,307 |
| `deposit_escrow` | 298,576 | 46,942 |
| `confirm_delivery` (InTransit → Delivered) | 454,009 | 69,924 |
| `release_escrow` | 223,487 | 34,370 |
| `refund_escrow` | 370,226 | 52,106 |
| `raise_dispute` | 349,159 | 52,143 |
| `resolve_dispute` (RefundToCompany) | 396,119 | 58,810 |
| `record_milestone` (single) | 165,317 | 25,686 |
| `cancel_shipment` | 314,479 | 46,016 |
| `handoff_shipment` | 262,004 | 39,462 |

---

## Network limits (Soroban v22)

| Resource | Limit |
|----------|-------|
| CPU instructions per transaction | 100,000,000 |
| Memory bytes per transaction | 41,943,040 (40 MiB) |

All operations must remain below these limits or the transaction is rejected
at the network level.

---

## Interpreting budget deltas

When comparing a new run against this baseline:

| Delta | Action |
|-------|--------|
| ±5 % | Noise — no action needed |
| ±10 % | Review the diff of the hot-path function before merging |
| > +20 % | Must be explained in the PR description; consider optimisation |
| > +50 % | Block the merge; investigate and rewrite the hot path |

### Common causes of CPU regressions

- Adding a `Vec::iter()` loop inside a hot path (O(n) per call)
- Extra storage reads (`env.storage().get()`) per invocation
- New `env.events().publish()` calls (each costs ~10 k instructions)
- Increased `BytesN` or `Symbol` allocations

### Common causes of memory regressions

- Cloning large `Vec` or `Map` values instead of borrowing
- Building intermediate `Vec<T>` just to discard them
- Storing large byte payloads on-chain (use the hash-and-emit pattern)

---

## Re-run cadence

Re-run and commit an updated table:

1. Before cutting a release branch
2. After any change to `lib.rs`, `storage.rs`, or `events.rs`
3. After upgrading the `soroban-sdk` dependency

---

## Schema compatibility

Schema compatibility tests live in
[`contracts/shipment/src/schema_compat.rs`](../contracts/shipment/src/schema_compat.rs).

They guard:
- `NavinError` discriminant values (31 variants, codes 1–31)
- `ShipmentStatus` FSM transition table
- All enum variant sets (`Role`, `BreachType`, `GeofenceEvent`, `DisputeResolution`, `NotificationType`, `AdminAction`)
- Struct field names and types (`Shipment`, `ShipmentInput`, `ContractMetadata`, `Analytics`)
- Event topic strings (`shipment_created`, `status_updated`, `escrow_deposited`, …)
- Public query function existence

Any intentional breaking change must:

1. Update the relevant `compat_*` test to the new expectation.
2. Include a comment explaining why the break is necessary.
3. Bump the contract version in the PR.
