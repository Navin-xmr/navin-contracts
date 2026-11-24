# Navin Shipment Event Payload Schemas

The canonical, human-readable reference for every event topic, its payload
fields, type reference, schema versioning policy, and TypeScript type
definitions now lives in [`docs/events.md`](events.md). That file supersedes
this one; see in particular:

- [Events Index](events.md#events-index) — full table of topics and payload fields
- [Schema Version and Idempotency](events.md#schema-version-and-idempotency) — `schema_version`, `event_counter`, `idempotency_key` policy
- [Soroban Type Reference](events.md#soroban-type-reference) — `Address`, `BytesN<32>`, enum type mappings
- [TypeScript Type Definitions](events.md#typescript-type-definitions) — generated types for indexers/frontends

For source-of-truth topic name constants, see
`contracts/shipment/src/event_topics.rs`.

## Conformance Fixtures

Canonical fixture test coverage for parser conformance lives in:

- `contracts/shipment/src/test_event_fixtures.rs`
- `contracts/shipment/test_snapshots/test_event_fixtures/`

Indexers should validate their decoders against these fixture outputs in CI.
