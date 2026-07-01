use std::fmt::Write as _;

use crate::{
    ContentStreamFilterClassification, ObjectStreamMemberExtractionRejection,
    extract_object_stream_member,
};

const MAX: usize = 4096;

/// Build the raw decoded object-stream body for `(object_number, body)` members
/// and return the `/First` header length plus the concatenated body bytes.
fn object_stream_body(members: &[(usize, &[u8])]) -> (usize, Vec<u8>) {
    let mut header = String::new();
    let mut objects = Vec::new();
    let mut offset = 0usize;
    for (number, body) in members {
        write!(header, "{number} {offset} ").expect("writing to a String cannot fail");
        objects.extend_from_slice(body);
        offset += body.len();
    }
    let first = header.len();
    let mut decoded = header.into_bytes();
    decoded.extend_from_slice(&objects);
    (first, decoded)
}

/// Wrap `inner` dictionary fields plus a computed `/Length` as a `5 0 obj`
/// `/ObjStm` object at source offset zero with the given raw stream body.
fn objstm(inner: &str, body: &[u8]) -> Vec<u8> {
    let dictionary = format!("<< {inner} /Length {} >>", body.len());
    let mut source = b"5 0 obj\n".to_vec();
    source.extend_from_slice(dictionary.as_bytes());
    source.extend_from_slice(b"\nstream\n");
    source.extend_from_slice(body);
    source.extend_from_slice(b"\nendstream\nendobj\n");
    source
}

/// Minimal valid zlib stream using a single stored (uncompressed) deflate block,
/// so the `/FlateDecode` path is exercised without a deflate encoder dependency.
fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01, 0x01];
    let len = u16::try_from(data.len()).expect("test body length fits u16");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn reason(
    source: &[u8],
    object_number: u32,
    index: usize,
) -> ObjectStreamMemberExtractionRejection {
    extract_object_stream_member(source, 0, object_number, index, MAX)
        .expect_err("object-stream member extraction should reject")
        .reason
}

#[test]
fn extracts_first_and_last_member_bodies_from_raw_object_stream() {
    let members: [(usize, &[u8]); 2] = [(10, b"<< /Type /Catalog >>"), (11, b"<< /Type /Pages >>")];
    let (first, body) = object_stream_body(&members);
    let source = objstm(&format!("/Type /ObjStm /N 2 /First {first}"), &body);

    let head =
        extract_object_stream_member(&source, 0, 10, 0, MAX).expect("first member should extract");
    assert_eq!(head.object_count, 2);
    assert_eq!(head.first_body_byte_offset, first);
    assert!(!head.has_extends);
    assert_eq!(
        &head.decoded_object_stream[head.object_body_span.clone()],
        b"<< /Type /Catalog >>"
    );

    let tail =
        extract_object_stream_member(&source, 0, 11, 1, MAX).expect("last member should extract");
    assert_eq!(tail.object_body_span.end, tail.decoded_object_stream.len());
    assert_eq!(
        &tail.decoded_object_stream[tail.object_body_span.clone()],
        b"<< /Type /Pages >>"
    );
}

#[test]
fn extracts_member_from_flate_decoded_object_stream() {
    let members: [(usize, &[u8]); 2] = [(10, b"<< /Type /Catalog >>"), (11, b"<< /Type /Pages >>")];
    let (first, body) = object_stream_body(&members);
    let encoded = zlib_store(&body);
    let source = objstm(
        &format!("/Type /ObjStm /N 2 /First {first} /Filter /FlateDecode"),
        &encoded,
    );

    let extracted = extract_object_stream_member(&source, 0, 11, 1, MAX)
        .expect("flate object-stream member should extract");
    assert_eq!(
        &extracted.decoded_object_stream[extracted.object_body_span.clone()],
        b"<< /Type /Pages >>"
    );
}

