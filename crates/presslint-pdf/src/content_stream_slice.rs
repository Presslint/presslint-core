use serde::{Deserialize, Serialize};

use crate::ContentStreamDataExtentInspection;

/// Error returned when a located content-stream extent cannot be bridged to a
/// borrowed byte slice.
///
/// This carries the offending stream-data offsets and the source length so a
/// caller can diagnose an out-of-bounds or inverted extent without re-deriving
/// them. It retains or copies no PDF bytes, stream bytes, or source slices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentStreamDataSliceError {
    /// Stream-data start offset reported by the extent.
    pub start_byte_offset: usize,
    /// Exclusive stream-data end offset reported by the extent.
    pub end_byte_offset: usize,
    /// Total source length the extent was bridged against.
    pub byte_len: usize,
    /// Structured failure reason.
    pub reason: ContentStreamDataSliceRejection,
}

/// Structured content-stream data slice rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ContentStreamDataSliceRejection {
    /// The reported start offset is greater than the reported end offset.
    InvertedExtent,
    /// The reported end offset lies beyond the source length.
    EndOutOfBounds,
}

/// Bridge a located content-stream data extent to the borrowed source slice.
///
/// The helper reads only the extent's existing
/// [`stream_data_start_byte_offset`](ContentStreamDataExtentInspection::stream_data_start_byte_offset)
/// and
/// [`stream_data_end_byte_offset`](ContentStreamDataExtentInspection::stream_data_end_byte_offset)
/// accessors, bounds-checks them against `input.len()`, and returns the
/// zero-copy borrow `&input[start..end]`. It reparses nothing, copies no bytes,
/// allocates nothing, and reimplements no `/Length` or `endstream` logic.
///
/// # Errors
///
/// Returns [`ContentStreamDataSliceError`] with
/// [`InvertedExtent`](ContentStreamDataSliceRejection::InvertedExtent) when the
/// reported start exceeds the reported end, or
/// [`EndOutOfBounds`](ContentStreamDataSliceRejection::EndOutOfBounds) when the
/// reported end lies beyond the source length. The error carries the offending
/// offsets and `byte_len` rather than panicking.
pub fn content_stream_data_slice<'input>(
    input: &'input [u8],
    extent: &ContentStreamDataExtentInspection,
) -> Result<&'input [u8], ContentStreamDataSliceError> {
    let start_byte_offset = extent.stream_data_start_byte_offset();
    let end_byte_offset = extent.stream_data_end_byte_offset();
    let byte_len = input.len();

    if start_byte_offset > end_byte_offset {
        return Err(ContentStreamDataSliceError {
            start_byte_offset,
            end_byte_offset,
            byte_len,
            reason: ContentStreamDataSliceRejection::InvertedExtent,
        });
    }

    if end_byte_offset > byte_len {
        return Err(ContentStreamDataSliceError {
            start_byte_offset,
            end_byte_offset,
            byte_len,
            reason: ContentStreamDataSliceRejection::EndOutOfBounds,
        });
    }

    // `start <= end <= byte_len`, so this borrow is in-bounds and cannot panic.
    Ok(&input[start_byte_offset..end_byte_offset])
}
