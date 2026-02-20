# Issue 18 Auto Release Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add an escrow auto-release timer so expired pending deliveries release escrow to the carrier.

**Architecture:** Extend the existing contract with a small delivery-escrow model keyed by `shipment_id`, keeping vault behavior unchanged. Add explicit pending/confirmed/disputed/auto-released states and a permissionless timeout check entrypoint.

**Tech Stack:** Rust, Soroban SDK smart contract, in-contract storage, contract unit tests.

---

## Task 1: Add escrow domain types

### Files (Task 1)

- Modify: `contracts/example-contract/src/types.rs`

### Step 1: Write minimal new types and storage key

- Add `DataKey::Escrow(BytesN<32>)`.
- Add `DeliveryStatus` enum.
- Add `DeliveryEscrow` struct with `carrier`, `receiver`, `amount`, `auto_release_after`, `status`.

### Step 2: Keep compatibility

- Preserve all existing keys/types and transaction types.

## Task 2: Add contract methods and event emission

### Files (Task 2)

- Modify: `contracts/example-contract/src/lib.rs`

### Step 1: Add error variants

- Extend `VaultError` for escrow not found, already exists, and invalid state transitions.

### Step 2: Add delivery lifecycle methods

- `create_delivery(...)` stores escrow timer and holds funds from sender.
- `confirm_delivery(...)` releases escrow to carrier and marks confirmed.
- `dispute_delivery(...)` marks disputed without release.
- `check_auto_release(...)` is permissionless and releases only when expired and pending.

### Step 3: Emit event

- Publish `escrow_auto_released` event when auto-release succeeds.

## Task 3: Add and run required tests

### Files (Task 3)

- Modify: `contracts/example-contract/src/test.rs`

### Step 1: Add required tests

- Timeout release happens.
- Early check does not release.
- Already confirmed does not release.
- Already disputed does not release.

### Step 2: Add one extra safety test

- Repeated checks are idempotent (no double release).

### Step 3: Verify

- Run targeted contract tests then full test suite.

## Task 4: QA pass

### Files (Task 4)

- Modify if needed based on findings.

### Step 1: Verify invariants

- No release before timeout.
- No release after confirm/dispute.
- Single payout only.

### Step 2: Lint and finalize

- Resolve any lints introduced by the new code and ensure CI-style checks pass locally.
