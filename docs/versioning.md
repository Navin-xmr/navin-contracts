# Contract Package Versioning Convention

## The Convention

**Every contract crate in this workspace stays at `version = "0.0.0"`. Do not bump it.**

```toml
[package]
name = "shipment"
version = "0.0.0"   # Always 0.0.0 — see docs/versioning.md
publish = false
```

This applies to all crates under `contracts/`: `token`, `shipment`, and `nft`.

## Why

The `version` field in a `Cargo.toml` is a [crates.io publication](https://doc.rust-lang.org/cargo/reference/manifest.html#the-version-field)
mechanism. It exists to satisfy semver resolution when a crate is uploaded to a registry
and consumed by a downstream `Cargo.toml` as a version *requirement* — `"1.2"` means
"any 1.x.y at or above 1.2.0".

None of that applies here:

1. **No crate is published.** `token` and `shipment` set `publish = false`
   (`contracts/token/Cargo.toml:5`, `contracts/shipment/Cargo.toml:5`), and no crate is
   consumed by a version requirement from outside the workspace. Dependencies between
   workspace members are declared by path
   (`navin-token = { path = "../token" }`, `contracts/shipment/Cargo.toml:22`), which ignores
   the version entirely.

   `contracts/nft/Cargo.toml` is missing `publish = false`, so it is technically
   publishable. That is a separate defect from the version drift this document addresses and
   is left for a dedicated fix; nothing has published it, and `version = "0.0.0"` is accurate
   either way.

2. **Nothing reads the number.** No build script, deploy script, CI job, or documentation
   generator reads it. `scripts/build.sh`, `scripts/deploy-testnet.sh`, and
   `scripts/release-check.sh` all reference contracts by WASM path or crate name, never by
   version. The generated schemas in `docs/contract-schema.*.json` are produced from
   Soroban contract specs and carry no version field.

3. **Deployments are identified by WASM hash, not version.** `scripts/deploy-testnet.sh`
   deploys `target/wasm32-unknown-unknown/release/*.wasm`. Two builds of the same crate
   version can be different contracts, and the same build can be deployed repeatedly.
   The crate version cannot distinguish them, so it cannot serve as a release identity.

4. **It has already drifted, meaninglessly.** `token` was at `0.1.0` while `shipment` and
   `nft` sat at `0.0.0`, with no commit anywhere recording a decision to make that
   distinction. The `0.1.0` arrived in `f018439` ("feat: Integrate Token Contract") as part
   of the crate's initial addition, not as a considered bump.

Setting everything to `0.0.0` makes the field say what it actually means: *this crate is
unpublished and externally version-untracked*. A uniform value cannot drift, because there
is nothing to drift from.

## What To Version Instead

The `version` field is not where release identity lives in this repo. Use these instead:

| Concern | Mechanism | Where |
|---|---|---|
| Which source revision is deployed | Git commit SHA | Git history; tag releases if needed |
| Which contract binary is on-chain | WASM hash | `stellar contract info`, deployment tx |
| Whether an event payload changed | `schema_version` field | `docs/event_schemas.md`, `contracts/token/src/event_topics.rs:37` |
| Whether on-chain storage changed | Storage layout docs | `docs/storage.md` |

Notably, event compatibility already has a real, working version signal:
`EVENT_SCHEMA_VERSION_STR: &str = "v1"` (`contracts/token/src/event_topics.rs:37`) is emitted
as a topic on every token event, and `docs/event_schemas.md` documents a `schema_version: u32`
field on shipment event families. These are on-chain and verifiable by indexers, which is
exactly what the crate `version` is not.

## Rules For Contributors

- Leave `version = "0.0.0"` in a new contract crate's `Cargo.toml`.
- If you are tempted to bump a version to signal "this contract changed", do not. Bump
  `EVENT_SCHEMA_VERSION_STR` or the `schema_version` field instead if you changed an event
  payload, and update `docs/storage.md` if you changed persistent storage keys.
- `cargo build` will rewrite the matching entries in `Cargo.lock`. Commit that change
  alongside your `Cargo.toml` edit; a stale `Cargo.lock` fails `cargo build --locked`.
- If a crate is ever published to a registry, this convention no longer applies to it.
  Revert it to real semver at that point and document the release process.

## Verification

```bash
# All contract crates must be 0.0.0
grep -H '^version' contracts/*/Cargo.toml
# contracts/nft/Cargo.toml:version = "0.0.0"
# contracts/shipment/Cargo.toml:version = "0.0.0"
# contracts/token/Cargo.toml:version = "0.0.0"
```

## Related

- Issue [#919](https://github.com/Navin-xmr/navin-contracts/issues/919) — established this convention.
- [`docs/event_schemas.md`](event_schemas.md) — event payload schemas and `schema_version` policy.
- [`docs/storage.md`](storage.md) — on-chain storage layout.
