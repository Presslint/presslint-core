use crate::{
    ClassicXrefEntry, ClassicXrefEntryState, ClassicXrefSubsection,
    ClassicXrefTableInspectionRejection, inspect_classic_xref_table,
};

#[test]
fn classic_xref_table_inspection_reports_one_subsection() {
    let source =
        b"%PDF-1.7\nxref\n0 2\n0000000000 65535 f \n0000000017 00000 n \ntrailer\n<< /Size 2 >>\n";

    let report = inspect_classic_xref_table(source, 9).expect("xref table should inspect");

    assert_eq!(report.table_byte_offset, 9);
    assert_eq!(report.trailer_byte_offset, 58);
    assert_eq!(
        report.subsections,
        vec![ClassicXrefSubsection {
            first_object_number: 0,
            entry_count: 2,
            entries: vec![
                ClassicXrefEntry {
                    object_number: 0,
                    generation: 65535,
                    byte_offset: 0,
                    state: ClassicXrefEntryState::Free,
                },
                ClassicXrefEntry {
                    object_number: 1,
                    generation: 0,
                    byte_offset: 17,
                    state: ClassicXrefEntryState::InUse,
                },
            ],
        }]
    );
}

#[test]
fn classic_xref_table_inspection_reports_multiple_subsections_in_source_order() {
    let source = b"xref\r\n0 1\r\n0000000000 65535 f \r\n5 2\r\n0000000100 00000 n \r\n0000000200 00002 n \r\ntrailer\r\n<<>>";

    let report = inspect_classic_xref_table(source, 0).expect("xref table should inspect");

    assert_eq!(report.trailer_byte_offset, 79);
    assert_eq!(report.subsections.len(), 2);
    assert_eq!(report.subsections[0].first_object_number, 0);
    assert_eq!(report.subsections[0].entries[0].object_number, 0);
    assert_eq!(report.subsections[1].first_object_number, 5);
    assert_eq!(
        report.subsections[1].entries,
        vec![
            ClassicXrefEntry {
                object_number: 5,
                generation: 0,
                byte_offset: 100,
                state: ClassicXrefEntryState::InUse,
            },
            ClassicXrefEntry {
                object_number: 6,
                generation: 2,
                byte_offset: 200,
                state: ClassicXrefEntryState::InUse,
            },
        ]
    );
}

#[test]
fn classic_xref_table_inspection_tolerates_pdf_whitespace() {
    let source = b"\0\t \rxref\n0\t1\x0c\n0000000000 65535 f \r\n \t\ntrailer\n<< /Ignored true >>";

    let report = inspect_classic_xref_table(source, 0).expect("xref table should inspect");

    assert_eq!(report.table_byte_offset, 4);
    assert_eq!(report.trailer_byte_offset, 38);
    assert_eq!(
        report.subsections[0].entries[0].state,
        ClassicXrefEntryState::Free
    );
}

#[test]
fn classic_xref_table_inspection_stops_before_trailer_dictionary_and_body_bytes() {
    let source =
        b"xref\n1 1\n0000000042 00000 n \ntrailer\n<< /Size 2 /Prev not-parsed /Root 9 0 R >>\n1 0 obj\n<< /NotParsed true >>\nendobj\n";

    let report = inspect_classic_xref_table(source, 0).expect("xref table should inspect");

    assert_eq!(report.trailer_byte_offset, 29);
    assert_eq!(report.subsections.len(), 1);
    assert_eq!(report.subsections[0].entries.len(), 1);

    let debug_report = format!("{report:?}");
    assert!(!debug_report.contains("/Prev"));
    assert!(!debug_report.contains("NotParsed"));
    assert!(!debug_report.contains("endobj"));
}

#[test]
fn classic_xref_table_inspection_accepts_trailer_keyword_at_eof() {
    let source = b"xref\n0 1\n0000000000 65535 f \ntrailer";

    let report = inspect_classic_xref_table(source, 0).expect("eof trailer should inspect");

    assert_eq!(report.trailer_byte_offset, 29);
    assert_eq!(report.subsections.len(), 1);
    assert_eq!(report.subsections[0].entry_count, 1);
}

