use serde::{Deserialize, Serialize};

use crate::xref_stream::{IntegerError, parse_non_negative_integer, unique_entry};
use crate::{
    ClassicXrefTrailerDictionaryInspection, ClassicXrefTrailerDictionaryInspectionRejection,
    DictionaryEntryByteRange, DictionaryEntryInspectionRejection, DictionaryValueKind,
};

const PREV_KEY: &[u8] = b"/Prev";

/// Parsed `/Prev` previous-section byte offset from a classic xref trailer.
///
/// This is the companion micro-locator of [`crate::build_classic_xref_chain`]:
/// it is the classic-table parallel of the `/Prev` field the cross-reference
/// stream trailer inspector already reads. It is only produced when the trailer
/// carries an exact top-level `/Prev` key; an absent key is reported by the
/// inspector as `Ok(None)`.
///
/// This report stores only structural metadata and small `usize` values. It does
/// not retain or copy trailer dictionary bytes, object bodies, stream bodies, or
/// source slices, and it does not follow the parsed offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassicXrefTrailerPrevInspection {
    /// Balanced trailer dictionary inspection that supplied the scanned range.
    pub trailer_dictionary: ClassicXrefTrailerDictionaryInspection,
    /// Byte range covering the exact top-level raw `/Prev` key.
    pub prev_key_range: DictionaryEntryByteRange,
    /// Byte range covering the `/Prev` value span.
    pub prev_value_range: DictionaryEntryByteRange,
    /// Parsed previous cross-reference-section byte offset.
    pub prev_byte_offset: usize,
}

/// Error returned when a classic xref trailer `/Prev` field cannot be inspected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassicXrefTrailerPrevInspectionError {
    /// Caller-supplied byte offset where trailer-prev inspection began.
    pub byte_offset: usize,
    /// Total source length.
    pub byte_len: usize,
    /// Byte offset where the malformed or unsupported construct was found, when
    /// available.
    pub error_byte_offset: Option<usize>,
    /// Structured failure reason.
    pub reason: ClassicXrefTrailerPrevInspectionRejection,
}

/// Structured classic xref trailer `/Prev` inspection rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ClassicXrefTrailerPrevInspectionRejection {
    /// A delegated trailer dictionary inspection failed.
    TrailerDictionary {
        /// Underlying trailer dictionary rejection reason.
        trailer_reason: ClassicXrefTrailerDictionaryInspectionRejection,
    },
    /// A delegated dictionary entry inspection failed.
    DictionaryEntries {
        /// Underlying dictionary entry rejection reason.
        dictionary_entries_reason: DictionaryEntryInspectionRejection,
    },
    /// The trailer dictionary has more than one exact top-level raw `/Prev` key.
    DuplicatePrev {
        /// First `/Prev` key range observed in source order.
        first_key_range: DictionaryEntryByteRange,
        /// Duplicate `/Prev` key range observed in source order.
        duplicate_key_range: DictionaryEntryByteRange,
    },
    /// The `/Prev` value is not a direct non-negative decimal integer (for
    /// example an indirect reference, decimal, signed, name, array, or
    /// dictionary value).
    NonIntegerPrevValue {
        /// Shallow value kind reported by dictionary entry inspection.
        value_kind: DictionaryValueKind,
    },
    /// The `/Prev` non-negative integer does not fit `usize`.
    PrevOutOfRange,
}

/// Inspect the optional top-level `/Prev` byte offset from a classic xref
/// trailer.
///
/// The helper composes the same bounded inspectors that
/// [`crate::inspect_classic_xref_trailer_root`] uses: it locates the trailer
/// dictionary with [`crate::inspect_classic_xref_trailer_dictionary`], scans
/// top-level entries with [`crate::inspect_dictionary_entries`], selects the
/// single exact raw key bytes `/Prev` with the shared exact-key/duplicate-key
/// [`unique_entry`] helper, and parses that value with the shared
/// [`parse_non_negative_integer`] helper so the classic `/Prev` offset follows
/// the same non-negative-decimal-integer-fitting-`usize` rule the cross-reference
/// stream trailer applies.
///
/// An absent `/Prev` key is `Ok(None)`; a single direct non-negative integer
/// value is `Ok(Some(..))`. It does not decode PDF name escapes, interpret
/// nested dictionaries, follow the parsed offset, or inspect catalog/page-tree
/// bodies.
///
/// # Errors
///
/// Returns [`ClassicXrefTrailerPrevInspectionError`] for delegated trailer or
/// dictionary-entry inspection failures, a duplicate exact `/Prev` key, a
/// non-integer `/Prev` value, or a `/Prev` integer that does not fit `usize`. It
/// never returns a partial inspection on error.
pub fn inspect_classic_xref_trailer_prev(
    input: &[u8],
    trailer_keyword_offset: usize,
) -> Result<Option<ClassicXrefTrailerPrevInspection>, ClassicXrefTrailerPrevInspectionError> {
    let trailer_dictionary =
        crate::inspect_classic_xref_trailer_dictionary(input, trailer_keyword_offset).map_err(
            |error| {
                trailer_prev_error(
                    input,
                    trailer_keyword_offset,
                    ClassicXrefTrailerPrevInspectionRejection::TrailerDictionary {
                        trailer_reason: error.reason,
                    },
                    error.error_byte_offset,
                )
            },
        )?;

    let entries =
        crate::inspect_dictionary_entries(input, trailer_dictionary.dictionary_open_byte_offset)
            .map_err(|error| {
                trailer_prev_error(
                    input,
                    trailer_keyword_offset,
                    ClassicXrefTrailerPrevInspectionRejection::DictionaryEntries {
                        dictionary_entries_reason: error.reason,
                    },
                    error.error_byte_offset,
                )
            })?;

    let Some(entry) = unique_entry(input, &entries.entries, PREV_KEY).map_err(
        |(first_key_range, duplicate_key_range)| {
            trailer_prev_error(
                input,
                trailer_keyword_offset,
                ClassicXrefTrailerPrevInspectionRejection::DuplicatePrev {
                    first_key_range,
                    duplicate_key_range,
                },
                Some(duplicate_key_range.start),
            )
        },
    )?
    else {
        return Ok(None);
    };

    let value = &input[entry.value_range.start..entry.value_range.end];
    let prev_byte_offset = match parse_non_negative_integer(value) {
        Ok(byte_offset) => byte_offset,
        Err(IntegerError::Malformed) => {
            return Err(trailer_prev_error(
                input,
                trailer_keyword_offset,
                ClassicXrefTrailerPrevInspectionRejection::NonIntegerPrevValue {
                    value_kind: entry.value_kind,
                },
                Some(entry.value_range.start),
            ));
        }
        Err(IntegerError::OutOfRange) => {
            return Err(trailer_prev_error(
                input,
                trailer_keyword_offset,
                ClassicXrefTrailerPrevInspectionRejection::PrevOutOfRange,
                Some(entry.value_range.start),
            ));
        }
    };

    Ok(Some(ClassicXrefTrailerPrevInspection {
        trailer_dictionary,
        prev_key_range: entry.key_range,
        prev_value_range: entry.value_range,
        prev_byte_offset,
    }))
}

const fn trailer_prev_error(
    input: &[u8],
    byte_offset: usize,
    reason: ClassicXrefTrailerPrevInspectionRejection,
    error_byte_offset: Option<usize>,
) -> ClassicXrefTrailerPrevInspectionError {
    ClassicXrefTrailerPrevInspectionError {
        byte_offset,
        byte_len: input.len(),
        error_byte_offset,
        reason,
    }
}
