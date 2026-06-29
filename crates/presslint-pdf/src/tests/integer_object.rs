use crate::{
    ClassicXrefEntryState, ClassicXrefIntegerObjectResolutionRejection, ClassicXrefTableInspection,
    DictionaryValueKind, IndirectObjectBodyLeadingTokenKind,
    IndirectObjectHeaderInspectionRejection, IndirectRef, IndirectReferenceInspectionRejection,
    inspect_dictionary_entries, resolve_classic_xref_integer_object,
};

use super::{classic_entry, classic_inspection, classic_subsection};

const PREFIX: &[u8] = b"%PDF-1.7\n";

/// Build a synthetic source whose object 7 is an in-use object at a known
/// offset, plus a classic xref table that locates it.
///
/// Returns the source bytes, the parsed-style xref inspection, and the byte
/// offset where the caller-supplied `N G R` value begins.
fn build_fixture(
    reference_text: &[u8],
    object_body: &[u8],
    entry_state: ClassicXrefEntryState,
) -> (Vec<u8>, ClassicXrefTableInspection, usize) {
    let mut source = Vec::new();
    source.extend_from_slice(PREFIX);
    let reference_offset = source.len();
    source.extend_from_slice(reference_text);
    source.extend_from_slice(b"\n");
    let object_offset = source.len();
    source.extend_from_slice(object_body);

    let inspection = classic_inspection(vec![
        classic_subsection(
            0,
            vec![classic_entry(0, 65535, 0, ClassicXrefEntryState::Free)],
        ),
        classic_subsection(7, vec![classic_entry(7, 0, object_offset, entry_state)]),
    ]);

    (source, inspection, reference_offset)
}

#[test]
fn resolves_in_use_indirect_integer_object() {
    let (source, xref, reference_offset) = build_fixture(
        b"7 0 R",
        b"7 0 obj\n42\nendobj\n",
        ClassicXrefEntryState::InUse,
    );

    let report = resolve_classic_xref_integer_object(&source, &xref, reference_offset)
        .expect("in-use integer object should resolve");

    assert_eq!(
        report.reference,
        IndirectRef {
            object_number: 7,
            generation: 0,
        }
    );
    assert_eq!(report.value, 42);
    assert_eq!(
        &source[report.value_range.start..report.value_range.end],
        b"42"
    );
    let object_offset = report.object_byte_offset;
    assert_eq!(&source[object_offset..object_offset + 7], b"7 0 obj");
}

#[test]
fn accepts_value_terminated_by_endobj_keyword() {
    let (source, xref, reference_offset) = build_fixture(
        b"7 0 R",
        b"7 0 obj\n42endobj\n",
        ClassicXrefEntryState::InUse,
    );

    let report = resolve_classic_xref_integer_object(&source, &xref, reference_offset)
        .expect("endobj-terminated integer should resolve");

    assert_eq!(report.value, 42);
    assert_eq!(
        &source[report.value_range.start..report.value_range.end],
        b"42"
    );
}

#[test]
fn rejects_free_location() {
    let (source, xref, reference_offset) = build_fixture(
        b"7 0 R",
        b"7 0 obj\n42\nendobj\n",
        ClassicXrefEntryState::Free,
    );

    let error = resolve_classic_xref_integer_object(&source, &xref, reference_offset)
        .expect_err("free location should reject");

    assert_eq!(
        error.reason,
        ClassicXrefIntegerObjectResolutionRejection::FreeObject
    );
    assert_eq!(error.object_number, Some(7));
}

#[test]
fn rejects_not_found_location() {
    let mut source = Vec::new();
    source.extend_from_slice(PREFIX);
    let reference_offset = source.len();
    source.extend_from_slice(b"7 0 R\n");
    let xref = classic_inspection(vec![classic_subsection(
        0,
        vec![classic_entry(0, 65535, 0, ClassicXrefEntryState::Free)],
    )]);

    let error = resolve_classic_xref_integer_object(&source, &xref, reference_offset)
        .expect_err("absent object should reject");

    assert_eq!(
        error.reason,
        ClassicXrefIntegerObjectResolutionRejection::ObjectNotFound
    );
    assert_eq!(error.object_number, Some(7));
}

#[test]
fn rejects_ambiguous_location() {
    let mut source = Vec::new();
    source.extend_from_slice(PREFIX);
    let reference_offset = source.len();
    source.extend_from_slice(b"7 0 R\n");
    let object_offset = source.len();
    source.extend_from_slice(b"7 0 obj\n42\nendobj\n");
    let xref = classic_inspection(vec![
        classic_subsection(
            7,
            vec![classic_entry(
                7,
                0,
                object_offset,
                ClassicXrefEntryState::InUse,
            )],
        ),
        classic_subsection(
            7,
            vec![classic_entry(
                7,
                1,
                object_offset,
                ClassicXrefEntryState::InUse,
            )],
        ),
    ]);

    let error = resolve_classic_xref_integer_object(&source, &xref, reference_offset)
        .expect_err("ambiguous object should reject");

    assert_eq!(
        error.reason,
        ClassicXrefIntegerObjectResolutionRejection::AmbiguousObject
    );
    assert_eq!(error.object_number, Some(7));
}

#[test]
fn rejects_generation_mismatch() {
    let (source, xref, reference_offset) = build_fixture(
        b"7 1 R",
        b"7 0 obj\n42\nendobj\n",
        ClassicXrefEntryState::InUse,
    );

    let error = resolve_classic_xref_integer_object(&source, &xref, reference_offset)
        .expect_err("generation mismatch should reject");

    assert_eq!(
        error.reason,
        ClassicXrefIntegerObjectResolutionRejection::ReferenceMismatch {
            resolved: IndirectRef {
                object_number: 7,
                generation: 0,
            },
        }
    );
    assert_eq!(error.object_number, Some(7));
}