#[test]
fn records_but_does_not_follow_extends() {
    let members: [(usize, &[u8]); 1] = [(10, b"<< /Type /Catalog >>")];
    let (first, body) = object_stream_body(&members);
    let source = objstm(
        &format!("/Type /ObjStm /N 1 /First {first} /Extends 9 0 R"),
        &body,
    );

    let extracted = extract_object_stream_member(&source, 0, 10, 0, MAX)
        .expect("member should extract despite /Extends");
    assert!(extracted.has_extends);
}

#[test]
fn rejects_non_objstm_type() {
    let members: [(usize, &[u8]); 1] = [(10, b"<<>>")];
    let (first, body) = object_stream_body(&members);
    let source = objstm(&format!("/Type /XRef /N 1 /First {first}"), &body);

    assert_eq!(
        reason(&source, 10, 0),
        ObjectStreamMemberExtractionRejection::UnexpectedType
    );
}

#[test]
fn rejects_missing_and_malformed_object_count() {
    let members: [(usize, &[u8]); 1] = [(10, b"<<>>")];
    let (first, body) = object_stream_body(&members);

    let missing = objstm(&format!("/Type /ObjStm /First {first}"), &body);
    assert_eq!(
        reason(&missing, 10, 0),
        ObjectStreamMemberExtractionRejection::MissingObjectCount
    );

    let malformed = objstm(&format!("/Type /ObjStm /N x /First {first}"), &body);
    assert_eq!(
        reason(&malformed, 10, 0),
        ObjectStreamMemberExtractionRejection::MalformedObjectCount
    );
}

#[test]
fn rejects_missing_and_malformed_first() {
    let members: [(usize, &[u8]); 1] = [(10, b"<<>>")];
    let (_first, body) = object_stream_body(&members);

    let missing = objstm("/Type /ObjStm /N 1", &body);
    assert_eq!(
        reason(&missing, 10, 0),
        ObjectStreamMemberExtractionRejection::MissingFirst
    );

    let malformed = objstm("/Type /ObjStm /N 1 /First x", &body);
    assert_eq!(
        reason(&malformed, 10, 0),
        ObjectStreamMemberExtractionRejection::MalformedFirst
    );
}

#[test]
fn rejects_first_beyond_decoded_body() {
    let members: [(usize, &[u8]); 1] = [(10, b"<<>>")];
    let (_first, body) = object_stream_body(&members);
    let decoded_len = body.len();
    let source = objstm("/Type /ObjStm /N 1 /First 9999", &body);

    assert_eq!(
        reason(&source, 10, 0),
        ObjectStreamMemberExtractionRejection::FirstBeyondDecoded {
            first: 9999,
            decoded_len,
        }
    );
}

#[test]
fn rejects_malformed_and_wrong_count_headers() {
    // A non-digit header token.
    let mut malformed = b"10 x 11 0 ".to_vec();
    let first = malformed.len();
    malformed.extend_from_slice(b"<<>><<>>");
    let source = objstm(&format!("/Type /ObjStm /N 2 /First {first}"), &malformed);
    assert_eq!(
        reason(&source, 10, 0),
        ObjectStreamMemberExtractionRejection::MalformedHeaderInteger
    );

    // Three header integers where `/N 2` needs four.
    let mut short = b"10 0 11 ".to_vec();
    let first = short.len();
    short.extend_from_slice(b"<<>><<>>");
    let source = objstm(&format!("/Type /ObjStm /N 2 /First {first}"), &short);
    assert_eq!(
        reason(&source, 10, 0),
        ObjectStreamMemberExtractionRejection::HeaderPairCountMismatch {
            expected_integers: 4,
            actual_integers: 3,
        }
    );
}

