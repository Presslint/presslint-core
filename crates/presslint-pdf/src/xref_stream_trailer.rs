use serde::{Deserialize, Serialize};

use crate::xref_stream::{IntegerError, parse_non_negative_integer, unique_entry};
use crate::{
    DictionaryEntryByteRange, DictionaryEntrySpan, DictionaryValueKind, IndirectRef,
    IndirectReferenceInspectionRejection, XrefStreamDictionaryInspection,
    XrefStreamDictionaryInspectionRejection,
};

const ROOT_KEY: &[u8] = b"/Root";
const PREV_KEY: &[u8] = b"/Prev";

/// Trailer-style navigation fields of a cross-reference-stream (`/Type /XRef`)
/// dictionary.
///
/// This report carries the delegated geometry inspection, the `/Root` key/value
/// byte ranges and parsed catalog reference, and the optional `/Prev` value byte
/// range and parsed byte offset. It stores only the delegated inspection, byte
/// ranges, an `IndirectRef`, and small `usize` values; it retains or copies no
/// PDF bytes, object bodies, stream bodies, or source slices, and it neither
/// follows `/Prev` nor resolves `/Root`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XrefStreamTrailerInspection {
    /// Delegated cross-reference-stream dictionary geometry inspection.
    pub xref_stream_dictionary: XrefStreamDictionaryInspection,
    /// Byte range covering the exact top-level raw `/Root` key.
    pub root_key_range: DictionaryEntryByteRange,
    /// Byte range covering the `/Root` value span.
    pub root_value_range: DictionaryEntryByteRange,
    /// Parsed document catalog indirect reference.
    pub root_reference: IndirectRef,
    /// Byte range covering the `/Prev` value span, when the key is present.
    pub prev_value_range: Option<DictionaryEntryByteRange>,
    /// Parsed `/Prev` previous cross-reference byte offset, when present.
    pub prev_byte_offset: Option<usize>,
}

/// Error returned when a cross-reference-stream trailer cannot be inspected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XrefStreamTrailerInspectionError {
    /// Caller-supplied byte offset where trailer inspection began.
    pub byte_offset: usize,
    /// Total source length.
    pub byte_len: usize,
    /// Byte offset where the resolved object header begins, when it was located.
    pub object_header_byte_offset: Option<usize>,
    /// Byte offset where the malformed or unsupported construct was found, when
    /// available.
    pub error_byte_offset: Option<usize>,
    /// Structured failure reason.
    pub reason: XrefStreamTrailerInspectionRejection,
}

/// Structured cross-reference-stream trailer inspection rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum XrefStreamTrailerInspectionRejection {
    /// The delegated cross-reference-stream dictionary geometry inspection
    /// failed.
    XrefStreamDictionary {
        /// Underlying cross-reference-stream dictionary rejection reason.
        xref_stream_reason: XrefStreamDictionaryInspectionRejection,
    },
    /// The dictionary has no exact top-level raw `/Root` key.
    MissingRoot,
    /// The dictionary has more than one exact top-level raw `/Root` key.
    DuplicateRoot {
        /// First `/Root` key range observed in source order.
        first_key_range: DictionaryEntryByteRange,
        /// Duplicate `/Root` key range observed in source order.
        duplicate_key_range: DictionaryEntryByteRange,
    },
    /// The `/Root` value is not shaped as an indirect reference value span.
    NonReferenceRootValue {
        /// Shallow value kind reported by dictionary entry inspection.
        value_kind: DictionaryValueKind,
    },
    /// The `/Root` value was shaped as an indirect reference but did not parse
    /// to a single `N G R` covering its entire value span.
    MalformedRootReference {
        /// Underlying indirect reference rejection reason.
        reference_reason: IndirectReferenceInspectionRejection,
    },
    /// The dictionary has more than one exact top-level raw `/Prev` key.
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

/// Inspect the trailer-style navigation fields of a cross-reference stream.
///
/// The helper delegates the `/Type /XRef`, `/W`, `/Size`, and `/Index` geometry
/// to [`crate::inspect_xref_stream_dictionary`] (reimplementing none of it),
/// then matches only the exact raw top-level keys `/Root` and `/Prev` over the
/// entries that geometry inspection already materialized. It requires:
///
/// - exactly one `/Root` key whose value is a single `N G R` indirect reference
///   covering its entire value span, parsed with
///   [`crate::parse_indirect_reference`];
/// - an optional `/Prev` key whose value is a direct non-negative decimal
///   integer byte offset that fits `usize`.
///
/// It does not decode cross-reference stream bytes, parse `/W`-width entry
/// records, build an object-offset map, follow `/Prev`, merge incremental
/// sections, or resolve `/Root`. The report retains or copies no PDF bytes.
///
/// # Errors
///
/// Returns [`XrefStreamTrailerInspectionError`] for a delegated geometry
/// failure, a missing/duplicate `/Root`, a non-reference or malformed `/Root`
/// value, a duplicate `/Prev`, or a non-integer/out-of-range `/Prev` value. It
/// never returns partial navigation fields on error.
pub fn inspect_xref_stream_trailer(
    input: &[u8],
    object_byte_offset: usize,
) -> Result<XrefStreamTrailerInspection, XrefStreamTrailerInspectionError> {
    let xref_stream_dictionary = crate::inspect_xref_stream_dictionary(input, object_byte_offset)
        .map_err(|error| XrefStreamTrailerInspectionError {
        byte_offset: error.byte_offset,
        byte_len: error.byte_len,
        object_header_byte_offset: error.object_header_byte_offset,
        error_byte_offset: error.error_byte_offset,
        reason: XrefStreamTrailerInspectionRejection::XrefStreamDictionary {
            xref_stream_reason: error.reason,
        },
    })?;

    let object_dictionary = &xref_stream_dictionary.object_dictionary;
    let ctx = ErrorContext {
        byte_offset: object_byte_offset,
        byte_len: input.len(),
        object_header_byte_offset: Some(object_dictionary.header_range.start),
    };
    let close_offset = object_dictionary.dictionary_close_byte_offset;
    let entries = &object_dictionary.entries;

    let root = require_root(input, entries, close_offset, ctx)?;
    let prev = require_prev(input, entries, ctx)?;

    Ok(XrefStreamTrailerInspection {
        xref_stream_dictionary,
        root_key_range: root.key_range,
        root_value_range: root.value_range,
        root_reference: root.reference,
        prev_value_range: prev.value_range,
        prev_byte_offset: prev.byte_offset,
    })
}

