use serde::{Deserialize, Serialize};

use crate::source_utils::{skip_comment, skip_hex_string, skip_literal_string, skip_whitespace};

/// Maximum `[` nesting depth this helper tracks before rejecting.
///
/// Kept private to bound pathological inputs: once the open-delimiter depth
/// reaches this constant, a further `[` yields a structured
/// [`ArrayExtentInspectionRejection::MaxNestingExceeded`] rather than unbounded
/// work.
const MAX_ARRAY_NESTING_DEPTH: usize = 256;

/// Balanced byte extent of a `[ ... ]` array at a caller-supplied offset.
///
/// This report stores only byte offsets and a small depth scalar. It does not
/// retain or copy PDF bytes, and it interprets no element, name, number, string,
/// or indirect reference inside the array.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrayExtentInspection {
    /// Caller-supplied byte offset where extent inspection began.
    pub byte_offset: usize,
    /// Byte offset of the opening `[` after optional PDF whitespace.
    pub open_byte_offset: usize,
    /// Byte offset of the matching closing `]` for the outermost `[`.
    pub close_byte_offset: usize,
    /// Exclusive byte offset immediately after the closing `]`.
    pub after_close_byte_offset: usize,
    /// Deepest `[` nesting depth observed; `1` for a flat array.
    pub max_observed_depth: usize,
}

/// Error returned when an array extent cannot be located.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrayExtentInspectionError {
    /// Caller-supplied byte offset where extent inspection began.
    pub byte_offset: usize,
    /// Total source length.
    pub byte_len: usize,
    /// Byte offset where the malformed or unsupported construct was found, when
    /// available.
    pub error_byte_offset: Option<usize>,
    /// Structured failure reason.
    pub reason: ArrayExtentInspectionRejection,
}

/// Structured array extent inspection rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ArrayExtentInspectionRejection {
    /// The caller-supplied offset lies beyond the source length or exactly at
    /// EOF.
    OffsetOutOfBounds,
    /// The offset and following bytes contain only PDF whitespace before EOF.
    NoSignificantToken,
    /// The first significant token is not the `[` array-open delimiter.
    NotArrayOpen,
    /// A literal or hex string was opened but not closed before EOF.
    UnterminatedString,
    /// EOF was reached before the `[` depth returned to zero.
    UnterminatedArray,
    /// The `[` nesting depth exceeded the bounded maximum.
    MaxNestingExceeded,
}

/// Locate the balanced `[ ... ]` extent of an array at a byte offset.
///
/// The helper skips optional PDF whitespace at `byte_offset`, requires the first
/// significant token to be the `[` array-open delimiter, and scans a single
/// bounded forward pass to the matching close of the outermost `[`. It
/// increments/decrements a `[`/`]` depth counter so a nested sub-array does not
/// close the outer one. Literal strings `( ... )`, hex strings `< ... >` (a `<`
/// not followed by `<`), and `%` comments are skipped as opaque spans so `]`
/// bytes inside them never affect the depth count. A `<<` dictionary open is
/// advanced past as a nested-dictionary delimiter so its leading `<` is not
/// misread as a hex-string open.
///
/// It performs no filesystem I/O, decodes no element, string, or name contents,
/// and never retains or copies PDF bytes; only byte offsets and a depth scalar
/// are reported.
///
/// # Errors
///
/// Returns [`ArrayExtentInspectionError`] when the offset is at or beyond EOF,
/// only whitespace remains before EOF, the first significant token is not `[`, a
/// literal or hex string is unterminated, the array is unterminated before the
/// depth returns to zero, or the bounded nesting depth is exceeded.
pub fn inspect_array_extent(
    input: &[u8],
    byte_offset: usize,
) -> Result<ArrayExtentInspection, ArrayExtentInspectionError> {
    if byte_offset >= input.len() {
        return Err(extent_error(
            input,
            byte_offset,
            ArrayExtentInspectionRejection::OffsetOutOfBounds,
            None,
        ));
    }

    let leading_whitespace = skip_whitespace(&input[byte_offset..]);
    let open_byte_offset = byte_offset + leading_whitespace;
    if open_byte_offset == input.len() {
        return Err(extent_error(
            input,
            byte_offset,
            ArrayExtentInspectionRejection::NoSignificantToken,
            Some(open_byte_offset),
        ));
    }

    if input[open_byte_offset] != b'[' {
        return Err(extent_error(
            input,
            byte_offset,
            ArrayExtentInspectionRejection::NotArrayOpen,
            Some(open_byte_offset),
        ));
    }

    let mut cursor = open_byte_offset + 1;
    let mut depth: usize = 1;
    let mut max_observed_depth: usize = 1;

    // Both opaque-string branches reject identically at the string's open offset.
    let unterminated_string = |at: usize| {
        extent_error(
            input,
            byte_offset,
            ArrayExtentInspectionRejection::UnterminatedString,
            Some(at),
        )
    };

    while let Some(&byte) = input.get(cursor) {
        match byte {
            b'%' => cursor = skip_comment(input, cursor),
            b'(' => {
                cursor = skip_literal_string(input, cursor)
                    .ok_or_else(|| unterminated_string(cursor))?;
            }
            // A `<<` dictionary open is advanced past as a nested-dictionary
            // delimiter so its leading `<` is not misread as a hex-string open.
            b'<' if input.get(cursor + 1) == Some(&b'<') => cursor += 2,
            b'<' => {
                cursor =
                    skip_hex_string(input, cursor).ok_or_else(|| unterminated_string(cursor))?;
            }
            b'[' => {
                if depth == MAX_ARRAY_NESTING_DEPTH {
                    return Err(extent_error(
                        input,
                        byte_offset,
                        ArrayExtentInspectionRejection::MaxNestingExceeded,
                        Some(cursor),
                    ));
                }
                depth += 1;
                max_observed_depth = max_observed_depth.max(depth);
                cursor += 1;
            }
            b']' => {
                depth -= 1;
                let after_close_byte_offset = cursor + 1;
                if depth == 0 {
                    return Ok(ArrayExtentInspection {
                        byte_offset,
                        open_byte_offset,
                        close_byte_offset: cursor,
                        after_close_byte_offset,
                        max_observed_depth,
                    });
                }
                cursor = after_close_byte_offset;
            }
            _ => cursor += 1,
        }
    }

    Err(extent_error(
        input,
        byte_offset,
        ArrayExtentInspectionRejection::UnterminatedArray,
        None,
    ))
}

const fn extent_error(
    input: &[u8],
    byte_offset: usize,
    reason: ArrayExtentInspectionRejection,
    error_byte_offset: Option<usize>,
) -> ArrayExtentInspectionError {
    ArrayExtentInspectionError {
        byte_offset,
        byte_len: input.len(),
        error_byte_offset,
        reason,
    }
}