#[test]
fn rejects_header_failure() {
    let (source, xref, reference_offset) = build_fixture(
        b"7 0 R",
        b"not an object header\n",
        ClassicXrefEntryState::InUse,
    );

    let error = resolve_classic_xref_integer_object(&source, &xref, reference_offset)
        .expect_err("malformed object header should reject");

    assert_eq!(
        error.reason,
        ClassicXrefIntegerObjectResolutionRejection::Header {
            header_reason: IndirectObjectHeaderInspectionRejection::MalformedHeader,
        }
    );
}

#[test]
fn rejects_non_numeric_body_token() {
    let (source, xref, reference_offset) = build_fixture(
        b"7 0 R",
        b"7 0 obj\n/Foo\nendobj\n",
        ClassicXrefEntryState::InUse,
    );

    let error = resolve_classic_xref_integer_object(&source, &xref, reference_offset)
        .expect_err("name body token should reject");

    assert_eq!(
        error.reason,
        ClassicXrefIntegerObjectResolutionRejection::NonIntegerBody {
            token_kind: IndirectObjectBodyLeadingTokenKind::Name,
        }
    );
}

#[test]
fn rejects_negative_integer() {
    let (source, xref, reference_offset) = build_fixture(
        b"7 0 R",
        b"7 0 obj\n-1\nendobj\n",
        ClassicXrefEntryState::InUse,
    );

    let error = resolve_classic_xref_integer_object(&source, &xref, reference_offset)
        .expect_err("negative integer should reject");

    assert_eq!(
        error.reason,
        ClassicXrefIntegerObjectResolutionRejection::MalformedInteger
    );
}

#[test]
fn rejects_decimal_value() {
    let (source, xref, reference_offset) = build_fixture(
        b"7 0 R",
        b"7 0 obj\n1.0\nendobj\n",
        ClassicXrefEntryState::InUse,
    );

    let error = resolve_classic_xref_integer_object(&source, &xref, reference_offset)
        .expect_err("decimal value should reject");

    assert_eq!(
        error.reason,
        ClassicXrefIntegerObjectResolutionRejection::MalformedInteger
    );
}

#[test]
fn rejects_signed_positive_integer() {
    let (source, xref, reference_offset) = build_fixture(
        b"7 0 R",
        b"7 0 obj\n+1\nendobj\n",
        ClassicXrefEntryState::InUse,
    );

    let error = resolve_classic_xref_integer_object(&source, &xref, reference_offset)
        .expect_err("explicitly signed integer should reject");

    assert_eq!(
        error.reason,
        ClassicXrefIntegerObjectResolutionRejection::MalformedInteger
    );
}

#[test]
fn rejects_trailing_non_delimiter_garbage() {
    let (source, xref, reference_offset) = build_fixture(
        b"7 0 R",
        b"7 0 obj\n42x\nendobj\n",
        ClassicXrefEntryState::InUse,
    );

    let error = resolve_classic_xref_integer_object(&source, &xref, reference_offset)
        .expect_err("trailing garbage should reject");

    assert_eq!(
        error.reason,
        ClassicXrefIntegerObjectResolutionRejection::MalformedInteger
    );
}

#[test]
fn rejects_out_of_range_integer() {
    let (source, xref, reference_offset) = build_fixture(
        b"7 0 R",
        b"7 0 obj\n999999999999999999999999999999\nendobj\n",
        ClassicXrefEntryState::InUse,
    );

    let error = resolve_classic_xref_integer_object(&source, &xref, reference_offset)
        .expect_err("oversized integer should reject");

    assert_eq!(
        error.reason,
        ClassicXrefIntegerObjectResolutionRejection::IntegerOutOfRange
    );
}

#[test]
fn rejects_malformed_reference_at_offset() {
    let (source, xref, reference_offset) = build_fixture(
        b"oops",
        b"7 0 obj\n42\nendobj\n",
        ClassicXrefEntryState::InUse,
    );

    let error = resolve_classic_xref_integer_object(&source, &xref, reference_offset)
        .expect_err("malformed N G R should reject");

    assert_eq!(
        error.reason,
        ClassicXrefIntegerObjectResolutionRejection::Reference {
            reference_reason: IndirectReferenceInspectionRejection::MalformedReference,
        }
    );
    assert_eq!(error.object_number, None);
}

#[test]
fn resolves_dictionary_length_style_indirect_reference() {
    let mut source = Vec::new();
    source.extend_from_slice(PREFIX);
    let dictionary_offset = source.len();
    source.extend_from_slice(b"<< /Length 7 0 R >>\n");
    let object_offset = source.len();
    source.extend_from_slice(b"7 0 obj\n11\nendobj\n");

    let entries = inspect_dictionary_entries(&source, dictionary_offset)
        .expect("dictionary entries should inspect");
    let length_entry = entries
        .entries
        .iter()
        .find(|entry| &source[entry.key_range.start..entry.key_range.end] == b"/Length")
        .copied()
        .expect("dictionary should expose a /Length entry");
    assert_eq!(
        length_entry.value_kind,
        DictionaryValueKind::IndirectReferenceLike
    );

    let xref = classic_inspection(vec![classic_subsection(
        7,
        vec![classic_entry(
            7,
            0,
            object_offset,
            ClassicXrefEntryState::InUse,
        )],
    )]);

    let report =
        resolve_classic_xref_integer_object(&source, &xref, length_entry.value_range.start)
            .expect("indirect /Length should resolve to an integer");

    assert_eq!(
        report.reference,
        IndirectRef {
            object_number: 7,
            generation: 0,
        }
    );
    assert_eq!(report.value, 11);
}
