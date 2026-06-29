mod serde_harness;

use serde_harness::{from_serde_value, serde_value};

use crate::{
    ClassicXrefEntry, ClassicXrefEntryState, ClassicXrefSubsection, ClassicXrefTableInspection,
    ContentStreamDataExtentInspection, ContentStreamDataExtentInspectionError,
    ContentStreamDataExtentInspectionRejection, ContentStreamStartInspectionRejection,
    DictionaryValueKind, DirectLengthContentStreamDataExtentInspectionRejection,
    IndirectLengthContentStreamDataExtentInspectionRejection, IndirectRef,
    PageContentTargetInspection, StreamEolIssue, inspect_catalog_pages, inspect_classic_xref_table,
    inspect_classic_xref_trailer_root, inspect_content_stream_data_extent,
    inspect_direct_length_content_stream_data_extent,
    inspect_indirect_length_content_stream_data_extent, inspect_page_content_targets,
    inspect_page_contents, inspect_page_tree_kids, inspect_page_tree_reference_target,
};

struct IndirectLengthFixture {
    source: Vec<u8>,
    xref_table: ClassicXrefTableInspection,
    content_offset: usize,
}

fn indirect_length_fixture(stream_bytes: &[u8], declared_length: &[u8]) -> IndirectLengthFixture {
    let prefix = b"%PDF-1.7\n";
    let content_offset = prefix.len();
    let length_object_prefix = format!(
        "8 0 obj\n{}\nendobj\n",
        String::from_utf8_lossy(declared_length)
    )
    .into_bytes();
    let content = format!(
        "5 0 obj\n<< /Length 8 0 R >>\nstream\n{}\nendstream\nendobj\n",
        String::from_utf8_lossy(stream_bytes)
    )
    .into_bytes();
    let length_offset = prefix.len() + content.len();

    let mut source = Vec::new();
    source.extend_from_slice(prefix);
    source.extend_from_slice(&content);
    source.extend_from_slice(&length_object_prefix);

    let xref_table = ClassicXrefTableInspection {
        table_byte_offset: source.len(),
        subsections: vec![ClassicXrefSubsection {
            first_object_number: 0,
            entry_count: 9,
            entries: vec![
                classic_entry(0, 65535, 0, ClassicXrefEntryState::Free),
                classic_entry(1, 0, 0, ClassicXrefEntryState::Free),
                classic_entry(2, 0, 0, ClassicXrefEntryState::Free),
                classic_entry(3, 0, 0, ClassicXrefEntryState::Free),
                classic_entry(4, 0, 0, ClassicXrefEntryState::Free),
                classic_entry(5, 0, content_offset, ClassicXrefEntryState::InUse),
                classic_entry(6, 0, 0, ClassicXrefEntryState::Free),
                classic_entry(7, 0, 0, ClassicXrefEntryState::Free),
                classic_entry(8, 0, length_offset, ClassicXrefEntryState::InUse),
            ],
        }],
        trailer_byte_offset: 0,
    };

    IndirectLengthFixture {
        source,
        xref_table,
        content_offset,
    }
}

fn classic_entry(
    object_number: u32,
    generation: u16,
    byte_offset: usize,
    state: ClassicXrefEntryState,
) -> ClassicXrefEntry {
    ClassicXrefEntry {
        object_number,
        generation,
        byte_offset,
        state,
    }
}

#[test]
fn content_stream_extent_dispatches_direct_length_like_focused_helper() {
    let source = b"5 0 obj\n<< /Length 12 >>\nstream\nhello world!\nendstream\nendobj\n";

    let combined = inspect_content_stream_data_extent(source, None, 0)
        .expect("combined direct-length stream extent should inspect");
    let focused = inspect_direct_length_content_stream_data_extent(source, 0)
        .expect("focused direct-length stream extent should inspect");

    assert_eq!(
        combined,
        ContentStreamDataExtentInspection::DirectLength(focused.clone())
    );
    assert_eq!(combined.length(), focused.length);
    assert_eq!(
        combined.stream_data_start_byte_offset(),
        focused.stream_data_start_byte_offset
    );
    assert_eq!(
        combined.stream_data_end_byte_offset(),
        focused.stream_data_end_byte_offset
    );
}

