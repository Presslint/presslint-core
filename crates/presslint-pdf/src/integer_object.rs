use serde::{Deserialize, Serialize};

use crate::source_utils::{
    consume_keyword, count_leading_digits, is_pdf_delimiter, is_pdf_whitespace, parse_usize_decimal,
};
use crate::{
    ClassicXrefObjectLocation, ClassicXrefTableInspection, IndirectObjectBodyLeadingTokenKind,
    IndirectObjectBodyTokenInspectionRejection, IndirectObjectHeaderInspectionRejection,
    IndirectRef, IndirectReferenceInspectionRejection, inspect_indirect_object_body_token,
    inspect_indirect_object_header, parse_indirect_reference, resolve_classic_xref_object,
};

const ENDOBJ_KEYWORD: &[u8] = b"endobj";

/// Source byte range covering a resolved integer object value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegerObjectValueByteRange {
    /// Inclusive start byte offset.
    pub start: usize,
    /// Exclusive end byte offset.
    pub end: usize,
}

/// Resolved non-negative integer object value reached through a classic
/// cross-reference table.
///
/// This report stores only the parsed reference, the resolved in-use object
/// byte offset, the integer value byte range, and the parsed `usize`. It does
/// not retain or copy PDF bytes, object bodies, stream bodies, dictionaries, or
/// source slices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassicXrefIntegerObjectResolution {
    /// Parsed `N G R` reference resolved through the classic xref table.
    pub reference: IndirectRef,
    /// Byte offset of the resolved in-use object, from the xref entry.
    pub object_byte_offset: usize,
    /// Byte range covering the non-negative integer value digits.
    pub value_range: IntegerObjectValueByteRange,
    /// Parsed non-negative integer value.
    pub value: usize,
}

/// Error returned when a classic-xref indirect integer object cannot be
/// resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassicXrefIntegerObjectResolutionError {
    /// Caller-supplied byte offset where the `N G R` value begins.
    pub byte_offset: usize,
    /// Total source length.
    pub byte_len: usize,
    /// Byte offset where the malformed or unsupported construct was found, when
    /// available.
    pub error_byte_offset: Option<usize>,
    /// Resolved object number, when a reference was parsed.
    pub object_number: Option<u32>,
    /// Structured failure reason.
    pub reason: ClassicXrefIntegerObjectResolutionRejection,
}

/// Structured classic-xref integer object resolution rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ClassicXrefIntegerObjectResolutionRejection {
    /// The `N G R` value at the caller offset could not be parsed.
    Reference {
        /// Underlying indirect-reference parsing rejection reason.
        reference_reason: IndirectReferenceInspectionRejection,
    },
    /// The resolved object number matched a free classic-xref entry.
    FreeObject,
    /// The resolved object number is absent from the classic-xref table.
    ObjectNotFound,
    /// The resolved object number appears in more than one classic-xref entry.
    AmbiguousObject,
    /// The resolved in-use object's header could not be inspected.
    Header {
        /// Underlying indirect-object-header inspection rejection reason.
        header_reason: IndirectObjectHeaderInspectionRejection,
    },
    /// The resolved object header's `IndirectRef` does not match the reference's
    /// object and generation numbers.
    ReferenceMismatch {
        /// `IndirectRef` parsed from the resolved object header.
        resolved: IndirectRef,
    },
    /// The resolved object body's leading token could not be classified.
    BodyToken {
        /// Underlying body-token classification rejection reason.
        body_token_reason: IndirectObjectBodyTokenInspectionRejection,
    },
    /// The resolved object body's leading token is not number-like.
    NonIntegerBody {
        /// Classified leading token family that was not number-like.
        token_kind: IndirectObjectBodyLeadingTokenKind,
    },
    /// The number-like body is not a non-negative ASCII-digit run terminated by
    /// PDF whitespace, a delimiter, or `endobj` (for example a sign, decimal,
    /// empty run, or trailing non-delimiter garbage).
    MalformedInteger,
    /// The non-negative integer value does not fit `usize`.
    IntegerOutOfRange,
}

