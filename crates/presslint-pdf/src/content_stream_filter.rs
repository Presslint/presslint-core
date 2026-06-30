use serde::{Deserialize, Serialize};

use crate::source_utils::{skip_name, skip_whitespace_and_comments};
use crate::xref_stream::unique_entry;
use crate::{
    ArrayExtentInspectionRejection, ContentStreamStartInspection,
    ContentStreamStartInspectionRejection, DictionaryEntryByteRange,
    DictionaryEntryInspectionRejection, DictionaryValueKind, IndirectObjectBodyLeadingTokenKind,
    IndirectObjectDictionaryInspectionRejection, inspect_array_extent,
    inspect_content_stream_start, inspect_indirect_object_body_token,
    inspect_indirect_object_header,
};

const FILTER_KEY: &[u8] = b"/Filter";
const FLATE_DECODE_NAME: &[u8] = b"/FlateDecode";

/// Decode-path classification of a content stream's top-level `/Filter` chain.
///
/// This is the decision point the future "real PDF -> inventory" bridge branches
/// on to choose between the identity path ([`Uncompressed`](Self::Uncompressed)),
/// the `FlateDecode` path ([`Flate`](Self::Flate)), and a structured skip for a
/// filter or filter chain this slice does not decode
/// ([`UnsupportedFilter`](Self::UnsupportedFilter) /
/// [`UnsupportedFilterChain`](Self::UnsupportedFilterChain)).
///
/// Every variant carries only byte ranges and small counts. It retains or copies
/// no PDF bytes, object bodies, stream bodies, decoded bytes, filter names, or
/// source slices; filter names are matched in place against `b"/FlateDecode"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "classification", rename_all = "snake_case")]
pub enum ContentStreamFilterClassification {
    /// No top-level `/Filter` key, or an empty `/Filter` array: the identity
    /// decode path.
    Uncompressed,
    /// Exactly one `/FlateDecode` filter (as a name or a single-element array):
    /// the `FlateDecode` decode path.
    Flate,
    /// Exactly one filter that is not `/FlateDecode`: a structured skip.
    UnsupportedFilter {
        /// Byte range covering the unsupported filter name value (including the
        /// leading `/`).
        filter_name_range: DictionaryEntryByteRange,
    },
    /// A `/Filter` array declaring two or more filters: a structured skip for the
    /// whole multi-filter chain.
    UnsupportedFilterChain {
        /// Byte range covering the `/Filter` array value span.
        filter_value_range: DictionaryEntryByteRange,
        /// Number of `/Name` elements scanned in the `/Filter` array.
        filter_count: usize,
    },
}

/// Error returned when a content stream's `/Filter` declaration is malformed.
///
/// `Err` is reserved for malformed structure; unsupported filters and
/// multi-filter chains are `Ok` classifications. This report retains or copies no
/// PDF bytes; it carries only offsets, the source length, and the structured
/// reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentStreamFilterClassificationError {
    /// Caller-supplied content stream object byte offset where classification
    /// began.
    pub byte_offset: usize,
    /// Total source length.
    pub byte_len: usize,
    /// Byte offset where the malformed construct was found, when available.
    pub error_byte_offset: Option<usize>,
    /// Structured failure reason.
    pub reason: ContentStreamFilterClassificationRejection,
}

/// Structured content stream `/Filter` classification rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ContentStreamFilterClassificationRejection {
    /// The delegated [`inspect_content_stream_start`] inspection failed, so the
    /// object is not a dictionary-bodied content stream.
    StreamStart {
        /// Underlying content-stream start rejection reason.
        stream_start_reason: ContentStreamStartInspectionRejection,
    },
    /// The stream dictionary has more than one exact top-level raw `/Filter` key.
    DuplicateFilter {
        /// First `/Filter` key range observed in source order.
        first_key_range: DictionaryEntryByteRange,
        /// Duplicate `/Filter` key range observed in source order.
        duplicate_key_range: DictionaryEntryByteRange,
    },
    /// The `/Filter` value is neither a name nor an array.
    NonNameOrArrayFilterValue {
        /// Shallow value kind reported by dictionary entry inspection.
        value_kind: DictionaryValueKind,
    },
    /// The `/Filter` array value could not be located as a balanced array.
    MalformedFilterArray {
        /// Underlying array extent rejection reason.
        array_reason: ArrayExtentInspectionRejection,
    },
    /// A `/Filter` array element is not a `/Name`.
    NonNameFilterArrayElement,
}

