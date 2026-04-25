# TTL Health Summary Query

## Overview

The TTL Health Summary query provides aggregated metrics for proactive archival risk monitoring in the Navin shipment contract. This feature enables operations dashboards and indexers to detect datasets approaching expiration and trigger preventive TTL extension or archival workflows.

## Feature Details

### Query Function

```rust
pub fn get_ttl_health_summary(env: Env) -> Result<TtlHealthSummary, NavinError>
```

### Response Structure

```rust
pub struct TtlHealthSummary {
    /// Total number of shipments created (from counter)
    pub total_shipment_count: u64,
    
    /// Number of shipments sampled for health check
    pub sampled_count: u32,
    
    /// Number of sampled shipments found in persistent storage
    pub persistent_count: u32,
    
    /// Number of sampled shipments not found in persistent storage
    pub missing_or_archived_count: u32,
    
    /// Percentage of sampled shipments in persistent storage (0-100)
    pub persistent_percentage: u32,
    
    /// Configured TTL threshold (in ledgers) from contract config
    pub ttl_threshold: u32,
    
    /// Configured TTL extension (in ledgers) from contract config
    pub ttl_extension: u32,
    
    /// Current ledger sequence number at the time of query
    pub current_ledger: u32,
    
    /// Timestamp of the query for correlation with external monitoring
    pub query_timestamp: u64,
}
```

## Sampling Strategy

Due to gas constraints, the query samples a representative subset of shipments rather than checking all shipments:

- **Small datasets (≤20 shipments)**: All shipments are sampled
- **Large datasets (>20 shipments)**: Up to 20 shipments are sampled, evenly distributed across the ID range
- **Sampling method**: Every Nth shipment is checked, where N = total_shipments / 20

This approach provides statistically representative metrics while maintaining predictable gas costs.

## Health Metrics

### Persistent Count
Number of sampled shipments found in persistent storage. These are active shipments with valid TTL.

### Missing or Archived Count
Number of sampled shipments not found in persistent storage. These shipments may be:
- Archived in temporary storage (terminal state)
- Expired due to TTL exhaustion
- Never created (gaps in ID sequence)

### Persistent Percentage
Percentage of sampled shipments still in persistent storage. This is the primary health indicator:

- **90-100%**: Excellent TTL health
- **75-89%**: Good health, monitor trends
- **50-74%**: Moderate concern, investigate archival patterns
- **<50%**: Critical, immediate investigation required

## Use Cases

### 1. Operations Dashboard

Display real-time TTL health metrics:

```typescript
const health = await contract.get_ttl_health_summary();

if (health.persistent_percentage < 90) {
  alert(`TTL Health Warning: ${health.persistent_percentage}% persistent`);
}
```

### 2. Automated Monitoring

Trigger alerts when health degrades:

```typescript
setInterval(async () => {
  const health = await contract.get_ttl_health_summary();
  
  if (health.persistent_percentage < 75) {
    await notifyOps({
      severity: 'warning',
      message: `TTL health at ${health.persistent_percentage}%`,
      persistent_count: health.persistent_count,
      missing_count: health.missing_or_archived_count,
    });
  }
}, 3600000); // Check hourly
```

### 3. Indexer Integration

Correlate on-chain health with off-chain TTL tracking:

```typescript
const health = await contract.get_ttl_health_summary();

// Store in time-series database
await db.insert('ttl_health_metrics', {
  timestamp: health.query_timestamp,
  ledger: health.current_ledger,
  total_shipments: health.total_shipment_count,
  persistent_pct: health.persistent_percentage,
  sampled: health.sampled_count,
});

// Analyze trends
const trend = await db.query(`
  SELECT 
    AVG(persistent_pct) as avg_health,
    MIN(persistent_pct) as min_health
  FROM ttl_health_metrics
  WHERE timestamp > NOW() - INTERVAL '24 hours'
`);
```

### 4. Preventive TTL Extension

Identify shipments needing TTL extension:

```typescript
const health = await contract.get_ttl_health_summary();

if (health.persistent_percentage < 95) {
  // Sample shipments and extend TTL for those approaching threshold
  for (let id = 1; id <= health.total_shipment_count; id++) {
    try {
      // Attempt to read shipment (will fail if expired)
      const shipment = await contract.get_shipment(id);
      
      // If successful, shipment exists - could extend TTL here
      // (Note: TTL extension is automatic on read in Soroban)
    } catch (e) {
      // Shipment not found or expired
      console.log(`Shipment ${id} not accessible`);
    }
  }
}
```

## Configuration Parameters

The summary includes TTL configuration from the contract:

- **ttl_threshold**: Minimum ledgers before TTL extension triggers (default: 17,280 ≈ 1 day)
- **ttl_extension**: Ledgers to extend TTL by (default: 518,400 ≈ 30 days)