#[test]
fn content_stream_extent_dispatches_indirect_length_like_focused_helper() {
    let fixture = indirect_length_fixture(b"ABCDEFG", b"7");

    let combined = inspect_content_stream_data_extent(
        &fixture.source,
        Some(&fixture.xref_table),
        fixture.content_offset,
    )
    .expect("combined indirect-length stream extent should inspect");
    let focused = inspect_indirect_length_content_stream_data_extent(
        &fixture.source,
        &fixture.xref_table,
        fixture.content_offset,
    )
    .expect("focused indirect-length stream extent should inspect");

    assert_eq!(
        combined,
        ContentStreamDataExtentInspection::IndirectLength(focused.clone())
    );
    assert_eq!(combined.length(), focused.length);
    assert_eq!(
        combined.stream_data_start_byte_offset(),
        focused.stream_data_start_byte_offset
    );
    assert_eq!(
        combined.stream_data_end_byte_offset(),
        focused.stream_data_end_byte_offset
    );
}

#[test]
fn content_stream_extent_rejects_indirect_length_without_xref_table() {
    let fixture = indirect_length_fixture(b"ABCDEFG", b"7");

    let error = inspect_content_stream_data_extent(&fixture.source, None, fixture.content_offset)
        .expect_err("indirect length without xref table should reject");

    assert_eq!(
        error.reason,
        ContentStreamDataExtentInspectionRejection::IndirectLengthRequiresXrefTable
    );
}

#[test]
fn content_stream_extent_rejects_unsupported_length_value_kinds() {
    for (source, value_kind) in [
        (
            &b"5 0 obj\n<< /Length /Seven >>\nstream\nABCDEFG\nendstream\nendobj\n"[..],
            DictionaryValueKind::Name,
        ),
        (
            &b"5 0 obj\n<< /Length [ 7 ] >>\nstream\nABCDEFG\nendstream\nendobj\n"[..],
            DictionaryValueKind::Array,
        ),
    ] {
        let error = inspect_content_stream_data_extent(source, None, 0)
            .expect_err("unsupported length kind should reject");

        assert_eq!(
            error.reason,
            ContentStreamDataExtentInspectionRejection::UnsupportedLengthValueKind { value_kind }
        );
    }
}

#[test]
fn content_stream_extent_rejects_missing_and_duplicate_length() {
    let missing = b"5 0 obj\n<< /Other 7 >>\nstream\nABCDEFG\nendstream\nendobj\n";

    let missing_error = inspect_content_stream_data_extent(missing, None, 0)
        .expect_err("missing length should reject");
    assert_eq!(
        missing_error.reason,
        ContentStreamDataExtentInspectionRejection::MissingLength
    );

    let duplicate =
        b"5 0 obj\n<< /Length 7 /Other 0 /Length 7 >>\nstream\nABCDEFG\nendstream\nendobj\n";

    let duplicate_error = inspect_content_stream_data_extent(duplicate, None, 0)
        .expect_err("duplicate length should reject");
    assert!(matches!(
        duplicate_error.reason,
        ContentStreamDataExtentInspectionRejection::DuplicateLength { .. }
    ));
}

#[test]
fn content_stream_extent_propagates_stream_start_failure() {
    let source = b"5 0 obj\n<< /Length 1 >>\nstream\rX\nendstream\nendobj\n";

    let error = inspect_content_stream_data_extent(source, None, 0)
        .expect_err("invalid stream-start EOL should reject");

    assert_eq!(
        error.reason,
        ContentStreamDataExtentInspectionRejection::StreamStart {
            stream_start_reason: ContentStreamStartInspectionRejection::InvalidStreamEol {
                eol_issue: StreamEolIssue::LoneCarriageReturn,
            },
        }
    );
}

