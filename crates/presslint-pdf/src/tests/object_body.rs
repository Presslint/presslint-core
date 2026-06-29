use crate::{
    ClassicXrefObjectLocation, IndirectObjectBodyLeadingTokenKind,
    IndirectObjectBodyTokenInspectionRejection, IndirectRef, inspect_classic_xref_table,
    inspect_indirect_object_body_token, inspect_indirect_object_header, inspect_pdf_source,
    resolve_classic_xref_object,
};

#[test]
fn indirect_object_body_token_reports_dictionary_open_after_whitespace() {
    let source = b" \t\r\n<< /Type /Example >>";

    let report = inspect_indirect_object_body_token(source, 0).expect("token should inspect");

    assert_eq!(report.byte_offset, 0);
    assert_eq!(report.first_token_byte_offset, 4);
    assert_eq!(
        report.token_kind,
        IndirectObjectBodyLeadingTokenKind::DictionaryOpen
    );
}

#[test]
fn indirect_object_body_token_distinguishes_hex_string_open_from_dictionary_open() {
    let source = b"<48656c6c6f>";

    let report = inspect_indirect_object_body_token(source, 0).expect("token should inspect");

    assert_eq!(report.first_token_byte_offset, 0);
    assert_eq!(
        report.token_kind,
        IndirectObjectBodyLeadingTokenKind::HexStringOpen
    );
}

#[test]
fn indirect_object_body_token_reports_array_open() {
    let report = inspect_indirect_object_body_token(b"[1 2 3]", 0).expect("token should inspect");

    assert_eq!(
        report.token_kind,
        IndirectObjectBodyLeadingTokenKind::ArrayOpen
    );
}

#[test]
fn indirect_object_body_token_reports_name() {
    let report =
        inspect_indirect_object_body_token(b"/DeviceRGB", 0).expect("token should inspect");

    assert_eq!(report.token_kind, IndirectObjectBodyLeadingTokenKind::Name);
}

#[test]
fn indirect_object_body_token_reports_literal_string() {
    let report =
        inspect_indirect_object_body_token(b"(not parsed)", 0).expect("token should inspect");

    assert_eq!(
        report.token_kind,
        IndirectObjectBodyLeadingTokenKind::LiteralString
    );
}

#[test]
fn indirect_object_body_token_reports_number_like_starts() {
    for source in [
        b"123".as_slice(),
        b"-42".as_slice(),
        b"+3".as_slice(),
        b".5".as_slice(),
    ] {
        let report = inspect_indirect_object_body_token(source, 0).expect("token should inspect");

        assert_eq!(
            report.token_kind,
            IndirectObjectBodyLeadingTokenKind::NumberLike
        );
    }
}

#[test]
fn indirect_object_body_token_reports_booleans_with_keyword_boundary() {
    for source in [b"true".as_slice(), b"false /Next".as_slice()] {
        let report = inspect_indirect_object_body_token(source, 0).expect("token should inspect");

        assert_eq!(
            report.token_kind,
            IndirectObjectBodyLeadingTokenKind::Boolean
        );
    }

    let error = inspect_indirect_object_body_token(b"trueValue", 0)
        .expect_err("regular token prefix should not classify as boolean");

    assert_eq!(
        error.reason,
        IndirectObjectBodyTokenInspectionRejection::UnclassifiedLeadingByte
    );
}

#[test]
fn indirect_object_body_token_reports_null_with_keyword_boundary() {
    let report = inspect_indirect_object_body_token(b"null]", 0).expect("token should inspect");

    assert_eq!(report.token_kind, IndirectObjectBodyLeadingTokenKind::Null);

    let error = inspect_indirect_object_body_token(b"nullish", 0)
        .expect_err("regular token prefix should not classify as null");

    assert_eq!(
        error.reason,
        IndirectObjectBodyTokenInspectionRejection::UnclassifiedLeadingByte
    );
}

#[test]
fn indirect_object_body_token_rejects_offset_at_eof_and_out_of_bounds() {
    let source = b"<<>>";

    let at_eof =
        inspect_indirect_object_body_token(source, source.len()).expect_err("eof should reject");
    let out_of_bounds = inspect_indirect_object_body_token(source, source.len() + 1)
        .expect_err("oob should reject");

    assert_eq!(
        at_eof.reason,
        IndirectObjectBodyTokenInspectionRejection::OffsetOutOfBounds
    );
    assert_eq!(
        out_of_bounds.reason,
        IndirectObjectBodyTokenInspectionRejection::OffsetOutOfBounds
    );
}

#[test]
fn indirect_object_body_token_rejects_whitespace_only_before_eof() {
    let source = b"obj \t\r\n";

    let error = inspect_indirect_object_body_token(source, 3).expect_err("no token should reject");

    assert_eq!(
        error.reason,
        IndirectObjectBodyTokenInspectionRejection::NoSignificantToken
    );
    assert_eq!(error.error_byte_offset, Some(source.len()));
}

#[test]
fn indirect_object_body_token_rejects_unclassified_leading_bytes_without_copying_body() {
    let source = b"% comment that is not copied\nendobj\n";

    let error = inspect_indirect_object_body_token(source, 0).expect_err("comment should reject");

    assert_eq!(
        error.reason,
        IndirectObjectBodyTokenInspectionRejection::UnclassifiedLeadingByte
    );
    assert_eq!(error.error_byte_offset, Some(0));
    assert_eq!(error.byte_len, source.len());

    let debug_report = format!("{error:?}");
    assert!(!debug_report.contains("comment"));
    assert!(!debug_report.contains("endobj"));
}

#[test]
fn indirect_object_body_token_composes_with_classic_xref_and_object_header() {
    let prefix = b"%PDF-1.7\n";
    let object = b"3 2 obj\n[ /BodyNotParsed 9 0 R ]\nendobj\n";
    let object_offset = prefix.len();
    let xref_offset = prefix.len() + object.len();
    let source = format!(
        "{}{}xref\n0 4\n0000000000 65535 f \n0000000000 65535 f \n0000000000 65535 f \n{object_offset:010} 00002 n \ntrailer\n<< /Size 4 >>\nstartxref\n{xref_offset}\n%%EOF\n",
        String::from_utf8_lossy(prefix),
        String::from_utf8_lossy(object),
    )
    .into_bytes();

    let source_report = inspect_pdf_source(&source).expect("source should inspect");
    let startxref = source_report.startxref.expect("startxref should inspect");
    let xref_report =
        inspect_classic_xref_table(&source, startxref.byte_offset).expect("xref should inspect");
    let location = resolve_classic_xref_object(&xref_report, 3);
    let expected_location = ClassicXrefObjectLocation::InUse {
        object_number: 3,
        generation: 2,
        byte_offset: object_offset,
    };
    let ClassicXrefObjectLocation::InUse {
        object_number,
        generation,
        byte_offset,
    } = location
    else {
        assert_eq!(location, expected_location);
        return;
    };
    let header =
        inspect_indirect_object_header(&source, byte_offset).expect("header should inspect");
    let body = inspect_indirect_object_body_token(&source, header.after_obj_keyword_offset)
        .expect("body token should inspect");

    assert_eq!(location, expected_location);
    assert_eq!(object_number, 3);
    assert_eq!(generation, 2);
    assert_eq!(
        header.reference,
        IndirectRef {
            object_number: 3,
            generation: 2,
        }
    );
    assert_eq!(
        body.first_token_byte_offset,
        object_offset + b"3 2 obj\n".len()
    );
    assert_eq!(
        body.token_kind,
        IndirectObjectBodyLeadingTokenKind::ArrayOpen
    );
}
