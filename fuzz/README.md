# Corpus-driven fuzzing

This directory holds `cargo-fuzz` (libFuzzer) targets that explore
coverage-guided random inputs against the `shipment` contract, as opposed to
the deterministic `fuzz_*.rs` property-test modules under
`contracts/shipment/src/`.

## Property tests vs. corpus fuzzing

- **`contracts/shipment/src/fuzz_*.rs`** — `#![cfg(test)]` modules that run
  under `cargo test`. They assert specific, hand-picked properties (e.g.
  "escrow never underflows", "non-admins are always rejected") against a
  fixed or seeded set of inputs. These run on every CI push/PR and are fast
  and deterministic, but they only exercise the cases their authors thought
  of.
- **`fuzz/fuzz_targets/*.rs`** (this directory) — real `cargo-fuzz` targets
  driven by libFuzzer. They mutate a byte-string corpus and explore inputs
  the property tests don't cover, growing a corpus of interesting/crashing
  inputs over time. These are not part of the fast CI test suite; they run
  on a schedule (see `.github/workflows/fuzz.yml`) because a useful fuzz
  session takes much longer than a unit test run.

## Targets

- `escrow_arithmetic` — fuzzes the checked-math helpers behind escrow
  accounting (`fuzz_api::add_i128`, `sub_i128`, `sub_escrow`,
  `mul_div_i128`), asserting they never panic and only return an error when
  native checked arithmetic would also fail.
- `rbac_authorization` — fuzzes the public RBAC surface
  (`add_company`, `add_carrier`, `revoke_role`, `get_role`) with an
  attacker-controlled action sequence, asserting a non-admin caller can
  never grant itself a role regardless of prior state.

## Why this is not a workspace member

This crate is a **standalone workspace**, not a member of the root workspace. The
empty `[workspace]` table at the end of `Cargo.toml` is what makes it one, and the
root `Cargo.toml` lists `fuzz` under `exclude`.

The reason is `libfuzzer-sys`. It links libFuzzer's C++ implementation by invoking
`clang++` through `cc-rs`, which has no `wasm32-unknown-unknown` target. Adding this
directory to `members` therefore breaks the WASM builds that the rest of the repo
depends on:

- `scripts/release-check.sh:24` — `cargo build --workspace --target wasm32-unknown-unknown --release`
- `.github/workflows/test.yml:84` — the `wasm-build` job

It also breaks linting and testing: the root manifest is virtual, so a bare
`cargo build`/`cargo test`/`cargo clippy --all-targets` applies to *all* members.
That would pull the `fuzz_target!` binaries into a stable-toolchain `cargo test`,
while fuzzing requires nightly plus sanitizer instrumentation
(`.github/workflows/fuzz.yml:24` pins `toolchain: nightly`).

So the exclusion is a build-target constraint, not an oversight.

## Keeping it from drifting

Being outside the workspace means this crate resolves its own dependency graph, so
`fuzz/Cargo.toml` must pin `soroban-sdk` to the same requirement as the root
`[workspace.dependencies]` — currently `"22.0.0"` in both. A path dependency
(`shipment = { path = "../contracts/shipment" }`) still picks up the current contract
source, so contract changes are always fuzzed against; only the SDK *version
requirement* can drift, and it is duplicated in exactly one place per manifest.

If you change the `soroban-sdk` version in the root `Cargo.toml`, change it here too.
A mismatch is the failure mode this exclusion makes possible, so it is the one thing
to watch when editing either manifest.

## Running locally

```bash
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run escrow_arithmetic -- -max_total_time=60
cargo +nightly fuzz run rbac_authorization -- -max_total_time=60
```

Crashing inputs are written to `fuzz/artifacts/<target>/`.