#[test]
fn content_stream_extent_propagates_delegated_direct_failure() {
    let source = b"5 0 obj\n<< /Length 99 >>\nstream\nX\nendstream\nendobj\n";

    let error = inspect_content_stream_data_extent(source, None, 0)
        .expect_err("focused direct helper failure should be wrapped");

    assert_eq!(
        error.reason,
        ContentStreamDataExtentInspectionRejection::DirectLength {
            direct_length_reason:
                DirectLengthContentStreamDataExtentInspectionRejection::StreamDataEndOutOfBounds,
        }
    );
}

#[test]
fn content_stream_extent_propagates_delegated_indirect_failure() {
    let fixture = indirect_length_fixture(b"ABCDEFG", b"99");

    let error = inspect_content_stream_data_extent(
        &fixture.source,
        Some(&fixture.xref_table),
        fixture.content_offset,
    )
    .expect_err("focused indirect helper failure should be wrapped");

    assert_eq!(
        error.reason,
        ContentStreamDataExtentInspectionRejection::IndirectLength {
            indirect_length_reason:
                IndirectLengthContentStreamDataExtentInspectionRejection::StreamDataEndOutOfBounds,
        }
    );
}

#[test]
fn content_stream_extent_debug_report_does_not_retain_source_bytes() {
    let source = b"5 0 obj\n<< /Secret (do-not-copy) /Length 14 >>\nstream\nsecret-payload\nendstream\nendobj\n";

    let report = inspect_content_stream_data_extent(source, None, 0)
        .expect("combined extent should inspect");

    let debug_report = format!("{report:?}");
    assert!(!debug_report.contains("secret-payload"));
    assert!(!debug_report.contains("do-not-copy"));
    assert!(!debug_report.contains("Secret"));
}

#[test]
fn content_stream_extent_serde_round_trips_success_and_rejection_shapes() {
    let source = b"5 0 obj\n<< /Length 3 >>\nstream\nabc\nendstream\nendobj\n";
    let report = inspect_content_stream_data_extent(source, None, 0)
        .expect("combined extent should inspect");
    let serialized_report = serde_value(&report).expect("success report should serialize");
    let round_tripped_report: ContentStreamDataExtentInspection =
        from_serde_value(serialized_report).expect("success report should deserialize");
    assert_eq!(round_tripped_report, report);

    let error = inspect_content_stream_data_extent(
        b"5 0 obj\n<< /Length /Bad >>\nstream\nabc\nendstream\nendobj\n",
        None,
        0,
    )
    .expect_err("unsupported length kind should reject");
    let serialized_error = serde_value(&error).expect("rejection error should serialize");
    let round_tripped_error: ContentStreamDataExtentInspectionError =
        from_serde_value(serialized_error).expect("rejection error should deserialize");
    assert_eq!(round_tripped_error, error);
}

struct SinglePageContentFixture {
    source: Vec<u8>,
    direct_content_offset: usize,
    indirect_content_offset: usize,
    direct_content_reference: IndirectRef,
    indirect_content_reference: IndirectRef,
}

