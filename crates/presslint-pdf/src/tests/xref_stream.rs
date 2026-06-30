#[path = "content_stream_extent/serde_harness.rs"]
#[allow(clippy::duplicate_mod)]
mod serde_harness;

use serde_harness::{from_serde_value, serde_value};

use crate::startxref::inspect_startxref;
use crate::xref_section::classify_xref_section;
use crate::{
    DictionaryValueKind, XrefSection, XrefStreamDictionaryInspection,
    XrefStreamDictionaryInspectionError, XrefStreamDictionaryInspectionRejection,
    XrefStreamSubsection, inspect_xref_stream_dictionary,
};

/// Wrap a dictionary as a minimal `5 0 obj` xref-stream object at offset zero.
fn object_source(dictionary: &[u8]) -> Vec<u8> {
    let mut source = b"5 0 obj\n".to_vec();
    source.extend_from_slice(dictionary);
    source.extend_from_slice(b"\nendobj\n");
    source
}

fn reason(dictionary: &[u8]) -> XrefStreamDictionaryInspectionRejection {
    let source = object_source(dictionary);
    inspect_xref_stream_dictionary(&source, 0)
        .expect_err("xref-stream dictionary should reject")
        .reason
}

fn subsection(first_object_number: usize, entry_count: usize) -> XrefStreamSubsection {
    XrefStreamSubsection {
        first_object_number,
        entry_count,
    }
}

#[test]
fn inspects_geometry_with_explicit_index() {
    let source = object_source(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Index [ 2 6 ] >>");

    let report =
        inspect_xref_stream_dictionary(&source, 0).expect("xref-stream dictionary should inspect");

    assert_eq!(
        &source[report.type_key_range.start..report.type_key_range.end],
        b"/Type"
    );
    assert_eq!(
        &source[report.type_value_range.start..report.type_value_range.end],
        b"/XRef"
    );
    assert_eq!(
        &source[report.w_value_range.start..report.w_value_range.end],
        b"[ 1 2 1 ]"
    );
    assert_eq!(report.widths, vec![1, 2, 1]);
    assert_eq!(
        &source[report.size_value_range.start..report.size_value_range.end],
        b"8"
    );
    assert_eq!(report.size, 8);
    let index_value_range = report.index_value_range.expect("explicit /Index range");
    assert_eq!(
        &source[index_value_range.start..index_value_range.end],
        b"[ 2 6 ]"
    );
    assert_eq!(report.index_subsections, vec![subsection(2, 6)]);
    assert_eq!(report.object_dictionary.reference.object_number, 5);
}

#[test]
fn defaults_index_to_single_subsection_when_absent() {
    let source = object_source(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] >>");

    let report =
        inspect_xref_stream_dictionary(&source, 0).expect("xref-stream dictionary should inspect");

    assert_eq!(report.index_value_range, None);
    assert_eq!(report.index_subsections, vec![subsection(0, 8)]);
}

#[test]
fn accepts_zero_width_field_and_multiple_index_subsections() {
    let source = object_source(b"<< /Type /XRef /Size 9 /W [ 1 0 2 ] /Index [ 0 3 6 3 ] >>");

    let report =
        inspect_xref_stream_dictionary(&source, 0).expect("xref-stream dictionary should inspect");

    assert_eq!(report.widths, vec![1, 0, 2]);
    assert_eq!(
        report.index_subsections,
        vec![subsection(0, 3), subsection(6, 3)]
    );
}

#[test]
fn chains_startxref_classification_into_dictionary() {
    let prefix = b"%PDF-1.5\n";
    let object =
        b"5 0 obj\n<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Index [ 0 8 ] /Root 1 0 R /Length 5 >>\nstream\nABCDE\nendstream\nendobj\n";
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

    let report = inspect_xref_stream_dictionary(&source, startxref.byte_offset)
        .expect("xref-stream dictionary should inspect");
    assert_eq!(
        &source[report.type_value_range.start..report.type_value_range.end],
        b"/XRef"
    );
    assert_eq!(report.widths, vec![1, 2, 1]);
    assert_eq!(report.size, 8);
    assert_eq!(report.index_subsections, vec![subsection(0, 8)]);
}

#[test]
fn rejects_type_field_shapes() {
    assert_eq!(
        reason(b"<< /Size 8 /W [ 1 2 1 ] >>"),
        XrefStreamDictionaryInspectionRejection::MissingType
    );
    assert!(matches!(
        reason(b"<< /Type /XRef /Type /XRef /Size 8 /W [ 1 2 1 ] >>"),
        XrefStreamDictionaryInspectionRejection::DuplicateType { .. }
    ));
    assert_eq!(
        reason(b"<< /Type 5 /Size 8 /W [ 1 2 1 ] >>"),
        XrefStreamDictionaryInspectionRejection::NonNameTypeValue {
            value_kind: DictionaryValueKind::NumberLike,
        }
    );
    assert_eq!(
        reason(b"<< /Type /ObjStm /Size 8 /W [ 1 2 1 ] >>"),
        XrefStreamDictionaryInspectionRejection::UnexpectedTypeName
    );
}

#[test]
fn rejects_w_field_shapes() {
    assert_eq!(
        reason(b"<< /Type /XRef /Size 8 >>"),
        XrefStreamDictionaryInspectionRejection::MissingW
    );
    assert!(matches!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /W [ 1 2 1 ] >>"),
        XrefStreamDictionaryInspectionRejection::DuplicateW { .. }
    ));
    assert_eq!(
        reason(b"<< /Type /XRef /Size 8 /W 3 >>"),
        XrefStreamDictionaryInspectionRejection::NonArrayWValue {
            value_kind: DictionaryValueKind::NumberLike,
        }
    );
    assert_eq!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 x 1 ] >>"),
        XrefStreamDictionaryInspectionRejection::MalformedWElement
    );
    assert_eq!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 ] >>"),
        XrefStreamDictionaryInspectionRejection::WrongWLength { width_count: 2 }
    );
    let overflow = format!("<< /Type /XRef /Size 8 /W [ 1 {}0 1 ] >>", usize::MAX);
    assert_eq!(
        reason(overflow.as_bytes()),
        XrefStreamDictionaryInspectionRejection::WidthOutOfRange
    );
}

