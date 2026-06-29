use crate::{
    ClassicXrefObjectLocation, DictionaryValueKind, IndirectRef, IndirectReferenceByteRange,
    IndirectReferenceInspectionRejection, inspect_classic_xref_table,
    inspect_classic_xref_trailer_dictionary, inspect_dictionary_entries, parse_indirect_reference,
    resolve_classic_xref_object,
};

#[test]
fn parses_reference_with_keyword_boundary_and_post_keyword_offset() {
    let source = b"12 0 R";

    let report = parse_indirect_reference(source, 0).expect("reference should parse");

    assert_eq!(
        report.reference,
        IndirectRef {
            object_number: 12,
            generation: 0,
        }
    );
    assert_eq!(report.reference_byte_offset, 0);
    assert_eq!(
        report.reference_range,
        IndirectReferenceByteRange { start: 0, end: 6 }
    );
    assert_eq!(report.after_keyword_offset, 6);
}

#[test]
fn reference_keyword_boundary_accepts_following_delimiter() {
    let source = b"12 0 R/Size 2";

    let report = parse_indirect_reference(source, 0).expect("reference should parse");

    assert_eq!(
        report.reference,
        IndirectRef {
            object_number: 12,
            generation: 0,
        }
    );
    assert_eq!(report.after_keyword_offset, 6);
}

#[test]
fn skips_leading_pdf_whitespace() {
    let source = b"\0\t \r\n\x0c7 5 R";

    let report = parse_indirect_reference(source, 0).expect("reference should parse");

    assert_eq!(report.reference_byte_offset, 6);
    assert_eq!(
        report.reference,
        IndirectRef {
            object_number: 7,
            generation: 5,
        }
    );
    assert_eq!(
        report.reference_range,
        IndirectReferenceByteRange {
            start: 6,
            end: source.len(),
        }
    );
}

#[test]
fn rejects_obj_header_keyword_instead_of_parsing_a_reference() {
    let source = b"12 0 obj\n<<>>";

    let error = parse_indirect_reference(source, 0).expect_err("obj header should reject");

    assert_eq!(
        error.reason,
        IndirectReferenceInspectionRejection::MalformedReference
    );
    assert_eq!(error.byte_offset, 0);
    assert_eq!(error.byte_len, source.len());
    assert_eq!(error.error_byte_offset, Some(5));
}

#[test]
fn rejects_trailing_bytes_after_reference_keyword() {
    let source = b"12 0 Robot";

    let error = parse_indirect_reference(source, 0).expect_err("Robot should reject");

    assert_eq!(
        error.reason,
        IndirectReferenceInspectionRejection::MalformedReference
    );
    assert_eq!(error.error_byte_offset, Some(5));
}

#[test]
fn rejects_offset_at_eof_and_out_of_bounds() {
    let source = b"1 0 R";

    let at_eof = parse_indirect_reference(source, source.len()).expect_err("eof should reject");
    let out_of_bounds =
        parse_indirect_reference(source, source.len() + 1).expect_err("oob should reject");

    assert_eq!(
        at_eof.reason,
        IndirectReferenceInspectionRejection::OffsetOutOfBounds
    );
    assert_eq!(
        out_of_bounds.reason,
        IndirectReferenceInspectionRejection::OffsetOutOfBounds
    );
}

#[test]
fn rejects_malformed_n_g_shape() {
    let source = b"12 R\n";

    let error = parse_indirect_reference(source, 0).expect_err("missing generation should reject");

    assert_eq!(
        error.reason,
        IndirectReferenceInspectionRejection::MalformedReference
    );
}

#[test]
fn rejects_out_of_range_object_number() {
    let source = b"4294967296 0 R\n";

    let error = parse_indirect_reference(source, 0).expect_err("large object should reject");

    assert_eq!(
        error.reason,
        IndirectReferenceInspectionRejection::ObjectNumberOutOfRange
    );
    assert_eq!(error.error_byte_offset, Some(0));
}

#[test]
fn rejects_out_of_range_generation_number() {
    let source = b"1 65536 R\n";

    let error = parse_indirect_reference(source, 0).expect_err("large generation should reject");

    assert_eq!(
        error.reason,
        IndirectReferenceInspectionRejection::GenerationOutOfRange
    );
    assert_eq!(error.error_byte_offset, Some(2));
}

#[test]
fn report_does_not_retain_source_bytes() {
    let source = b"  314 1 R corpus-detail-not-copied";

    let report = parse_indirect_reference(source, 0).expect("reference should parse");

    let debug_report = format!("{report:?}");
    assert!(!debug_report.contains("corpus-detail-not-copied"));
}

#[test]
fn parses_root_reference_span_and_resolves_catalog_offset() {
    let prefix = b"%PDF-1.7\n";
    let object = b"1 0 obj\n<< /Type /Catalog >>\nendobj\n";
    let object_offset = prefix.len();
    let xref_offset = prefix.len() + object.len();
    let source = format!(
        "{}{}xref\n0 2\n0000000000 65535 f \n{object_offset:010} 00000 n \ntrailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
        String::from_utf8_lossy(prefix),
        String::from_utf8_lossy(object),
    )
    .into_bytes();

    let xref_report =
        inspect_classic_xref_table(&source, xref_offset).expect("xref should inspect");
    let trailer_report =
        inspect_classic_xref_trailer_dictionary(&source, xref_report.trailer_byte_offset)
            .expect("trailer dictionary should inspect");
    let entries = inspect_dictionary_entries(&source, trailer_report.dictionary_open_byte_offset)
        .expect("entries should inspect");

    let root_entry = entries
        .entries
        .iter()
        .find(|entry| &source[entry.key_range.start..entry.key_range.end] == b"/Root")
        .expect("trailer should declare /Root");
    assert_eq!(
        root_entry.value_kind,
        DictionaryValueKind::IndirectReferenceLike
    );

    let reference = parse_indirect_reference(&source, root_entry.value_range.start)
        .expect("root reference should parse");
    assert_eq!(
        reference.reference,
        IndirectRef {
            object_number: 1,
            generation: 0,
        }
    );

    let location = resolve_classic_xref_object(&xref_report, reference.reference.object_number);
    assert_eq!(
        location,
        ClassicXrefObjectLocation::InUse {
            object_number: 1,
            generation: 0,
            byte_offset: object_offset,
        }
    );
}