/// Located `/Root` key/value byte ranges and the parsed catalog reference.
struct RootResult {
    key_range: DictionaryEntryByteRange,
    value_range: DictionaryEntryByteRange,
    reference: IndirectRef,
}

/// Locate the single exact `/Root` key and parse its value as one `N G R`
/// indirect reference covering the entire value span.
fn require_root(
    input: &[u8],
    entries: &[DictionaryEntrySpan],
    close_offset: usize,
    ctx: ErrorContext,
) -> Result<RootResult, XrefStreamTrailerInspectionError> {
    let entry = unique_entry(input, entries, ROOT_KEY)
        .map_err(|(first_key_range, duplicate_key_range)| {
            ctx.error(
                XrefStreamTrailerInspectionRejection::DuplicateRoot {
                    first_key_range,
                    duplicate_key_range,
                },
                Some(duplicate_key_range.start),
            )
        })?
        .ok_or_else(|| {
            ctx.error(
                XrefStreamTrailerInspectionRejection::MissingRoot,
                Some(close_offset),
            )
        })?;

    if !matches!(
        entry.value_kind,
        DictionaryValueKind::IndirectReferenceLike | DictionaryValueKind::OtherScalar
    ) {
        return Err(ctx.error(
            XrefStreamTrailerInspectionRejection::NonReferenceRootValue {
                value_kind: entry.value_kind,
            },
            Some(entry.value_range.start),
        ));
    }

    let reference =
        crate::parse_indirect_reference(input, entry.value_range.start).map_err(|error| {
            ctx.error(
                XrefStreamTrailerInspectionRejection::MalformedRootReference {
                    reference_reason: error.reason,
                },
                error.error_byte_offset,
            )
        })?;

    if reference.after_keyword_offset != entry.value_range.end {
        return Err(ctx.error(
            XrefStreamTrailerInspectionRejection::MalformedRootReference {
                reference_reason: IndirectReferenceInspectionRejection::MalformedReference,
            },
            Some(reference.after_keyword_offset),
        ));
    }

    Ok(RootResult {
        key_range: entry.key_range,
        value_range: entry.value_range,
        reference: reference.reference,
    })
}

/// Optional `/Prev` location: a value byte range and parsed byte offset when the
/// key is present, both `None` when absent.
struct PrevResult {
    value_range: Option<DictionaryEntryByteRange>,
    byte_offset: Option<usize>,
}

/// Locate the optional single exact `/Prev` key and parse its value as a direct
/// non-negative decimal integer byte offset.
fn require_prev(
    input: &[u8],
    entries: &[DictionaryEntrySpan],
    ctx: ErrorContext,
) -> Result<PrevResult, XrefStreamTrailerInspectionError> {
    let Some(entry) = unique_entry(input, entries, PREV_KEY).map_err(
        |(first_key_range, duplicate_key_range)| {
            ctx.error(
                XrefStreamTrailerInspectionRejection::DuplicatePrev {
                    first_key_range,
                    duplicate_key_range,
                },
                Some(duplicate_key_range.start),
            )
        },
    )?
    else {
        return Ok(PrevResult {
            value_range: None,
            byte_offset: None,
        });
    };

    let value = &input[entry.value_range.start..entry.value_range.end];
    let byte_offset = match parse_non_negative_integer(value) {
        Ok(byte_offset) => byte_offset,
        Err(IntegerError::Malformed) => {
            return Err(ctx.error(
                XrefStreamTrailerInspectionRejection::NonIntegerPrevValue {
                    value_kind: entry.value_kind,
                },
                Some(entry.value_range.start),
            ));
        }
        Err(IntegerError::OutOfRange) => {
            return Err(ctx.error(
                XrefStreamTrailerInspectionRejection::PrevOutOfRange,
                Some(entry.value_range.start),
            ));
        }
    };

    Ok(PrevResult {
        value_range: Some(entry.value_range),
        byte_offset: Some(byte_offset),
    })
}

/// Copyable byte-context shared by the field-requirement helpers so each can
/// build an [`XrefStreamTrailerInspectionError`] without re-threading the caller
/// offset, source length, and resolved object-header offset.
#[derive(Clone, Copy)]
struct ErrorContext {
    byte_offset: usize,
    byte_len: usize,
    object_header_byte_offset: Option<usize>,
}

impl ErrorContext {
    const fn error(
        self,
        reason: XrefStreamTrailerInspectionRejection,
        error_byte_offset: Option<usize>,
    ) -> XrefStreamTrailerInspectionError {
        XrefStreamTrailerInspectionError {
            byte_offset: self.byte_offset,
            byte_len: self.byte_len,
            object_header_byte_offset: self.object_header_byte_offset,
            error_byte_offset,
            reason,
        }
    }
}
