use serde::{Deserialize, Serialize};

use crate::IndirectRef;
use crate::source_utils::{ObjectReferenceShapeRejection, parse_object_reference_shape};

const INDIRECT_OBJECT_HEADER_SCAN_LIMIT: usize = 128;
const OBJ_KEYWORD: &[u8] = b"obj";

/// Source byte range covering an indirect object header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndirectObjectHeaderByteRange {
    /// Inclusive start offset.
    pub start: usize,
    /// Exclusive end offset.
    pub end: usize,
}

/// Parsed metadata for an indirect object header at a caller-supplied offset.
///
/// This report stores only structural metadata. It does not retain or copy PDF
/// bytes and does not inspect the indirect object body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndirectObjectHeaderInspection {
    /// Parsed indirect object identifier.
    pub reference: IndirectRef,
    /// Byte offset where the header begins after optional PDF whitespace.
    pub header_byte_offset: usize,
    /// Byte range covering `object_number generation obj`.
    pub header_range: IndirectObjectHeaderByteRange,
    /// Byte offset immediately after the `obj` keyword.
    pub after_obj_keyword_offset: usize,
}

/// Error returned when an indirect object header cannot be inspected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndirectObjectHeaderInspectionError {
    /// Caller-supplied byte offset where inspection began.
    pub byte_offset: usize,
    /// Total source length.
    pub byte_len: usize,
    /// Byte offset where the malformed construct was found, when available.
    pub error_byte_offset: Option<usize>,
    /// Structured failure reason.
    pub reason: IndirectObjectHeaderInspectionRejection,
}

/// Structured indirect object header inspection rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum IndirectObjectHeaderInspectionRejection {
    /// The caller-supplied offset lies beyond the source length or exactly at
    /// EOF.
    OffsetOutOfBounds,
    /// The bytes at the resolved offset are not shaped as `N G obj`.
    MalformedHeader,
    /// The parsed object number does not fit `u32`.
    ObjectNumberOutOfRange,
    /// The parsed generation number does not fit `u16`.
    GenerationOutOfRange,
}

/// Inspect an indirect object header at a caller-supplied byte offset.
///
/// The helper skips optional PDF whitespace at `byte_offset`, parses only the
/// `object-number generation obj` header, and stops immediately after the
/// `obj` keyword. It performs no filesystem I/O and does not inspect object
/// bodies, dictionaries, streams, `endobj`, trailers, page trees, catalogs, or
/// content streams.
///
/// # Errors
///
/// Returns [`IndirectObjectHeaderInspectionError`] when the offset is outside
/// the source bytes, the header is malformed, or the parsed object or
/// generation number exceeds the public `u32`/`u16` report fields.
pub fn inspect_indirect_object_header(
    input: &[u8],
    byte_offset: usize,
) -> Result<IndirectObjectHeaderInspection, IndirectObjectHeaderInspectionError> {
    match parse_object_reference_shape(
        input,
        byte_offset,
        INDIRECT_OBJECT_HEADER_SCAN_LIMIT,
        OBJ_KEYWORD,
    ) {
        Ok(shape) => Ok(IndirectObjectHeaderInspection {
            reference: IndirectRef {
                object_number: shape.object_number,
                generation: shape.generation,
            },
            header_byte_offset: shape.reference_byte_offset,
            header_range: IndirectObjectHeaderByteRange {
                start: shape.reference_byte_offset,
                end: shape.after_keyword_offset,
            },
            after_obj_keyword_offset: shape.after_keyword_offset,
        }),
        Err(error) => Err(IndirectObjectHeaderInspectionError {
            byte_offset,
            byte_len: input.len(),
            error_byte_offset: error.error_byte_offset,
            reason: match error.reason {
                ObjectReferenceShapeRejection::OffsetOutOfBounds => {
                    IndirectObjectHeaderInspectionRejection::OffsetOutOfBounds
                }
                ObjectReferenceShapeRejection::Malformed => {
                    IndirectObjectHeaderInspectionRejection::MalformedHeader
                }
                ObjectReferenceShapeRejection::ObjectNumberOutOfRange => {
                    IndirectObjectHeaderInspectionRejection::ObjectNumberOutOfRange
                }
                ObjectReferenceShapeRejection::GenerationOutOfRange => {
                    IndirectObjectHeaderInspectionRejection::GenerationOutOfRange
                }
            },
        }),
    }
}
