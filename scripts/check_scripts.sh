#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

failed=0

while IFS= read -r -d '' script; do
  if [[ ! -x "$script" ]]; then
    echo "script is not executable: ${script}" >&2
    failed=1
  fi
done < <(find scripts -maxdepth 1 -type f -name '*.sh' -print0 | sort -z)

if [[ "$failed" -ne 0 ]]; then
  exit 1
fi

echo "script permission check passed"
