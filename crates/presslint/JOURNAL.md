# presslint Journal

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
