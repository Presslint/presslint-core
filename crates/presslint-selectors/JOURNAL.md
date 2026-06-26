# presslint-selectors Journal

## Current State

- Defines serializable boolean selector expressions and leaf predicates.
- Current predicates cover object kind, observed color space, page index,
  edit capability, and content scope.
- Provides an in-memory matcher over `presslint_inventory::InventoryEntry`.
- The content-scope predicate matches by full `ContentScope` equality against
  `entry.provenance.scope`, including the form `XObject` resource name.
- Focused serde tests lock the public JSON shape for selector boolean variants
  and predicate fixtures, including page, named form-XObject, and annotation
  appearance scope fixtures.
- Tests live in a `tests` submodule split across files: `tests.rs` holds the
  shape and matcher tests, and `tests/json.rs` holds the test-only in-memory
  JSON serde harness, keeping `lib.rs` focused on production code and under the
  file-size gate.

## Follow-Ups

- Keep selector JSON compatibility explicit when adding future predicates or
  consumer-facing recipes.
- A categorical "any form regardless of name" scope matcher is intentionally
  deferred to avoid a premature shared scope-kind discriminant; revisit only
  when a consumer needs it.
