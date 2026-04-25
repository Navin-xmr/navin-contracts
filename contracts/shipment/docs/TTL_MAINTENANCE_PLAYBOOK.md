# TTL Maintenance Playbook

## Overview

This playbook provides concrete operational procedures for maintaining Time-To-Live (TTL) health in the Navin shipment contract. It covers routine maintenance, monitoring, emergency archival handling, and incident response for TTL-related issues.

**Target Audience**: DevOps engineers, SREs, and contract operators responsible for maintaining active-state data availability.

**Prerequisites**:
- Access to Stellar RPC endpoint
- Soroban CLI installed (`stellar` or `soroban` command)
- Contract address and network configuration
- Admin credentials for contract operations

---

## Table of Contents

1. [Understanding TTL in Soroban](#understanding-ttl-in-soroban)
2. [Contract TTL Configuration](#contract-ttl-configuration)
3. [Routine Maintenance Procedures](#routine-maintenance-procedures)
4. [Monitoring and Health Checks](#monitoring-and-health-checks)
5. [Emergency Archival Handling](#emergency-archival-handling)
6. [Incident Response Runbook](#incident-response-runbook)
7. [Command Reference](#command-reference)
8. [Troubleshooting Guide](#troubleshooting-guide)

---

## Understanding TTL in Soroban

### What is TTL?

Time-To-Live (TTL) is Soroban's mechanism for managing state rent. Each storage entry has a TTL measured in ledgers (blocks). When TTL reaches zero, the entry is archived and becomes inaccessible until restored.

### Storage Types

| Storage Type | Use Case | TTL Behavior | Cost |
|--------------|----------|--------------|------|
| **Persistent** | Active shipments, escrow, critical data | Requires manual extension | Higher rent |
| **Temporary** | Archived shipments, idempotency windows | Auto-expires after TTL | Lower rent |
| **Instance** | Contract config, counters, roles | Shared with contract instance | Moderate |

### TTL Lifecycle

```
┌─────────────┐     TTL Extension      ┌─────────────┐
│   Active    │◄──────────────────────│   Active    │
│  (30 days)  │                        │  (30 days)  │
└──────┬──────┘                        └─────────────┘
       │
       │ TTL Expires (no extension)
       ▼
┌─────────────┐     Restore Tx        ┌─────────────┐
│  Archived   │──────────────────────►│   Active    │
│ (read-only) │                        │  (30 days)  │
└─────────────┘                        └─────────────┘
```

### Contract-Specific TTL Settings

The Navin contract uses these default TTL parameters (configurable via `update_config`):

- **TTL Threshold**: `17,280` ledgers (~1 day at 5s/ledger)
  - Minimum TTL before extension is triggered
- **TTL Extension**: `518,400` ledgers (~30 days)
  - Amount to extend TTL by when threshold is reached

---

## Contract TTL Configuration

### View Current Configuration

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_config
```

**Expected Output**:
```json
{
  "shipment_ttl_threshold": 17280,
  "shipment_ttl_extension": 518400,
  ...
}
```

### Update TTL Configuration

**⚠️ Admin-only operation**

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  --source <ADMIN_SECRET_KEY> \
  -- \
  update_config \
  --config '{
    "shipment_ttl_threshold": 17280,
    "shipment_ttl_extension": 518400,
    ...
  }'
```

### Recommended TTL Settings by Environment

| Environment | Threshold (ledgers) | Extension (ledgers) | Rationale |
|-------------|---------------------|---------------------|-----------|
| **Production** | 17,280 (~1 day) | 518,400 (~30 days) | Balance cost and safety |
| **Staging** | 8,640 (~12 hours) | 259,200 (~15 days) | Faster testing cycles |
| **Development** | 1,000 (~1.4 hours) | 86,400 (~5 days) | Rapid iteration |

---

## Routine Maintenance Procedures

### Daily TTL Health Check

**Frequency**: Every 24 hours  
**Duration**: ~30 seconds  
**Automation**: Recommended (cron job or monitoring system)

#### Procedure

1. **Query TTL Health Summary**

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_ttl_health_summary
```

2. **Interpret Results**

```json
{
  "total_shipment_count": 1250,
  "sampled_count": 20,
  "persistent_count": 19,
  "missing_or_archived_count": 1,
  "persistent_percentage": 95,
  "ttl_threshold": 17280,
  "ttl_extension": 518400,
  "current_ledger": 1234567,
  "query_timestamp": 1714089600
}
```

**Health Status Interpretation**:

| Persistent % | Status | Action Required |
|--------------|--------|-----------------|
| 90-100% | ✅ Excellent | None - routine monitoring |
| 75-89% | ⚠️ Good | Monitor trends, investigate if declining |
| 50-74% | 🟠 Moderate | Investigate archival patterns, consider bulk extension |
| <50% | 🔴 Critical | **Immediate action required** - see Emergency Procedures |

3. **Log Results**

```bash
# Append to monitoring log
echo "$(date -Iseconds),$(stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_ttl_health_summary | jq -r '.persistent_percentage')" >> /var/log/navin/ttl_health.csv
```

### Weekly Bulk TTL Extension

**Frequency**: Every 7 days  
**Duration**: ~5-10 minutes (depends on shipment count)  
**Automation**: Recommended (scheduled job)

#### Procedure

1. **Identify Active Shipments**

```bash
# Get total shipment count
TOTAL=$(stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_shipment_count | jq -r '.')

echo "Total shipments: $TOTAL"
```

2. **Extend TTL for Active Shipments**

The contract provides an `extend_shipment_ttl` function that extends TTL for a specific shipment:

```bash
# Extend TTL for shipment ID 1
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  extend_shipment_ttl \
  --shipment_id 1
```

3. **Bulk Extension Script**

```bash
#!/bin/bash
# bulk_ttl_extend.sh

CONTRACT_ID="<YOUR_CONTRACT_ID>"
NETWORK="<YOUR_NETWORK>"
START_ID=1
END_ID=100  # Adjust based on your shipment count

for id in $(seq $START_ID $END_ID); do
  echo "Extending TTL for shipment $id..."
  
  stellar contract invoke \
    --id "$CONTRACT_ID" \
    --network "$NETWORK" \
    -- \
    extend_shipment_ttl \
    --shipment_id "$id" 2>&1 | tee -a ttl_extend.log
  
  # Rate limiting: 1 request per second
  sleep 1
done

echo "Bulk TTL extension complete. Check ttl_extend.log for details."
```

**Usage**:
```bash
chmod +x bulk_ttl_extend.sh
./bulk_ttl_extend.sh
```

### Monthly Archival Cleanup

**Frequency**: Monthly (1st of each month)  
**Duration**: ~10-30 minutes  
**Automation**: Optional (manual review recommended)

#### Procedure

1. **Identify Finalized Shipments**

Finalized shipments (Delivered/Cancelled with zero escrow) are candidates for archival:

```bash
# Query shipments by status
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_shipments_by_status \
  --status Delivered \
  --limit 100
```

2. **Archive Finalized Shipments**

**⚠️ Admin-only operation**

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  --source <ADMIN_SECRET_KEY> \
  -- \
  archive_shipment \
  --admin <ADMIN_ADDRESS> \
  --shipment_id <SHIPMENT_ID>
```

3. **Verify Archival**

```bash
# Shipment should still be readable (from temporary storage)
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_shipment \
  --shipment_id <SHIPMENT_ID>
```

**Archival Benefits**:
- Reduces persistent storage costs by ~70%
- Maintains read access for historical queries
- Prevents accidental mutations to finalized state

---

## Monitoring and Health Checks

### Automated Monitoring Setup

#### 1. Prometheus Exporter Script

```python
#!/usr/bin/env python3
# ttl_health_exporter.py

import json
import subprocess
import time
from prometheus_client import start_http_server, Gauge

# Metrics
ttl_persistent_percentage = Gauge('navin_ttl_persistent_percentage', 'Percentage of shipments in persistent storage')
ttl_total_shipments = Gauge('navin_ttl_total_shipments', 'Total number of shipments')
ttl_persistent_count = Gauge('navin_ttl_persistent_count', 'Number of persistent shipments')
ttl_missing_count = Gauge('navin_ttl_missing_count', 'Number of missing/archived shipments')

CONTRACT_ID = "<YOUR_CONTRACT_ID>"
NETWORK = "<YOUR_NETWORK>"

def fetch_ttl_health():
    """Query contract for TTL health summary"""
    result = subprocess.run([
        'stellar', 'contract', 'invoke',
        '--id', CONTRACT_ID,
        '--network', NETWORK,
        '--',
        'get_ttl_health_summary'
    ], capture_output=True, text=True)
    
    return json.loads(result.stdout)

def update_metrics():
    """Update Prometheus metrics"""
    health = fetch_ttl_health()
    
    ttl_persistent_percentage.set(health['persistent_percentage'])
    ttl_total_shipments.set(health['total_shipment_count'])
    ttl_persistent_count.set(health['persistent_count'])
    ttl_missing_count.set(health['missing_or_archived_count'])

if __name__ == '__main__':
    # Start Prometheus HTTP server on port 8000
    start_http_server(8000)
    print("TTL Health Exporter running on :8000")
    
    while True:
        try:
            update_metrics()
        except Exception as e:
            print(f"Error updating metrics: {e}")
        
        # Update every 5 minutes
        time.sleep(300)
```

**Usage**:
```bash
pip install prometheus-client
python3 ttl_health_exporter.py
```

#### 2. Grafana Dashboard Query

```promql
# TTL Health Percentage (target: >90%)
navin_ttl_persistent_percentage

# Alert when health drops below 90%
navin_ttl_persistent_percentage < 90
```

#### 3. Alert Configuration (Alertmanager)

```yaml
# alertmanager.yml
groups:
  - name: navin_ttl_alerts
    interval: 5m
    rules:
      - alert: TTLHealthDegraded
        expr: navin_ttl_persistent_percentage < 90
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "TTL health degraded to {{ $value }}%"
          description: "Persistent storage percentage below 90%. Investigate archival patterns."
      
      - alert: TTLHealthCritical
        expr: navin_ttl_persistent_percentage < 75
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "CRITICAL: TTL health at {{ $value }}%"
          description: "Immediate action required. Run bulk TTL extension."
```

### Manual Health Check Commands

#### Quick Health Check

```bash
# One-liner health check
stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_ttl_health_summary | jq '{persistent_percentage, total_shipment_count, persistent_count}'
```

#### Detailed Diagnostics

```bash
# Check specific shipment TTL status
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_shipment \
  --shipment_id <SHIPMENT_ID>
```

---

## Emergency Archival Handling

### Scenario 1: Mass TTL Expiration Detected

**Symptoms**:
- TTL health percentage drops below 50%
- Multiple shipments become inaccessible
- Users report "ShipmentNotFound" errors

#### Emergency Response Procedure

**⏱️ Time-sensitive: Execute within 1 hour**

1. **Assess Impact**

```bash
# Check current health
stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_ttl_health_summary

# Identify affected shipment range
TOTAL=$(stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_shipment_count | jq -r '.')
echo "Total shipments: $TOTAL"
```

2. **Emergency Bulk TTL Extension**

```bash
#!/bin/bash
# emergency_ttl_restore.sh

CONTRACT_ID="<YOUR_CONTRACT_ID>"
NETWORK="<YOUR_NETWORK>"
TOTAL_SHIPMENTS=<TOTAL_COUNT>

# Parallel execution for speed (use GNU parallel if available)
seq 1 $TOTAL_SHIPMENTS | parallel -j 10 \
  "stellar contract invoke --id $CONTRACT_ID --network $NETWORK -- extend_shipment_ttl --shipment_id {} 2>&1 | tee -a emergency_restore.log"
```

3. **Verify Recovery**

```bash
# Re-check health after 10 minutes
sleep 600
stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_ttl_health_summary
```

4. **Post-Incident Report**

Document:
- Root cause (missed maintenance, config error, etc.)
- Number of affected shipments
- Recovery time
- Preventive measures

### Scenario 2: Individual Shipment Restoration

**Symptoms**:
- Specific shipment ID returns "ShipmentNotFound"
- Shipment was recently active

#### Restoration Procedure

1. **Verify Shipment Exists**

```bash
# Check if shipment is archived
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_shipment \
  --shipment_id <SHIPMENT_ID>
```

2. **Extend TTL**

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  extend_shipment_ttl \
  --shipment_id <SHIPMENT_ID>
```

3. **Verify Restoration**

```bash
# Shipment should now be accessible
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_shipment \
  --shipment_id <SHIPMENT_ID>
```

### Scenario 3: Archived Shipment Access

**Symptoms**:
- Shipment is in "Delivered" or "Cancelled" status
- Shipment is finalized (escrow cleared)
- Shipment is readable but not mutable

#### Access Procedure

Archived shipments are **read-only** and stored in temporary storage:

```bash
# Read archived shipment (works normally)
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_shipment \
  --shipment_id <SHIPMENT_ID>
```

**Note**: Archived shipments cannot be mutated. This is by design to prevent accidental modifications to finalized state.

---

## Incident Response Runbook

### Incident Classification

| Severity | Persistent % | Response Time | Escalation |
|----------|--------------|---------------|------------|
| **P0 - Critical** | <50% | Immediate (15 min) | On-call engineer + Manager |
| **P1 - High** | 50-74% | 1 hour | On-call engineer |
| **P2 - Medium** | 75-89% | 4 hours | Standard support |
| **P3 - Low** | 90-94% | Next business day | Monitoring only |

### P0 Critical Incident Response

**Trigger**: TTL health <50% OR mass shipment unavailability

#### Response Checklist

- [ ] **0-5 min**: Acknowledge incident, notify team
- [ ] **5-10 min**: Run emergency health check
  ```bash
  stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_ttl_health_summary
  ```
- [ ] **10-20 min**: Execute emergency bulk TTL extension script
- [ ] **20-30 min**: Monitor recovery progress
- [ ] **30-45 min**: Verify health returns to >90%
- [ ] **45-60 min**: Document incident and root cause
- [ ] **Post-incident**: Schedule retrospective, update runbook

#### Communication Template

```
INCIDENT: Navin Contract TTL Health Critical

Status: [INVESTIGATING | MITIGATING | RESOLVED]
Severity: P0 - Critical
Impact: [X]% of shipments inaccessible
Start Time: [YYYY-MM-DD HH:MM UTC]
Current Health: [X]% persistent

Actions Taken:
- [Timestamp] Initiated emergency TTL extension
- [Timestamp] Verified [X] shipments restored

Next Update: [HH:MM UTC]
```

### P1 High Incident Response

**Trigger**: TTL health 50-74%

#### Response Checklist

- [ ] **0-15 min**: Assess health trend (improving or degrading?)
- [ ] **15-30 min**: Identify affected shipment ranges
- [ ] **30-60 min**: Execute targeted TTL extension
- [ ] **60-90 min**: Verify health stabilization
- [ ] **Post-incident**: Update monitoring thresholds

---

## Command Reference

### Core TTL Commands

#### Get TTL Health Summary

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_ttl_health_summary
```

**Output**:
```json
{
  "total_shipment_count": 1250,
  "sampled_count": 20,
  "persistent_count": 19,
  "missing_or_archived_count": 1,
  "persistent_percentage": 95,
  "ttl_threshold": 17280,
  "ttl_extension": 518400,
  "current_ledger": 1234567,
  "query_timestamp": 1714089600
}
```

#### Extend Shipment TTL

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  extend_shipment_ttl \
  --shipment_id <SHIPMENT_ID>
```

**Effect**: Extends TTL for shipment and associated data (escrow, confirmation hash, event count) by `ttl_extension` ledgers.

#### Archive Shipment (Admin Only)

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  --source <ADMIN_SECRET_KEY> \
  -- \
  archive_shipment \
  --admin <ADMIN_ADDRESS> \
  --shipment_id <SHIPMENT_ID>
```

**Prerequisites**:
- Shipment must be finalized (Delivered/Cancelled with zero escrow)
- Caller must be contract admin

**Effect**: Moves shipment from persistent to temporary storage (reduces cost, maintains read access).

### Configuration Commands

#### Get Contract Configuration

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_config
```

#### Update TTL Configuration (Admin Only)

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  --source <ADMIN_SECRET_KEY> \
  -- \
  update_config \
  --admin <ADMIN_ADDRESS> \
  --config '{
    "shipment_ttl_threshold": 17280,
    "shipment_ttl_extension": 518400,
    "min_status_update_interval": 60,
    "batch_operation_limit": 10,
    "max_metadata_entries": 5,
    "default_shipment_limit": 100,
    "multisig_min_admins": 2,
    "multisig_max_admins": 10,
    "proposal_expiry_seconds": 604800,
    "deadline_grace_seconds": 0,
    "idempotency_window_seconds": 300,
    "auto_dispute_breach": false
  }'
```

### Query Commands

#### Get Shipment Count

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_shipment_count
```

#### Get Shipment by ID

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_shipment \
  --shipment_id <SHIPMENT_ID>
```

#### Get Shipments by Status

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_shipments_by_status \
  --status <STATUS> \
  --limit <LIMIT>
```

**Valid Statuses**: `Created`, `InTransit`, `AtCheckpoint`, `Delivered`, `Disputed`, `Cancelled`

---

## Troubleshooting Guide

### Issue: "ShipmentNotFound" Error

**Possible Causes**:
1. Shipment TTL expired (not extended in time)
2. Shipment ID doesn't exist
3. Shipment was archived and temporary TTL expired

**Diagnosis**:

```bash
# Check if shipment ID is valid
TOTAL=$(stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_shipment_count | jq -r '.')
echo "Valid shipment IDs: 1 to $TOTAL"

# Check TTL health
stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_ttl_health_summary
```

**Resolution**:

If shipment ID is valid but not found:
1. TTL likely expired - shipment data is lost
2. Prevention: Implement automated TTL extension (see Monitoring section)
3. Recovery: Not possible - data is permanently archived by Soroban

### Issue: TTL Health Percentage Declining

**Possible Causes**:
1. Missed routine TTL extension
2. High shipment creation rate without proportional extension
3. Configuration error (threshold too low)

**Diagnosis**:

```bash
# Check health trend over time
tail -n 100 /var/log/navin/ttl_health.csv | awk -F',' '{print $2}' | sort -n
```

**Resolution**:

1. **Immediate**: Run bulk TTL extension script
2. **Short-term**: Increase extension frequency
3. **Long-term**: Adjust TTL configuration or implement automated extension

### Issue: Archive Operation Fails

**Error**: `ShipmentNotFinalized` or `ShipmentUnavailable`

**Possible Causes**:
1. Shipment has non-zero escrow balance
2. Shipment status is not terminal (Delivered/Cancelled)
3. Shipment is already archived

**Diagnosis**:

```bash
# Check shipment status and escrow
stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_shipment --shipment_id <SHIPMENT_ID>
```

**Resolution**:

1. Ensure shipment is in terminal state (Delivered/Cancelled)
2. Verify escrow is zero (released or refunded)
3. If already archived, no action needed

### Issue: High Storage Costs

**Symptoms**:
- Increasing state rent fees
- Many old shipments in persistent storage

**Diagnosis**:

```bash
# Check shipment status distribution
for status in Created InTransit AtCheckpoint Delivered Disputed Cancelled; do
  echo -n "$status: "
  stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_shipments_by_status --status $status --limit 1000 | jq '. | length'
done
```

**Resolution**:

1. Archive finalized shipments (Delivered/Cancelled with zero escrow)
2. Implement monthly archival cleanup procedure
3. Consider adjusting TTL extension period for cost optimization

### Issue: Bulk Extension Script Fails

**Error**: Rate limiting or timeout errors

**Possible Causes**:
1. RPC rate limits exceeded
2. Network congestion
3. Insufficient XLM for transaction fees

**Resolution**:

1. Add rate limiting to script (1-2 requests/second)
2. Use parallel execution with controlled concurrency
3. Ensure admin account has sufficient XLM balance

```bash
# Check admin balance
stellar account --id <ADMIN_ADDRESS> --network <NETWORK>
```

---

## Best Practices

### DO ✅

- **Automate TTL health monitoring** with alerts at 90% threshold
- **Run weekly bulk TTL extensions** during low-traffic periods
- **Archive finalized shipments monthly** to reduce costs
- **Document all emergency TTL operations** for audit trail
- **Test TTL extension scripts** in staging before production
- **Monitor RPC rate limits** when running bulk operations
- **Keep admin credentials secure** (use hardware wallets for production)

### DON'T ❌

- **Don't rely on manual TTL checks** - automate monitoring
- **Don't archive active shipments** (non-terminal status or non-zero escrow)
- **Don't ignore declining health trends** - investigate early
- **Don't run bulk operations without rate limiting** - respect RPC limits
- **Don't modify TTL config without testing** - validate in staging first
- **Don't assume archived data is permanent** - temporary storage has shorter TTL

---

## Maintenance Schedule Template

### Daily (Automated)

- [ ] 00:00 UTC: TTL health check (log to monitoring system)
- [ ] 06:00 UTC: Alert if health <90%
- [ ] 12:00 UTC: TTL health check
- [ ] 18:00 UTC: TTL health check

### Weekly (Automated)

- [ ] Sunday 02:00 UTC: Bulk TTL extension for all active shipments
- [ ] Sunday 03:00 UTC: Verify health returns to >95%
- [ ] Sunday 04:00 UTC: Generate weekly TTL health report

### Monthly (Manual Review)

- [ ] 1st of month: Review finalized shipments for archival
- [ ] 1st of month: Execute archival for eligible shipments
- [ ] 1st of month: Review TTL configuration and adjust if needed
- [ ] 1st of month: Audit TTL extension logs for anomalies

### Quarterly (Strategic Review)

- [ ] Review TTL-related incidents and trends
- [ ] Optimize TTL configuration based on usage patterns
- [ ] Update runbook based on lessons learned
- [ ] Capacity planning for storage costs

---

## Additional Resources

- **Soroban TTL Documentation**: https://soroban.stellar.org/docs/learn/persisting-data/ttl
- **Contract Source Code**: `contracts/shipment/src/storage.rs`
- **TTL Health Summary Documentation**: `contracts/shipment/docs/TTL_HEALTH_SUMMARY.md`
- **Stellar CLI Documentation**: https://developers.stellar.org/docs/tools/developer-tools

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-04-25 | Initial playbook release |

---

## Support

For questions or issues related to TTL maintenance:

1. Check this playbook and troubleshooting guide
2. Review contract documentation in `contracts/shipment/docs/`
3. Contact DevOps team via [your support channel]
4. For critical incidents (P0/P1), page on-call engineer

---

**Document Owner**: DevOps Team  
**Last Reviewed**: 2026-04-25  
**Next Review**: 2026-07-25
