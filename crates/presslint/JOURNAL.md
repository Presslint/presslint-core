# presslint Journal

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
