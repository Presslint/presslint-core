# presslint-pdf Journal

Older accumulated journal history lives in [JOURNAL-archive.md](JOURNAL-archive.md).

## Current State

### T088 - Bounded FlateDecode Stream Decoding

- Added a focused `/FlateDecode` stream helper that accepts borrowed compressed
  bytes plus explicit decode parameters and returns bounded owned decoded
  bytes for downstream tokenizer/inventory consumers.
- The helper uses pinned `miniz_oxide =0.9.1` with default features disabled
  and only the allocation feature enabled. Inflate uses the zlib-wrapped
  bounded API, so over-limit output becomes a structured rejection instead of
  unbounded allocation.
- Supported predictor cases are no predictor / `/Predictor 1`, TIFF Predictor
  2, and PNG predictors 10-15. Predictor failures are explicit for unsupported
  predictors, malformed parameters, row geometry mismatches, integer overflow,
  and unknown PNG filter bytes.
- The owned decoded buffer is intentional at this seam: decompression creates a
  new byte stream. Predictor reversal avoids an additional decoded copy; PNG
  rows are compacted in place after filter reversal.
- Non-goals remain unchanged: no xref streams, no filter arrays or chained
  filters, no additional PDF filters, no recompression or mutation, no object
  maps/caches/document opener, and no inventory/action/color work.

### T089 - Inspect Cross-Reference Stream Dictionary Geometry Fields

- Adds `inspect_xref_stream_dictionary(input, object_byte_offset)`, the first
  cross-reference-stream (`/Type /XRef`) slice. Given caller bytes and the byte
  offset of an indirect object (the offset `classify_xref_section` reports as
  `XrefSection::Stream`), it extracts the geometry fields a later step needs to
  slice the eventually-decoded entry table: `/Type` (must be `/XRef`), `/W` (the
  three field widths), `/Size`, and `/Index` (the subsection pairs).
- It delegates the object header and top-level entry spans to
  `inspect_indirect_object_dictionary`, reimplementing no header, body-token,
  dictionary-open, or entry-span scanning, and matches only the exact raw key
  bytes `/Type`, `/W`, `/Size`, and `/Index` the same way
  `inspect_classic_xref_trailer_root` matches `/Root` (one shared
  `unique_entry` helper reports missing as `Ok(None)`, exactly-one as
  `Ok(Some)`, and more-than-one as a duplicate-key error).
- `/Type` must be exactly one name value whose raw bytes are `/XRef`; a missing,
  duplicate, non-name, or non-`/XRef` `/Type` is a distinct rejection
  (`MissingType`, `DuplicateType`, `NonNameTypeValue`, `UnexpectedTypeName`).
- `/W` must be exactly one array value, located with `inspect_array_extent` and
  scanned by the one new bounded abstraction: a whitespace/comment-separated
  decimal-integer element scan over the located array extent. Exactly three
  non-negative integers are required and a width of `0` (omitted field) is
  accepted; missing, duplicate, non-array, malformed-array, malformed-element,
  width-overflow, and wrong-length cases are distinct rejections (`MissingW`,
  `DuplicateW`, `NonArrayWValue`, `MalformedWArray`, `MalformedWElement`,
  `WidthOutOfRange`, `WrongWLength`).
- `/Size` must be exactly one direct non-negative integer that fits `usize`;
  missing, duplicate, non-integer (any non-pure-digit value span, including an
  indirect `N G R` or a decimal), and out-of-range cases are distinct
  rejections (`MissingSize`, `DuplicateSize`, `NonIntegerSizeValue`,
  `SizeOutOfRange`).
- `/Index` is optional: when absent it defaults to a single `(0, Size)`
  subsection with `index_value_range` `None`; when present it must be one array
  of an even count of non-negative integers parsed as
  `(first_object_number, entry_count)` pairs, and a duplicate, non-array,
  malformed-array, malformed-element, odd-length, or integer-overflow `/Index`
  is a distinct rejection (`DuplicateIndex`, `NonArrayIndexValue`,
  `MalformedIndexArray`, `MalformedIndexElement`, `OddIndexLength`,
  `IndexIntegerOutOfRange`). Geometry is never fabricated when the key is
  present but malformed.
- `XrefStreamDictionaryInspection` carries the delegated
  `IndirectObjectDictionaryInspection`, the `/Type` key/value byte ranges, the
  `/W` value byte range and parsed `widths`, the `/Size` value byte range and
  parsed `size`, and the `/Index` value byte range (when present) plus the
  ordered `index_subsections`. It retains or copies no PDF bytes, object bodies,
  stream bodies, decoded bytes, or source slices; the only owned allocations are
  the three-element `widths` vector and the small `index_subsections` pair
  vector (the acceptable copy budget for bounded report materialization), so no
  benchmark was added.
- Every failure path is a distinct structured rejection variant and the helper
  never returns partial geometry on error. It lives in the new focused
  `xref_stream.rs` module, re-exported from `lib.rs`; tests live in
  `src/tests/xref_stream.rs`.
- A composition test chains `inspect_startxref -> classify_xref_section
  (== XrefSection::Stream) -> inspect_xref_stream_dictionary` over a synthetic
  xref-stream fixture and confirms `/Type /XRef`, the three `/W` widths,
  `/Size`, and the defaulted/explicit `/Index` subsections; a serde round-trip
  test pins the public JSON shape of the report and rejection enum.
- Non-goals for this slice: no decoding/inflating/reading of the cross-reference
  stream body bytes, no `/W`-width entry-record parsing or object offset map, no
  `/Root` or `/Prev` parsing, no `/Prev` following, incremental-section merging,
  or hybrid-reference (`/XRefStm`) support, no indirect-reference resolution,
  catalog/page-tree/`/Contents` reading, no stream-data extent location or
  `endstream` validation, and no filesystem I/O, document opener, caches, object
  maps, or whole-document eager parsing.
- Ablation (behavior-preserving): the four field-requirement helpers
  (`require_type`/`require_widths`/`require_size`/`require_index`) no longer take
  a generic `E: Fn(..) -> Error + Copy` error closure with the same four-line
  bound repeated verbatim; they take a small `Copy` `ErrorContext` struct whose
  `error(reason, offset)` method builds the rejection. This removes the generic
  parameter from every helper, the closure built in
  `inspect_xref_stream_dictionary`, the free `xref_stream_error` constructor, and
  the four single-use `duplicate_*` variant wrappers (their duplicate-key ranges
  are now destructured inline at each call site). No public type, field, serde
  shape, rejection variant, error offset, or behavior changed; all `xref_stream`
  tests, the full `presslint-pdf` suite, `cargo check --workspace --all-targets`,
  clippy, and `./scripts/ci_check.sh` pass unchanged.

## Follow-Ups

- Next C slices: parse `/Root`/`/Prev` from the xref-stream dictionary, then
  decode the stream body (via the T088 FlateDecode helper) and slice it into
  `/W`-width entry records over the `/Index` subsections.
