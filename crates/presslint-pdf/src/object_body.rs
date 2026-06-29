use serde::{Deserialize, Serialize};

use crate::source_utils::{consume_keyword, skip_whitespace};

/// Shallow classification of the first significant token in an indirect object body.
///
/// The classifier reports only the broad token family at the resolved byte
/// offset. It does not parse dictionaries, arrays, strings, names, numbers,
/// streams, `endstream`, or `endobj`, and it never retains or copies source
/// bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndirectObjectBodyTokenInspection {
    /// Caller-supplied byte offset where object body inspection began.
    pub byte_offset: usize,
    /// Byte offset of the first non-whitespace byte classified as the body token.
    pub first_token_byte_offset: usize,
    /// Broad leading token family.
    pub token_kind: IndirectObjectBodyLeadingTokenKind,
}

/// Broad leading token families for an indirect object body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndirectObjectBodyLeadingTokenKind {
    /// Dictionary open delimiter `<<`.
    DictionaryOpen,
    /// Hexadecimal string open delimiter `<`.
    HexStringOpen,
    /// Array open delimiter `[`.
    ArrayOpen,
    /// Name object beginning with `/`.
    Name,
    /// Literal string beginning with `(`.
    LiteralString,
    /// Number-like leading byte: a digit, sign, or decimal point.
    NumberLike,
    /// Boolean keyword `true` or `false`.
    Boolean,
    /// Null keyword `null`.
    Null,
}

/// Error returned when an indirect object body leading token cannot be classified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndirectObjectBodyTokenInspectionError {
    /// Caller-supplied byte offset where object body inspection began.
    pub byte_offset: usize,
    /// Total source length.
    pub byte_len: usize,
    /// Byte offset where the malformed or unsupported construct was found, when available.
    pub error_byte_offset: Option<usize>,
    /// Structured failure reason.
    pub reason: IndirectObjectBodyTokenInspectionRejection,
}

/// Structured indirect object body token inspection rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum IndirectObjectBodyTokenInspectionRejection {
    /// The caller-supplied offset lies beyond the source length or exactly at EOF.
    OffsetOutOfBounds,
    /// The offset and following bytes contain only PDF whitespace before EOF.
    NoSignificantToken,
    /// The first significant byte is not one of the recognized shallow token starts.
    UnclassifiedLeadingByte,
}

/// Classify the first significant token of an indirect object body.
///
/// The helper skips PDF whitespace at `byte_offset`, reports the resolved
/// first-token byte offset, and classifies only the broad leading token family.
/// It performs no filesystem I/O and does not parse or retain object bodies,
/// stream bodies, dictionaries, arrays, string contents, names, or numeric
/// values.
///
/// # Errors
///
/// Returns [`IndirectObjectBodyTokenInspectionError`] when the supplied offset
/// is at or beyond EOF, only whitespace remains before EOF, or the first
/// significant byte is not a recognized leading token.
pub fn inspect_indirect_object_body_token(
    input: &[u8],
    byte_offset: usize,
) -> Result<IndirectObjectBodyTokenInspection, IndirectObjectBodyTokenInspectionError> {
    if byte_offset >= input.len() {
        return Err(object_body_error(
            input,
            byte_offset,
            IndirectObjectBodyTokenInspectionRejection::OffsetOutOfBounds,
            None,
        ));
    }

    let leading_whitespace = skip_whitespace(&input[byte_offset..]);
    let first_token_byte_offset = byte_offset + leading_whitespace;
    if first_token_byte_offset == input.len() {
        return Err(object_body_error(
            input,
            byte_offset,
            IndirectObjectBodyTokenInspectionRejection::NoSignificantToken,
            Some(first_token_byte_offset),
        ));
    }

    let content = &input[first_token_byte_offset..];
    let first_byte = content[0];

    let token_kind = match first_byte {
        b'<' if content.get(1) == Some(&b'<') => IndirectObjectBodyLeadingTokenKind::DictionaryOpen,
        b'<' => IndirectObjectBodyLeadingTokenKind::HexStringOpen,
        b'[' => IndirectObjectBodyLeadingTokenKind::ArrayOpen,
        b'/' => IndirectObjectBodyLeadingTokenKind::Name,
        b'(' => IndirectObjectBodyLeadingTokenKind::LiteralString,
        b'+' | b'-' | b'.' | b'0'..=b'9' => IndirectObjectBodyLeadingTokenKind::NumberLike,
        b't' if consume_keyword(content, b"true").is_some() => {
            IndirectObjectBodyLeadingTokenKind::Boolean
        }
        b'f' if consume_keyword(content, b"false").is_some() => {
            IndirectObjectBodyLeadingTokenKind::Boolean
        }
        b'n' if consume_keyword(content, b"null").is_some() => {
            IndirectObjectBodyLeadingTokenKind::Null
        }
        _ => {
            return Err(object_body_error(
                input,
                byte_offset,
                IndirectObjectBodyTokenInspectionRejection::UnclassifiedLeadingByte,
                Some(first_token_byte_offset),
            ));
        }
    };

    Ok(IndirectObjectBodyTokenInspection {
        byte_offset,
        first_token_byte_offset,
        token_kind,
    })
}

const fn object_body_error(
    input: &[u8],
    byte_offset: usize,
    reason: IndirectObjectBodyTokenInspectionRejection,
    error_byte_offset: Option<usize>,
) -> IndirectObjectBodyTokenInspectionError {
    IndirectObjectBodyTokenInspectionError {
        byte_offset,
        byte_len: input.len(),
        error_byte_offset,
        reason,
    }
}
