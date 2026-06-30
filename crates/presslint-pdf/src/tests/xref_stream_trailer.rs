#[path = "content_stream_extent/serde_harness.rs"]
#[allow(clippy::duplicate_mod)]
mod serde_harness;

use serde_harness::{from_serde_value, serde_value};

use crate::startxref::inspect_startxref;
use crate::xref_section::classify_xref_section;
use crate::{
    DictionaryValueKind, IndirectRef, XrefSection, XrefStreamTrailerInspection,
    XrefStreamTrailerInspectionError, XrefStreamTrailerInspectionRejection,
    inspect_xref_stream_trailer,
};

/// Wrap a dictionary as a minimal `5 0 obj` xref-stream object at offset zero.
fn object_source(dictionary: &[u8]) -> Vec<u8> {
    let mut source = b"5 0 obj\n".to_vec();
    source.extend_from_slice(dictionary);
    source.extend_from_slice(b"\nendobj\n");
    source
}

fn reason(dictionary: &[u8]) -> XrefStreamTrailerInspectionRejection {
    let source = object_source(dictionary);
    inspect_xref_stream_trailer(&source, 0)
        .expect_err("xref-stream trailer should reject")
        .reason
}

fn indirect_ref(object_number: u32, generation: u16) -> IndirectRef {
    IndirectRef {
        object_number,
        generation,
    }
}

#[test]
fn inspects_root_reference_without_prev() {
    let source =
        object_source(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Index [ 0 8 ] /Root 1 0 R >>");

    let report =
        inspect_xref_stream_trailer(&source, 0).expect("xref-stream trailer should inspect");

    assert_eq!(
        &source[report.root_key_range.start..report.root_key_range.end],
        b"/Root"
    );
    assert_eq!(
        &source[report.root_value_range.start..report.root_value_range.end],
        b"1 0 R"
    );
    assert_eq!(report.root_reference, indirect_ref(1, 0));
    assert_eq!(report.prev_value_range, None);
    assert_eq!(report.prev_byte_offset, None);
    // Delegated geometry remains available through the report.
    assert_eq!(report.xref_stream_dictionary.size, 8);
    assert_eq!(report.xref_stream_dictionary.widths, vec![1, 2, 1]);
}

#[test]
fn inspects_root_reference_and_prev_offset() {
    let source = object_source(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Root 4 0 R /Prev 116 >>");

    let report =
        inspect_xref_stream_trailer(&source, 0).expect("xref-stream trailer should inspect");

    assert_eq!(report.root_reference, indirect_ref(4, 0));
    let prev_value_range = report.prev_value_range.expect("present /Prev value range");
    assert_eq!(
        &source[prev_value_range.start..prev_value_range.end],
        b"116"
    );
    assert_eq!(report.prev_byte_offset, Some(116));
}

#[test]
fn rejects_root_field_shapes() {
    assert_eq!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] >>"),
        XrefStreamTrailerInspectionRejection::MissingRoot
    );
    assert!(matches!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Root 1 0 R /Root 2 0 R >>"),
        XrefStreamTrailerInspectionRejection::DuplicateRoot { .. }
    ));
    assert_eq!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Root /Catalog >>"),
        XrefStreamTrailerInspectionRejection::NonReferenceRootValue {
            value_kind: DictionaryValueKind::Name,
        }
    );
    assert_eq!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Root 1 >>"),
        XrefStreamTrailerInspectionRejection::NonReferenceRootValue {
            value_kind: DictionaryValueKind::NumberLike,
        }
    );
    assert!(matches!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Root 1 0 obj >>"),
        XrefStreamTrailerInspectionRejection::MalformedRootReference { .. }
    ));
    assert!(matches!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Root 1 0 R extra >>"),
        XrefStreamTrailerInspectionRejection::MalformedRootReference { .. }
    ));
}

