# Test Snapshots

This directory contains sanitized JSON snapshots of Soroban test execution states. These snapshots are used for regression testing to ensure contract behavior remains consistent across changes.

## What Are These Files?

Each `.json` file is a complete snapshot of the Soroban ledger state at the end of a test, including:

- Contract storage entries
- Emitted events
- Authorization trees
- Ledger metadata

## Sanitization

All snapshots in this directory have been **sanitized** to remove non-deterministic fields that would otherwise cause noisy diffs on every test run.

### Normalized Fields

The following fields are normalized to canonical values:

- `generators.address` → `0`
- `generators.nonce` → `0`
- `ledger.timestamp` → `86400`
- `ledger.sequence_number` → `1`
- `ledger_key_nonce.nonce` → `0`
- `event.contract_id` → `"0000...0000"` (32-byte zero hash)
- Event idempotency keys → `"0000...0000"` (32-byte zero hash)

### What's Preserved

The sanitizer intentionally preserves:

- Event topics and data structure
- Business data (shipment IDs, amounts, addresses)
- Contract state and storage values
- Event ordering

This ensures that **true contract behavior changes produce diffs** while eliminating ledger-specific noise.

## Updating Snapshots

### After Making Contract Changes

1. Run tests to generate new snapshots:

   ```bash
   cargo test --lib
   ```

2. Sanitize all snapshots:

   ```bash
   cargo run --example sanitize_snapshots
   ```

3. Review the diffs:

   ```bash
   git diff test_snapshots/
   ```

4. If the changes are expected, commit them:
   ```bash
   git add test_snapshots/
   git commit -m "Update snapshots for [feature/fix]"
   ```

### Unexpected Diffs

If you see unexpected diffs in snapshots:

1. **Check if the change is intentional** - Did you modify event payloads, storage structure, or contract logic?

2. **Verify sanitization** - Run the sanitizer again to ensure all volatile fields are normalized:

   ```bash
   cargo run --example sanitize_snapshots
   ```

3. **Investigate the root cause** - If diffs persist after sanitization, they represent real behavior changes that need review.

## CI Integration

In CI, snapshot tests verify that:

1. All committed snapshots are properly sanitized
2. Test execution produces snapshots matching the committed versions
3. No unexpected behavior changes have been introduced

## Documentation

For detailed information about the sanitization process, see:

- [EVENT_SNAPSHOT_SANITIZER.md](../docs/EVENT_SNAPSHOT_SANITIZER.md)

## Invalid Snapshots

Some snapshot files may be invalid JSON due to test failures or incomplete writes. The sanitizer tool gracefully skips these files and reports them. If you encounter invalid snapshots:

1. Re-run the specific test to regenerate the snapshot
2. Run the sanitizer again
3. If the problem persists, investigate the test for potential issues