/// Classify a content stream object's top-level `/Filter` declaration into a
/// decode-path classification.
///
/// The helper delegates dictionary and `stream`-keyword validation to
/// [`inspect_content_stream_start`] and reuses its delegated top-level
/// `dictionary.entries`, reimplementing no header, body-token, dictionary-open,
/// or entry-span scanning. It then matches the exact top-level raw key bytes
/// `/Filter` with the same unique-entry pattern the `/Length` and `/W` matchers
/// use:
///
/// - a missing `/Filter` classifies as
///   [`Uncompressed`](ContentStreamFilterClassification::Uncompressed);
/// - exactly one `/Filter` is classified;
/// - more than one `/Filter` is a [`DuplicateFilter`](ContentStreamFilterClassificationRejection::DuplicateFilter)
///   rejection.
///
/// A single `/Name` value of exactly `/FlateDecode` classifies as
/// [`Flate`](ContentStreamFilterClassification::Flate); any other single name is
/// an [`UnsupportedFilter`](ContentStreamFilterClassification::UnsupportedFilter).
/// An array value is located with [`inspect_array_extent`] and scanned for
/// whitespace/comment-separated `/Name` elements: an empty array is
/// `Uncompressed`, one `/FlateDecode` element is `Flate`, one other element is
/// `UnsupportedFilter`, and two or more elements are an
/// [`UnsupportedFilterChain`](ContentStreamFilterClassification::UnsupportedFilterChain).
///
/// It reads, decodes, inflates, and tokenizes no stream-body bytes, parses no
/// `/DecodeParms`, resolves no indirect-reference `/Filter` value, and mutates no
/// PDF bytes. The classification carries only byte ranges and small counts;
/// filter names are matched by comparing `input[range]` against `b"/FlateDecode"`,
/// never copied.
///
/// # Errors
///
/// Returns [`ContentStreamFilterClassificationError`] for a delegated
/// [`StreamStart`](ContentStreamFilterClassificationRejection::StreamStart)
/// failure, a [`DuplicateFilter`](ContentStreamFilterClassificationRejection::DuplicateFilter),
/// a `/Filter` value that is neither a name nor an array
/// ([`NonNameOrArrayFilterValue`](ContentStreamFilterClassificationRejection::NonNameOrArrayFilterValue)),
/// an unlocatable/unbalanced `/Filter` array
/// ([`MalformedFilterArray`](ContentStreamFilterClassificationRejection::MalformedFilterArray)),
/// or a non-name element inside the `/Filter` array
/// ([`NonNameFilterArrayElement`](ContentStreamFilterClassificationRejection::NonNameFilterArrayElement)).
pub fn classify_content_stream_filter(
    input: &[u8],
    object_offset: usize,
) -> Result<ContentStreamFilterClassification, ContentStreamFilterClassificationError> {
    let stream_start = inspect_content_stream_start(input, object_offset).map_err(|error| {
        if let Some((array_reason, error_byte_offset)) =
            malformed_filter_array_from_stream_start_failure(input, object_offset, error.reason)
        {
            return classification_error(
                input,
                object_offset,
                error_byte_offset,
                ContentStreamFilterClassificationRejection::MalformedFilterArray { array_reason },
            );
        }

        classification_error(
            input,
            object_offset,
            error.error_byte_offset,
            ContentStreamFilterClassificationRejection::StreamStart {
                stream_start_reason: error.reason,
            },
        )
    })?;

    let Some(filter_entry) = filter_entry(input, &stream_start, object_offset)? else {
        return Ok(ContentStreamFilterClassification::Uncompressed);
    };

    match filter_entry.value_kind {
        DictionaryValueKind::Name => Ok(classify_single_name(input, filter_entry.value_range)),
        DictionaryValueKind::Array => {
            classify_filter_array(input, object_offset, filter_entry.value_range)
        }
        value_kind => Err(classification_error(
            input,
            object_offset,
            Some(filter_entry.value_range.start),
            ContentStreamFilterClassificationRejection::NonNameOrArrayFilterValue { value_kind },
        )),
    }
}

fn malformed_filter_array_from_stream_start_failure(
    input: &[u8],
    object_offset: usize,
    stream_start_reason: ContentStreamStartInspectionRejection,
) -> Option<(ArrayExtentInspectionRejection, Option<usize>)> {
    let ContentStreamStartInspectionRejection::ObjectDictionary {
        object_dictionary_reason:
            IndirectObjectDictionaryInspectionRejection::DictionaryEntries {
                dictionary_entries_reason:
                    DictionaryEntryInspectionRejection::ArrayExtent { array_reason },
            },
    } = stream_start_reason
    else {
        return None;
    };

    let header = inspect_indirect_object_header(input, object_offset).ok()?;
    let body_token =
        inspect_indirect_object_body_token(input, header.after_obj_keyword_offset).ok()?;
    if body_token.token_kind != IndirectObjectBodyLeadingTokenKind::DictionaryOpen {
        return None;
    }

    let (filter_array_reason, error_byte_offset) =
        crate::dictionary_entries::top_level_array_extent_error_for_key(
            input,
            body_token.first_token_byte_offset,
            FILTER_KEY,
        )?;
    if filter_array_reason == array_reason {
        Some((filter_array_reason, error_byte_offset))
    } else {
        None
    }
}

