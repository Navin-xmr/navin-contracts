# TTL Maintenance Quick Reference Card

**📋 Print this page and keep it handy for quick TTL operations**

---

## Emergency Contacts

| Role | Contact | Escalation |
|------|---------|------------|
| On-Call Engineer | [Your contact] | Immediate |
| DevOps Lead | [Your contact] | P0/P1 incidents |
| Contract Admin | [Your contact] | Admin operations |

---

## Health Status Quick Check

```bash
stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_ttl_health_summary | jq '{persistent_percentage, total_shipment_count}'
```

### Health Interpretation

| Persistent % | Status | Action |
|--------------|--------|--------|
| 90-100% | ✅ Excellent | None |
| 75-89% | ⚠️ Good | Monitor |
| 50-74% | 🟠 Moderate | Investigate |
| <50% | 🔴 **CRITICAL** | **Emergency response** |

---

## Common Operations

### 1. Extend Single Shipment TTL

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  extend_shipment_ttl \
  --shipment_id <ID>
```

### 2. Get Shipment Details

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_shipment \
  --shipment_id <ID>
```

### 3. Archive Finalized Shipment (Admin)

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  --source <ADMIN_KEY> \
  -- \
  archive_shipment \
  --admin <ADMIN_ADDR> \
  --shipment_id <ID>
```

### 4. View Contract Configuration

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- \
  get_config | jq '{shipment_ttl_threshold, shipment_ttl_extension}'
```

---

## Emergency Response (P0 Critical)

**Trigger**: Health <50% OR mass unavailability

### Response Steps (Execute in order)

1. **Acknowledge** (0-5 min)
   ```bash
   # Check health
   stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_ttl_health_summary
   ```

2. **Assess** (5-10 min)
   ```bash
   # Get total shipments
   TOTAL=$(stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_shipment_count | jq -r '.')
   echo "Total: $TOTAL"
   ```

3. **Execute Bulk Extension** (10-30 min)
   ```bash
   # Run emergency script
   ./emergency_ttl_restore.sh
   ```

4. **Verify** (30-45 min)
   ```bash
   # Re-check health
   stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_ttl_health_summary
   ```

5. **Document** (45-60 min)
   - Log incident details
   - Identify root cause
   - Update runbook

---

## Bulk Extension Script Template

```bash
#!/bin/bash
# Save as: bulk_ttl_extend.sh

CONTRACT_ID="<YOUR_CONTRACT_ID>"
NETWORK="<YOUR_NETWORK>"
START_ID=1
END_ID=100  # Adjust based on shipment count

for id in $(seq $START_ID $END_ID); do
  echo "Extending TTL for shipment $id..."
  stellar contract invoke \
    --id "$CONTRACT_ID" \
    --network "$NETWORK" \
    -- \
    extend_shipment_ttl \
    --shipment_id "$id" 2>&1 | tee -a ttl_extend.log
  sleep 1  # Rate limiting
done
```

**Usage**:
```bash
chmod +x bulk_ttl_extend.sh
./bulk_ttl_extend.sh
```

---

## Troubleshooting Quick Fixes

### Issue: "ShipmentNotFound"

**Quick Check**:
```bash
# Verify shipment ID is valid
TOTAL=$(stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_shipment_count | jq -r '.')
echo "Valid IDs: 1 to $TOTAL"
```

**Fix**: If ID is valid, TTL expired - run extension script

### Issue: Archive Fails

**Quick Check**:
```bash
# Check shipment status and escrow
stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> -- get_shipment --shipment_id <ID> | jq '{status, escrow_amount, finalized}'
```

**Fix**: Ensure status is `Delivered` or `Cancelled`, escrow is `0`, and `finalized` is `true`

### Issue: Health Declining

**Quick Check**:
```bash
# Check trend (last 10 readings)
tail -n 10 /var/log/navin/ttl_health.csv
```

**Fix**: Run weekly bulk extension script immediately

---

## Configuration Values

### Default TTL Settings

| Parameter | Value | Meaning |
|-----------|-------|---------|
| `shipment_ttl_threshold` | 17,280 ledgers | ~1 day |
| `shipment_ttl_extension` | 518,400 ledgers | ~30 days |

### Ledger Time Conversion

| Ledgers | Time (approx) |
|---------|---------------|
| 1,000 | ~1.4 hours |
| 8,640 | ~12 hours |
| 17,280 | ~1 day |
| 86,400 | ~5 days |
| 259,200 | ~15 days |
| 518,400 | ~30 days |

**Formula**: `ledgers × 5 seconds = total seconds`

---

## Monitoring Alerts

### Alert Thresholds

| Metric | Warning | Critical |
|--------|---------|----------|
| Persistent % | <90% | <75% |
| Response Time | 1 hour | 15 minutes |

### Alert Query (Prometheus)

```promql
# Warning alert
navin_ttl_persistent_percentage < 90

# Critical alert
navin_ttl_persistent_percentage < 75
```

---

## Maintenance Schedule

### Daily (Automated)
- ✅ 00:00, 06:00, 12:00, 18:00 UTC: Health checks

### Weekly (Automated)
- ✅ Sunday 02:00 UTC: Bulk TTL extension

### Monthly (Manual)
- ✅ 1st of month: Archive finalized shipments
- ✅ 1st of month: Review configuration

---

## Key Files & Locations

| Resource | Location |
|----------|----------|
| **Full Playbook** | `contracts/shipment/docs/TTL_MAINTENANCE_PLAYBOOK.md` |
| **Health Summary Docs** | `contracts/shipment/docs/TTL_HEALTH_SUMMARY.md` |
| **Storage Module** | `contracts/shipment/src/storage.rs` |
| **Config Module** | `contracts/shipment/src/config.rs` |
| **Health Logs** | `/var/log/navin/ttl_health.csv` |
| **Extension Logs** | `/var/log/navin/ttl_extend.log` |

---

## Important Notes

⚠️ **Critical Reminders**:
- Archived shipments are **read-only** (by design)
- TTL extension requires **no admin auth** (anyone can extend)
- Archive operation requires **admin auth**
- Bulk operations should include **rate limiting** (1-2 req/sec)
- Always **test in staging** before production

✅ **Best Practices**:
- Automate health monitoring with alerts
- Run weekly bulk extensions during low-traffic periods
- Archive finalized shipments monthly to reduce costs
- Document all emergency operations
- Keep admin credentials secure

---

## Support Escalation

1. **Check this quick reference** for common operations
2. **Review full playbook** for detailed procedures
3. **Check troubleshooting guide** for specific issues
4. **Contact on-call engineer** for P0/P1 incidents
5. **Page DevOps lead** if on-call is unavailable

---

**Last Updated**: 2026-04-25  
**Version**: 1.0.0  
**Owner**: DevOps Team

---

**📌 Bookmark this page**: `contracts/shipment/docs/TTL_QUICK_REFERENCE.md`
