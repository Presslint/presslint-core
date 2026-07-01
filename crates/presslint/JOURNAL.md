# presslint Journal

## T108 - Ablation

- Doc-accuracy only, no behavior change: the `PreflightReason::UnmodeledOrUnresolvedColorSpace`
  doc listed the review-severity color spaces but omitted `CalGray`, which the
  wildcard match arm already routes to review; added it so the doc matches the
  code. The `DeviceCmyk | DeviceGray => None` arm and every other arm are unchanged.
- Added `pure_gray_page_passes_with_no_findings`, a focused test that pins the
  previously-untested `DeviceGray` half of the `DeviceCmyk | DeviceGray => None`
  pass-compatible arm (the CMYK half was already covered), protecting that
  simplification from regression.

## T108 - Read-Only `check_no_rgb_in_print` Preflight Over PDF Inventory

- Added `check_no_rgb_in_print(input, max_decoded_stream_bytes) -> PreflightReport`
  in the new `preflight.rs` module: the first real user-facing prepress
  deliverable. It builds the neutral inventory with `build_pdf_inventory`
  verbatim, then scans `report.inventory.entries` once and applies a fixed
  color policy. This is READ-ONLY: it lives in the umbrella crate, NOT
  `presslint-actions`; it plans nothing and mutates nothing.
- New public surface: `PreflightReport { check, status, findings, inventory }`,
  `PreflightStatus` (`Pass | Fail | NeedsReview`), `PreflightCheck`
  (`NoRgbInPrint`), `PreflightSeverity` (`Error | Review`), `PreflightReason`
  (`RgbDeviceColor | UnmodeledOrUnresolvedColorSpace | CoverageIncomplete`),
  and `PreflightFinding`.
- Pass/review/fail partition (the check owns this policy, it is not a thin
  selector wrapper): `DeviceRgb` in any marking observation is the only
  `Error` (`RgbDeviceColor`) and forces `Fail`. `DeviceCmyk` and `DeviceGray`
  are pass-compatible and emit no finding. Every other observed `ColorSpace`
  (`IccBased`, `CalRgb`, `Lab`, `Indexed`, `Separation`, `DeviceN`, `Pattern`,
  `Resource(_)`, `Unknown` on a non-image observation) is a `Review`
  (`UnmodeledOrUnresolvedColorSpace`). A marking object with multiple
  observations (fill + stroke) is scanned per observation, one finding per
  offending observation, in observation order, so `usage`/`color_space` stay
  precise.
- Status aggregation is exactly: `Fail` if any `Error`; else `NeedsReview` if
  any `Review` finding OR coverage gap (all coverage gaps are `Review`
  severity); else `Pass`. A clean `Pass` means "no observed DeviceRGB in
  inventoried marking content AND no review/coverage blocker", subject to the
  recorded coverage limits — it does NOT claim "no RGB anywhere".
- Three coverage-honesty signals, all `CoverageIncomplete`/`Review`: (a) every
  page whose `PdfInventoryPageResult` is `Skipped`; (b) every image observation
  modeled as `Unknown` (image color is not decoded yet); (c) one signal per
  `FormXObject` entry, because nested form content is not walked so RGB inside a
  form is currently invisible.
- Coverage-finding representation: object-anchored fields
  (`object`, `entry_index`, `kind`, `usage`, `color_space`) are `Option` and
  populated only for entry-anchored findings. Per-object color findings and the
  image-`Unknown` coverage finding carry all of them; the form coverage finding
  carries `object`/`entry_index`/`kind` but no color observation
  (`usage`/`color_space` are `None`); a skipped-page coverage finding carries
  only the page.
- Determinism: `collect_findings` walks pages in document order and entries in
  content order in lockstep, using each `Inventoried { entry_count }` to bound a
  page's contiguous entry run, so skipped-page and per-object findings interleave
  in strict document/page/entry/observation order in a single pass.
- Copy budget: the full `PdfInventory` is moved into `PreflightReport.inventory`
  exactly once (scanned by borrow, never cloned; no matched-entry clones).
  Findings own only `Copy`/enum discriminants plus a cloned `ObjectId` (small,
  no source bytes) and a cloned `ColorSpace` (scalar, or the `Resource` name it
  already carries). `ColorObservation.components`, decoded streams, and PDF
  source bytes are never copied into findings.
