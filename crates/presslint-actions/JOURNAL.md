# presslint-actions Journal

## Current State

- Defines serializable recipe, recipe-step, action payload, no-op patch-plan,
  and action-plan report data contracts.
- Plans recipes against an `Inventory` by evaluating selectors in inventory
  order with `presslint-selectors`.
- Reports matched targets and matched-but-skipped objects with structured
  reasons: `UnsupportedCapability` for entries lacking the action's required
  edit capability, and `NonProcessColor` for `ConvertColor` entries that
  advertise the rewrite capability but carry no process device color
  observation (`DeviceGray` / `DeviceRGB` / `DeviceCMYK`). Process-color
  entries without a color-operator source range are reported separately as
  `MissingColorSource`; entries with multiple sourced process-color operands
  are reported as `AmbiguousColorSource`.
- `ConvertColor` process eligibility is a pure planning diagnostic: it never
  converts color or rewrites operands. A target must advertise
  `RewriteColorOperand` and carry exactly one sourced process device color
  observation, with no unsourced process device observations. Non-process
  observations may coexist with that single sourced process operand.
  `SpreadText` and `MinimumStrokeWidth` stay capability-only.
- Actions are requests only; they do not mutate documents directly.
- Depends on `presslint-selectors` for selector data and `presslint-core` for
  object identities and edit capabilities.

## Follow-Ups

- Add the first executor only after patch byte serialization and mutation
  boundaries are designed.