#[test]
fn rejects_out_of_range_and_non_increasing_offsets() {
    let mut out_of_range = b"10 0 11 9999 ".to_vec();
    let first = out_of_range.len();
    out_of_range.extend_from_slice(b"<<>><<>>");
    let decoded_len = out_of_range.len();
    let source = objstm(&format!("/Type /ObjStm /N 2 /First {first}"), &out_of_range);
    assert_eq!(
        reason(&source, 10, 0),
        ObjectStreamMemberExtractionRejection::OffsetOutOfRange {
            index: 1,
            offset: 9999,
            decoded_len,
        }
    );

    let mut flat = b"10 0 11 0 ".to_vec();
    let first = flat.len();
    flat.extend_from_slice(b"<<>><<>>");
    let source = objstm(&format!("/Type /ObjStm /N 2 /First {first}"), &flat);
    assert_eq!(
        reason(&source, 10, 0),
        ObjectStreamMemberExtractionRejection::OffsetNotStrictlyIncreasing { index: 1 }
    );
}

#[test]
fn rejects_out_of_range_index() {
    let members: [(usize, &[u8]); 2] = [(10, b"<<>>"), (11, b"<<>>")];
    let (first, body) = object_stream_body(&members);
    let source = objstm(&format!("/Type /ObjStm /N 2 /First {first}"), &body);

    assert_eq!(
        reason(&source, 10, 5),
        ObjectStreamMemberExtractionRejection::IndexOutOfRange {
            index: 5,
            object_count: 2,
        }
    );
}

#[test]
fn rejects_object_number_mismatch() {
    let members: [(usize, &[u8]); 2] = [(10, b"<<>>"), (11, b"<<>>")];
    let (first, body) = object_stream_body(&members);
    let source = objstm(&format!("/Type /ObjStm /N 2 /First {first}"), &body);

    assert_eq!(
        reason(&source, 99, 0),
        ObjectStreamMemberExtractionRejection::ObjectNumberMismatch {
            expected: 99,
            found: 10,
        }
    );
}

#[test]
fn rejects_member_body_with_indirect_header() {
    let members: [(usize, &[u8]); 1] = [(10, b"10 0 obj\n<< >>\nendobj")];
    let (first, body) = object_stream_body(&members);
    let source = objstm(&format!("/Type /ObjStm /N 1 /First {first}"), &body);

    assert_eq!(
        reason(&source, 10, 0),
        ObjectStreamMemberExtractionRejection::BodyBeginsWithIndirectHeader
    );
}

#[test]
fn rejects_unsupported_filter() {
    let members: [(usize, &[u8]); 1] = [(10, b"<<>>")];
    let (first, body) = object_stream_body(&members);
    let source = objstm(
        &format!("/Type /ObjStm /N 1 /First {first} /Filter /ASCIIHexDecode"),
        &body,
    );

    assert!(matches!(
        reason(&source, 10, 0),
        ObjectStreamMemberExtractionRejection::UnsupportedFilter {
            classification: ContentStreamFilterClassification::UnsupportedFilter { .. },
        }
    ));
}

#[test]
fn rejects_body_exceeding_decode_limit() {
    let members: [(usize, &[u8]); 1] = [(10, b"<< /Type /Catalog >>")];
    let (first, body) = object_stream_body(&members);
    let length = body.len();
    let source = objstm(&format!("/Type /ObjStm /N 1 /First {first}"), &body);

    assert_eq!(
        extract_object_stream_member(&source, 0, 10, 0, length - 1)
            .expect_err("an unfiltered body over the limit should reject")
            .reason,
        ObjectStreamMemberExtractionRejection::DecodedObjectStreamTooLarge {
            length,
            limit: length - 1,
        }
    );
}

#[test]
fn extracted_member_does_not_retain_dictionary_bytes() {
    let members: [(usize, &[u8]); 1] = [(10, b"<< /Type /Catalog >>")];
    let (first, body) = object_stream_body(&members);
    let source = objstm(
        &format!("/Type /ObjStm /Secret (do-not-copy) /N 1 /First {first}"),
        &body,
    );

    let extracted =
        extract_object_stream_member(&source, 0, 10, 0, MAX).expect("member should extract");
    let debug = format!("{extracted:?}");
    assert!(!debug.contains("do-not-copy"));
    assert!(!debug.contains("Secret"));
}
