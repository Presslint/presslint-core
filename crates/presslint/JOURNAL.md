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
