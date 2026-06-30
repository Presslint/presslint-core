//! End-to-end bridge from a located content-stream extent to tokenized bytes.
//!
//! These tests exercise the thin `content_stream_data_slice` helper directly
//! (valid window plus inverted and out-of-bounds errors) and then prove the
//! whole classic-xref navigation chain has concrete meaning: a synthetic
//! uncompressed single-content-stream PDF is walked to its first located page
//! extent, bridged to a borrowed slice, tokenized with `presslint-syntax`, and
//! shown to round-trip byte-identically and to carry real content operators.

use presslint_syntax::{TokenKind, serialize_unmodified, tokenize};

use crate::{
    ContentStreamDataExtentInspection, ContentStreamDataSliceRejection,
    DocumentPageContentExtentResult, DocumentPageContentExtentsInspection, FlateDecodeParameters,
    PageContentExtentInspection, content_stream_data_slice, decode_flate_stream,
    inspect_catalog_pages, inspect_classic_xref_table, inspect_classic_xref_trailer_root,
    inspect_content_stream_data_extent, inspect_document_page_content_extents,
    inspect_page_tree_reference_target,
};
use miniz_oxide::deflate::compress_to_vec_zlib;

const PDF_PREFIX: &[u8] = b"%PDF-1.7\n";

/// A minimal object holding a five-byte raw `hello` content stream, shared by
/// the direct helper unit tests so they all bridge the same source window.
const HELLO_SLICE_SOURCE: &[u8] =
    b"%PDF-1.7\n4 0 obj\n<< /Length 5 >>\nstream\nhello\nendstream\nendobj\n";

