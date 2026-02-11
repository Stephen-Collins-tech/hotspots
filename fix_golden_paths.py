#!/usr/bin/env python3
"""Convert absolute paths to relative paths in all golden test files"""

import json
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent
GOLDEN_DIR = PROJECT_ROOT / "tests" / "golden"

def normalize_path_to_relative(path_str, project_root):
    """Convert an absolute path to a relative path from project root"""
    path = Path(path_str)

    # If already relative, return as-is
    if not path.is_absolute():
        return path_str

    # Try to make it relative to project root
    try:
        relative = path.relative_to(project_root)
        # Use forward slashes for cross-platform compatibility
        return str(relative).replace('\\', '/')
    except ValueError:
        # Path is not under project root - look for common patterns
        # Find "tests/fixtures/" in the path and extract from there
        path_str_normalized = str(path).replace('\\', '/')
        if 'tests/fixtures/' in path_str_normalized:
            idx = path_str_normalized.index('tests/fixtures/')
            return path_str_normalized[idx:]

        # If we can't convert it, return as-is (shouldn't happen for our golden files)
        return path_str

def normalize_paths_in_json(data, project_root):
    """Recursively normalize paths in JSON structure"""
    if isinstance(data, list):
        for item in data:
            normalize_paths_in_json(item, project_root)
    elif isinstance(data, dict):
        if "file" in data and isinstance(data["file"], str):
            data["file"] = normalize_path_to_relative(data["file"], project_root)
        for value in data.values():
            normalize_paths_in_json(value, project_root)

def fix_golden_file(golden_path):
    """Update a golden file to use relative paths"""
    print(f"  Fixing {golden_path.name}...")

    # Read the JSON
    with open(golden_path, 'r') as f:
        data = json.load(f)

    # Normalize all paths to relative
    normalize_paths_in_json(data, PROJECT_ROOT)

    # Write back with pretty formatting
    with open(golden_path, 'w') as f:
        json.dump(data, f, indent=2)
        f.write('\n')  # Add trailing newline

def main():
    print("Converting golden test files to use relative paths...\n")

    # Find all JSON files in the golden directory
    golden_files = sorted(GOLDEN_DIR.glob("*.json"))

    if not golden_files:
        print("✗ No golden files found!", file=sys.stderr)
        return 1

    for golden_file in golden_files:
        try:
            fix_golden_file(golden_file)
        except Exception as e:
            print(f"✗ Error fixing {golden_file.name}: {e}", file=sys.stderr)
            return 1

    print(f"\n✓ Successfully fixed {len(golden_files)} golden files!")
    print("\nAll paths are now relative to the project root.")
    return 0

if __name__ == "__main__":
    sys.exit(main())
