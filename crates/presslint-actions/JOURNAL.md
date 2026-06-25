# presslint-actions Journal

## Current State

- Defines serializable recipe, recipe-step, action payload, no-op patch-plan,
  and action-plan report data contracts.
- Plans recipes against an `Inventory` by evaluating selectors in inventory
  order with `presslint-selectors`.
- Reports matched targets and matched-but-skipped objects with structured
  unsupported-capability reasons.
- Actions are requests only; they do not mutate documents directly.
- Depends on `presslint-selectors` for selector data and `presslint-core` for
  object identities and edit capabilities.

## Follow-Ups

- Add the first executor only after patch byte serialization and mutation
  boundaries are designed.