/// Build a synthetic uncompressed single-content-stream classic-xref PDF.
///
/// The content stream is raw (no `/Filter`) so its located bytes are directly
/// tokenizable, and its `/Length` is a direct integer equal to `content_data`.
fn single_content_stream_pdf(content_data: &[u8]) -> Vec<u8> {
    let catalog = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
    let pages = b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>\nendobj\n";
    let page = b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>\nendobj\n";

    let mut content = Vec::new();
    content.extend_from_slice(b"4 0 obj\n<< /Length ");
    content.extend_from_slice(content_data.len().to_string().as_bytes());
    content.extend_from_slice(b" >>\nstream\n");
    content.extend_from_slice(content_data);
    content.extend_from_slice(b"\nendstream\nendobj\n");

    let mut source = Vec::new();
    source.extend_from_slice(PDF_PREFIX);
    let catalog_offset = source.len();
    source.extend_from_slice(catalog);
    let pages_offset = source.len();
    source.extend_from_slice(pages);
    let page_offset = source.len();
    source.extend_from_slice(page);
    let content_offset = source.len();
    source.extend_from_slice(&content);

    let xref_offset = source.len();
    source.extend_from_slice(b"xref\n0 5\n");
    source.extend_from_slice(b"0000000000 65535 f \n");
    for offset in [catalog_offset, pages_offset, page_offset, content_offset] {
        source.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    source.extend_from_slice(
        format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );
    source
}

/// Build a synthetic single-content-stream PDF whose content stream uses a
/// single `/FlateDecode` filter.
fn single_flate_content_stream_pdf(decoded_content: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let compressed = compress_to_vec_zlib(decoded_content, 6);
    let catalog = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
    let pages = b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>\nendobj\n";
    let page = b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>\nendobj\n";

    let mut content = Vec::new();
    content.extend_from_slice(b"4 0 obj\n<< /Length ");
    content.extend_from_slice(compressed.len().to_string().as_bytes());
    content.extend_from_slice(b" /Filter /FlateDecode >>\nstream\n");
    content.extend_from_slice(&compressed);
    content.extend_from_slice(b"\nendstream\nendobj\n");

    let mut source = Vec::new();
    source.extend_from_slice(PDF_PREFIX);
    let catalog_offset = source.len();
    source.extend_from_slice(catalog);
    let pages_offset = source.len();
    source.extend_from_slice(pages);
    let page_offset = source.len();
    source.extend_from_slice(page);
    let content_offset = source.len();
    source.extend_from_slice(&content);

    let xref_offset = source.len();
    source.extend_from_slice(b"xref\n0 5\n");
    source.extend_from_slice(b"0000000000 65535 f \n");
    for offset in [catalog_offset, pages_offset, page_offset, content_offset] {
        source.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    source.extend_from_slice(
        format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );
    (source, compressed)
}

/// Inspect a direct-length content-stream extent for an object placed right
/// after [`PDF_PREFIX`].
fn direct_length_extent(source: &[u8]) -> ContentStreamDataExtentInspection {
    inspect_content_stream_data_extent(source, None, PDF_PREFIX.len())
        .expect("direct-length extent should inspect")
}

/// Replace an extent's reported stream-data offsets to craft error fixtures.
fn with_offsets(
    mut extent: ContentStreamDataExtentInspection,
    start: usize,
    end: usize,
) -> ContentStreamDataExtentInspection {
    match &mut extent {
        ContentStreamDataExtentInspection::DirectLength(report) => {
            report.stream_data_start_byte_offset = start;
            report.stream_data_end_byte_offset = end;
        }
        ContentStreamDataExtentInspection::IndirectLength(report) => {
            report.stream_data_start_byte_offset = start;
            report.stream_data_end_byte_offset = end;
        }
    }
    extent
}

/// Navigate a parsed report to its first fully located page extent.
fn first_located_extent(
    report: &DocumentPageContentExtentsInspection,
) -> Option<&ContentStreamDataExtentInspection> {
    let page = report.pages.first()?;
    let DocumentPageContentExtentResult::Inspected { extents, .. } = &page.result else {
        return None;
    };
    match extents.entries.first()? {
        PageContentExtentInspection::Located { extent, .. } => Some(extent),
        _ => None,
    }
}

#[test]
fn content_stream_data_slice_returns_exact_window() {
    let source = HELLO_SLICE_SOURCE;
    let extent = direct_length_extent(source);

    let slice = content_stream_data_slice(source, &extent).expect("valid extent should bridge");

    assert_eq!(slice, b"hello");
    assert_eq!(
        slice,
        &source[extent.stream_data_start_byte_offset()..extent.stream_data_end_byte_offset()]
    );
}

#[test]
fn content_stream_data_slice_rejects_inverted_extent() {
    let source = HELLO_SLICE_SOURCE;
    let extent = with_offsets(direct_length_extent(source), 10, 5);

    let error =
        content_stream_data_slice(source, &extent).expect_err("inverted extent should reject");

    assert_eq!(
        error.reason,
        ContentStreamDataSliceRejection::InvertedExtent
    );
    assert_eq!(error.start_byte_offset, 10);
    assert_eq!(error.end_byte_offset, 5);
    assert_eq!(error.byte_len, source.len());
}

#[test]
fn content_stream_data_slice_rejects_out_of_bounds_extent() {
    let source = HELLO_SLICE_SOURCE;
    let out_of_bounds_end = source.len() + 1;
    let extent = with_offsets(direct_length_extent(source), 0, out_of_bounds_end);

    let error =
        content_stream_data_slice(source, &extent).expect_err("out-of-bounds extent should reject");

    assert_eq!(
        error.reason,
        ContentStreamDataSliceRejection::EndOutOfBounds
    );
    assert_eq!(error.start_byte_offset, 0);
    assert_eq!(error.end_byte_offset, out_of_bounds_end);
    assert_eq!(error.byte_len, source.len());
}

#[test]
fn bridges_located_extent_to_tokenizable_content_stream() {
    let content_data = b"q\n0 0 1 rg\n12 12 80 80 re\nf\nQ";
    let source = single_content_stream_pdf(content_data);

    let xref_offset = source
        .windows(b"xref".len())
        .position(|window| window == b"xref")
        .expect("classic xref keyword present");
    let xref = inspect_classic_xref_table(&source, xref_offset).expect("xref table should inspect");
    let root = inspect_classic_xref_trailer_root(&source, xref.trailer_byte_offset)
        .expect("trailer /Root should inspect");
    let catalog = inspect_page_tree_reference_target(&source, &xref, root.root_reference)
        .expect("catalog should resolve");
    let catalog_pages = inspect_catalog_pages(&source, catalog.object_byte_offset)
        .expect("catalog /Pages should inspect");
    let pages = inspect_page_tree_reference_target(&source, &xref, catalog_pages.pages_reference)
        .expect("page tree root should resolve");
    let report = inspect_document_page_content_extents(&source, &xref, pages.object_byte_offset)
        .expect("document page content extents should inspect");

    assert_eq!(report.page_count(), 1);
    assert_eq!(report.located_page_count(), 1);

    let extent = first_located_extent(&report).expect("first page extent should be located");
    let slice = content_stream_data_slice(&source, extent).expect("located extent should bridge");

    // The bridged window is exactly the raw content-stream data: no `stream` /
    // `endstream` framing leaked in, so the located bytes are not an off-by-one.
    assert_eq!(slice, content_data);
    assert!(slice.starts_with(b"q"));
    assert!(slice.ends_with(b"Q"));
    assert!(!slice.windows(b"endstream".len()).any(|w| w == b"endstream"));

    // parse -> serialize round-trips byte-identically on the located bytes.
    let tokens = tokenize(slice).expect("content stream should tokenize");
    assert_eq!(serialize_unmodified(slice), slice);

    // The located bytes carry the real content operators placed in the stream.
    let operators = tokens
        .iter()
        .filter(|token| token.kind == TokenKind::Operator)
        .filter_map(|token| token.source_bytes(slice))
        .collect::<Vec<_>>();
    assert!(operators.contains(&&b"f"[..]), "paint operator `f` present");
    assert!(
        operators.contains(&&b"re"[..]),
        "rectangle operator present"
    );
    assert!(
        operators.contains(&&b"rg"[..]),
        "fill-color operator present"
    );
}

#[test]
fn bridges_located_flate_extent_to_decoded_tokenizable_content_stream() {
    let content_data = b"q\n0 0 1 rg\n12 12 80 80 re\nf\nQ";
    let (source, compressed) = single_flate_content_stream_pdf(content_data);

    let xref_offset = source
        .windows(b"xref".len())
        .position(|window| window == b"xref")
        .expect("classic xref keyword present");
    let xref = inspect_classic_xref_table(&source, xref_offset).expect("xref table should inspect");
    let root = inspect_classic_xref_trailer_root(&source, xref.trailer_byte_offset)
        .expect("trailer /Root should inspect");
    let catalog = inspect_page_tree_reference_target(&source, &xref, root.root_reference)
        .expect("catalog should resolve");
    let catalog_pages = inspect_catalog_pages(&source, catalog.object_byte_offset)
        .expect("catalog /Pages should inspect");
    let pages = inspect_page_tree_reference_target(&source, &xref, catalog_pages.pages_reference)
        .expect("page tree root should resolve");
    let report = inspect_document_page_content_extents(&source, &xref, pages.object_byte_offset)
        .expect("document page content extents should inspect");

    let extent = first_located_extent(&report).expect("first page extent should be located");
    let slice = content_stream_data_slice(&source, extent).expect("located extent should bridge");
    assert_eq!(slice, compressed);

    let decoded = decode_flate_stream(slice, FlateDecodeParameters::default(), 1024)
        .expect("compressed content stream should decode");
    let tokens = tokenize(&decoded).expect("decoded content stream should tokenize");

    assert_eq!(decoded, content_data);
    assert_eq!(serialize_unmodified(&decoded), decoded);
    assert!(tokens.iter().any(|token| {
        token.kind == TokenKind::Operator && token.source_bytes(&decoded) == Some(&b"rg"[..])
    }));
}
