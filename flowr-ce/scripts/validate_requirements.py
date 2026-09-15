#!/usr/bin/env python3
"""Validate all requirements_*.yaml files against the schema."""

import glob
import sys
import yaml

try:
    import jsonschema
except ImportError:
    print("Install jsonschema: pip install jsonschema")
    sys.exit(1)

SCHEMA_PATH = ".spec/schema/requirements.schema.yaml"
REQUIREMENTS_GLOB = ".spec/requirements_*.yaml"

def main():
    with open(SCHEMA_PATH) as f:
        schema = yaml.safe_load(f)

    files = sorted(glob.glob(REQUIREMENTS_GLOB))
    if not files:
        print("FAIL: No requirements files found matching", REQUIREMENTS_GLOB)
        sys.exit(1)

    total_reqs = 0
    all_ids = []

    for path in files:
        with open(path) as f:
            requirements = yaml.safe_load(f)

        try:
            jsonschema.validate(requirements, schema)
        except jsonschema.ValidationError as e:
            print(f"FAIL: {path}: {e.message}")
            print(f"  Path: {'.'.join(str(p) for p in e.absolute_path)}")
            sys.exit(1)

        ids = [r["id"] for r in requirements["requirements"]]
        count = len(ids)
        total_reqs += count
        all_ids.extend(ids)
        print(f"  OK: {path} — {count} requirements (v{requirements['version']})")

    # Check for duplicate IDs across all files
    dupes = [id for id in all_ids if all_ids.count(id) > 1]
    if dupes:
        print(f"\nFAIL: Duplicate requirement IDs across files: {set(dupes)}")
        sys.exit(1)

    print(f"\nOK: {total_reqs} requirements validated across {len(files)} files")

if __name__ == "__main__":
    main()
