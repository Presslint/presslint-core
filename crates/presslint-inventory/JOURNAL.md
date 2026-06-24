# presslint-inventory Journal

## Current State

- Defines deterministic inventory and inventory-entry data contracts.
- Includes the first graphics-state walker over
  `presslint-syntax::OperatorRecord`.
- The walker emits ordered events with operator and record byte provenance.
- Supported state slice: `q`, `Q`, `cm`, device color operators (`G`, `g`,
  `RG`, `rg`, `K`, `k`), and basic path paint operators.
- Unsupported operators emit explicit no-op events.
- Structured errors cover graphics-state stack underflow, malformed operand
  counts, malformed numeric operands, non-finite numeric operands, and invalid
  source ranges.

## Follow-Ups

- Build Inventory slice 1 from path-paint events into vector inventory entries
  with color observations and edit capabilities.
- Do not create text/image/form/shading inventory before the vector slice is
  stable.
