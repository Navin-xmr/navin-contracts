# Frontend RPC Verification Flow

This document outlines the canonical procedure for a frontend to verify Navin contract events and transaction results directly via a Stellar RPC node. This ensures that the frontend can operate without trusting the backend (Express indexer) by verifying data against the ledger.

## 1. Step-by-Step Verification Procedure

### A. Fetch Transaction Results
When a user performs an action (e.g., creating a shipment), the frontend receives a transaction hash. 
1. Call the Stellar RPC `getTransaction` method with the hash.
2. Verify that the transaction status is `SUCCESS`.

### B. Extract and Verify Events
1. From the transaction meta (or using `getEvents` RPC method), extract events emitted by the Navin contract.
2. **Filter by Contract ID**: Ensure the `contractId` of the event matches the known Navin contract address.
3. **Verify Topics**:
   - The first topic (index 0) is the event type (e.g., `shipment_created`, `status_updated`).
   - Match this against the expected event type for the action performed.
4. **Verify Data Fields**:
   - Navin events follow a consistent data structure.
   - For `shipment_created`, the data array contains:
     `[shipment_id, sender, receiver, data_hash, version, counter, idempotency_key]`
   - Verify that `sender` and `receiver` match the expected addresses.
   - Verify that `data_hash` matches the SHA-256 hash of the off-chain data held by the frontend.

### C. Verify Idempotency Key
The `idempotency_key` is a SHA-256 hash of:
`shipment_id (u64 BE) | event_type (Symbol XDR) | event_counter (u32 BE)`

Frontends can recompute this hash to ensure the event's metadata hasn't been tampered with and that it matches the specific shipment and sequence.

---

## 2. Sample Request/Response Traces

### RPC `getEvents` Request
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "getEvents",
  "params": {
    "startLedger": 123456,
    "filters": [
      {
        "type": "contract",
        "contractIds": ["CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M"],
        "topics": [["shipment_created"]]
      }
    ],
    "limit": 1
  }
}
```

### RPC `getEvents` Response (Simplified)
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "latestLedger": 123460,
    "events": [
      {
        "type": "contract",
        "ledger": 123458,
        "ledgerClosedAt": "2026-04-27T10:00:00Z",
        "id": "0000000053026365440-0000000001",
        "pagingToken": "0000000053026365440-0000000001",
        "contractId": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M",
        "topic": ["shipment_created"],
        "value": {
          "xdr": "AAAAAgAAAAAAAAABAAAAAAAAAAFAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAITA4AAAAAAAAAABQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAK3IMAAAAAAAAAABQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAMDR4AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAABAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        },
        "inSuccessfulContractCall": true
      }
    ]
  }
}
```

---

## 3. Reference Test Vectors

### Shipment Creation Event
| Field | Value |
|-------|-------|
| Shipment ID | `1` |
| Event Type | `shipment_created` |
| Event Counter | `1` |
| Data Hash | `0x010101...01` (32 bytes) |
| Idempotency Key | `0x...` (See computation below) |

### Idempotency Key Computation
1. **Shipment ID (u64 BE)**: `0000000000000001`
2. **Event Type (Symbol XDR)**: `00000010 73686970 6d656e74 5f637265 61746564` (XDR encoded `shipment_created`)
3. **Event Counter (u32 BE)**: `00000001`

**Concat**: `0000000000000001` + `00000010736869706d656e745f63726561746564` + `00000001`
**SHA-256**: `[result hash]`

---

## 4. Verification Implementation (Rust/Pseudo-code)

See the reference test in [test_verification.rs](file:///Users/mac/Desktop/navin/navin-contracts/contracts/shipment/src/test_verification.rs) for the complete verification logic implementation.

```rust
// Verify Contract ID
assert_eq!(event.contract_id, NAVIN_CONTRACT_ID);

// Verify Topics
assert_eq!(event.topics[0], "shipment_created");

// Verify Data
let data = decode_xdr(event.value);
assert_eq!(data.shipment_id, expected_id);
assert_eq!(data.data_hash, expected_hash);

// Verify Idempotency
let computed_key = sha256(shipment_id_be + event_type_xdr + counter_be);
assert_eq!(data.idempotency_key, computed_key);
```
