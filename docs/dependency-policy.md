# Dependency Policy

PressLint Core keeps its dependency set compatible with Apache-2.0
distribution and suitable for public open-source use. New dependencies should
be small, maintained, deterministic in output-sensitive paths, and licensed
under terms listed here.

## Allowed License Families

Third-party Rust dependencies may use these license families:

- Apache-2.0
- MIT
- BSD-2-Clause
- BSD-3-Clause
- ISC
- Zlib
- Unicode-3.0
- CC0-1.0

Dual-licensed dependencies are accepted when every license identifier in the
Cargo metadata expression is in the allowed set. Workspace path packages are
internal and are not checked by the third-party gate.

## Forbidden License Families

Do not add dependencies whose Cargo metadata includes these license families:

- AGPL
- GPL
- LGPL
- SSPL
- BUSL
- FSL
- PolyForm
- Commons-Clause
- source-available or non-commercial-only terms

Dependencies with missing, unclear, custom, or unknown license metadata are
rejected until their licensing is reviewed and this policy is updated.

## Local Check

Run the license gate before submitting changes:

```bash
scripts/check_licenses.sh
```

The script reads `cargo metadata --format-version 1 --locked`, skips workspace
path packages, and fails closed for third-party packages with missing,
forbidden, or unlisted license identifiers.
