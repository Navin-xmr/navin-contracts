# Add tests for MetadataLimitExceeded and MilestoneAlreadyPaid error variants

## Summary

Uncomments and fixes existing placeholder tests for `MetadataLimitExceeded` (error 20) and `MilestoneAlreadyPaid` (error 19) error variants. The commented-out tests had bugs and used the `should_panic` pattern; the fixed tests use `try_` client methods to verify the exact `NavinError` variant.

## Changes

### Issue #611 — MetadataLimitExceeded Error Variant

- **`test_set_shipment_metadata_returns_metadata_limit_exceeded`** (previously commented out): Fixed a bug where all 5 metadata entries used the same key (`"key"`) causing them to overwrite each other instead of filling the limit. Now uses unique keys (`key0`–`key4`) with unique values (`val0`–`val4`). Uses `try_set_shipment_metadata` to assert `Err(Ok(NavinError::MetadataLimitExceeded))` on the 6th entry.

### Issue #612 — MilestoneAlreadyPaid Error Variant

- **`test_record_milestone_returns_milestone_already_paid`** (previously commented out): Replaces the `should_panic` pattern with `try_record_milestone` to assert `Err(Ok(NavinError::MilestoneAlreadyPaid))` when recording the same milestone checkpoint twice.

## Acceptance Criteria Verification

### #611

- [x] **Metadata limit is enforced**: Adding a 6th unique metadata entry returns `MetadataLimitExceeded`.
- [x] **Error variant is returned correctly**: `try_set_shipment_metadata` confirms `NavinError::MetadataLimitExceeded`.
- [x] **Valid metadata additions work**: Existing happy-path tests (`test_set_metadata_with_valid_symbols`, `test_metadata_symbols_multiple_entries`, etc.) continue to pass.

### #612

- [x] **Duplicate milestone payments are rejected**: Recording the same checkpoint twice returns `MilestoneAlreadyPaid`.
- [x] **Error variant is returned correctly**: `try_record_milestone` confirms `NavinError::MilestoneAlreadyPaid`.
- [x] **Single payments succeed**: Existing happy-path tests (`test_milestone_payment_success`, `test_milestone_payment_duplicate_record_no_double_pay`, etc.) continue to pass.

## Files Modified

- `contracts/shipment/src/test.rs` — Uncommented and fixed 2 tests: `test_record_milestone_returns_milestone_already_paid` and `test_set_shipment_metadata_returns_metadata_limit_exceeded`.

closes #611
closes #612
