use crate::{
    ArrayExtentInspectionRejection, IndirectObjectBodyLeadingTokenKind, inspect_array_extent,
    inspect_indirect_object_body_token, inspect_indirect_object_header,
};

#[test]
fn flat_array_reports_close_and_after_close_offsets() {
    let source = b"[ 1 2 3 ]";

    let report = inspect_array_extent(source, 0).expect("flat array should inspect");

    assert_eq!(report.byte_offset, 0);
    assert_eq!(report.open_byte_offset, 0);
    assert_eq!(report.close_byte_offset, source.len() - 1);
    assert_eq!(report.after_close_byte_offset, source.len());
    assert_eq!(report.max_observed_depth, 1);
    assert_eq!(
        &source[report.close_byte_offset..report.after_close_byte_offset],
        b"]"
    );
}

#[test]
fn leading_whitespace_is_skipped_before_the_array_open() {
    let source = b" \t\r\n[ 1 2 ]tail";

    let report = inspect_array_extent(source, 0).expect("array should inspect");

    assert_eq!(report.open_byte_offset, 4);
    assert_eq!(report.after_close_byte_offset, source.len() - b"tail".len());
    assert_eq!(
        &source[report.open_byte_offset..report.after_close_byte_offset],
        b"[ 1 2 ]"
    );
}

#[test]
fn nested_array_reports_outermost_matching_close() {
    let source = b"[ 1 [ 2 3 ] 4 ]";

    let report = inspect_array_extent(source, 0).expect("nested array should inspect");

    assert_eq!(report.close_byte_offset, source.len() - 1);
    assert_eq!(report.after_close_byte_offset, source.len());
    assert_eq!(report.max_observed_depth, 2);
    // The first inner `]` must not be reported as the close.
    let first_inner_close = source
        .iter()
        .position(|&b| b == b']')
        .expect("inner close exists");
    assert!(report.close_byte_offset > first_inner_close);
}

#[test]
fn close_delimiter_inside_literal_string_does_not_close_array() {
    let source = b"[ (a ] b) 1 ]";

    let report = inspect_array_extent(source, 0).expect("array should inspect");

    assert_eq!(report.close_byte_offset, source.len() - 1);
    assert_eq!(report.after_close_byte_offset, source.len());
    assert_eq!(report.max_observed_depth, 1);
}

#[test]
fn close_delimiter_inside_hex_string_does_not_close_array() {
    // `5D` is the ASCII code for `]`; it must stay opaque inside the hex string.
    let source = b"[ <5D> 1 ]";

    let report = inspect_array_extent(source, 0).expect("array should inspect");

    assert_eq!(report.close_byte_offset, source.len() - 1);
    assert_eq!(report.after_close_byte_offset, source.len());
    assert_eq!(report.max_observed_depth, 1);
}

#[test]
fn close_delimiter_inside_comment_does_not_close_array() {
    let source = b"[ 1 % a comment with ]\n2 ]";

    let report = inspect_array_extent(source, 0).expect("array should inspect");

    assert_eq!(report.close_byte_offset, source.len() - 1);
    assert_eq!(report.after_close_byte_offset, source.len());
}

#[test]
fn escaped_parentheses_inside_literal_string_keep_string_balanced() {
    // The escaped `\)` does not close the string, so the `]` inside it is opaque.
    let source = b"[ (escaped \\) ] still open) 1 ]";

    let report = inspect_array_extent(source, 0).expect("array should inspect");

    assert_eq!(report.close_byte_offset, source.len() - 1);
    assert_eq!(report.after_close_byte_offset, source.len());
}

#[test]
fn nested_dictionary_value_does_not_break_depth_tracking() {
    // The `<<` open must not be misread as a hex-string open, and the `]` inside
    // the nested dictionary's hex string must stay opaque.
    let source = b"[ << /K <5D> /M [ 7 ] >> 1 ]";

    let report = inspect_array_extent(source, 0).expect("array should inspect");

    assert_eq!(report.close_byte_offset, source.len() - 1);
    assert_eq!(report.after_close_byte_offset, source.len());
    // Only `[`/`]` nesting is counted: the outer array plus the `[ 7 ]` value.
    assert_eq!(report.max_observed_depth, 2);
}

#[test]
fn empty_array_reports_zero_length_body() {
    let report = inspect_array_extent(b"[]", 0).expect("empty array should inspect");

    assert_eq!(report.open_byte_offset, 0);
    assert_eq!(report.close_byte_offset, 1);
    assert_eq!(report.after_close_byte_offset, 2);
    assert_eq!(report.max_observed_depth, 1);
}