/// Resolve an indirect-reference-shaped value to a non-negative integer object
/// value through a caller-supplied classic cross-reference table.
///
/// The helper parses the `N G R` value at `value_byte_offset` (typically a
/// `DictionaryEntrySpan` `value_range.start` classified `IndirectReferenceLike`)
/// with [`parse_indirect_reference`], locates the referenced object with
/// [`resolve_classic_xref_object`], and accepts only a single
/// [`ClassicXrefObjectLocation::InUse`] entry. It validates the resolved object
/// header with [`inspect_indirect_object_header`], confirms the header's parsed
/// [`IndirectRef`] matches the reference's object and generation numbers,
/// classifies the body's leading token with
/// [`inspect_indirect_object_body_token`] and requires `NumberLike`, then parses
/// the leading ASCII-digit run as a non-negative `usize` terminated by PDF
/// whitespace, a delimiter, or the `endobj` keyword.
///
/// It resolves exactly one reference one level deep: it does not follow chains
/// of indirect references, read `/Prev`, parse object streams, or resolve
/// anything beyond the single referenced integer object. It performs no
/// filesystem I/O and retains or copies no PDF bytes, object bodies, stream
/// bodies, dictionaries, or source slices.
///
/// # Errors
///
/// Returns [`ClassicXrefIntegerObjectResolutionError`] for a malformed `N G R`
/// value (`Reference`), a free/not-found/ambiguous classic-xref location
/// (`FreeObject`/`ObjectNotFound`/`AmbiguousObject`), a delegated header failure
/// (`Header`), a header/reference mismatch (`ReferenceMismatch`), a delegated
/// body-token failure (`BodyToken`), a non-number-like body (`NonIntegerBody`),
/// a malformed integer body (`MalformedInteger`), or an integer that does not
/// fit `usize` (`IntegerOutOfRange`).
pub fn resolve_classic_xref_integer_object(
    input: &[u8],
    xref_table: &ClassicXrefTableInspection,
    value_byte_offset: usize,
) -> Result<ClassicXrefIntegerObjectResolution, ClassicXrefIntegerObjectResolutionError> {
    let reference = parse_indirect_reference(input, value_byte_offset)
        .map_err(|error| {
            integer_error(
                input,
                value_byte_offset,
                error.error_byte_offset,
                None,
                ClassicXrefIntegerObjectResolutionRejection::Reference {
                    reference_reason: error.reason,
                },
            )
        })?
        .reference;
    let object_number = Some(reference.object_number);

    let object_byte_offset = resolve_in_use_offset(xref_table, reference.object_number)
        .map_err(|reason| integer_error(input, value_byte_offset, None, object_number, reason))?;

    let header = inspect_indirect_object_header(input, object_byte_offset).map_err(|error| {
        integer_error(
            input,
            value_byte_offset,
            error.error_byte_offset,
            object_number,
            ClassicXrefIntegerObjectResolutionRejection::Header {
                header_reason: error.reason,
            },
        )
    })?;

    if header.reference != reference {
        return Err(integer_error(
            input,
            value_byte_offset,
            Some(header.header_byte_offset),
            object_number,
            ClassicXrefIntegerObjectResolutionRejection::ReferenceMismatch {
                resolved: header.reference,
            },
        ));
    }

    let body = inspect_indirect_object_body_token(input, header.after_obj_keyword_offset).map_err(
        |error| {
            integer_error(
                input,
                value_byte_offset,
                error.error_byte_offset,
                object_number,
                ClassicXrefIntegerObjectResolutionRejection::BodyToken {
                    body_token_reason: error.reason,
                },
            )
        },
    )?;

    if body.token_kind != IndirectObjectBodyLeadingTokenKind::NumberLike {
        return Err(integer_error(
            input,
            value_byte_offset,
            Some(body.first_token_byte_offset),
            object_number,
            ClassicXrefIntegerObjectResolutionRejection::NonIntegerBody {
                token_kind: body.token_kind,
            },
        ));
    }

    let (value_range, value) =
        parse_integer_body(input, body.first_token_byte_offset).map_err(|(offset, reason)| {
            integer_error(
                input,
                value_byte_offset,
                Some(offset),
                object_number,
                reason,
            )
        })?;

    Ok(ClassicXrefIntegerObjectResolution {
        reference,
        object_byte_offset,
        value_range,
        value,
    })
}