#[test]
fn classic_xref_table_inspection_rejects_non_table_offset() {
    let source = b"1 0 obj\n<<>>\nendobj\n";

    let error = inspect_classic_xref_table(source, 0).expect_err("object body is not xref table");

    assert_eq!(error.byte_offset, 0);
    assert_eq!(error.byte_len, source.len());
    assert_eq!(
        error.reason,
        ClassicXrefTableInspectionRejection::NotXrefTable
    );
}

#[test]
fn classic_xref_table_inspection_rejects_malformed_subsection_header() {
    let source = b"xref\n0 nope\n0000000000 65535 f \ntrailer\n<<>>";

    let error = inspect_classic_xref_table(source, 0).expect_err("bad header should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTableInspectionRejection::MalformedSubsectionHeader
    );
    assert_eq!(error.error_byte_offset, Some(5));
    assert_eq!(error.object_number, None);
}

#[test]
fn classic_xref_table_inspection_rejects_malformed_entry() {
    let source = b"xref\n0 1\n0000000000 65535 x \ntrailer\n<<>>";

    let error = inspect_classic_xref_table(source, 0).expect_err("bad entry should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTableInspectionRejection::MalformedEntry
    );
    assert_eq!(error.error_byte_offset, Some(9));
    assert_eq!(error.object_number, Some(0));
}

#[test]
fn classic_xref_table_inspection_rejects_entry_without_trailing_separator() {
    let source = b"xref\n0 1\n0000000000 65535 f\ntrailer\n<<>>";

    let error = inspect_classic_xref_table(source, 0).expect_err("short entry should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTableInspectionRejection::MalformedEntry
    );
    assert_eq!(error.error_byte_offset, Some(9));
    assert_eq!(error.object_number, Some(0));
}

#[test]
fn classic_xref_table_inspection_rejects_short_entry_before_blank_line() {
    let source = b"xref\n0 1\n0000000000 65535 f\n \ntrailer\n<<>>";

    let error = inspect_classic_xref_table(source, 0).expect_err("short entry should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTableInspectionRejection::MalformedEntry
    );
    assert_eq!(error.error_byte_offset, Some(9));
    assert_eq!(error.object_number, Some(0));
}

#[test]
fn classic_xref_table_inspection_rejects_out_of_range_subsection_object_number() {
    let source = b"xref\n99999999999 1\n0000000000 65535 f \ntrailer\n<<>>";

    let error = inspect_classic_xref_table(source, 0).expect_err("large object should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTableInspectionRejection::SubsectionObjectNumberOutOfRange
    );
    assert_eq!(error.error_byte_offset, Some(5));
    assert_eq!(error.object_number, None);
}

#[test]
fn classic_xref_table_inspection_rejects_out_of_range_generation_number() {
    let source = b"xref\n0 1\n0000000000 99999 f \ntrailer\n<<>>";

    let error = inspect_classic_xref_table(source, 0).expect_err("large generation should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTableInspectionRejection::EntryGenerationOutOfRange
    );
    assert_eq!(error.error_byte_offset, Some(9));
    assert_eq!(error.object_number, Some(0));
}

#[test]
fn classic_xref_table_inspection_rejects_missing_trailer() {
    let source = b"xref\n0 1\n0000000000 65535 f \n";

    let error = inspect_classic_xref_table(source, 0).expect_err("missing trailer should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTableInspectionRejection::MissingTrailer
    );
    assert_eq!(error.error_byte_offset, Some(source.len()));
    assert_eq!(error.object_number, None);
}

#[test]
fn classic_xref_table_inspection_reports_bare_xref_as_missing_trailer() {
    let source = b"xref";

    let error = inspect_classic_xref_table(source, 0).expect_err("bare xref should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTableInspectionRejection::MissingTrailer
    );
    assert_eq!(error.error_byte_offset, Some(source.len()));
}

#[test]
fn classic_xref_table_inspection_reports_whitespace_only_xref_as_missing_trailer() {
    let source = b"xref \t\r\n\x0c";

    let error =
        inspect_classic_xref_table(source, 0).expect_err("xref without trailer should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTableInspectionRejection::MissingTrailer
    );
    assert_eq!(error.error_byte_offset, Some(source.len()));
}
