# Governance Token and Snapshot Voting Implementation Plan

> **For Claude:** Implement governance token integration (#213) and proposal snapshot voting (#212) in the shipment contract.

**Goal:** Add token-weighted voting for proposals with snapshot at creation time, delegation, minimum token threshold, and vote locking. PR must close #212 and #213 with no agent trailers or watermarks.

**Architecture:** Extend `ContractConfig` with governance token settings; add `Proposal.snapshot_ledger` and snapshot-based power; add storage for token balance snapshots, delegations, and vote locks; keep existing multi-sig proposal flow but gate creation/voting by token power and use snapshot for approval checks.

**Tech Stack:** Rust, Soroban SDK, existing shipment contract (lib.rs, types.rs, config.rs, storage.rs, test.rs).

**File paths (this repo):** `contracts/shipment/src/{lib.rs, types.rs, config.rs, storage.rs, test.rs}`.

---

## Task 1: Types and config (governance token + snapshot)

**Files:** `contracts/shipment/src/types.rs`, `contracts/shipment/src/config.rs`

- Add to `ContractConfig`: `governance_token: Option<Address>`, `min_proposal_tokens: i128`, `vote_lock_ledgers: u32`.
- Add to `Proposal`: `snapshot_ledger: u32` (voting power determined at this ledger).
- Add `DataKey` variants: `VoteLock(Address, u64)`, `Delegation(Address)`, `SnapshotBalance(u64, Address)` (proposal_id, address -> balance at snapshot).
- Add types: `VotePower` or use `i128` for snapshot balance; ensure `Proposal` is backward-compatible (default snapshot_ledger to 0 if needed for existing tests).

---

## Task 2: Storage helpers

**Files:** `contracts/shipment/src/storage.rs`

- Implement get/set for snapshot balance per (proposal_id, address).
- Implement get/set for delegation (delegatee -> delegator).
- Implement get/set for vote lock (address, proposal_id) -> lock until ledger.
- Add helpers: `get_voting_power_at_ledger(env, proposal_id, address)` using snapshot first, then fallback to current token balance if no snapshot (for backward compat during rollout).

---

## Task 3: Lib – token config, snapshot at creation, vote logic

**Files:** `contracts/shipment/src/lib.rs`

- In `initialize` / config update: persist governance token and min_proposal_tokens, vote_lock_ledgers.
- In `propose_action`: require proposer’s token balance (or delegated balance) >= min_proposal_tokens; capture current ledger as `proposal.snapshot_ledger`; snapshot voting power for proposer (and optionally all admins) into `SnapshotBalance(proposal_id, address)`.
- When counting approvals: use snapshot-based voting power (sum of snapshot balances of approvers) and require total snapshot power >= threshold (e.g. threshold can be a fixed quorum or existing multisig count). Alternatively: keep “N of M admins” but require each approver to have had at least X tokens at snapshot; document choice.
- Add token lock on vote: when an admin approves, lock their tokens until `current_ledger + vote_lock_ledgers` (store in `VoteLock(address, proposal_id)` or by proposal expiry).
- Delegation: add `set_delegation(env, delegator, delegatee)` so delegatee’s voting power = delegator’s balance; store in `Delegation(delegatee)` -> delegator; when reading balance for snapshot/voting, use delegated balance if configured.
- Add `get_proposal_snapshot_ledger(env, proposal_id)` and snapshot verification / historical query helpers as needed.

---

## Task 4: Tests

**Files:** `contracts/shipment/src/test.rs`

- Governance token: config set with token address and min_proposal_tokens; proposal creation fails if below threshold; proposal creation succeeds when above; token-weighted approval check (or min-token check per approver).
- Snapshot: create proposal, capture snapshot_ledger; transfer tokens away from approver; approval still uses snapshot power so vote counts.
- Vote lock: after approve, tokens are locked until lock expiry; withdraw/transfer restricted or lock enforced in contract.
- Delegation: set delegation, create proposal; delegatee’s voting power equals delegator’s balance at snapshot.
- Snapshot verification and historical query tests.
- All existing proposal tests still pass (backward compat).

---

## Acceptance (from issues)

- #213: Token configuration, balance checking, weighted calculation, token delegation, minimum threshold, vote locking, tests pass.
- #212: snapshot_ledger field, power snapshot at creation, snapshot-based checking, storage optimization, verification, historical queries, tests pass.

---

## PR / Commit

- One branch: `feature/governance-token-and-snapshot-voting`.
- Single clean commit (or minimal commits) with message that references closing issues, e.g. “Implement governance token integration and snapshot voting. Fixes #212. Fixes #213.” — no Cursor/agent trailers or watermarks.
- Push to fork and open PR; ensure PR description closes #212 and #213 (Fixes #212, Fixes #213).
