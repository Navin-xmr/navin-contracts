Add fractional milestone amount tests

Adds explicit coverage for non-uniform milestone percentage allocations (e.g., 17/33/50 split) that exercise integer-division rounding behavior during milestone payouts and ensure invalid totals are still rejected.

Changes:
- `contracts/shipment/src/types.rs` — Add `FRACTIONAL_MILESTONE_PCTS` constant
- `contracts/shipment/src/test.rs` — Add 4 tests:
  - `test_fractional_milestone_sums_to_100` — fractional split accepted
  - `test_fractional_milestone_payout_math` — exact per-milestone payout assertions
  - `test_fractional_milestone_payout_rounding` — integer-division rounding with dust-carry
  - `test_fractional_milestone_invalid_total_rejected` — sum ≠ 100 rejected
- `contracts/shipment/src/fuzz_milestone_releases.rs` — Add fuzz property `fuzz_milestone_fractional_allocation` for random fractional allocations

closes #462
