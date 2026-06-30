## Summary
Add proposal approval threshold boundary tests (#464) that pin the multi-sig behavior at each critical approval-count boundary: threshold-1 (rejection), threshold (auto-execute), and threshold+1 (blocked).

## Changes

### `contracts/shipment/src/error_map.rs`
- Add missing `InvalidSymbol`, `NoteNotFound`, `EvidenceNotFound` error code mappings (pre-existing gap).

### `contracts/shipment/src/test_proposal_digest.rs`
Add 6 threshold boundary tests:
- `threshold_minus_one_does_not_execute` — threshold=4, exactly 3 approvals do not auto-execute; explicit execute returns `InsufficientApprovals`.
- `threshold_exact_auto_executes` — threshold=3, 3rd approval triggers auto-execution.
- `threshold_one_execute_succeeds` — threshold=1, proposer alone meets threshold; explicit execute succeeds.
- `approve_after_auto_execute_is_blocked` — after auto-execute at threshold, additional approve returns `ProposalAlreadyExecuted`.
- `under_threshold_execute_rejected` — only 1 approval vs threshold=5; execute returns `InsufficientApprovals`.
- `approval_count_tracking_is_consistent` — tracks 1→2→3→4 approvals at each step, verifying no auto-execute until threshold.

### `contracts/shipment/src/test_auth.rs`
Add 3 multi-sig auth tree tests:
- `test_auth_tree_init_multisig` — `init_multisig` records correct admin auth.
- `test_auth_tree_propose_action` — `propose_action` records correct proposer auth.
- `test_auth_tree_approve_action` — `approve_action` records correct approver auth.

### Pre-existing fixes
- `TryIntoVal` trait import added to `test.rs`.
- `start_shipment` → `update_status` updated in `test_panic_free_invariants.rs` and `test_symbol_validation.rs`.

## Testing
- `cargo test` — 1132 passed, 3 pre-existing audit-trail snapshot failures (unrelated).
- `cargo clippy --all-targets --all-features` — clean.
- `cargo fmt --all --check` — clean.
- `cargo build --target wasm32-unknown-unknown --release` — passes.

closes #464
