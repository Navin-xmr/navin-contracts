# Navin Shipment Event Payload Schemas

Canonical JSON-schema-like payload contracts for indexers and frontend parsers.

This file is the central reference for tuple layouts emitted by the shipment contract.
For source-of-truth topic names, see `contracts/shipment/src/event_topics.rs`.

## Shared Type Notes

- `Address`: Soroban address (account or contract)
- `BytesN<32>`: 32-byte hash payload
- `ShipmentStatus`: enum from `contracts/shipment/src/types.rs`
- `EscrowFreezeReason`: enum from `contracts/shipment/src/types.rs`

## Schema Versioning Policy

Some event families include trailing versioning fields:

- `schema_version: u32`
- `event_counter: u32`
- `idempotency_key: BytesN<32>`

When present, these are always the final tuple elements and are **required**.

## Event Schemas

### `shipment_created`

```json
{
  "topic": ["shipment_created"],
  "data": {
    "type": "tuple",
    "required": [
      "shipment_id",
      "sender",
      "receiver",
      "data_hash",
      "schema_version",
      "event_counter",
      "idempotency_key"
    ],
    "fields": [
      { "name": "shipment_id", "type": "u64" },
      { "name": "sender", "type": "Address" },
      { "name": "receiver", "type": "Address" },
      { "name": "data_hash", "type": "BytesN<32>" },
      { "name": "schema_version", "type": "u32" },
      { "name": "event_counter", "type": "u32" },
      { "name": "idempotency_key", "type": "BytesN<32>" }
    ]
  }
}
```

### `status_updated`

```json
{
  "topic": ["status_updated"],
  "data": {
    "type": "tuple",
    "required": [
      "shipment_id",
      "old_status",
      "new_status",
      "data_hash",
      "schema_version",
      "event_counter",
      "idempotency_key"
    ],
    "fields": [
      { "name": "shipment_id", "type": "u64" },
      { "name": "old_status", "type": "ShipmentStatus" },
      { "name": "new_status", "type": "ShipmentStatus" },
      { "name": "data_hash", "type": "BytesN<32>" },
      { "name": "schema_version", "type": "u32" },
      { "name": "event_counter", "type": "u32" },
      { "name": "idempotency_key", "type": "BytesN<32>" }
    ]
  }
}
```

### `escrow_deposited`

```json
{
  "topic": ["escrow_deposited"],
  "data": {
    "type": "tuple",
    "required": [
      "shipment_id",
      "from",
      "amount",
      "schema_version",
      "event_counter",
      "idempotency_key"
    ],
    "fields": [
      { "name": "shipment_id", "type": "u64" },
      { "name": "from", "type": "Address" },
      { "name": "amount", "type": "i128" },
      { "name": "schema_version", "type": "u32" },
      { "name": "event_counter", "type": "u32" },
      { "name": "idempotency_key", "type": "BytesN<32>" }
    ]
  }
}
```

### `dispute_raised`

```json
{
  "topic": ["dispute_raised"],
  "data": {
    "type": "tuple",
    "required": ["shipment_id", "raised_by", "reason_hash"],
    "fields": [
      { "name": "shipment_id", "type": "u64" },
      { "name": "raised_by", "type": "Address" },
      { "name": "reason_hash", "type": "BytesN<32>" }
    ]
  }
}
```

### `escrow_frozen`

```json
{
  "topic": ["escrow_frozen"],
  "data": {
    "type": "tuple",
    "required": ["shipment_id", "reason", "caller", "timestamp"],
    "fields": [
      { "name": "shipment_id", "type": "u64" },
      { "name": "reason", "type": "EscrowFreezeReason" },
      { "name": "caller", "type": "Address" },
      { "name": "timestamp", "type": "u64" }
    ]
  }
}
```

## Conformance Fixtures

Canonical fixture test coverage for parser conformance lives in:

- `contracts/shipment/src/test_event_fixtures.rs`
- `contracts/shipment/test_snapshots/test_event_fixtures/`

Indexers should validate their decoders against these fixture outputs in CI.
