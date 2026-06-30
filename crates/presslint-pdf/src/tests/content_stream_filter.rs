//! Focused tests for the content-stream `/Filter` decode-path classifier.
//!
//! These exercise the classifier directly over single stream objects (no
//! `/Filter`, a name filter, an array filter, the empty array, a multi-filter
//! chain, and every malformed-structure rejection) and then prove the classifier
//! composes with the classic-xref document-access spine: a synthetic single-page
//! `/FlateDecode` content stream is navigated end to end and classified as
//! `Flate`. A serde round-trip pins the public JSON shape of the classification
//! and rejection enums.

#[path = "content_stream_extent/serde_harness.rs"]
#[allow(clippy::duplicate_mod)]
mod serde_harness;

use serde_harness::{from_serde_value, serde_value};

use crate::{
    ContentStreamFilterClassification, ContentStreamFilterClassificationError,
    ContentStreamFilterClassificationRejection, ContentStreamStartInspectionRejection,
    DictionaryValueKind, IndirectObjectBodyLeadingTokenKind, PageContentTargetInspection,
    classify_content_stream_filter, inspect_classic_document_access, inspect_page_content_targets,
    inspect_page_contents,
};

const PDF_PREFIX: &[u8] = b"%PDF-1.7\n";

/// Place a single `4 0 obj` stream object with the given dictionary body right
/// after [`PDF_PREFIX`], classify its `/Filter` declaration, and return both the
/// source bytes and the result so a range-carrying result can be checked against
/// the original bytes. The stream body is arbitrary: the classifier never reads
/// it.
fn classify_capturing(
    dictionary: &[u8],
) -> (
    Vec<u8>,
    Result<ContentStreamFilterClassification, ContentStreamFilterClassificationError>,
) {
    let mut source = PDF_PREFIX.to_vec();
    source.extend_from_slice(b"4 0 obj\n");
    source.extend_from_slice(dictionary);
    source.extend_from_slice(b"\nstream\nIGNORED-BODY\nendstream\nendobj\n");
    let result = classify_content_stream_filter(&source, PDF_PREFIX.len());
    (source, result)
}

/// Classify the single-object source for `dictionary`, discarding the source.
fn classify(
    dictionary: &[u8],
) -> Result<ContentStreamFilterClassification, ContentStreamFilterClassificationError> {
    classify_capturing(dictionary).1
}

/// Classify a dictionary that is expected to succeed.
fn classify_ok(dictionary: &[u8]) -> ContentStreamFilterClassification {
    classify(dictionary).expect("filter classification should succeed")
}

/// Classify a dictionary that is expected to be rejected as malformed.
fn classify_err(dictionary: &[u8]) -> ContentStreamFilterClassificationRejection {
    classify(dictionary)
        .expect_err("filter classification should reject")
        .reason
}

#[test]
fn no_filter_classifies_as_uncompressed() {
    assert_eq!(
        classify_ok(b"<< /Length 12 >>"),
        ContentStreamFilterClassification::Uncompressed
    );
}

#[test]
fn name_flate_classifies_as_flate() {
    assert_eq!(
        classify_ok(b"<< /Length 12 /Filter /FlateDecode >>"),
        ContentStreamFilterClassification::Flate
    );
}

#[test]
fn name_other_classifies_as_unsupported_filter() {
    let (source, result) = classify_capturing(b"<< /Filter /LZWDecode >>");
    let classification = result.expect("name filter classification should succeed");

    let ContentStreamFilterClassification::UnsupportedFilter { filter_name_range } = classification
    else {
        unreachable!("expected an unsupported single filter, got {classification:?}");
    };
    // The report carries only the value byte range, never the filter name bytes.
    assert_eq!(
        &source[filter_name_range.start..filter_name_range.end],
        b"/LZWDecode"
    );
}

#[test]
fn single_element_flate_array_classifies_as_flate() {
    assert_eq!(
        classify_ok(b"<< /Filter [ /FlateDecode ] >>"),
        ContentStreamFilterClassification::Flate
    );
}

#[test]
fn single_element_other_array_classifies_as_unsupported_filter() {
    let (source, result) = classify_capturing(b"<< /Filter [ /DCTDecode ] >>");
    let classification = result.expect("single-element array classification should succeed");

    let ContentStreamFilterClassification::UnsupportedFilter { filter_name_range } = classification
    else {
        unreachable!("expected an unsupported single filter, got {classification:?}");
    };
    assert_eq!(
        &source[filter_name_range.start..filter_name_range.end],
        b"/DCTDecode"
    );
}

#[test]
fn empty_filter_array_classifies_as_uncompressed() {
    assert_eq!(
        classify_ok(b"<< /Filter [] >>"),
        ContentStreamFilterClassification::Uncompressed
    );
    // Whitespace-only array bodies are equivalent to the empty array.
    assert_eq!(
        classify_ok(b"<< /Filter [   ] >>"),
        ContentStreamFilterClassification::Uncompressed
    );
}

#[test]
fn two_filter_array_classifies_as_unsupported_filter_chain() {
    let (source, result) = classify_capturing(b"<< /Filter [ /ASCII85Decode /FlateDecode ] >>");
    let classification = result.expect("two-filter array classification should succeed");

    let ContentStreamFilterClassification::UnsupportedFilterChain {
        filter_value_range,
        filter_count,
    } = classification
    else {
        unreachable!("expected an unsupported filter chain, got {classification:?}");
    };
    assert_eq!(filter_count, 2);
    assert_eq!(
        &source[filter_value_range.start..filter_value_range.end],
        b"[ /ASCII85Decode /FlateDecode ]"
    );
}

