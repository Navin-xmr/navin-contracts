# Issue #790 implementation note

Issue [#790](https://github.com/Navin-xmr/navin-contracts/issues/790) is addressed
by routing the inline whitelist, multisig, proposal, and circuit-breaker publishers
through `events.rs` helpers. Each migrated payload keeps its existing fields and
appends `EVENT_SCHEMA_VERSION`, a contract-wide `event_counter`, and a deterministic
`idempotency_key` as the documented schema suffix.