#[test]
fn rejects_prev_field_shapes() {
    assert!(matches!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Root 1 0 R /Prev 10 /Prev 20 >>"),
        XrefStreamTrailerInspectionRejection::DuplicatePrev { .. }
    ));
    assert_eq!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Root 1 0 R /Prev 1 0 R >>"),
        XrefStreamTrailerInspectionRejection::NonIntegerPrevValue {
            value_kind: DictionaryValueKind::IndirectReferenceLike,
        }
    );
    assert_eq!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Root 1 0 R /Prev 1.5 >>"),
        XrefStreamTrailerInspectionRejection::NonIntegerPrevValue {
            value_kind: DictionaryValueKind::NumberLike,
        }
    );
    assert_eq!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Root 1 0 R /Prev /Foo >>"),
        XrefStreamTrailerInspectionRejection::NonIntegerPrevValue {
            value_kind: DictionaryValueKind::Name,
        }
    );
    let overflow = format!(
        "<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Root 1 0 R /Prev {}0 >>",
        usize::MAX
    );
    assert_eq!(
        reason(overflow.as_bytes()),
        XrefStreamTrailerInspectionRejection::PrevOutOfRange
    );
}

#[test]
fn propagates_geometry_failure() {
    // A missing `/Type` fails inside the delegated geometry inspector.
    assert!(matches!(
        reason(b"<< /Size 8 /W [ 1 2 1 ] /Root 1 0 R >>"),
        XrefStreamTrailerInspectionRejection::XrefStreamDictionary { .. }
    ));
}

#[test]
fn chains_startxref_classification_into_trailer() {
    let prefix = b"%PDF-1.5\n";
    let object =
        b"5 0 obj\n<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Index [ 0 8 ] /Root 1 0 R /Prev 9 /Length 5 >>\nstream\nABCDE\nendstream\nendobj\n";
    let object_offset = prefix.len();
    let mut source = prefix.to_vec();
    source.extend_from_slice(object);
    source.extend_from_slice(format!("startxref\n{object_offset}\n%%EOF\n").as_bytes());

    let startxref = inspect_startxref(&source).expect("startxref should inspect");
    assert_eq!(startxref.byte_offset, object_offset);

    let section =
        classify_xref_section(&source, startxref.byte_offset).expect("section should classify");
    assert_eq!(
        section,
        XrefSection::Stream {
            object_number: 5,
            generation: 0,
        }
    );

    let report = inspect_xref_stream_trailer(&source, startxref.byte_offset)
        .expect("xref-stream trailer should inspect");
    assert_eq!(report.root_reference, indirect_ref(1, 0));
    assert_eq!(report.prev_byte_offset, Some(9));
    assert_eq!(report.xref_stream_dictionary.size, 8);
}

#[test]
fn report_does_not_retain_source_bytes() {
    let source = object_source(
        b"<< /Secret (do-not-copy) /Type /XRef /Size 8 /W [ 1 2 1 ] /Root 1 0 R /Prev 116 >>",
    );

    let report =
        inspect_xref_stream_trailer(&source, 0).expect("xref-stream trailer should inspect");
    let debug_report = format!("{report:?}");

    assert!(!debug_report.contains("do-not-copy"));
    assert!(!debug_report.contains("Secret"));
}

#[test]
fn serde_round_trip_preserves_report_and_rejection_shapes() {
    let source = object_source(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Root 1 0 R /Prev 116 >>");
    let report =
        inspect_xref_stream_trailer(&source, 0).expect("xref-stream trailer should inspect");

    let value = serde_value(&report).expect("report should serialize");
    let decoded: XrefStreamTrailerInspection =
        from_serde_value(value).expect("report should deserialize");
    assert_eq!(decoded, report);

    let error =
        inspect_xref_stream_trailer(&object_source(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] >>"), 0)
            .expect_err("missing /Root should reject");
    let error_value = serde_value(&error).expect("error should serialize");
    let decoded_error: XrefStreamTrailerInspectionError =
        from_serde_value(error_value).expect("error should deserialize");
    assert_eq!(decoded_error, error);
}