- No new benchmark target: this is a build-once + scan-once aggregation over the
  already-timed inventory build path, the same shape as `query_pdf_inventory`;
  selector/inventory throughput is already covered by existing Criterion benches.
- Next queue after ACT: FORM (Form `XObject` recursion + ACT hardening so RGB
  inside forms is caught), then IMG (image `/Width`/`/Height`/`/BitsPerComponent`/
  `/ColorSpace`), then S-a, the Y2 design note + Y2, then F3 (#29) design notes.

## T107 - Page `XObject` Resources in Real-PDF Inventory

- `build_pdf_inventory` now runs the new page `XObject` resource inspector
  through the same `ObjectLookup` backend selected by `inspect_document_access`,
  then passes each page's classified image/form resource-name lists into the
  existing combined `presslint_inventory::build_inventory` path. Real page-scope
  `/Im Do` and `/Fm Do` operators now produce image and form inventory entries
  when the page resources classify them as `/Subtype /Image` or `/Subtype /Form`.
- `build_classic_pdf_inventory` gets the same behavior via the classic
  `inspect_document_page_xobject_resources` wrapper. A shared page helper still
  decodes/tokenizes/builds inventory once per page, now with caller-supplied
  image/form name slices.
- Resource inspection is non-fatal for text/vector inventory. If the document
  resource pass cannot begin, the report records `xobject_resource_error` and
  pages inventory with empty image/form lists. Per-page resource diagnostics are
  exposed as `xobject_resource_skipped` on each page report.
- Duplicate raw names in a page's direct `/XObject` dictionary are surfaced as
  page-local diagnostics by `presslint-pdf`; the bridge receives disjoint
  image/form resource-name lists, with the first duplicate occurrence winning
  deterministically.
- Copy budget: raw content streams stay borrowed, Flate streams allocate only
  bounded decoded buffers, multi-stream pages allocate only the bounded joined
  content buffer, and the new resource bridge converts small raw PDF resource
  names into shared inventory `PdfName` values. No PDF source bytes, object
  bodies, resource dictionaries, image streams, or decoded image data are
  retained.
- Deferred: no Form XObject content recursion, no image pixel/filter/color-space
  inspection, no indirect `/XObject` subdictionary support, and no object-stream
  resolution beyond the existing structural lookup behavior.

## T106 - Document-Level Selector Query Over PDF Inventory

- Added `query_pdf_inventory(input, selector, max_decoded_stream_bytes)` in the
  new `pdf_query.rs` module: the first end-to-end "query a real PDF" path. It
  reuses `build_pdf_inventory` verbatim for the neutral document/page path, then
  scans the merged, page-ordered `report.inventory.entries` once, calling the
  already-benchmarked `presslint_selectors::matches` per entry.
- New public report types `PdfInventoryQuery { report, matches }` and
  `PdfInventoryMatch { entry_index, page_index }`. `entry_index` is a stable
  index into `report.inventory.entries`; `page_index` is the matched entry's own
  `entry.id.page` (the zero-based document-order ordinal threaded by
  `build_pdf_inventory`). Both derive `Debug, Clone, PartialEq, Serialize,
  Deserialize`; `PdfInventoryMatch` additionally derives `Copy, Eq`. The query
  result stays `PartialEq`-only because `PdfInventory` carries float
  bounds/components and is not `Eq`.
- Index-not-clone contract: matched entries are never cloned into the result.
  `matches` holds only `PdfInventoryMatch { usize, PageIndex }` (both `Copy`) and
  the full report is moved into `PdfInventoryQuery.report` exactly once. No
  source bytes, decoded streams, or entry payloads are copied by the query.
- The query is a strict superset (build, then select), so top-level failures
  surface as the same `PdfInventoryError` as `build_pdf_inventory`, unchanged.
  Matches are pushed in ascending `entry_index` order.
- No new abstraction beyond the query result pair; no new selector predicate, no
  JSON parsing, no CLI. `build_pdf_inventory` / `build_classic_pdf_inventory`
  behavior and serde shapes are untouched.
- No new benchmark target: this slice is a build-once + scan-once composition
  with no new hot loop; selector-matching throughput is already covered by the
  `presslint-selectors` Criterion bench. Stated here per the performance note
  rather than adding a bench.

## T105 - Multi-Stream Page Content Inventory

- `build_page_inventory` now inventories pages with multiple located content
  streams when every stream is supported and decodable, so both
  `build_pdf_inventory` and `build_classic_pdf_inventory` get the behavior
  through the shared helper.
- Added the private `page_content` helper: a single raw stream still returns a
  borrowed source slice, while Flate streams allocate only their bounded decoded
  output and multi-stream pages allocate one bounded joined page-content buffer.
- Multi-stream joins insert an explicit whitespace byte between decoded streams
  before tokenization, and the remaining decode budget is enforced across the
  whole joined page content, including separators.
- Unsupported filters, target/extent failures, decode failures, tokenizer,
  assembler, and graphics-walk failures continue to surface as deterministic
  structured page skips. `MultipleContentStreams` remains in the public skip
  enums for serde compatibility, but decodable multi-stream pages no longer emit
  it.

## T104 - Classic Incremental-Update Inventory End-to-End

- `build_pdf_inventory` now inventories classic incrementally-updated PDFs
  end-to-end. The only change in this crate is one mechanical dispatch arm: the
  `match &access.backend` site maps the new
  `DocumentAccessBackend::ClassicXrefChain { chain }` to
  `ObjectLookup::ClassicXrefChain(chain)`, so a classic trailer carrying `/Prev`
  now navigates and inventories through the same neutral spine as the classic
  single-table, single-section xref-stream, and xref-stream `/Prev`-chain
  backends.
- A classic two-section fixture whose newest section redefines the page
  `/Contents` object is inventoried to the updated content stream, proving the
  newest-wins classic chain resolves the page content through the bridge.
- Copy budget is unchanged: raw streams stay borrowed, Flate streams allocate
  only the bounded decoded buffer, reports retain no PDF source or stream bytes,
  and no per-page object map/cache is built over `ObjectLookup`.
- Next queue: `#26` document-level inventory merge, then the Y2 design note (a
  third mixed-chain abstraction unifying the parallel classic and xref-stream
  `/Prev` chain builders as feeders).

## T095 - Classic PDF Inventory Bridge

- Added `build_classic_pdf_inventory`, the umbrella-crate bridge from borrowed
  classic-xref PDF bytes to combined page-object `Inventory`.
- Scope is deliberately narrow: a page is inventoried only when it has exactly
  one located content stream and that stream is raw or a single `/FlateDecode`
  with resolved non-array `/DecodeParms`.
- Unsupported page and stream shapes are reported as structured skips, including
  target/extent locate failures, unsupported filters, unsupported
  `/DecodeParms`, decode failures, tokenizer/assembler failures, and
  graphics-walk failures.
- Copy budget: raw streams remain borrowed slices; Flate streams allocate only
  the bounded decoded buffer returned by the existing decoder. The bridge does
  not concatenate multiple streams or retain source bytes in report records.

## T102 - Neutral PDF Inventory Bridge

- Added `build_pdf_inventory`, the umbrella-crate bridge from borrowed PDF bytes
  to combined page-object `Inventory` over either a classic xref table or one
  `/Type /XRef` stream section.
- The bridge calls `inspect_document_access`, selects `ObjectLookup` from the
  returned `DocumentAccessBackend`, and locates page content extents through
  `inspect_document_page_content_extents_with_lookup`.
- Shared the backend-independent page decode/tokenize/assemble/build path as a
  private helper used by both the classic and neutral bridges. The public
  `Classic*` report types and serde shapes are unchanged.
- Top-level neutral document-access failures are wrapped as structured
  `PdfInventoryRejection::DocumentAccess` errors, including the delegated
  `PrevPresentUnsupported` stop for xref-stream `/Prev`.
- Preserved the page-skip taxonomy for content failures, multi-stream pages,
  unresolved or compressed targets, unsupported filters, unsupported
  `/DecodeParms`, decode failures, tokenizer/assembler failures, and
  graphics-walk failures.
- Copy budget is unchanged from the classic bridge: raw streams stay borrowed,
  Flate streams allocate only the bounded decoded buffer, reports retain no PDF
  source or stream bytes, and no per-page object map/cache is built over
  `ObjectLookup`.
- Next queue after X: #28 TAIL (`/Prev` chaining plus multi-section merge),
  then #26, then F3 (#29, design-notes only).
