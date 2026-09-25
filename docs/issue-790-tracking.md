# Issue #790 tracking note

Issue [#790](https://github.com/Navin-xmr/navin-contracts/issues/790) tracks
the remaining inline event publishers in `shipment/src/lib.rs` that need to be
migrated to the centralized event-schema helpers. This PR does not silently
rewrite those event payloads: the event-counter and idempotency-key contract
must be finalized with the maintainer before changing the public indexer
schema.
