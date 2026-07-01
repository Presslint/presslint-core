use crate::{
    ClassicXrefObjectLocation, DictionaryEntryInspectionRejection, DictionaryValueKind,
    IndirectObjectBodyLeadingTokenKind, IndirectObjectBodyTokenInspectionRejection,
    IndirectObjectDictionaryInspectionRejection, IndirectObjectHeaderInspectionRejection,
    IndirectRef, ResolvedObject, ResolvedObjectData, ResolvedObjectDictionaryInspection,
    ResolvedObjectDictionaryInspectionRejection, inspect_classic_xref_table,
    inspect_indirect_object_dictionary, inspect_object_dictionary, resolve_classic_xref_object,
};

fn uncompressed(object_byte_offset: usize, object_number: u32) -> ResolvedObjectData {
    ResolvedObjectData::Uncompressed {
        resolved: ResolvedObject {
            reference: IndirectRef {
                object_number,
                generation: 0,
            },
            object_byte_offset,
            xref_generation: 0,
        },
    }
}

#[test]
fn inspect_object_dictionary_delegates_for_uncompressed_objects() {
    let source = b"3 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";

    let inspection = inspect_object_dictionary(source, &uncompressed(0, 3))
        .expect("uncompressed object dictionary should inspect");

    let ResolvedObjectDictionaryInspection::Uncompressed(report) = inspection else {
        unreachable!("uncompressed data should report uncompressed inspection")
    };
    assert_eq!(
        report.reference,
        IndirectRef {
            object_number: 3,
            generation: 0,
        }
    );
    assert_eq!(report.entries.len(), 2);
    assert_eq!(entry_bytes(source, report.entries[0].key_range), b"/Type");
}

#[test]
fn inspect_object_dictionary_propagates_uncompressed_failure() {
    let source = b"3 0 obj\n[ 1 2 3 ]\nendobj\n";

    let error = inspect_object_dictionary(source, &uncompressed(0, 3))
        .expect_err("an array-bodied object must reject");

    assert_eq!(
        error.reason,
        ResolvedObjectDictionaryInspectionRejection::Uncompressed {
            object_dictionary_reason:
                IndirectObjectDictionaryInspectionRejection::NonDictionaryBody {
                    token_kind: IndirectObjectBodyLeadingTokenKind::ArrayOpen,
                },
        }
    );
}

#[test]
fn inspect_object_dictionary_scans_compressed_member_body() {
    let body: &[u8] = b"<< /Type /Catalog /Pages 2 0 R >>";
    let mut decoded = b"header 0 ".to_vec();
    let start = decoded.len();
    decoded.extend_from_slice(body);
    let resolved = ResolvedObjectData::Compressed {
        reference: IndirectRef {
            object_number: 10,
            generation: 0,
        },
        object_stream_number: 5,
        index_within_object_stream: 0,
        decoded_object_stream: decoded.clone(),
        object_body_span: start..decoded.len(),
    };

    let inspection = inspect_object_dictionary(&[], &resolved)
        .expect("compressed member dictionary should inspect");

    let ResolvedObjectDictionaryInspection::Compressed(report) = inspection else {
        unreachable!("compressed data should report compressed inspection")
    };
    assert_eq!(
        report.reference,
        IndirectRef {
            object_number: 10,
            generation: 0,
        }
    );
    assert_eq!(report.entries.len(), 2);
    // Offsets are relative to the extracted member body, not the decoded buffer.
    assert_eq!(entry_bytes(body, report.entries[0].key_range), b"/Type");
    assert_eq!(entry_bytes(body, report.entries[1].key_range), b"/Pages");
}

