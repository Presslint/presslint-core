# presslint-inventory Journal

## Current State

- Defines deterministic inventory and inventory-entry data contracts.
- Includes the first graphics-state walker over
  `presslint-syntax::OperatorRecord`.
- The walker emits ordered events with operator and record byte provenance.
- Supported state slice: `q`, `Q`, `cm`, device color operators (`G`, `g`,
  `RG`, `rg`, `K`, `k`), text rendering mode (`Tr`), basic path paint
  operators, and first-slice text-showing operators (`Tj`, `TJ`, `'`, `"`).
- Unsupported operators emit explicit no-op events.
- Structured errors cover graphics-state stack underflow, malformed operand
  counts, malformed numeric operands, non-finite numeric operands, and invalid
  source ranges.
- Builds the first vector inventory slice from supported path-paint events,
  carrying caller-provided page/content scope, path-paint byte provenance,
  observed stroke/fill colors, deterministic object IDs, and color-operand
  rewrite capability.
- Builds the first text inventory slice from text-showing events, carrying
  caller-provided page/content scope, text-showing byte provenance, unset
  bounds, deterministic object IDs, and color observations for supported
  visible rendering modes.
- Supported visible text rendering modes advertise color-operand rewrite and
  text spread-stroke capabilities. Invisible text and unsupported text
  rendering modes remain represented but carry no color-edit capability.

## Follow-Ups

- Do not create image/form/shading inventory before the text and vector slices
  are stable.
- Add geometry/bounds only after path construction interpretation is designed.
- Add glyph decoding, font resource lookup, CMaps, shaping, and text geometry
  only after the text inventory provenance model is stable.
