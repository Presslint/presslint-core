#!/usr/bin/env bash
set -euo pipefail

scripts/check_licenses.sh
scripts/check_scripts.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