/// Locate the single exact top-level raw `/Filter` entry.
///
/// Returns `Ok(None)` when no `/Filter` key is present (the no-filter result),
/// `Ok(Some(entry))` for exactly one, and a
/// [`DuplicateFilter`](ContentStreamFilterClassificationRejection::DuplicateFilter)
/// error for more than one, mirroring the `find_length_entry`/`unique_entry`
/// pattern.
fn filter_entry(
    input: &[u8],
    stream_start: &ContentStreamStartInspection,
    object_offset: usize,
) -> Result<Option<crate::DictionaryEntrySpan>, ContentStreamFilterClassificationError> {
    unique_entry(input, &stream_start.dictionary.entries, FILTER_KEY).map_err(
        |(first_key_range, duplicate_key_range)| {
            classification_error(
                input,
                object_offset,
                Some(duplicate_key_range.start),
                ContentStreamFilterClassificationRejection::DuplicateFilter {
                    first_key_range,
                    duplicate_key_range,
                },
            )
        },
    )
}

/// Classify a single `/Name` filter value by comparing its raw bytes in place.
fn classify_single_name(
    input: &[u8],
    filter_name_range: DictionaryEntryByteRange,
) -> ContentStreamFilterClassification {
    if name_is_flate(input, filter_name_range) {
        ContentStreamFilterClassification::Flate
    } else {
        ContentStreamFilterClassification::UnsupportedFilter { filter_name_range }
    }
}

/// Classify a `/Filter` array value by scanning its `/Name` elements.
fn classify_filter_array(
    input: &[u8],
    object_offset: usize,
    filter_value_range: DictionaryEntryByteRange,
) -> Result<ContentStreamFilterClassification, ContentStreamFilterClassificationError> {
    let names = scan_name_array(input, filter_value_range.start).map_err(|error| match error {
        NameArrayError::Array {
            array_reason,
            error_byte_offset,
        } => classification_error(
            input,
            object_offset,
            error_byte_offset,
            ContentStreamFilterClassificationRejection::MalformedFilterArray { array_reason },
        ),
        NameArrayError::NonNameElement { error_byte_offset } => classification_error(
            input,
            object_offset,
            Some(error_byte_offset),
            ContentStreamFilterClassificationRejection::NonNameFilterArrayElement,
        ),
    })?;

    Ok(match (names.name_count, names.first_name_range) {
        (0, _) => ContentStreamFilterClassification::Uncompressed,
        (1, Some(filter_name_range)) => classify_single_name(input, filter_name_range),
        _ => ContentStreamFilterClassification::UnsupportedFilterChain {
            filter_value_range,
            filter_count: names.name_count,
        },
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NameArrayScan {
    first_name_range: Option<DictionaryEntryByteRange>,
    name_count: usize,
}

/// Failure reason for [`scan_name_array`].
enum NameArrayError {
    /// The balanced array extent could not be located.
    Array {
        array_reason: ArrayExtentInspectionRejection,
        error_byte_offset: Option<usize>,
    },
    /// An array element is not a `/Name`.
    NonNameElement { error_byte_offset: usize },
}

/// Locate the balanced array at `value_start` and scan its body into the byte
/// range of each `/Name` element.
///
/// The only new logic over the existing extent helpers: a bounded forward scan of
/// whitespace- and `%`-comment-separated `/Name` elements inside the
/// already-located array extent, mirroring the decimal-integer element scan used
/// for `/W` and `/Index`. A non-name token where an element was expected is a
/// distinct structured failure; no PDF bytes are retained or copied.
fn scan_name_array(input: &[u8], value_start: usize) -> Result<NameArrayScan, NameArrayError> {
    let array =
        inspect_array_extent(input, value_start).map_err(|error| NameArrayError::Array {
            array_reason: error.reason,
            error_byte_offset: error.error_byte_offset,
        })?;

    let body_end = array.close_byte_offset;
    let mut cursor = array.open_byte_offset + 1;
    let mut first_name_range = None;
    let mut name_count = 0usize;

    loop {
        cursor = skip_whitespace_and_comments(input, cursor, body_end);
        if cursor >= body_end {
            break;
        }

        if input[cursor] != b'/' {
            return Err(NameArrayError::NonNameElement {
                error_byte_offset: cursor,
            });
        }

        let name_start = cursor;
        cursor = skip_name(input, cursor, body_end);
        let name_range = DictionaryEntryByteRange {
            start: name_start,
            end: cursor,
        };
        if first_name_range.is_none() {
            first_name_range = Some(name_range);
        }
        name_count += 1;
    }

    Ok(NameArrayScan {
        first_name_range,
        name_count,
    })
}

/// Compare a name value's raw bytes in place against `/FlateDecode`.
fn name_is_flate(input: &[u8], range: DictionaryEntryByteRange) -> bool {
    input.get(range.start..range.end) == Some(FLATE_DECODE_NAME)
}

const fn classification_error(
    input: &[u8],
    object_offset: usize,
    error_byte_offset: Option<usize>,
    reason: ContentStreamFilterClassificationRejection,
) -> ContentStreamFilterClassificationError {
    ContentStreamFilterClassificationError {
        byte_offset: object_offset,
        byte_len: input.len(),
        error_byte_offset,
        reason,
    }
}
