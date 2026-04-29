# Testing Guidelines for Shipment Contract

This document outlines testing patterns and fixtures for the Navin shipment contract to ensure deterministic and reproducible test behavior.

## Ledger Timestamp Fixtures

### Deterministic Ledger Setup

All shipment tests must use the standardized ledger setup from `test_utils::setup_env()` to ensure deterministic timestamps and sequence numbers.

**Expected Pattern:**

- **Timestamp**: `86400` (Unix epoch + 1 day)
- **Sequence Number**: `1`
- **Protocol Version**: `22`

### Usage in Tests

```rust
use crate::test_utils;

#[test]
fn test_shipment_deadline() {
    let (env, admin) = test_utils::setup_env();
    // env.ledger().timestamp() == 86400
    // env.ledger().sequence() == 1

    // Test logic here
}
```

### Time Advancement

For tests requiring time progression, use the provided helpers instead of direct ledger manipulation:

```rust
// ✅ Preferred - explicit intent
test_utils::advance_ledger_time(&env, 3600); // Advance 1 hour

// ❌ Avoid - magic numbers, unclear intent
env.ledger().with_mut(|l| l.timestamp += 3600);
```

Available helpers:

- `advance_ledger_time(&env, seconds)` - Advance by N seconds
- `advance_past_rate_limit(&env)` - Clear 60-second rate limit window
- `advance_past_multisig_expiry(&env)` - Expire multisig proposals

### Fixture Consistency

When creating new test fixtures, always start with `test_utils::setup_env()` to maintain consistency across all tests. This ensures:

- Reproducible test runs
- Clear intent in time-sensitive operations
- Consistent baseline state for all fixtures</content>
  <parameter name="filePath">c:\Users\AY_o101o\Desktop\drips_contribution\navin-contracts\contracts\shipment\docs\TESTING.md