These values help operators understand the contract's TTL management behavior.

## Important Notes

### TTL Queryability Limitation

Direct TTL values are **not queryable** from within Soroban contracts in production. The Soroban SDK's `get_ttl()` method is only available in test utilities. This is a platform limitation, not a contract design choice.

As a result, this query provides **observable metrics** (persistent storage presence) rather than direct TTL values. Operators should:

1. Use this query to detect anomalies in persistent storage presence
2. Correlate with external TTL tracking (e.g., ledger snapshots)
3. Monitor trends over time rather than absolute TTL values

### Gas Costs

The query has predictable gas costs:
- **Small datasets**: O(n) where n ≤ 20
- **Large datasets**: O(20) constant cost

Maximum gas cost is bounded by the 20-shipment sample limit.

### Deterministic Behavior

The query is deterministic for a given ledger state:
- Same shipment set → same results
- No randomness in sampling
- Reproducible for testing and auditing

## Testing

Comprehensive test coverage is provided in `test_ttl_health.rs`:

```bash
cargo test test_ttl_health
```

Tests cover:
- Empty contract state
- Single and multiple shipments
- Sampling strategy (boundary cases at 20, 21 shipments)
- Deterministic behavior
- Configuration value inclusion
- Percentage calculation accuracy
- Count consistency

## Integration Example

Complete integration with monitoring system:

```typescript
import { Contract, SorobanRpc } from '@stellar/stellar-sdk';

class TtlHealthMonitor {
  constructor(
    private contract: Contract,
    private rpc: SorobanRpc.Server,
    private alertThreshold: number = 90
  ) {}

  async checkHealth(): Promise<void> {
    const health = await this.contract.get_ttl_health_summary();
    
    console.log('TTL Health Report:');
    console.log(`  Total Shipments: ${health.total_shipment_count}`);
    console.log(`  Sampled: ${health.sampled_count}`);
    console.log(`  Persistent: ${health.persistent_count} (${health.persistent_percentage}%)`);
    console.log(`  Missing/Archived: ${health.missing_or_archived_count}`);
    console.log(`  Ledger: ${health.current_ledger}`);
    console.log(`  Timestamp: ${new Date(health.query_timestamp * 1000).toISOString()}`);
    
    if (health.persistent_percentage < this.alertThreshold) {
      await this.sendAlert(health);
    }
  }

  private async sendAlert(health: TtlHealthSummary): Promise<void> {
    // Send to monitoring system (PagerDuty, Slack, etc.)
    await fetch('https://monitoring.example.com/alerts', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        severity: health.persistent_percentage < 75 ? 'critical' : 'warning',
        title: 'TTL Health Degradation Detected',
        details: {
          persistent_percentage: health.persistent_percentage,
          persistent_count: health.persistent_count,
          missing_count: health.missing_or_archived_count,
          total_shipments: health.total_shipment_count,
          ledger: health.current_ledger,
        },
      }),
    });
  }
}

// Usage
const monitor = new TtlHealthMonitor(contract, rpc, 90);
setInterval(() => monitor.checkHealth(), 3600000); // Check hourly
```

## Acceptance Criteria

✅ **TTL health summary query is available and deterministic**
- Query function `get_ttl_health_summary()` is implemented
- Returns consistent results for the same contract state
- No randomness in sampling or calculation

✅ **Output supports operations dashboards/indexers**
- Provides persistent storage presence metrics
- Includes configuration parameters (threshold, extension)
- Includes ledger and timestamp for correlation
- Percentage metric for easy health assessment

✅ **Representative TTL metrics for active records**
- Samples up to 20 shipments evenly distributed
- Calculates persistent percentage as primary health indicator
- Provides counts for persistent and missing/archived shipments

✅ **Tests for summary consistency**
- Comprehensive test suite in `test_ttl_health.rs`
- Tests cover edge cases, sampling strategy, and determinism
- All tests pass successfully

## Future Enhancements

Potential improvements for future versions:

1. **Per-Status Health**: Break down metrics by shipment status
2. **Historical Trends**: Store health snapshots on-chain for trend analysis
3. **Configurable Sample Size**: Allow operators to adjust sample size
4. **Weighted Sampling**: Prioritize recent shipments in sampling
5. **External TTL Oracle**: Integrate with off-chain TTL tracking service

## References

- [Soroban Storage Documentation](https://soroban.stellar.org/docs/learn/persisting-data)
- [TTL Management Best Practices](https://soroban.stellar.org/docs/learn/persisting-data/ttl)
- Contract Configuration: `contracts/shipment/src/config.rs`
- Storage Module: `contracts/shipment/src/storage.rs`
- Type Definitions: `contracts/shipment/src/types.rs`
