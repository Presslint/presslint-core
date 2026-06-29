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

token_pattern = re.compile(r"\(|\)|\bAND\b|\bOR\b|\bWITH\b|[A-Za-z0-9][A-Za-z0-9.+-]*(?:/[A-Za-z0-9][A-Za-z0-9.+-]*)*")


class LicenseParseError(ValueError):
    pass


def tokenize_license_expression(expression):
    tokens = token_pattern.findall(expression)
    if "".join(tokens) != re.sub(r"\s+", "", expression):
        raise LicenseParseError("contains unsupported characters")
    return tokens


def forbidden_identifiers(tokens):
    identifiers = []
    for token in tokens:
        if token in {"(", ")", "AND", "OR", "WITH"}:
            continue
        identifiers.extend(token.split("/"))
    return [
        license_id
        for license_id in identifiers
        if license_id.startswith(forbidden_prefixes)
    ]


class LicenseParser:
    def __init__(self, tokens):
        self.tokens = tokens
        self.position = 0

    def parse(self):
        if not self.tokens:
            raise LicenseParseError("empty expression")
        result = self.parse_or()
        if self.position != len(self.tokens):
            raise LicenseParseError(f"unexpected token {self.tokens[self.position]!r}")
        return result

    def parse_or(self):
        result = self.parse_and()
        while self.match("OR"):
            right = self.parse_and()
            result = result or right
        return result

    def parse_and(self):
        result = self.parse_atom()
        while self.match("AND"):
            right = self.parse_atom()
            result = result and right
        return result

    def parse_atom(self):
        if self.match("("):
            result = self.parse_or()
            if not self.match(")"):
                raise LicenseParseError("missing closing parenthesis")
            return result

        token = self.advance()
        if token is None:
            raise LicenseParseError("unexpected end of expression")
        if token in {")", "AND", "OR"}:
            raise LicenseParseError(f"unexpected token {token!r}")
        if token == "WITH":
            raise LicenseParseError("WITH exceptions are not supported")

        result = any(license_id in allowed for license_id in token.split("/"))

        if self.match("WITH"):
            raise LicenseParseError("WITH exceptions are not supported")

        return result

    def match(self, expected):
        if self.position < len(self.tokens) and self.tokens[self.position] == expected:
            self.position += 1
            return True
        return False

    def advance(self):
        if self.position >= len(self.tokens):
            return None
        token = self.tokens[self.position]
        self.position += 1
        return token

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

    try:
        tokens = tokenize_license_expression(license_expr)
    except LicenseParseError as error:
        failures.append(f"{name} {version}: unparseable license expression {license_expr!r}: {error}")
        continue

    forbidden = forbidden_identifiers(tokens)
    if forbidden:
        for license_id in forbidden:
            failures.append(f"{name} {version}: forbidden license {license_id} in {license_expr!r}")
        continue

    try:
        if not LicenseParser(tokens).parse():
            failures.append(f"{name} {version}: no allowed license branch in {license_expr!r}")
    except LicenseParseError as error:
        failures.append(f"{name} {version}: unparseable license expression {license_expr!r}: {error}")

if failures:
    print("dependency license check failed:", file=sys.stderr)
    for failure in failures:
        print(f"  - {failure}", file=sys.stderr)
    sys.exit(1)

print(f"dependency license check passed ({checked} third-party packages)")
PY
