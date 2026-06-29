use crate::{
    ClassicXrefObjectLocation, ClassicXrefTrailerDictionaryInspectionRejection,
    ClassicXrefTrailerRootInspectionRejection, DictionaryEntryInspectionRejection,
    DictionaryValueKind, IndirectRef, IndirectReferenceInspectionRejection,
    inspect_classic_xref_table, inspect_classic_xref_trailer_root, inspect_indirect_object_header,
    resolve_classic_xref_object,
};

#[test]
fn trailer_root_reports_root_reference_and_ranges() {
    let source = b"trailer\n<< /Size 2 /Root 1 0 R >>";

    let report = inspect_classic_xref_trailer_root(source, 0).expect("root should inspect");

    assert_eq!(report.trailer_dictionary.trailer_byte_offset, 0);
    assert_eq!(report.trailer_dictionary.dictionary_open_byte_offset, 8);
    assert_eq!(
        &source[report.root_key_range.start..report.root_key_range.end],
        b"/Root"
    );
    assert_eq!(
        &source[report.root_value_range.start..report.root_value_range.end],
        b"1 0 R"
    );
    assert_eq!(
        report.root_reference,
        IndirectRef {
            object_number: 1,
            generation: 0,
        }
    );
}

#[test]
fn trailer_root_skips_leading_whitespace_before_trailer_keyword() {
    let source = b"\t \r\ntrailer << /Root 1 0 R >>";

    let report = inspect_classic_xref_trailer_root(source, 0).expect("root should inspect");

    assert_eq!(report.trailer_dictionary.byte_offset, 0);
    assert_eq!(report.trailer_dictionary.trailer_byte_offset, 4);
    assert_eq!(
        report.root_reference,
        IndirectRef {
            object_number: 1,
            generation: 0,
        }
    );
}

#[test]
fn trailer_root_propagates_trailer_dictionary_rejection() {
    let source = b"not-trailer << /Root 1 0 R >>";

    let error =
        inspect_classic_xref_trailer_root(source, 0).expect_err("missing trailer should reject");

    assert_eq!(error.byte_offset, 0);
    assert_eq!(error.byte_len, source.len());
    assert_eq!(error.error_byte_offset, Some(0));
    assert_eq!(
        error.reason,
        ClassicXrefTrailerRootInspectionRejection::TrailerDictionary {
            trailer_reason: ClassicXrefTrailerDictionaryInspectionRejection::MissingTrailerKeyword,
        }
    );
}

#[test]
fn trailer_root_propagates_dictionary_entry_rejection() {
    let source = b"trailer << /Root >>";

    let error =
        inspect_classic_xref_trailer_root(source, 0).expect_err("missing value should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTrailerRootInspectionRejection::DictionaryEntries {
            dictionary_entries_reason: DictionaryEntryInspectionRejection::MissingValue,
        }
    );
}

#[test]
fn trailer_root_rejects_missing_root() {
    let source = b"trailer << /Size 2 /Root#20 1 0 R >>";

    let error =
        inspect_classic_xref_trailer_root(source, 0).expect_err("missing root should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTrailerRootInspectionRejection::MissingRoot
    );
}

#[test]
fn trailer_root_rejects_duplicate_root() {
    let source = b"trailer << /Root 1 0 R /Size 2 /Root 2 0 R >>";

    let error =
        inspect_classic_xref_trailer_root(source, 0).expect_err("duplicate root should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTrailerRootInspectionRejection::DuplicateRoot {
            first_key_range: crate::DictionaryEntryByteRange { start: 11, end: 16 },
            duplicate_key_range: crate::DictionaryEntryByteRange { start: 31, end: 36 },
        }
    );
    assert_eq!(error.error_byte_offset, Some(31));
}

#[test]
fn trailer_root_rejects_direct_dictionary_name_and_number_values() {
    for (source, expected_kind) in [
        (
            b"trailer << /Root << /Type /Catalog >> >>".as_slice(),
            DictionaryValueKind::Dictionary,
        ),
        (
            b"trailer << /Root /Catalog >>".as_slice(),
            DictionaryValueKind::Name,
        ),
        (
            b"trailer << /Root 1 >>".as_slice(),
            DictionaryValueKind::NumberLike,
        ),
    ] {
        let error =
            inspect_classic_xref_trailer_root(source, 0).expect_err("direct value should reject");

        assert_eq!(
            error.reason,
            ClassicXrefTrailerRootInspectionRejection::NonReferenceRootValue {
                value_kind: expected_kind,
            }
        );
    }
}

#[test]
fn trailer_root_rejects_malformed_obj_keyword_reference() {
    let source = b"trailer << /Root 1 0 obj >>";

    let error = inspect_classic_xref_trailer_root(source, 0).expect_err("obj value should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTrailerRootInspectionRejection::MalformedRootReference {
            reference_reason: IndirectReferenceInspectionRejection::MalformedReference,
        }
    );
    assert_eq!(error.error_byte_offset, Some(21));
}

#[test]
fn trailer_root_rejects_reference_with_extra_scalar_token() {
    let source = b"trailer << /Root 1 0 R extra >>";

    let error =
        inspect_classic_xref_trailer_root(source, 0).expect_err("extra token should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTrailerRootInspectionRejection::MalformedRootReference {
            reference_reason: IndirectReferenceInspectionRejection::MalformedReference,
        }
    );
    assert_eq!(error.error_byte_offset, Some(22));
}

#[test]
fn trailer_root_report_does_not_retain_source_bytes() {
    let source = b"trailer << /Root 1 0 R /Secret (corpus-detail not copied) >>";

    let report = inspect_classic_xref_trailer_root(source, 0).expect("root should inspect");

    let debug_report = format!("{report:?}");
    assert!(!debug_report.contains("Secret"));
    assert!(!debug_report.contains("corpus-detail"));
}

#[test]
fn trailer_root_composes_to_catalog_object_header_without_body_inspection() {
    let prefix = b"%PDF-1.7\n";
    let catalog = b"1 0 obj\n<< /Type /Catalog /Secret (body-not-inspected) >>\nendobj\n";
    let pages = b"2 0 obj\n<< /Type /Pages >>\nendobj\n";
    let catalog_offset = prefix.len();
    let pages_offset = prefix.len() + catalog.len();
    let xref_offset = prefix.len() + catalog.len() + pages.len();
    let source = format!(
        "{}{}{}xref\n0 3\n0000000000 65535 f \n{catalog_offset:010} 00000 n \n{pages_offset:010} 00000 n \ntrailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
        String::from_utf8_lossy(prefix),
        String::from_utf8_lossy(catalog),
        String::from_utf8_lossy(pages),
    )
    .into_bytes();

    let xref_report =
        inspect_classic_xref_table(&source, xref_offset).expect("xref should inspect");
    let root_report = inspect_classic_xref_trailer_root(&source, xref_report.trailer_byte_offset)
        .expect("root should inspect");
    let location =
        resolve_classic_xref_object(&xref_report, root_report.root_reference.object_number);
    assert_eq!(
        location,
        ClassicXrefObjectLocation::InUse {
            object_number: 1,
            generation: 0,
            byte_offset: catalog_offset,
        }
    );

    let header =
        inspect_indirect_object_header(&source, catalog_offset).expect("header should inspect");
    assert_eq!(header.reference, root_report.root_reference);
    assert_eq!(header.header_byte_offset, catalog_offset);
    assert_eq!(
        header.after_obj_keyword_offset,
        catalog_offset + b"1 0 obj".len()
    );
}