/// Resolve an object number to a single in-use byte offset, mapping free,
/// not-found, and ambiguous classic-xref locations to structured rejections.
fn resolve_in_use_offset(
    xref_table: &ClassicXrefTableInspection,
    object_number: u32,
) -> Result<usize, ClassicXrefIntegerObjectResolutionRejection> {
    match resolve_classic_xref_object(xref_table, object_number) {
        ClassicXrefObjectLocation::InUse { byte_offset, .. } => Ok(byte_offset),
        ClassicXrefObjectLocation::Free { .. } => {
            Err(ClassicXrefIntegerObjectResolutionRejection::FreeObject)
        }
        ClassicXrefObjectLocation::NotFound { .. } => {
            Err(ClassicXrefIntegerObjectResolutionRejection::ObjectNotFound)
        }
        ClassicXrefObjectLocation::Ambiguous { .. } => {
            Err(ClassicXrefIntegerObjectResolutionRejection::AmbiguousObject)
        }
    }
}

/// Parse a non-negative ASCII-digit integer beginning at `first_token_byte_offset`.
///
/// The digit run must be non-empty and terminated by PDF whitespace, a
/// delimiter, or the `endobj` keyword; otherwise it is rejected as malformed. A
/// value that does not fit `usize` is a distinct out-of-range rejection. On
/// failure it returns the byte offset of the value start with the rejection.
fn parse_integer_body(
    input: &[u8],
    first_token_byte_offset: usize,
) -> Result<
    (IntegerObjectValueByteRange, usize),
    (usize, ClassicXrefIntegerObjectResolutionRejection),
> {
    let value_start = first_token_byte_offset;
    let value_end = value_start + count_leading_digits(&input[value_start..]);
    if value_end == value_start || !integer_terminated(input, value_end) {
        return Err((
            value_start,
            ClassicXrefIntegerObjectResolutionRejection::MalformedInteger,
        ));
    }

    let value = parse_usize_decimal(&input[value_start..value_end]).ok_or((
        value_start,
        ClassicXrefIntegerObjectResolutionRejection::IntegerOutOfRange,
    ))?;

    Ok((
        IntegerObjectValueByteRange {
            start: value_start,
            end: value_end,
        },
        value,
    ))
}

/// Accept only an integer digit run terminated by PDF whitespace, a delimiter,
/// or the exact `endobj` keyword. A trailing non-delimiter byte (including the
/// decimal point or a sign) and end-of-file are rejected as malformed.
fn integer_terminated(input: &[u8], after_digits: usize) -> bool {
    match input.get(after_digits) {
        Some(&byte) if is_pdf_whitespace(byte) || is_pdf_delimiter(byte) => true,
        Some(_) => consume_keyword(&input[after_digits..], ENDOBJ_KEYWORD).is_some(),
        None => false,
    }
}

const fn integer_error(
    input: &[u8],
    byte_offset: usize,
    error_byte_offset: Option<usize>,
    object_number: Option<u32>,
    reason: ClassicXrefIntegerObjectResolutionRejection,
) -> ClassicXrefIntegerObjectResolutionError {
    ClassicXrefIntegerObjectResolutionError {
        byte_offset,
        byte_len: input.len(),
        error_byte_offset,
        object_number,
        reason,
    }
}
