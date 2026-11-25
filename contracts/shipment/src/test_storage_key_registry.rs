//! Storage key registry coverage (issue #1266).
//!
//! `docs/storage-key-registry.md` is the registry of every storage key in this
//! crate. These tests read every `#[contracttype]` key enum straight from the
//! source and fail when a variant is missing from the registry, so adding a key
//! without documenting it is caught here rather than in review.

extern crate std;

use std::string::String;
use std::vec::Vec;

const REGISTRY: &str = include_str!("../../../docs/storage-key-registry.md");
const TYPES_SRC: &str = include_str!("types.rs");
const LIB_SRC: &str = include_str!("lib.rs");

/// A storage key variant: its enum, name and payload (e.g. `(u64, u32)`).
struct KeyVariant {
    enum_name: String,
    name: String,
    payload: String,
}

/// Every key enum declared in `src`, in declaration order.
///
/// A key enum is any `enum` whose name ends in `Key` and that is annotated
/// with `#[contracttype]`.
fn key_enums(src: &str) -> Vec<(String, &str)> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(pos) = rest.find("enum ") {
        let before = &rest[..pos];
        let after = &rest[pos + "enum ".len()..];
        rest = after;

        let name_end = after
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        let name = &after[..name_end];
        if !name.ends_with("Key") {
            continue;
        }
        // The attribute must sit directly above the declaration (doc
        // comments and other attributes may be in between).
        let attr_window = before.rsplit("\n\n").next().unwrap_or(before);
        if !attr_window.contains("#[contracttype") {
            continue;
        }
        let Some(open) = after.find('{') else {
            continue;
        };
        let close = after[open..]
            .find("\n}")
            .map(|i| open + i)
            .unwrap_or_else(|| panic!("enum {name} has no closing brace"));
        out.push((String::from(name), &after[open + 1..close]));
    }
    out
}

fn variants(enum_name: &str, body: &str) -> Vec<KeyVariant> {
    body.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//") && !l.starts_with('#'))
        .map(|l| {
            let l = l.trim_end_matches(',');
            let (name, payload) = match l.find('(') {
                Some(i) => (&l[..i], &l[i..]),
                None => (l, ""),
            };
            KeyVariant {
                enum_name: String::from(enum_name),
                name: String::from(name.trim()),
                payload: String::from(payload),
            }
        })
        .collect()
}

fn all_key_variants() -> Vec<KeyVariant> {
    let mut out = Vec::new();
    for src in [TYPES_SRC, LIB_SRC] {
        for (enum_name, body) in key_enums(src) {
            out.extend(variants(&enum_name, body));
        }
    }
    out
}

#[test]
fn test_registry_walk_finds_every_key_enum() {
    let names: Vec<String> = all_key_variants()
        .into_iter()
        .map(|v| v.enum_name)
        .collect();
    for expected in [
        "DataKey",
        "DisputeKey",
        "SettlementKey",
        "StatusHashKey",
        "DeliveryProofKey",
        "SettlementProofKey",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "source walk did not find key enum {expected}; the parser is out of date"
        );
    }
}

#[test]
fn test_every_storage_key_variant_is_in_registry() {
    let mut missing = Vec::new();
    for v in all_key_variants() {
        let entry = std::format!("`{}::{}{}`", v.enum_name, v.name, v.payload);
        if !REGISTRY.contains(&entry) {
            missing.push(entry);
        }
    }
    assert!(
        missing.is_empty(),
        "storage keys missing from docs/storage-key-registry.md: {missing:?}"
    );
}

/// Soroban encodes an enum key as `[Symbol(variant_name), payload...]`; the
/// enum type is not part of the key. Two key enums declaring the same variant
/// name with the same payload would therefore address the same storage entry.
#[test]
fn test_no_two_key_enums_encode_the_same_key() {
    let all = all_key_variants();
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            let same_key = a.name == b.name && a.payload == b.payload;
            assert!(
                !same_key,
                "{}::{}{} and {}::{}{} encode to the same storage key",
                a.enum_name,
                a.name,
                a.payload,
                b.enum_name,
                b.name,
                b.payload
            );
        }
    }
}
