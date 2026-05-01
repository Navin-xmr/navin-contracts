//! Snapshot Sanitizer Tool
//!
//! Sanitizes all JSON snapshot files in the test_snapshots directory.
//!
//! ## Usage
//!
//! ```bash
//! # Run tests to generate snapshots
//! cargo test --lib
//!
//! # Sanitize all snapshots
//! cargo run --example sanitize_snapshots
//! ```
//!
//! ## What it does
//!
//! 1. Recursively finds all .json files in test_snapshots/
//! 2. Applies sanitize_json_snapshot() to each file
//! 3. Writes the sanitized version back to the same file
//! 4. Reports the number of files processed

use std::fs;
use std::path::Path;

fn sanitize_json_snapshot(json: &str) -> String {
    use serde_json::Value;

    let mut v: Value = serde_json::from_str(json).expect("Invalid JSON for sanitization");

    fn walk(v: &mut Value) {
        match v {
            Value::Object(map) => {
                // Sanitize known non-deterministic fields
                if map.contains_key("ledger_key_nonce") {
                    if let Some(nonce_obj) = map
                        .get_mut("ledger_key_nonce")
                        .and_then(|n| n.as_object_mut())
                    {
                        nonce_obj.insert("nonce".to_string(), Value::from(0));
                    }
                }

                // Sanitize generators
                if let Some(gen) = map.get_mut("generators").and_then(|g| g.as_object_mut()) {
                    if gen.contains_key("address") {
                        gen.insert("address".to_string(), Value::from(0));
                    }
                    if gen.contains_key("nonce") {
                        gen.insert("nonce".to_string(), Value::from(0));
                    }
                }

                // Sanitize ledger
                if let Some(ledger) = map.get_mut("ledger").and_then(|l| l.as_object_mut()) {
                    if ledger.contains_key("timestamp") {
                        ledger.insert("timestamp".to_string(), Value::from(86400));
                    }
                    if ledger.contains_key("sequence_number") {
                        ledger.insert("sequence_number".to_string(), Value::from(1));
                    }
                }

                // Sanitize event contract_id (generated contract addresses)
                if map.contains_key("event") {
                    if let Some(event_obj) = map.get_mut("event").and_then(|e| e.as_object_mut()) {
                        if event_obj.contains_key("contract_id") {
                            event_obj.insert(
                                "contract_id".to_string(),
                                Value::from("0000000000000000000000000000000000000000000000000000000000000000"),
                            );
                        }
                    }
                }

                // Sanitize event idempotency keys
                if map.contains_key("body") {
                    if let Some(body) = map.get_mut("body").and_then(|b| b.as_object_mut()) {
                        if let Some(v0) = body.get_mut("v0").and_then(|v| v.as_object_mut()) {
                            if let Some(data) = v0.get_mut("data").and_then(|d| d.as_object_mut()) {
                                if let Some(vec) =
                                    data.get_mut("vec").and_then(|v| v.as_array_mut())
                                {
                                    if let Some(last) = vec.last_mut() {
                                        if let Some(obj) = last.as_object_mut() {
                                            if obj.contains_key("bytes") {
                                                if let Some(bytes_val) =
                                                    obj.get("bytes").and_then(|b| b.as_str())
                                                {
                                                    if bytes_val.len() == 64 {
                                                        obj.insert(
                                                            "bytes".to_string(),
                                                            Value::from("0000000000000000000000000000000000000000000000000000000000000000"),
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                for value in map.values_mut() {
                    walk(value);
                }
            }
            Value::Array(arr) => {
                for value in arr.iter_mut() {
                    walk(value);
                }
            }
            _ => {}
        }
    }

    walk(&mut v);
    serde_json::to_string_pretty(&v).expect("Failed to serialize sanitized JSON")
}

fn sanitize_directory(
    dir: &Path,
    count: &mut usize,
    errors: &mut Vec<String>,
) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                sanitize_directory(&path, count, errors)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("json") {
                // Read, sanitize, and write back
                match fs::read_to_string(&path) {
                    Ok(raw) => {
                        match serde_json::from_str::<serde_json::Value>(&raw) {
                            Ok(_) => {
                                let sanitized = sanitize_json_snapshot(&raw);

                                // Only write if changed
                                if raw != sanitized {
                                    fs::write(&path, sanitized)?;
                                    println!("✓ Sanitized: {}", path.display());
                                    *count += 1;
                                } else {
                                    println!("  Already sanitized: {}", path.display());
                                }
                            }
                            Err(e) => {
                                let error_msg =
                                    format!("⚠ Skipping invalid JSON {}: {}", path.display(), e);
                                eprintln!("{}", error_msg);
                                errors.push(error_msg);
                            }
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("⚠ Failed to read {}: {}", path.display(), e);
                        eprintln!("{}", error_msg);
                        errors.push(error_msg);
                    }
                }
            }
        }
    }
    Ok(())
}

fn main() {
    let snapshot_dir = Path::new("test_snapshots");

    if !snapshot_dir.exists() {
        eprintln!("❌ Error: test_snapshots directory not found");
        eprintln!("   Run 'cargo test --lib' first to generate snapshots");
        std::process::exit(1);
    }

    println!("Sanitizing snapshots in {}...\n", snapshot_dir.display());

    let mut count = 0;
    let mut errors = Vec::new();
    match sanitize_directory(snapshot_dir, &mut count, &mut errors) {
        Ok(()) => {
            println!("\n✅ Done! Sanitized {} snapshot file(s)", count);
            if !errors.is_empty() {
                println!("\n⚠ {} file(s) skipped due to errors", errors.len());
            }
            if count > 0 {
                println!("\nCommit the sanitized snapshots:");
                println!("  git add test_snapshots/");
                println!("  git commit -m 'Sanitize test snapshots'");
            }
        }
        Err(e) => {
            eprintln!("❌ Error: {}", e);
            std::process::exit(1);
        }
    }
}
