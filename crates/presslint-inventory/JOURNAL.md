# presslint-inventory Journal

## Current State

- Defines deterministic inventory and inventory-entry data contracts.
- The crate root is a small public facade over focused internal modules for
  inventory builders, graphics-state walking, digest stability, operand
  parsing, and tests.
- Includes the first graphics-state walker over
  `presslint-syntax::OperatorRecord`.
- The walker emits ordered events with operator and record byte provenance.
- Supported state slice: `q`, `Q`, `cm`, device color operators (`G`, `g`,
  `RG`, `rg`, `K`, `k`), text rendering mode (`Tr`), basic path paint
  operators, first-slice text-showing operators (`Tj`, `TJ`, `'`, `"`), and
  XObject invocation (`Do`).
- Unsupported operators emit explicit no-op events.
- Structured errors cover graphics-state stack underflow, malformed operand
  counts, malformed numeric operands, non-finite numeric operands, and invalid
  source ranges.
- Builds the first vector inventory slice from supported path-paint events,
  carrying caller-provided page/content scope, path-paint byte provenance,
  observed stroke/fill colors, deterministic object IDs, and color-operand
  rewrite capability.
- `GraphicsDeviceColor` records the device-color operator's record byte range as
  the color `source`. The range is stamped when `G`/`g`/`RG`/`rg`/`K`/`k` set
  the color, travels with the saved snapshot across `q`/`Q`, and is surfaced on
  vector/text `ColorObservation`s so they point at the color operator rather
  than the paint/text-show operator. The page-default color and the synthesized
  image observation carry `None`. The stroking/nonstroking setters share a
  single `sourced_device_color` helper that resolves the operator and stamps its
  record range, keeping the source invariant in one place. Digest version tags
  were bumped to `presslint.vector.v2`/`.text.v2`/`.image.v2` to make the
  object-ID change explicit, and a locked digest test pins the new value.
- Builds the first text inventory slice from text-showing events, carrying
  caller-provided page/content scope, text-showing byte provenance, unset
  bounds, deterministic object IDs, and color observations for supported
  visible rendering modes.
- Supported visible text rendering modes advertise color-operand rewrite and
  text spread-stroke capabilities. Invisible text and unsupported text
  rendering modes remain represented but carry no color-edit capability.
- Builds the first image inventory slice from `Do` XObject invocation events.
  Image entries are emitted only for caller-declared image XObject resource
  names, carry caller-provided page/content scope and `Do` provenance, leave
  bounds unset, record an unknown image color observation, and advertise only
  read-only capability.
- Builds the first form XObject invocation inventory slice from the same `Do`
  events. Form entries are emitted only for caller-declared form XObject
  resource names, carry caller-provided page/content scope and `Do` provenance,
  leave bounds unset, synthesize no color observations, use a dedicated
  `presslint.form.v1` digest tag, and advertise only read-only capability.
- Adds a combined page-object inventory builder pair (`build_inventory` plus
  `inventory_from_graphics_events`) that walks the graphics-state events exactly
  once and merges the vector, text, image, and form slices into a single
  `Inventory` in content (event) order. One monotonic `sequence` counter is
  shared across all kinds, so the merged inventory is a single content-ordered
  identity space rather than four disjoint per-kind ones. Each merged entry's
  kind, provenance, colors, and capabilities equal what the matching per-kind
  builder would produce for the same event; only the global `sequence` (and
  therefore the digest) differs. `XObjectInvoke` names are classified image
  first, then form, so a name present in both the image and form lists (which
  are disjoint by contract) is classified as an image. The per-kind builders now
  share a single `collect_entries` walk plus per-event entry helpers, so the
  combined and per-kind paths construct entries from the same code and the
  existing per-kind builders keep identical signatures, behavior, and digests.
  The image and form entry helpers share a single `matched_xobject` lookup for
  the `Do`-name classification instead of duplicating the `XObjectInvoke` match
  and name-list check, with no change to the resolved name or any digest.
- Adds focused dependency-free serde shape tests for `Inventory` and
  `InventoryEntry`. The locked fixtures round-trip through an in-memory JSON
  harness and pin the public encoding of nested core inventory-report fields:
  object IDs, page indexes, provenance, content scopes, byte ranges, PDF names,
  bounds, color observations, color spaces/usages, object kinds, and edit
  capabilities. The fixture includes bounded vector output, sourced color
  provenance, and a read-only form-style entry with empty colors.

## Follow-Ups

- Do not create shading inventory before the text, vector, image, and form
  slices are stable.
- Add geometry/bounds only after path construction interpretation is designed.
- Add glyph decoding, font resource lookup, CMaps, shaping, and text geometry
  only after the text inventory provenance model is stable.
- Add page resource traversal, image stream inspection, image bounds, and image
  replacement only after the invocation-level image inventory model is stable.
- Add form stream recursion, page resource traversal, shared-object ownership
  analysis, and form geometry only after the invocation-level form inventory
  model is stable.