#[test]
fn inspect_object_dictionary_rejects_non_dictionary_compressed_body() {
    let body: &[u8] = b"[ 1 2 3 ]";
    let resolved = ResolvedObjectData::Compressed {
        reference: IndirectRef {
            object_number: 10,
            generation: 0,
        },
        object_stream_number: 5,
        index_within_object_stream: 0,
        decoded_object_stream: body.to_vec(),
        object_body_span: 0..body.len(),
    };

    let error = inspect_object_dictionary(&[], &resolved)
        .expect_err("a non-dictionary compressed body must reject");

    assert_eq!(
        error.reason,
        ResolvedObjectDictionaryInspectionRejection::CompressedNonDictionaryBody {
            token_kind: IndirectObjectBodyLeadingTokenKind::ArrayOpen,
        }
    );
}

fn entry_bytes(source: &[u8], range: crate::DictionaryEntryByteRange) -> &[u8] {
    &source[range.start..range.end]
}

#[test]
fn object_dictionary_reports_top_level_entry_spans() {
    let source = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";

    let report = inspect_indirect_object_dictionary(source, 0).expect("object should inspect");

    assert_eq!(
        report.reference,
        IndirectRef {
            object_number: 1,
            generation: 0,
        }
    );
    assert_eq!(
        &source[report.header_range.start..report.header_range.end],
        b"1 0 obj"
    );
    assert_eq!(report.dictionary_open_byte_offset, 8);
    assert_eq!(report.max_observed_dictionary_depth, 1);

    assert_eq!(report.entries.len(), 2);

    assert_eq!(entry_bytes(source, report.entries[0].key_range), b"/Type");
    assert_eq!(
        entry_bytes(source, report.entries[0].value_range),
        b"/Catalog"
    );
    assert_eq!(report.entries[0].value_kind, DictionaryValueKind::Name);

    assert_eq!(entry_bytes(source, report.entries[1].key_range), b"/Pages");
    assert_eq!(entry_bytes(source, report.entries[1].value_range), b"2 0 R");
    assert_eq!(
        report.entries[1].value_kind,
        DictionaryValueKind::IndirectReferenceLike
    );
}

#[test]
fn object_dictionary_skips_leading_whitespace_before_header() {
    let source = b"\t \r\n10 5 obj << /Type /Pages /Count 0 >>\nendobj";

    let report = inspect_indirect_object_dictionary(source, 0).expect("object should inspect");

    assert_eq!(
        report.reference,
        IndirectRef {
            object_number: 10,
            generation: 5,
        }
    );
    assert_eq!(
        &source[report.header_range.start..report.header_range.end],
        b"10 5 obj"
    );
    assert_eq!(report.entries.len(), 2);
    assert_eq!(entry_bytes(source, report.entries[0].key_range), b"/Type");
    assert_eq!(entry_bytes(source, report.entries[1].key_range), b"/Count");
}

#[test]
fn object_dictionary_rejects_array_body() {
    let source = b"1 0 obj\n[ 1 2 3 ]\nendobj\n";

    let error =
        inspect_indirect_object_dictionary(source, 0).expect_err("array body should reject");

    assert_eq!(error.byte_offset, 0);
    assert_eq!(error.byte_len, source.len());
    assert_eq!(error.header_byte_offset, Some(0));
    assert_eq!(
        error.reason,
        IndirectObjectDictionaryInspectionRejection::NonDictionaryBody {
            token_kind: IndirectObjectBodyLeadingTokenKind::ArrayOpen,
        }
    );
    assert_eq!(error.error_byte_offset, Some(8));
}

#[test]
fn object_dictionary_rejects_numeric_literal_body() {
    let source = b"7 0 obj 42 endobj";

    let error =
        inspect_indirect_object_dictionary(source, 0).expect_err("numeric body should reject");

    assert_eq!(
        error.reason,
        IndirectObjectDictionaryInspectionRejection::NonDictionaryBody {
            token_kind: IndirectObjectBodyLeadingTokenKind::NumberLike,
        }
    );
    assert_eq!(error.header_byte_offset, Some(0));
}