#[test]
fn rejects_size_field_shapes() {
    assert_eq!(
        reason(b"<< /Type /XRef /W [ 1 2 1 ] >>"),
        XrefStreamDictionaryInspectionRejection::MissingSize
    );
    assert!(matches!(
        reason(b"<< /Type /XRef /Size 8 /Size 8 /W [ 1 2 1 ] >>"),
        XrefStreamDictionaryInspectionRejection::DuplicateSize { .. }
    ));
    assert_eq!(
        reason(b"<< /Type /XRef /Size /Foo /W [ 1 2 1 ] >>"),
        XrefStreamDictionaryInspectionRejection::NonIntegerSizeValue {
            value_kind: DictionaryValueKind::Name,
        }
    );
    assert_eq!(
        reason(b"<< /Type /XRef /Size 1.0 /W [ 1 2 1 ] >>"),
        XrefStreamDictionaryInspectionRejection::NonIntegerSizeValue {
            value_kind: DictionaryValueKind::NumberLike,
        }
    );
    let overflow = format!("<< /Type /XRef /Size {}0 /W [ 1 2 1 ] >>", usize::MAX);
    assert_eq!(
        reason(overflow.as_bytes()),
        XrefStreamDictionaryInspectionRejection::SizeOutOfRange
    );
}

#[test]
fn rejects_index_field_shapes() {
    assert!(matches!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Index [ 0 8 ] /Index [ 0 8 ] >>"),
        XrefStreamDictionaryInspectionRejection::DuplicateIndex { .. }
    ));
    assert_eq!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Index 5 >>"),
        XrefStreamDictionaryInspectionRejection::NonArrayIndexValue {
            value_kind: DictionaryValueKind::NumberLike,
        }
    );
    assert_eq!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Index [ 0 y ] >>"),
        XrefStreamDictionaryInspectionRejection::MalformedIndexElement
    );
    assert_eq!(
        reason(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Index [ 0 8 1 ] >>"),
        XrefStreamDictionaryInspectionRejection::OddIndexLength { integer_count: 3 }
    );
    let overflow = format!(
        "<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Index [ 0 {}0 ] >>",
        usize::MAX
    );
    assert_eq!(
        reason(overflow.as_bytes()),
        XrefStreamDictionaryInspectionRejection::IndexIntegerOutOfRange
    );
}

#[test]
fn propagates_object_dictionary_failure() {
    // A non-dictionary body fails at the delegated object-dictionary inspector.
    let source = b"5 0 obj\n[ /Type /XRef ]\nendobj\n";

    let error =
        inspect_xref_stream_dictionary(source, 0).expect_err("non-dictionary body should reject");

    assert!(matches!(
        error.reason,
        XrefStreamDictionaryInspectionRejection::ObjectDictionary { .. }
    ));
}

#[test]
fn report_does_not_retain_source_bytes() {
    let source = object_source(
        b"<< /Secret (do-not-copy) /Type /XRef /Size 8 /W [ 1 2 1 ] /Index [ 0 8 ] >>",
    );

    let report =
        inspect_xref_stream_dictionary(&source, 0).expect("xref-stream dictionary should inspect");
    let debug_report = format!("{report:?}");

    assert!(!debug_report.contains("do-not-copy"));
    assert!(!debug_report.contains("Secret"));
}

#[test]
fn serde_round_trip_preserves_report_and_rejection_shapes() {
    let source = object_source(b"<< /Type /XRef /Size 8 /W [ 1 2 1 ] /Index [ 0 8 ] >>");
    let report =
        inspect_xref_stream_dictionary(&source, 0).expect("xref-stream dictionary should inspect");

    let value = serde_value(&report).expect("report should serialize");
    let decoded: XrefStreamDictionaryInspection =
        from_serde_value(value).expect("report should deserialize");
    assert_eq!(decoded, report);

    let error = inspect_xref_stream_dictionary(&object_source(b"<< /Size 8 /W [ 1 2 1 ] >>"), 0)
        .expect_err("missing /Type should reject");
    let error_value = serde_value(&error).expect("error should serialize");
    let decoded_error: XrefStreamDictionaryInspectionError =
        from_serde_value(error_value).expect("error should deserialize");
    assert_eq!(decoded_error, error);
}