#[test]
fn duplicate_filter_is_rejected() {
    let reason = classify_err(b"<< /Filter /FlateDecode /Other 0 /Filter /FlateDecode >>");
    assert!(matches!(
        reason,
        ContentStreamFilterClassificationRejection::DuplicateFilter { .. }
    ));
}

#[test]
fn indirect_reference_filter_value_is_rejected() {
    assert_eq!(
        classify_err(b"<< /Filter 5 0 R >>"),
        ContentStreamFilterClassificationRejection::NonNameOrArrayFilterValue {
            value_kind: DictionaryValueKind::IndirectReferenceLike,
        }
    );
}

#[test]
fn numeric_filter_value_is_rejected() {
    assert_eq!(
        classify_err(b"<< /Filter 1 >>"),
        ContentStreamFilterClassificationRejection::NonNameOrArrayFilterValue {
            value_kind: DictionaryValueKind::NumberLike,
        }
    );
}

#[test]
fn non_name_filter_array_element_is_rejected() {
    assert_eq!(
        classify_err(b"<< /Filter [ /FlateDecode 1 ] >>"),
        ContentStreamFilterClassificationRejection::NonNameFilterArrayElement
    );
}

#[test]
fn malformed_filter_array_is_rejected() {
    let reason = classify_err(b"<< /Filter [ /FlateDecode >>");
    assert!(matches!(
        reason,
        ContentStreamFilterClassificationRejection::MalformedFilterArray { .. }
    ));
}

#[test]
fn delegated_stream_start_failure_is_rejected() {
    // An array-bodied object is not a dictionary-bodied stream, so the delegated
    // `inspect_content_stream_start` failure surfaces verbatim.
    let mut source = PDF_PREFIX.to_vec();
    source.extend_from_slice(b"4 0 obj\n[ /Filter /FlateDecode ]\nendobj\n");

    let reason = classify_content_stream_filter(&source, PDF_PREFIX.len())
        .expect_err("non-dictionary body should reject")
        .reason;

    assert_eq!(
        reason,
        ContentStreamFilterClassificationRejection::StreamStart {
            stream_start_reason: ContentStreamStartInspectionRejection::NonDictionaryBody {
                token_kind: IndirectObjectBodyLeadingTokenKind::ArrayOpen,
            },
        }
    );
}

#[test]
fn report_retains_no_filter_name_bytes() {
    let classification = classify_ok(b"<< /Filter /SecretFilterName >>");
    let debug = format!("{classification:?}");
    assert!(!debug.contains("SecretFilterName"));
}

/// Build a synthetic single-page classic-xref PDF whose one content stream uses
/// the given dictionary body. The stream body is arbitrary; only the `/Filter`
/// declaration is classified.
fn single_page_pdf(content_dict: &[u8]) -> Vec<u8> {
    let catalog = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
    let pages = b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>\nendobj\n";
    let page = b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>\nendobj\n";

    let mut content = Vec::new();
    content.extend_from_slice(b"4 0 obj\n");
    content.extend_from_slice(content_dict);
    content.extend_from_slice(b"\nstream\nIGNORED-BODY\nendstream\nendobj\n");

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

#[test]
fn composes_classifier_over_document_access_spine() {
    let source = single_page_pdf(b"<< /Length 12 /Filter /FlateDecode >>");

    let access = inspect_classic_document_access(&source)
        .expect("classic document-access spine should compose");
    assert_eq!(access.page_leaves.leaf_count(), 1);
    let page_offset = access.page_leaves.leaves[0].object_byte_offset;

    let contents =
        inspect_page_contents(&source, page_offset).expect("page /Contents should inspect");
    let targets = inspect_page_content_targets(&source, &access.xref_table, &contents);
    let PageContentTargetInspection::Resolved {
        object_byte_offset, ..
    } = targets.entries[0]
    else {
        unreachable!("the single content reference should resolve to an object offset");
    };

    let classification = classify_content_stream_filter(&source, object_byte_offset)
        .expect("resolved content stream should classify");
    assert_eq!(classification, ContentStreamFilterClassification::Flate);
}

#[test]
fn serde_round_trips_classification_shapes() {
    let chain = classify_ok(b"<< /Filter [ /A /B ] >>");

    for classification in [
        ContentStreamFilterClassification::Uncompressed,
        ContentStreamFilterClassification::Flate,
        classify_ok(b"<< /Filter /LZWDecode >>"),
        chain,
    ] {
        let value = serde_value(&classification).expect("classification should serialize");
        let restored: ContentStreamFilterClassification =
            from_serde_value(value).expect("classification should deserialize");
        assert_eq!(restored, classification);
    }
}

#[test]
fn serde_round_trips_rejection_shape() {
    let error = classify(b"<< /Filter 5 0 R >>").expect_err("indirect filter should reject");

    let value = serde_value(&error).expect("error should serialize");
    let restored: ContentStreamFilterClassificationError =
        from_serde_value(value).expect("error should deserialize");
    assert_eq!(restored, error);
}