#[test]
fn object_dictionary_propagates_header_rejection() {
    let source = b"xref\n0 1\n0000000000 65535 f \n";

    let error =
        inspect_indirect_object_dictionary(source, 0).expect_err("non-header should reject");

    assert_eq!(error.header_byte_offset, None);
    assert_eq!(
        error.reason,
        IndirectObjectDictionaryInspectionRejection::Header {
            header_reason: IndirectObjectHeaderInspectionRejection::MalformedHeader,
        }
    );
}

#[test]
fn object_dictionary_propagates_dictionary_entry_rejection() {
    let source = b"1 0 obj\n<< /Type /Catalog notakey >>\nendobj\n";

    let error = inspect_indirect_object_dictionary(source, 0).expect_err("bad key should reject");

    assert_eq!(error.header_byte_offset, Some(0));
    assert_eq!(
        error.reason,
        IndirectObjectDictionaryInspectionRejection::DictionaryEntries {
            dictionary_entries_reason: DictionaryEntryInspectionRejection::NonNameTopLevelKey,
        }
    );
}

#[test]
fn object_dictionary_propagates_body_token_rejection_at_eof() {
    let source = b"1 0 obj";

    let error =
        inspect_indirect_object_dictionary(source, 0).expect_err("empty body should reject");

    assert_eq!(error.header_byte_offset, Some(0));
    assert_eq!(
        error.reason,
        IndirectObjectDictionaryInspectionRejection::BodyToken {
            body_token_reason: IndirectObjectBodyTokenInspectionRejection::OffsetOutOfBounds,
        }
    );
}

#[test]
fn object_dictionary_report_does_not_retain_source_bytes() {
    let source = b"1 0 obj\n<< /Type /Catalog /Secret (something-not-copied) >>\nendobj\n";

    let report = inspect_indirect_object_dictionary(source, 0).expect("object should inspect");

    let debug_report = format!("{report:?}");
    assert!(!debug_report.contains("Secret"));
    assert!(!debug_report.contains("something-not-copied"));
    assert!(!debug_report.contains("Catalog"));
}

#[test]
fn object_dictionary_composes_from_classic_xref_resolution() {
    let prefix = b"%PDF-1.7\n";
    let catalog = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
    let pages = b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>\nendobj\n";
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

    // Locate the catalog object (object number 1) and read its entry spans.
    let catalog_location = resolve_classic_xref_object(&xref_report, 1);
    assert_eq!(
        catalog_location,
        ClassicXrefObjectLocation::InUse {
            object_number: 1,
            generation: 0,
            byte_offset: catalog_offset,
        }
    );

    let catalog_report = inspect_indirect_object_dictionary(&source, catalog_offset)
        .expect("catalog should inspect");
    assert_eq!(
        catalog_report.reference,
        IndirectRef {
            object_number: 1,
            generation: 0,
        }
    );
    assert_eq!(catalog_report.entries.len(), 2);
    assert_eq!(
        entry_bytes(&source, catalog_report.entries[0].key_range),
        b"/Type"
    );
    assert_eq!(
        entry_bytes(&source, catalog_report.entries[1].key_range),
        b"/Pages"
    );
    // Keys are addressed by range only; no key/value bytes are copied.
    let debug_report = format!("{catalog_report:?}");
    assert!(!debug_report.contains("Catalog"));
    assert!(!debug_report.contains("Pages"));

    // The same primitive serves the page-tree root object.
    let pages_location = resolve_classic_xref_object(&xref_report, 2);
    assert_eq!(
        pages_location,
        ClassicXrefObjectLocation::InUse {
            object_number: 2,
            generation: 0,
            byte_offset: pages_offset,
        }
    );
    let pages_report =
        inspect_indirect_object_dictionary(&source, pages_offset).expect("pages should inspect");
    let keys: Vec<&[u8]> = pages_report
        .entries
        .iter()
        .map(|entry| entry_bytes(&source, entry.key_range))
        .collect();
    assert_eq!(keys, vec![&b"/Type"[..], &b"/Kids"[..], &b"/Count"[..]]);
}