#[test]
fn unterminated_array_is_rejected() {
    let source = b"[ 1 [ 2 ] 3 ";

    let error = inspect_array_extent(source, 0).expect_err("missing close should reject");

    assert_eq!(
        error.reason,
        ArrayExtentInspectionRejection::UnterminatedArray
    );
    assert_eq!(error.error_byte_offset, None);
    assert_eq!(error.byte_len, source.len());
}

#[test]
fn unterminated_literal_string_is_rejected_at_its_open() {
    let source = b"[ (never closed ] ";

    let error = inspect_array_extent(source, 0).expect_err("open string should reject");

    assert_eq!(
        error.reason,
        ArrayExtentInspectionRejection::UnterminatedString
    );
    let open_paren = source
        .iter()
        .position(|&b| b == b'(')
        .expect("paren exists");
    assert_eq!(error.error_byte_offset, Some(open_paren));
}

#[test]
fn unterminated_hex_string_is_rejected_at_its_open() {
    let source = b"[ <41 42 43 ";

    let error = inspect_array_extent(source, 0).expect_err("open hex should reject");

    assert_eq!(
        error.reason,
        ArrayExtentInspectionRejection::UnterminatedString
    );
    let open_hex = source
        .iter()
        .position(|&b| b == b'<')
        .expect("hex open exists");
    assert_eq!(error.error_byte_offset, Some(open_hex));
}

#[test]
fn non_array_first_token_is_rejected() {
    let source = b"<< /NotAnArray 1 >>";

    let error = inspect_array_extent(source, 0).expect_err("dictionary should reject");

    assert_eq!(error.reason, ArrayExtentInspectionRejection::NotArrayOpen);
    assert_eq!(error.error_byte_offset, Some(0));
}

#[test]
fn offset_at_or_after_eof_is_rejected() {
    let source = b"[]";

    let at_eof = inspect_array_extent(source, source.len()).expect_err("eof should reject");
    let out_of_bounds =
        inspect_array_extent(source, source.len() + 1).expect_err("oob should reject");

    assert_eq!(
        at_eof.reason,
        ArrayExtentInspectionRejection::OffsetOutOfBounds
    );
    assert_eq!(
        out_of_bounds.reason,
        ArrayExtentInspectionRejection::OffsetOutOfBounds
    );
}

#[test]
fn whitespace_only_tail_is_rejected() {
    let source = b"obj \t\r\n";

    let error = inspect_array_extent(source, 3).expect_err("no token should reject");

    assert_eq!(
        error.reason,
        ArrayExtentInspectionRejection::NoSignificantToken
    );
    assert_eq!(error.error_byte_offset, Some(source.len()));
}

#[test]
fn excessive_nesting_is_rejected_without_unbounded_work() {
    let source = "[".repeat(300).into_bytes();

    let error = inspect_array_extent(&source, 0).expect_err("deep nesting should reject");

    assert_eq!(
        error.reason,
        ArrayExtentInspectionRejection::MaxNestingExceeded
    );
    assert!(error.error_byte_offset.is_some());
}

#[test]
fn rejection_does_not_retain_array_bytes() {
    let source = b"[ (corpus-detail not copied) 1 ";

    let error = inspect_array_extent(source, 0).expect_err("unterminated should reject");

    let debug_report = format!("{error:?}");
    assert!(!debug_report.contains("corpus-detail"));
}

#[test]
fn array_extent_composes_with_object_header_and_body_token() {
    let prefix = b"%PDF-1.7\n";
    let header = b"5 0 obj\n";
    let array = b"[ /Foo 1 [ 2 3 ] (bar) ]";
    let suffix = b"\nendobj\n";

    let mut source = Vec::new();
    source.extend_from_slice(prefix);
    source.extend_from_slice(header);
    source.extend_from_slice(array);
    source.extend_from_slice(suffix);

    let object_offset = prefix.len();
    let array_offset = prefix.len() + header.len();

    let header_report =
        inspect_indirect_object_header(&source, object_offset).expect("header should inspect");
    let body = inspect_indirect_object_body_token(&source, header_report.after_obj_keyword_offset)
        .expect("body token should inspect");

    assert_eq!(
        body.token_kind,
        IndirectObjectBodyLeadingTokenKind::ArrayOpen
    );
    assert_eq!(body.first_token_byte_offset, array_offset);

    let extent =
        inspect_array_extent(&source, body.first_token_byte_offset).expect("extent should inspect");

    assert_eq!(extent.open_byte_offset, array_offset);
    assert_eq!(extent.after_close_byte_offset, array_offset + array.len());
    assert_eq!(
        &source[extent.close_byte_offset..extent.after_close_byte_offset],
        b"]"
    );
    // The outer array plus the nested `[ 2 3 ]` sub-array.
    assert_eq!(extent.max_observed_depth, 2);
    // The reported extent ends exactly where the trailing `\nendobj` begins.
    assert_eq!(&source[extent.after_close_byte_offset..], suffix);
}