fn single_page_content_fixture() -> SinglePageContentFixture {
    let prefix = b"%PDF-1.7\n";
    let catalog = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
    let pages = b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>\nendobj\n";
    let page = b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents [ 5 0 R 6 0 R ] >>\nendobj\n";
    let direct_content = b"5 0 obj\n<< /Length 11 >>\nstream\nABCDEFGHIJK\nendstream\nendobj\n";
    let indirect_content = b"6 0 obj\n<< /Length 8 0 R >>\nstream\nLMNOPQR\nendstream\nendobj\n";
    let length_object = b"8 0 obj\n7\nendobj\n";
    let catalog_offset = prefix.len();
    let pages_offset = catalog_offset + catalog.len();
    let page_offset = pages_offset + pages.len();
    let direct_content_offset = page_offset + page.len();
    let indirect_content_offset = direct_content_offset + direct_content.len();
    let length_offset = indirect_content_offset + indirect_content.len();
    let xref_offset = length_offset + length_object.len();
    let source = format!(
        "{}{}{}{}{}{}{}xref\n0 9\n0000000000 65535 f \n{catalog_offset:010} 00000 n \n{pages_offset:010} 00000 n \n{page_offset:010} 00000 n \n0000000000 00000 f \n{direct_content_offset:010} 00000 n \n{indirect_content_offset:010} 00000 n \n0000000000 00000 f \n{length_offset:010} 00000 n \ntrailer\n<< /Size 9 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
        String::from_utf8_lossy(prefix),
        String::from_utf8_lossy(catalog),
        String::from_utf8_lossy(pages),
        String::from_utf8_lossy(page),
        String::from_utf8_lossy(direct_content),
        String::from_utf8_lossy(indirect_content),
        String::from_utf8_lossy(length_object),
    )
    .into_bytes();

    SinglePageContentFixture {
        source,
        direct_content_offset,
        indirect_content_offset,
        direct_content_reference: IndirectRef {
            object_number: 5,
            generation: 0,
        },
        indirect_content_reference: IndirectRef {
            object_number: 6,
            generation: 0,
        },
    }
}

#[test]
fn content_stream_extent_composes_from_resolved_page_content_targets() {
    let fixture = single_page_content_fixture();
    let source = &fixture.source;
    let xref_offset = source
        .windows(b"xref".len())
        .position(|window| window == b"xref")
        .expect("xref keyword present");
    let xref_report = inspect_classic_xref_table(source, xref_offset).expect("xref should inspect");
    let root_report = inspect_classic_xref_trailer_root(source, xref_report.trailer_byte_offset)
        .expect("root should inspect");
    let catalog_target =
        inspect_page_tree_reference_target(source, &xref_report, root_report.root_reference)
            .expect("catalog reference should resolve");
    let catalog_pages = inspect_catalog_pages(source, catalog_target.object_byte_offset)
        .expect("catalog pages should inspect");
    let page_tree =
        inspect_page_tree_reference_target(source, &xref_report, catalog_pages.pages_reference)
            .expect("page tree should resolve");
    let kids =
        inspect_page_tree_kids(source, page_tree.object_byte_offset).expect("kids should inspect");
    let page_target =
        inspect_page_tree_reference_target(source, &xref_report, kids.kids[0].reference)
            .expect("page should resolve");
    let contents = inspect_page_contents(source, page_target.object_byte_offset)
        .expect("contents should inspect");
    let targets = inspect_page_content_targets(source, &xref_report, &contents);

    assert_eq!(
        targets.entries[0],
        PageContentTargetInspection::Resolved {
            content_reference: contents.contents[0],
            object_byte_offset: fixture.direct_content_offset,
            xref_generation: 0,
        }
    );
    assert_eq!(
        contents.contents[0].reference,
        fixture.direct_content_reference
    );
    assert_eq!(
        targets.entries[1],
        PageContentTargetInspection::Resolved {
            content_reference: contents.contents[1],
            object_byte_offset: fixture.indirect_content_offset,
            xref_generation: 0,
        }
    );
    assert_eq!(
        contents.contents[1].reference,
        fixture.indirect_content_reference
    );

    let direct_extent = inspect_content_stream_data_extent(
        source,
        Some(&xref_report),
        fixture.direct_content_offset,
    )
    .expect("resolved direct-length stream extent should inspect");
    let indirect_extent = inspect_content_stream_data_extent(
        source,
        Some(&xref_report),
        fixture.indirect_content_offset,
    )
    .expect("resolved indirect-length stream extent should inspect");

    assert_eq!(direct_extent.length(), 11);
    assert_eq!(
        &source[direct_extent.stream_data_start_byte_offset()
            ..direct_extent.stream_data_end_byte_offset()],
        b"ABCDEFGHIJK"
    );
    assert_eq!(indirect_extent.length(), 7);
    assert_eq!(
        &source[indirect_extent.stream_data_start_byte_offset()
            ..indirect_extent.stream_data_end_byte_offset()],
        b"LMNOPQR"
    );
}
