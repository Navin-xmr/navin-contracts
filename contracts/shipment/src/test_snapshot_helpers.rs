//! # Snapshot Test Helpers
//!
//! Utilities for working with sanitized snapshots in regression tests.
//!
//! ## How Soroban Snapshots Work
//!
//! The Soroban SDK automatically generates snapshot JSON files in the
//! `test_snapshots/` directory when tests run. These snapshots capture
//! the complete ledger state including events, storage, and auth trees.
//!
//! ## The Problem
//!
//! Raw snapshots contain non-deterministic fields (timestamps, nonces,
//! generated addresses) that change on every test run, making snapshot
//! comparison impractical.
//!
//! ## The Solution
//!
//! Use `sanitize_json_snapshot()` from `test_utils` to normalize volatile
//! fields before committing snapshots or comparing them in CI.
//!
//! ## Workflow
//!
//! ### 1. Generate Raw Snapshots
//! ```bash
//! cargo test --lib
//! ```
//! This creates files in `test_snapshots/<test_module>/<test_name>.1.json`
//!
//! ### 2. Sanitize Snapshots
//! ```rust
//! use std::fs;
//! use crate::test_utils::sanitize_json_snapshot;
//!
//! let raw = fs::read_to_string("test_snapshots/e2e_test/test_happy_path.1.json")?;
//! let sanitized = sanitize_json_snapshot(&raw);
//! fs::write("test_snapshots/e2e_test/test_happy_path.1.json", sanitized)?;
//! ```
//!
//! ### 3. Commit Sanitized Snapshots
//! ```bash
//! git add test_snapshots/
//! git commit -m "Update sanitized snapshots"
//! ```
//!
//! ## Automated Sanitization Script
//!
//! For convenience, you can create a script to sanitize all snapshots:
//!
//! ```bash
//! #!/bin/bash
//! # scripts/sanitize_snapshots.sh
//!
//! cargo test --lib
//! cargo run --bin sanitize_snapshots
//! ```

#![cfg(test)]

extern crate std;

use std::fs;
use std::path::Path;

/// Sanitizes all JSON snapshot files in a directory recursively.
///
/// # Arguments
/// * `snapshot_dir` - Path to the snapshot directory (e.g., "test_snapshots")
///
/// # Returns
/// Number of files sanitized
///
/// # Example
/// ```rust,no_run
/// use crate::test_snapshot_helpers::sanitize_snapshot_directory;
///
/// let count = sanitize_snapshot_directory("test_snapshots").unwrap();
/// println!("Sanitized {} snapshot files", count);
/// ```
#[allow(dead_code)]
pub fn sanitize_snapshot_directory<P: AsRef<Path>>(snapshot_dir: P) -> std::io::Result<usize> {
    let mut count = 0;

    fn visit_dir(dir: &Path, count: &mut usize) -> std::io::Result<()> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    visit_dir(&path, count)?;
                } else if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    // Read, sanitize, and write back
                    let raw = fs::read_to_string(&path)?;
                    let sanitized = crate::test_utils::sanitize_json_snapshot(&raw);
                    fs::write(&path, sanitized)?;
                    *count += 1;
                    std::println!("✓ Sanitized: {}", path.display());
                }
            }
        }
        Ok(())
    }

    visit_dir(snapshot_dir.as_ref(), &mut count)?;
    Ok(count)
}

/// Sanitizes a single snapshot file in place.
///
/// # Arguments
/// * `snapshot_path` - Path to the snapshot JSON file
///
/// # Example
/// ```rust,no_run
/// use crate::test_snapshot_helpers::sanitize_snapshot_file;
///
/// sanitize_snapshot_file("test_snapshots/e2e_test/test_happy_path.1.json").unwrap();
/// ```
#[allow(dead_code)]
pub fn sanitize_snapshot_file<P: AsRef<Path>>(snapshot_path: P) -> std::io::Result<()> {
    let path = snapshot_path.as_ref();
    let raw = fs::read_to_string(path)?;
    let sanitized = crate::test_utils::sanitize_json_snapshot(&raw);
    fs::write(path, sanitized)?;
    std::println!("✓ Sanitized: {}", path.display());
    Ok(())
}

/// Compares a snapshot file against its sanitized version.
/// Returns true if they match, false otherwise.
///
/// Useful for CI checks to ensure committed snapshots are sanitized.
#[allow(dead_code)]
pub fn verify_snapshot_is_sanitized<P: AsRef<Path>>(snapshot_path: P) -> std::io::Result<bool> {
    let path = snapshot_path.as_ref();
    let current = fs::read_to_string(path)?;
    let sanitized = crate::test_utils::sanitize_json_snapshot(&current);
    Ok(current == sanitized)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_sanitize_snapshot_file_example() {
        // This test demonstrates the workflow but doesn't actually run
        // because we don't want to modify real snapshot files in tests

        let example_json = r#"{
            "generators": { "address": 10, "nonce": 5 },
            "ledger": { "timestamp": 123456, "sequence_number": 99 },
            "events": [{
                "event": {
                    "contract_id": "0000000000000000000000000000000000000000000000000000000000000006",
                    "body": {
                        "v0": {
                            "data": {
                                "vec": [
                                    {"u64": 1},
                                    {"bytes": "4d665e5885d370938b6ef4915d3e18cce2280979a315d468afc7bef8d99362b4"}
                                ]
                            }
                        }
                    }
                }
            }]
        }"#;

        let sanitized = crate::test_utils::sanitize_json_snapshot(example_json);

        // Verify normalization
        assert!(sanitized.contains(r#""address": 0"#));
        assert!(sanitized.contains(r#""timestamp": 86400"#));
        assert!(sanitized.contains(
            r#""contract_id": "0000000000000000000000000000000000000000000000000000000000000000""#
        ));
        assert!(sanitized.contains(
            r#""bytes": "0000000000000000000000000000000000000000000000000000000000000000""#
        ));
    }
}
