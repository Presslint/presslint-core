#[path = "content_stream_extent/serde_harness.rs"]
#[allow(clippy::duplicate_mod)]
mod serde_harness;

use serde_harness::{from_serde_value, serde_value};

use crate::{
    ClassicXrefTrailerDictionaryInspectionRejection, ClassicXrefTrailerPrevInspection,
    ClassicXrefTrailerPrevInspectionError, ClassicXrefTrailerPrevInspectionRejection,
    DictionaryValueKind, inspect_classic_xref_trailer_prev,
};

#[test]
fn absent_prev_reports_ok_none() {
    let source = b"trailer\n<< /Size 2 /Root 1 0 R >>";

    let report = inspect_classic_xref_trailer_prev(source, 0).expect("trailer prev should inspect");

    assert_eq!(report, None);
}

#[test]
fn single_direct_prev_reports_offset_and_ranges() {
    let source = b"trailer\n<< /Size 8 /Root 1 0 R /Prev 116 >>";

    let report = inspect_classic_xref_trailer_prev(source, 0)
        .expect("trailer prev should inspect")
        .expect("trailer prev should be present");

    assert_eq!(report.prev_byte_offset, 116);
    assert_eq!(
        &source[report.prev_key_range.start..report.prev_key_range.end],
        b"/Prev"
    );
    assert_eq!(
        &source[report.prev_value_range.start..report.prev_value_range.end],
        b"116"
    );
    assert_eq!(report.trailer_dictionary.trailer_byte_offset, 0);
}

#[test]
fn propagates_trailer_dictionary_rejection() {
    let source = b"not-trailer << /Prev 10 >>";

    let error =
        inspect_classic_xref_trailer_prev(source, 0).expect_err("missing trailer should reject");

    assert_eq!(error.byte_offset, 0);
    assert_eq!(error.byte_len, source.len());
    assert_eq!(
        error.reason,
        ClassicXrefTrailerPrevInspectionRejection::TrailerDictionary {
            trailer_reason: ClassicXrefTrailerDictionaryInspectionRejection::MissingTrailerKeyword,
        }
    );
}

#[test]
fn rejects_duplicate_prev() {
    let source = b"trailer << /Prev 10 /Size 2 /Prev 20 >>";

    let error =
        inspect_classic_xref_trailer_prev(source, 0).expect_err("duplicate prev should reject");

    assert!(matches!(
        error.reason,
        ClassicXrefTrailerPrevInspectionRejection::DuplicatePrev { .. }
    ));
}

#[test]
fn rejects_non_integer_prev_value() {
    for (source, expected_kind) in [
        (
            b"trailer << /Prev 5 0 R >>".as_slice(),
            DictionaryValueKind::IndirectReferenceLike,
        ),
        (
            b"trailer << /Prev /Name >>".as_slice(),
            DictionaryValueKind::Name,
        ),
        (
            b"trailer << /Prev -3 >>".as_slice(),
            DictionaryValueKind::NumberLike,
        ),
    ] {
        let error = inspect_classic_xref_trailer_prev(source, 0)
            .expect_err("non-integer prev should reject");

        assert_eq!(
            error.reason,
            ClassicXrefTrailerPrevInspectionRejection::NonIntegerPrevValue {
                value_kind: expected_kind,
            }
        );
    }
}

#[test]
fn rejects_out_of_range_prev_value() {
    let source = format!("trailer << /Prev {}0 >>", usize::MAX).into_bytes();

    let error =
        inspect_classic_xref_trailer_prev(&source, 0).expect_err("overflow prev should reject");

    assert_eq!(
        error.reason,
        ClassicXrefTrailerPrevInspectionRejection::PrevOutOfRange
    );
}

#[test]
fn report_does_not_retain_source_bytes() {
    let source = b"trailer << /Prev 10 /Secret (do-not-copy detail) >>";

    let report = inspect_classic_xref_trailer_prev(source, 0)
        .expect("trailer prev should inspect")
        .expect("trailer prev should be present");

    let debug = format!("{report:?}");
    assert!(!debug.contains("Secret"));
    assert!(!debug.contains("do-not-copy"));
}

#[test]
fn serde_round_trips_inspection_and_rejection_shapes() {
    let source = b"trailer\n<< /Size 8 /Root 1 0 R /Prev 116 >>";
    let report = inspect_classic_xref_trailer_prev(source, 0)
        .expect("trailer prev should inspect")
        .expect("trailer prev should be present");
    let value = serde_value(&report).expect("inspection should serialize");
    let restored: ClassicXrefTrailerPrevInspection =
        from_serde_value(value).expect("inspection should deserialize");
    assert_eq!(restored, report);

    let error =
        inspect_classic_xref_trailer_prev(b"trailer << /Prev -3 >>", 0).expect_err("should reject");
    let value = serde_value(&error).expect("rejection should serialize");
    let restored: ClassicXrefTrailerPrevInspectionError =
        from_serde_value(value).expect("rejection should deserialize");
    assert_eq!(restored, error);
}
