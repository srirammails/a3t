#!/usr/bin/env python3
"""Check that every requirement has at least one tagged scenario.

Each feature file MUST declare which requirements file it covers via a comment:
  # requirements: requirements_runtime.yaml

Coverage is checked per-file — a feature can only satisfy requirements
from the file it declares.
"""

import glob
import re
import sys
import yaml

REQUIREMENTS_GLOB = ".spec/requirements_*.yaml"
FEATURES_DIR = "features/"

def main():
    # Load all requirements from all files
    req_files = sorted(glob.glob(REQUIREMENTS_GLOB))
    if not req_files:
        print("FAIL: No requirements files found")
        sys.exit(1)

    all_reqs = {}  # id → file
    for path in req_files:
        with open(path) as f:
            data = yaml.safe_load(f)
        basename = path.split("/")[-1]
        for r in data["requirements"]:
            rid = r["id"].lower().replace("_", "-")
            all_reqs[rid] = basename

    # Collect tags from feature files
    feature_files = sorted(glob.glob(f"{FEATURES_DIR}*.feature"))
    tagged = set()
    features_without_ref = []

    for path in feature_files:
        with open(path) as f:
            content = f.read()

        # Check for requirements file declaration
        ref_match = re.search(r"#\s*requirements:\s*(\S+)", content)
        if not ref_match:
            features_without_ref.append(path)

        # Collect all @req-* tags
        for match in re.finditer(r"@req-([a-z0-9-]+)", content):
            tagged.add(f"req-{match.group(1)}")

    # Compute coverage
    all_req_ids = set(all_reqs.keys())
    covered = all_req_ids & tagged
    uncovered = all_req_ids - tagged
    orphans = tagged - all_req_ids

    # Report per requirements file
    print("Coverage by requirements file:")
    for path in req_files:
        basename = path.split("/")[-1]
        file_reqs = {rid for rid, f in all_reqs.items() if f == basename}
        file_covered = file_reqs & tagged
        file_uncovered = file_reqs - tagged
        pct = int(100 * len(file_covered) / len(file_reqs)) if file_reqs else 0
        status = "PASS" if not file_uncovered else "FAIL"
        print(f"  {status}: {basename} — {len(file_covered)}/{len(file_reqs)} ({pct}%)")
        if file_uncovered:
            for r in sorted(file_uncovered):
                print(f"    uncovered: {r}")

    print(f"\nTotal: {len(covered)}/{len(all_req_ids)} requirements covered")
    print(f"Feature files: {len(feature_files)}")
    print(f"Tagged scenarios: {len(tagged)}")

    if features_without_ref:
        print(f"\nWarning: {len(features_without_ref)} feature files missing '# requirements: <file>' declaration:")
        for f in features_without_ref:
            print(f"  - {f}")

    if orphans:
        print(f"\nOrphan tags (no matching requirement):")
        for o in sorted(orphans):
            print(f"  - {o}")

    if uncovered:
        print(f"\nFAIL: {len(uncovered)} requirements have no scenarios")
        sys.exit(1)

    if orphans:
        print(f"\nFAIL: {len(orphans)} orphan tags found")
        sys.exit(1)

    print(f"\nOK: All {len(all_req_ids)} requirements have scenarios")

if __name__ == "__main__":
    main()
