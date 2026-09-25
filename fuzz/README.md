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

## Running locally

```bash
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run escrow_arithmetic -- -max_total_time=60
cargo +nightly fuzz run rbac_authorization -- -max_total_time=60
```

Crashing inputs are written to `fuzz/artifacts/<target>/`.
