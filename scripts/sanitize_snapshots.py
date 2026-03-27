#!/usr/bin/env python3
import os
import json
import re

def sanitize_snapshot(data):
    if isinstance(data, dict):
        # Sanitize known non-deterministic fields
        if "ledger_key_nonce" in data:
            if isinstance(data["ledger_key_nonce"], dict) and "nonce" in data["ledger_key_nonce"]:
                data["ledger_key_nonce"]["nonce"] = 0
        
        # Sanitize generators
        if "generators" in data and isinstance(data["generators"], dict):
            if "address" in data["generators"]:
                data["generators"]["address"] = 0
            if "nonce" in data["generators"]:
                data["generators"]["nonce"] = 0
        
        # Sanitize ledger
        if "ledger" in data and isinstance(data["ledger"], dict):
            if "timestamp" in data["ledger"]:
                data["ledger"]["timestamp"] = 86400
            if "sequence_number" in data["ledger"]:
                data["ledger"]["sequence_number"] = 1
        
        # Recurse
        for key in data:
            sanitize_snapshot(data[key])
    elif isinstance(data, list):
        for item in data:
            sanitize_snapshot(item)

def process_file(filepath):
    with open(filepath, 'r') as f:
        try:
            data = json.load(f)
        except json.JSONDecodeError:
            print(f"Skipping {filepath}: Invalid JSON")
            return

    sanitize_snapshot(data)

    with open(filepath, 'w') as f:
        json.dump(data, f, indent=2)
        f.write('\n')

def main():
    root_dir = "."
    for root, dirs, files in os.walk(root_dir):
        if "test_snapshots" in root:
            for file in files:
                if file.endswith(".json"):
                    filepath = os.path.join(root, file)
                    print(f"Sanitizing {filepath}...")
                    process_file(filepath)

if __name__ == "__main__":
    main()
