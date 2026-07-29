# Add Tests for ProposalExpired and ProposalAlreadyExecuted Error Variants

## Summary

Adds comprehensive test coverage for two multi-signature proposal error variants
in `contracts/shipment/src/test.rs`:

- **`ProposalExpired` (error code 24)** — resolves issue #607
- **`ProposalAlreadyExecuted` (error code 23)** — resolves issue #608

All new tests use the `try_*` method variants for explicit `Err(Ok(...))` error
assertions, following the project's established testing patterns.

---

## Changes

**File modified:** `contracts/shipment/src/test.rs`

A shared helper function and 11 new `#[test]` functions were appended at the
end of the file.

### Helper: `setup_multisig_with_pending_proposal`

Returns a fully-configured multisig environment with a `TransferAdmin` proposal
that has **not yet expired** and has **not yet reached the approval threshold**
(threshold is 3, only the proposer has approved). This is shared across most
expiry tests to eliminate boilerplate.

---

## Issue #607 — ProposalExpired (error code 24)

### Tasks covered

| Task | Test |
|---|---|
| Attempt to approve expired proposal | `test_approve_expired_proposal_returns_proposal_expired` |
| Attempt to execute expired proposal | `test_execute_expired_proposal_returns_proposal_expired` |
| Verify `ProposalExpired` error (code 24) | `test_proposal_expired_error_code_is_24` |
| Non-expired proposals work normally | `test_non_expired_proposal_can_be_approved` |
| Non-expired proposals auto-execute at threshold | `test_non_expired_proposal_executes_at_threshold` |
| All future approvals blocked after expiry | `test_all_approvals_fail_after_expiry` |

### Test descriptions

- **`test_approve_expired_proposal_returns_proposal_expired`**  
  Advances the ledger 7 days + 1 second past proposal creation using
  `test_utils::advance_past_multisig_expiry`, then calls `try_approve_action`.
  Asserts `Err(Ok(NavinError::ProposalExpired))`.

- **`test_execute_expired_proposal_returns_proposal_expired`**  
  Adds a second approval (keeping the proposal below threshold), advances past
  expiry, then calls `try_execute_proposal`. Asserts `ProposalExpired`.

- **`test_proposal_expired_error_code_is_24`**  
  Pins `NavinError::ProposalExpired as u32 == 24`. Prevents silent discriminant
  drift across refactors.

- **`test_non_expired_proposal_can_be_approved`**  
  Calls `try_approve_action` with no time advance. Asserts `Ok(())` and confirms
  the approval count increments to 2 without triggering auto-execution (threshold
  is 3).

- **`test_non_expired_proposal_executes_at_threshold`**  
  Uses a threshold-2 setup. Asserts `try_approve_action` returns `Ok(())` and
  that `get_admin()` returns the new admin, confirming the action was applied.

- **`test_all_approvals_fail_after_expiry`**  
  One admin approves before expiry, then the ledger advances. A third admin
  (fresh, never approved) calls `try_approve_action`. Asserts `ProposalExpired`
  — expiry blocks all approvers, not just those who previously approved.

---

## Issue #608 — ProposalAlreadyExecuted (error code 23)

### Tasks covered

| Task | Test |
|---|---|
| Verify `ProposalAlreadyExecuted` error (code 23) | `test_proposal_already_executed_error_code_is_23` |
| Execute proposal twice | `test_execute_already_executed_proposal_returns_error` |
| Approve already-executed proposal | `test_approve_already_executed_proposal_returns_error` |
| Single execution succeeds | `test_single_execution_succeeds_and_applies_action` |
| Single execution for `ForceRelease`; duplicate rejected | `test_force_release_single_execution_and_duplicate_rejected` |

### Test descriptions

- **`test_proposal_already_executed_error_code_is_23`**  
  Pins `NavinError::ProposalAlreadyExecuted as u32 == 23`.

- **`test_execute_already_executed_proposal_returns_error`**  
  Uses a 2-of-2 multisig with a `TransferAdmin` action. After `approve_action`
  auto-executes the proposal, `try_execute_proposal` is called again. Asserts
  `Err(Ok(NavinError::ProposalAlreadyExecuted))`.

- **`test_approve_already_executed_proposal_returns_error`**  
  Uses a 3-admin / threshold-2 setup. After the proposal auto-executes on the
  second approval, a third admin calls `try_approve_action`. Asserts
  `ProposalAlreadyExecuted` — guarding both `execute_proposal` and
  `approve_action` paths.

- **`test_single_execution_succeeds_and_applies_action`**  
  Verifies the complete happy path: proposal created → not yet executed →
  threshold reached → `executed` flag set → admin actually transferred.
  Uses `TransferAdmin` (not `Upgrade`) so `get_proposal` remains callable after
  execution.

- **`test_force_release_single_execution_and_duplicate_rejected`**  
  Creates a shipment with escrow, proposes `ForceRelease`, approves to threshold.
  Asserts `escrow_amount == 0` (escrow was released), then calls
  `try_execute_proposal` again and asserts `ProposalAlreadyExecuted`. Verifies
  the guard works for non-`Upgrade` action variants.

---

## Testing

Rust/Cargo is not installed in this codespace environment so tests cannot be run
locally. The CI pipeline (`cargo test`) on the upstream repository will execute
and verify all tests on PR open.

---

## Acceptance Criteria

### Issue #607 ✅
- [x] Expired proposals cannot be approved — `test_approve_expired_proposal_returns_proposal_expired`
- [x] Expired proposals cannot be executed — `test_execute_expired_proposal_returns_proposal_expired`
- [x] Error variant returned correctly (code 24) — `test_proposal_expired_error_code_is_24`
- [x] Non-expired proposals work normally — `test_non_expired_proposal_can_be_approved`, `test_non_expired_proposal_executes_at_threshold`

### Issue #608 ✅
- [x] Duplicate executions are rejected — `test_execute_already_executed_proposal_returns_error`
- [x] Error variant returned correctly (code 23) — `test_proposal_already_executed_error_code_is_23`
- [x] Single execution succeeds — `test_single_execution_succeeds_and_applies_action`

---

closes #607
closes #608
