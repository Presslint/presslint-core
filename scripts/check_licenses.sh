#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
metadata_file="$(mktemp "${TMPDIR:-/tmp}/presslint-cargo-metadata.XXXXXX")"
trap 'rm -f "$metadata_file"' EXIT

cd "$repo_root"
cargo metadata --format-version 1 --locked >"$metadata_file"

python3 - "$metadata_file" <<'PY'
import json
import re
import sys

metadata_path = sys.argv[1]

allowed = {
    "Apache-2.0",
    "MIT",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Zlib",
    "Unicode-3.0",
    "CC0-1.0",
}

forbidden_prefixes = (
    "AGPL",
    "GPL",
    "LGPL",
    "SSPL",
    "BUSL",
    "FSL",
    "PolyForm",
    "Commons-Clause",
)

operators = {"AND", "OR", "WITH"}

with open(metadata_path, "r", encoding="utf-8") as metadata_json:
    metadata = json.load(metadata_json)

workspace_members = set(metadata.get("workspace_members", []))
failures = []
checked = 0

for package in sorted(metadata.get("packages", []), key=lambda item: (item["name"], item["version"], item["id"])):
    package_id = package["id"]
    name = package["name"]
    version = package["version"]
    source = package.get("source")

    if source is None and package_id in workspace_members:
        continue

    if source is None:
        failures.append(f"{name} {version}: non-workspace path dependency is not allowed by this gate")
        continue

    checked += 1
    license_expr = package.get("license")
    if not license_expr:
        failures.append(f"{name} {version}: missing license metadata")
        continue

    identifiers = [
        token
        for token in re.split(r"[\s()]+", license_expr)
        if token and token not in operators
    ]

    if not identifiers:
        failures.append(f"{name} {version}: empty or unparseable license expression {license_expr!r}")
        continue

    for license_id in identifiers:
        if license_id in allowed:
            continue
        if license_id.startswith(forbidden_prefixes):
            failures.append(f"{name} {version}: forbidden license {license_id} in {license_expr!r}")
        else:
            failures.append(f"{name} {version}: unknown license {license_id} in {license_expr!r}")

if failures:
    print("dependency license check failed:", file=sys.stderr)
    for failure in failures:
        print(f"  - {failure}", file=sys.stderr)
    sys.exit(1)

print(f"dependency license check passed ({checked} third-party packages)")
PY
