NotAnAdmin and AlreadyApproved error-variant tests + pre-existing compilation fixes

Adds explicit coverage for the `NotAnAdmin` and `AlreadyApproved` error
variants in the multi-sig proposal and approval paths.

Changes:
- `contracts/shipment/src/test.rs` — Add 6 tests:
  - `test_non_admin_propose_action_returns_not_an_admin` — non-admin proposes action
  - `test_non_admin_approve_action_returns_not_an_admin` — non-admin approves proposal
  - `test_admin_propose_action_succeeds` — admin proposes action successfully
  - `test_admin_approve_action_succeeds` — admin approves proposal successfully
  - `test_same_admin_approve_twice_returns_already_approved` — duplicate approval rejected
  - `test_different_admin_approval_succeeds` — second admin approval succeeds
- `contracts/shipment/src/types.rs` — Add `ProposalSalt` and `ShipmentDependents` DataKey variants
- `contracts/shipment/src/storage.rs` — Add proposal-salt and shipment-dependency storage helpers
- `contracts/shipment/src/lib.rs` — Add `propose_action_with_salt` and `add_shipment_dependency` contract functions; rename long function name

Pre-existing compilation fixes:
- Close unclosed delimiter in `test_get_platform_fee_config_reflects_updated_value`
- Rename `check_consistency_violations_paginated` → `get_consistency_violations` (39 chars exceeds Soroban 32-char limit)

closes #605
closes #606
