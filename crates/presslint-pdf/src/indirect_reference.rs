use serde::{Deserialize, Serialize};

use crate::IndirectRef;
use crate::source_utils::{ObjectReferenceShapeRejection, parse_object_reference_shape};

const INDIRECT_REFERENCE_SCAN_LIMIT: usize = 128;
const REFERENCE_KEYWORD: &[u8] = b"R";

/// Source byte range covering an indirect reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndirectReferenceByteRange {
    /// Inclusive start offset.
    pub start: usize,
    /// Exclusive end offset.
    pub end: usize,
}

/// Parsed metadata for an `N G R` indirect reference at a caller-supplied
/// offset.
///
/// This report stores only structural metadata. It does not retain or copy PDF
/// bytes and does not resolve or follow the parsed reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndirectReferenceInspection {
    /// Parsed indirect reference identifier.
    pub reference: IndirectRef,
    /// Byte offset where the reference begins after optional PDF whitespace.
    pub reference_byte_offset: usize,
    /// Byte range covering `object_number generation R`.
    pub reference_range: IndirectReferenceByteRange,
    /// Byte offset immediately after the `R` keyword.
    pub after_keyword_offset: usize,
}

/// Error returned when an indirect reference cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndirectReferenceInspectionError {
    /// Caller-supplied byte offset where parsing began.
    pub byte_offset: usize,
    /// Total source length.
    pub byte_len: usize,
    /// Byte offset where the malformed construct was found, when available.
    pub error_byte_offset: Option<usize>,
    /// Structured failure reason.
    pub reason: IndirectReferenceInspectionRejection,
}

/// Structured indirect reference parsing rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum IndirectReferenceInspectionRejection {
    /// The caller-supplied offset lies beyond the source length or exactly at
    /// EOF.
    OffsetOutOfBounds,
    /// The bytes at the resolved offset are not shaped as `N G R`.
    MalformedReference,
    /// The parsed object number does not fit `u32`.
    ObjectNumberOutOfRange,
    /// The parsed generation number does not fit `u16`.
    GenerationOutOfRange,
}

/// Parse an `N G R` indirect reference at a caller-supplied byte offset.
///
/// The helper skips optional PDF whitespace at `byte_offset`, parses only the
/// `object-number generation R` reference, and stops immediately after the `R`
/// keyword. The keyword boundary is validated with the shared
/// keyword-boundary rule, so trailing bytes such as `Robot` or `R0` are
/// rejected. It accepts only the `R` reference keyword: an `N G obj` header (or
/// any other trailing keyword) is rejected as malformed rather than parsed as a
/// reference.
///
/// It performs no filesystem I/O and does not resolve or follow the reference,
/// inspect the referenced object's header or body, or read any dictionary,
/// array, stream, trailer, or `/Prev` chain.
///
/// # Errors
///
/// Returns [`IndirectReferenceInspectionError`] when the offset is outside the
/// source bytes, the reference is malformed, or the parsed object or generation
/// number exceeds the public `u32`/`u16` report fields.
pub fn parse_indirect_reference(
    input: &[u8],
    byte_offset: usize,
) -> Result<IndirectReferenceInspection, IndirectReferenceInspectionError> {
    match parse_object_reference_shape(
        input,
        byte_offset,
        INDIRECT_REFERENCE_SCAN_LIMIT,
        REFERENCE_KEYWORD,
    ) {
        Ok(shape) => Ok(IndirectReferenceInspection {
            reference: IndirectRef {
                object_number: shape.object_number,
                generation: shape.generation,
            },
            reference_byte_offset: shape.reference_byte_offset,
            reference_range: IndirectReferenceByteRange {
                start: shape.reference_byte_offset,
                end: shape.after_keyword_offset,
            },
            after_keyword_offset: shape.after_keyword_offset,
        }),
        Err(error) => Err(IndirectReferenceInspectionError {
            byte_offset,
            byte_len: input.len(),
            error_byte_offset: error.error_byte_offset,
            reason: match error.reason {
                ObjectReferenceShapeRejection::OffsetOutOfBounds => {
                    IndirectReferenceInspectionRejection::OffsetOutOfBounds
                }
                ObjectReferenceShapeRejection::Malformed => {
                    IndirectReferenceInspectionRejection::MalformedReference
                }
                ObjectReferenceShapeRejection::ObjectNumberOutOfRange => {
                    IndirectReferenceInspectionRejection::ObjectNumberOutOfRange
                }
                ObjectReferenceShapeRejection::GenerationOutOfRange => {
                    IndirectReferenceInspectionRejection::GenerationOutOfRange
                }
            },
        }),
    }
}
