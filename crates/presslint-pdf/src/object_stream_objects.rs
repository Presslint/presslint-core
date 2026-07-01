use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::source_utils::is_pdf_whitespace;
use crate::xref_stream::{IntegerError, parse_non_negative_integer, unique_entry};
use crate::{
    ContentStreamDataSliceRejection, ContentStreamFilterClassification,
    ContentStreamFilterClassificationRejection, DictionaryEntryByteRange, DictionaryEntrySpan,
    DictionaryValueKind, FlateDecodeParametersResolution, FlateDecodeParametersResolutionRejection,
    FlateDecodeStreamRejection, IndirectObjectDictionaryInspection,
    IndirectObjectDictionaryInspectionRejection, classify_content_stream_filter,
    content_stream_data_slice, decode_flate_stream, inspect_content_stream_data_extent,
    inspect_indirect_object_dictionary, inspect_indirect_object_header,
    resolve_flate_decode_parameters,
};

const TYPE_KEY: &[u8] = b"/Type";
const N_KEY: &[u8] = b"/N";
const FIRST_KEY: &[u8] = b"/First";
const EXTENDS_KEY: &[u8] = b"/Extends";
const OBJSTM_TYPE_VALUE: &[u8] = b"/ObjStm";

/// One member object extracted from a decoded `/ObjStm` object stream.
///
/// This is the body-aware currency the compressed-object resolution path needs:
/// it owns the bounded decoded object-stream buffer and reports the byte span of
/// exactly one member body inside it. Compressed member bodies live only in the
/// decoded stream bytes, not in the original source, so the buffer is the single
/// intentional owned allocation. No source PDF bytes are retained; the span is
/// addressed relative to [`decoded_object_stream`](Self::decoded_object_stream).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedObjectStreamMember {
    /// Parsed `/N` object count of the containing object stream.
    pub object_count: usize,
    /// Parsed `/First` offset of the first member body within the decoded body.
    pub first_body_byte_offset: usize,
    /// Whether the object-stream dictionary carried an `/Extends` key. The chain
    /// is tolerated for diagnostics but never followed by this slice.
    pub has_extends: bool,
    /// Bounded decoded object-stream body buffer that owns the member bytes.
    pub decoded_object_stream: Vec<u8>,
    /// Byte span of the selected member body within `decoded_object_stream`.
    pub object_body_span: Range<usize>,
}

/// Error returned when a `/ObjStm` member cannot be validated and extracted.
///
/// This report retains or copies no PDF source bytes; it carries only the
/// object-stream object byte offset, the source length, an optional source
/// error offset, and the structured reason. Failures anchored inside the decoded
/// buffer (header, offset, and body-shape checks) report no source
/// `error_byte_offset` because the decoded buffer is not addressable in `input`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectStreamMemberExtractionError {
    /// Caller-supplied object-stream object byte offset where extraction began.
    pub object_stream_byte_offset: usize,
    /// Total source length.
    pub byte_len: usize,
    /// Byte offset in `input` where a malformed construct was found, when the
    /// failing stage operated on the source bytes.
    pub error_byte_offset: Option<usize>,
    /// Structured failure reason.
    pub reason: ObjectStreamMemberExtractionRejection,
}

/// Structured `/ObjStm` member extraction rejection reasons.
///
/// Every variant carries only small `Copy` values (byte ranges, delegated
/// rejection-reason enums, and integers), so the reason stays `Copy` and can be
/// embedded in the `Copy` object-resolution rejection without retaining bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ObjectStreamMemberExtractionRejection {
    /// The delegated object-dictionary inspection of the `/ObjStm` object failed.
    ObjectDictionary {
        /// Underlying object-dictionary rejection reason.
        object_dictionary_reason: IndirectObjectDictionaryInspectionRejection,
    },
    /// The dictionary has no exact top-level raw `/Type` key.
    MissingType,
    /// The dictionary has more than one exact top-level raw `/Type` key.
    DuplicateType {
        /// First `/Type` key range observed in source order.
        first_key_range: DictionaryEntryByteRange,
        /// Duplicate `/Type` key range observed in source order.
        duplicate_key_range: DictionaryEntryByteRange,
    },
    /// The `/Type` value is not shaped as a name.
    NonNameType {
        /// Shallow value kind reported by dictionary entry inspection.
        value_kind: DictionaryValueKind,
    },
    /// The `/Type` value is a name other than `/ObjStm`.
    UnexpectedType,
    /// The dictionary has no exact top-level raw `/N` key.
    MissingObjectCount,
    /// The dictionary has more than one exact top-level raw `/N` key.
    DuplicateObjectCount {
        /// First `/N` key range observed in source order.
        first_key_range: DictionaryEntryByteRange,
        /// Duplicate `/N` key range observed in source order.
        duplicate_key_range: DictionaryEntryByteRange,
    },
    /// The `/N` value is not a direct non-negative integer.
    MalformedObjectCount,
    /// The `/N` non-negative integer does not fit `usize`.
    ObjectCountOutOfRange,
    /// The dictionary has no exact top-level raw `/First` key.
    MissingFirst,
    /// The dictionary has more than one exact top-level raw `/First` key.
    DuplicateFirst {
        /// First `/First` key range observed in source order.
        first_key_range: DictionaryEntryByteRange,
        /// Duplicate `/First` key range observed in source order.
        duplicate_key_range: DictionaryEntryByteRange,
    },
    /// The `/First` value is not a direct non-negative integer.
    MalformedFirst,
    /// The `/First` non-negative integer does not fit `usize`.
    FirstOutOfRange,
    /// `/First` points past the end of the decoded object-stream body.
    FirstBeyondDecoded {
        /// Parsed `/First` byte offset.
        first: usize,
        /// Decoded object-stream body length.
        decoded_len: usize,
    },
    /// The stream-data byte extent of the object stream could not be located.
    ///
    /// The failing source offset is carried by the enclosing
    /// [`ObjectStreamMemberExtractionError::error_byte_offset`]; the delegated
    /// extent rejection reason is not embedded because the object stream is
    /// decoded through the direct-`/Length` path only.
    StreamExtent,
    /// The located extent could not be bridged to a borrowed source slice.
    Slice {
        /// Underlying slice rejection reason.
        slice_reason: ContentStreamDataSliceRejection,
    },
    /// The stream `/Filter` declaration was malformed.
    FilterClassification {
        /// Underlying filter-classification rejection reason.
        filter_reason: ContentStreamFilterClassificationRejection,
    },
    /// The object stream uses a filter shape this slice does not decode
    /// (non-Flate, chained, or otherwise unsupported).
    UnsupportedFilter {
        /// Delegated filter classification.
        classification: ContentStreamFilterClassification,
    },
    /// The stream `/DecodeParms` declaration was malformed.
    DecodeParms {
        /// Underlying `/DecodeParms` rejection reason.
        decode_parms_reason: FlateDecodeParametersResolutionRejection,
    },
    /// The object stream uses the unsupported array `/DecodeParms` form.
    UnsupportedDecodeParms {
        /// Byte range covering the `/DecodeParms` array value span.
        decode_parms_value_range: DictionaryEntryByteRange,
    },
    /// The bounded `/FlateDecode` operation failed.
    FlateDecode {
        /// Underlying Flate decode rejection reason.
        flate_reason: FlateDecodeStreamRejection,
    },
    /// The unfiltered object-stream body exceeds the decode byte limit.
    DecodedObjectStreamTooLarge {
        /// Unfiltered body length.
        length: usize,
        /// Caller-supplied decode byte limit.
        limit: usize,
    },
    /// A header integer in `decoded[..First]` is not a non-negative decimal.
    MalformedHeaderInteger,
    /// A header integer in `decoded[..First]` does not fit `usize`.
    HeaderIntegerOutOfRange,
    /// The `decoded[..First]` header did not contain exactly `2 * N` integers.
    HeaderPairCountMismatch {
        /// Required integer count (`2 * N`).
        expected_integers: usize,
        /// Integer count actually scanned in the header area.
        actual_integers: usize,
    },
    /// The requested member index is not less than `/N`.
    IndexOutOfRange {
        /// Requested member index.
        index: usize,
        /// Parsed `/N` object count.
        object_count: usize,
    },
    /// A member offset plus `/First` points past the decoded body.
    OffsetOutOfRange {
        /// Member index whose offset is out of range.
        index: usize,
        /// Offending relative offset value.
        offset: usize,
        /// Decoded object-stream body length.
        decoded_len: usize,
    },
    /// The member offsets are not strictly increasing.
    OffsetNotStrictlyIncreasing {
        /// Index of the first offset that did not exceed its predecessor.
        index: usize,
    },
    /// The selected header pair's object number does not equal the request.
    ObjectNumberMismatch {
        /// Requested object number.
        expected: u32,
        /// Object number stored in the selected header pair.
        found: usize,
    },
    /// The extracted member body begins with an indirect-object header. Object
    /// stream members are bare object bodies, never `N G obj ... endobj`.
    BodyBeginsWithIndirectHeader,
}

/// Validate a `/ObjStm` object and extract exactly one compressed member body.
///
/// The helper resolves nothing itself: the caller supplies the already-resolved
/// object-stream object byte offset (typically from
/// [`resolve_xref_object_offset`](crate::resolve_xref_object_offset)). It then:
///
/// - delegates dictionary validation to [`inspect_indirect_object_dictionary`],
///   requiring exactly `/Type /ObjStm` and exactly one direct non-negative
///   integer `/N` and `/First` (both fitting `usize`);
/// - decodes the object-stream body through the existing stream-extent, filter
///   classification, `/DecodeParms`, and bounded Flate helpers, copying an
///   unfiltered body into a bounded owned buffer so the result never borrows
///   source bytes;
/// - requires `/First <= decoded.len()`, parses `decoded[..First]` as exactly
///   `N` `(object number, offset)` integer pairs, and requires the offsets to be
///   in range and strictly increasing;
/// - selects member `index`, requires `index < N` and the selected pair's object
///   number to equal `requested_object_number`, and computes the member body
///   span `First + offset_index .. First + offset_{index + 1}` (or `decoded.len()`
///   for the last member);
/// - rejects a member body that begins with an indirect-object header.
///
/// It follows no `/Extends` chain (only records its presence), builds no cache,
/// and mutates no bytes.
///
/// # Errors
///
/// Returns [`ObjectStreamMemberExtractionError`] for a delegated dictionary
/// failure, a bad `/Type`/`/N`/`/First`, a decode-path failure or byte-limit
/// overflow, a malformed or wrong-count header, an out-of-range index or offset,
/// an object-number mismatch, or a member body with an indirect-object header.
pub fn extract_object_stream_member(
    input: &[u8],
    object_stream_byte_offset: usize,
    requested_object_number: u32,
    index: usize,
    max_decoded_object_stream_bytes: usize,
) -> Result<ExtractedObjectStreamMember, ObjectStreamMemberExtractionError> {
    let ctx = Ctx {
        object_stream_byte_offset,
        byte_len: input.len(),
    };

    let object_dictionary = inspect_indirect_object_dictionary(input, object_stream_byte_offset)
        .map_err(|error| {
            ctx.error(
                error.error_byte_offset,
                ObjectStreamMemberExtractionRejection::ObjectDictionary {
                    object_dictionary_reason: error.reason,
                },
            )
        })?;

    require_objstm_type(input, &object_dictionary, ctx)?;
    let object_count = require_object_count(input, &object_dictionary, ctx)?;
    let first = require_first(input, &object_dictionary, ctx)?;
    let has_extends = object_dictionary
        .entries
        .iter()
        .any(|entry| input.get(entry.key_range.start..entry.key_range.end) == Some(EXTENDS_KEY));

    let decoded = decode_object_stream_body(
        input,
        ctx,
        &object_dictionary,
        max_decoded_object_stream_bytes,
    )?;

    if first > decoded.len() {
        return Err(ctx.error(
            None,
            ObjectStreamMemberExtractionRejection::FirstBeyondDecoded {
                first,
                decoded_len: decoded.len(),
            },
        ));
    }

    let (object_numbers, offsets) = parse_header_pairs(&decoded[..first], object_count, ctx)?;
    validate_offsets(&offsets, first, decoded.len(), ctx)?;

    if index >= object_count {
        return Err(ctx.error(
            None,
            ObjectStreamMemberExtractionRejection::IndexOutOfRange {
                index,
                object_count,
            },
        ));
    }

    if object_numbers[index] != requested_object_number as usize {
        return Err(ctx.error(
            None,
            ObjectStreamMemberExtractionRejection::ObjectNumberMismatch {
                expected: requested_object_number,
                found: object_numbers[index],
            },
        ));
    }

    let body_start = first + offsets[index];
    let body_end = if index + 1 < object_count {
        first + offsets[index + 1]
    } else {
        decoded.len()
    };

    if inspect_indirect_object_header(&decoded[body_start..body_end], 0).is_ok() {
        return Err(ctx.error(
            None,
            ObjectStreamMemberExtractionRejection::BodyBeginsWithIndirectHeader,
        ));
    }

    Ok(ExtractedObjectStreamMember {
        object_count,
        first_body_byte_offset: first,
        has_extends,
        decoded_object_stream: decoded,
        object_body_span: body_start..body_end,
    })
}

/// Locate the single exact `key` entry the `/ObjStm` dictionary must carry,
/// mapping a duplicate or a missing key to the caller-supplied rejection.
///
/// A duplicate anchors the error at the second key; a missing key anchors it at
/// the dictionary close, matching every `/ObjStm` field-requirement rule.
fn require_unique_entry(
    input: &[u8],
    object_dictionary: &IndirectObjectDictionaryInspection,
    key: &[u8],
    ctx: Ctx,
    on_duplicate: impl FnOnce(
        DictionaryEntryByteRange,
        DictionaryEntryByteRange,
    ) -> ObjectStreamMemberExtractionRejection,
    on_missing: ObjectStreamMemberExtractionRejection,
) -> Result<DictionaryEntrySpan, ObjectStreamMemberExtractionError> {
    unique_entry(input, &object_dictionary.entries, key)
        .map_err(|(first_key_range, duplicate_key_range)| {
            ctx.error(
                Some(duplicate_key_range.start),
                on_duplicate(first_key_range, duplicate_key_range),
            )
        })?
        .ok_or_else(|| {
            ctx.error(
                Some(object_dictionary.dictionary_close_byte_offset),
                on_missing,
            )
        })
}

/// Parse a located entry's value as a direct non-negative integer, mapping the
/// two integer failure modes to the caller-supplied rejections. Both failures
/// anchor at the value start.
fn require_non_negative_integer(
    input: &[u8],
    entry: DictionaryEntrySpan,
    ctx: Ctx,
    on_malformed: ObjectStreamMemberExtractionRejection,
    on_out_of_range: ObjectStreamMemberExtractionRejection,
) -> Result<usize, ObjectStreamMemberExtractionError> {
    match parse_non_negative_integer(value_bytes(input, entry.value_range)) {
        Ok(value) => Ok(value),
        Err(IntegerError::Malformed) => Err(ctx.error(Some(entry.value_range.start), on_malformed)),
        Err(IntegerError::OutOfRange) => {
            Err(ctx.error(Some(entry.value_range.start), on_out_of_range))
        }
    }
}

/// Locate the single exact `/Type` key and confirm its value is the `/ObjStm`
/// name.
fn require_objstm_type(
    input: &[u8],
    object_dictionary: &IndirectObjectDictionaryInspection,
    ctx: Ctx,
) -> Result<(), ObjectStreamMemberExtractionError> {
    let entry = require_unique_entry(
        input,
        object_dictionary,
        TYPE_KEY,
        ctx,
        |first_key_range, duplicate_key_range| {
            ObjectStreamMemberExtractionRejection::DuplicateType {
                first_key_range,
                duplicate_key_range,
            }
        },
        ObjectStreamMemberExtractionRejection::MissingType,
    )?;

    if entry.value_kind != DictionaryValueKind::Name {
        return Err(ctx.error(
            Some(entry.value_range.start),
            ObjectStreamMemberExtractionRejection::NonNameType {
                value_kind: entry.value_kind,
            },
        ));
    }

    if value_bytes(input, entry.value_range) != OBJSTM_TYPE_VALUE {
        return Err(ctx.error(
            Some(entry.value_range.start),
            ObjectStreamMemberExtractionRejection::UnexpectedType,
        ));
    }

    Ok(())
}

/// Locate the single exact `/N` key and parse its value as a direct non-negative
/// integer.
fn require_object_count(
    input: &[u8],
    object_dictionary: &IndirectObjectDictionaryInspection,
    ctx: Ctx,
) -> Result<usize, ObjectStreamMemberExtractionError> {
    let entry = require_unique_entry(
        input,
        object_dictionary,
        N_KEY,
        ctx,
        |first_key_range, duplicate_key_range| {
            ObjectStreamMemberExtractionRejection::DuplicateObjectCount {
                first_key_range,
                duplicate_key_range,
            }
        },
        ObjectStreamMemberExtractionRejection::MissingObjectCount,
    )?;

    require_non_negative_integer(
        input,
        entry,
        ctx,
        ObjectStreamMemberExtractionRejection::MalformedObjectCount,
        ObjectStreamMemberExtractionRejection::ObjectCountOutOfRange,
    )
}

/// Locate the single exact `/First` key and parse its value as a direct
/// non-negative integer.
fn require_first(
    input: &[u8],
    object_dictionary: &IndirectObjectDictionaryInspection,
    ctx: Ctx,
) -> Result<usize, ObjectStreamMemberExtractionError> {
    let entry = require_unique_entry(
        input,
        object_dictionary,
        FIRST_KEY,
        ctx,
        |first_key_range, duplicate_key_range| {
            ObjectStreamMemberExtractionRejection::DuplicateFirst {
                first_key_range,
                duplicate_key_range,
            }
        },
        ObjectStreamMemberExtractionRejection::MissingFirst,
    )?;

    require_non_negative_integer(
        input,
        entry,
        ctx,
        ObjectStreamMemberExtractionRejection::MalformedFirst,
        ObjectStreamMemberExtractionRejection::FirstOutOfRange,
    )
}

/// Locate, borrow, and (when `/FlateDecode`) decode the object-stream body into a
/// bounded owned buffer, mirroring the cross-reference-stream section decoder. An
/// unfiltered body is copied so the returned buffer never borrows source bytes.
fn decode_object_stream_body(
    input: &[u8],
    ctx: Ctx,
    object_dictionary: &IndirectObjectDictionaryInspection,
    max_decoded_object_stream_bytes: usize,
) -> Result<Vec<u8>, ObjectStreamMemberExtractionError> {
    let object_offset = object_dictionary.header_range.start;
    let extent =
        inspect_content_stream_data_extent(input, None, object_offset).map_err(|error| {
            ctx.error(
                error.error_byte_offset,
                ObjectStreamMemberExtractionRejection::StreamExtent,
            )
        })?;

    let stream_data = content_stream_data_slice(input, &extent).map_err(|error| {
        ctx.error(
            None,
            ObjectStreamMemberExtractionRejection::Slice {
                slice_reason: error.reason,
            },
        )
    })?;

    match classify_content_stream_filter(input, object_offset).map_err(|error| {
        ctx.error(
            error.error_byte_offset,
            ObjectStreamMemberExtractionRejection::FilterClassification {
                filter_reason: error.reason,
            },
        )
    })? {
        ContentStreamFilterClassification::Uncompressed => {
            if stream_data.len() > max_decoded_object_stream_bytes {
                return Err(ctx.error(
                    None,
                    ObjectStreamMemberExtractionRejection::DecodedObjectStreamTooLarge {
                        length: stream_data.len(),
                        limit: max_decoded_object_stream_bytes,
                    },
                ));
            }
            Ok(stream_data.to_vec())
        }
        ContentStreamFilterClassification::Flate => {
            let resolution =
                resolve_flate_decode_parameters(input, object_offset).map_err(|error| {
                    ctx.error(
                        error.error_byte_offset,
                        ObjectStreamMemberExtractionRejection::DecodeParms {
                            decode_parms_reason: error.reason,
                        },
                    )
                })?;
            let parameters = match resolution {
                FlateDecodeParametersResolution::Resolved { parameters, .. } => parameters,
                FlateDecodeParametersResolution::UnsupportedArrayParms {
                    decode_parms_value_range,
                } => {
                    return Err(ctx.error(
                        None,
                        ObjectStreamMemberExtractionRejection::UnsupportedDecodeParms {
                            decode_parms_value_range,
                        },
                    ));
                }
            };
            decode_flate_stream(stream_data, parameters, max_decoded_object_stream_bytes).map_err(
                |error| {
                    ctx.error(
                        None,
                        ObjectStreamMemberExtractionRejection::FlateDecode {
                            flate_reason: error.reason,
                        },
                    )
                },
            )
        }
        classification @ (ContentStreamFilterClassification::UnsupportedFilter { .. }
        | ContentStreamFilterClassification::UnsupportedFilterChain { .. }) => Err(ctx.error(
            None,
            ObjectStreamMemberExtractionRejection::UnsupportedFilter { classification },
        )),
    }
}

/// Parse `decoded[..First]` as exactly `N` `(object number, offset)` pairs.
fn parse_header_pairs(
    header: &[u8],
    object_count: usize,
    ctx: Ctx,
) -> Result<(Vec<usize>, Vec<usize>), ObjectStreamMemberExtractionError> {
    let integers = scan_header_integers(header).map_err(|scan| {
        ctx.error(
            None,
            match scan {
                HeaderScanError::Malformed => {
                    ObjectStreamMemberExtractionRejection::MalformedHeaderInteger
                }
                HeaderScanError::OutOfRange => {
                    ObjectStreamMemberExtractionRejection::HeaderIntegerOutOfRange
                }
            },
        )
    })?;

    let expected_integers = object_count.saturating_mul(2);
    if integers.len() != expected_integers {
        return Err(ctx.error(
            None,
            ObjectStreamMemberExtractionRejection::HeaderPairCountMismatch {
                expected_integers,
                actual_integers: integers.len(),
            },
        ));
    }

    let mut object_numbers = Vec::with_capacity(object_count);
    let mut offsets = Vec::with_capacity(object_count);
    for pair in integers.chunks_exact(2) {
        object_numbers.push(pair[0]);
        offsets.push(pair[1]);
    }
    Ok((object_numbers, offsets))
}

/// Require every `First + offset` to stay within the decoded body and the
/// offsets to be strictly increasing.
fn validate_offsets(
    offsets: &[usize],
    first: usize,
    decoded_len: usize,
    ctx: Ctx,
) -> Result<(), ObjectStreamMemberExtractionError> {
    let mut previous: Option<usize> = None;
    for (index, &offset) in offsets.iter().enumerate() {
        match first.checked_add(offset) {
            Some(absolute) if absolute <= decoded_len => {}
            _ => {
                return Err(ctx.error(
                    None,
                    ObjectStreamMemberExtractionRejection::OffsetOutOfRange {
                        index,
                        offset,
                        decoded_len,
                    },
                ));
            }
        }

        if previous.is_some_and(|prior| offset <= prior) {
            return Err(ctx.error(
                None,
                ObjectStreamMemberExtractionRejection::OffsetNotStrictlyIncreasing { index },
            ));
        }
        previous = Some(offset);
    }
    Ok(())
}

/// Non-`usize`-fitting or non-digit failure of [`scan_header_integers`].
enum HeaderScanError {
    Malformed,
    OutOfRange,
}

/// Scan a byte region into whitespace-separated non-negative decimal integers.
fn scan_header_integers(region: &[u8]) -> Result<Vec<usize>, HeaderScanError> {
    let mut values = Vec::new();
    let mut cursor = 0;
    while cursor < region.len() {
        while cursor < region.len() && is_pdf_whitespace(region[cursor]) {
            cursor += 1;
        }
        if cursor >= region.len() {
            break;
        }

        let token_start = cursor;
        while cursor < region.len() && !is_pdf_whitespace(region[cursor]) {
            cursor += 1;
        }

        match parse_non_negative_integer(&region[token_start..cursor]) {
            Ok(value) => values.push(value),
            Err(IntegerError::Malformed) => return Err(HeaderScanError::Malformed),
            Err(IntegerError::OutOfRange) => return Err(HeaderScanError::OutOfRange),
        }
    }
    Ok(values)
}

fn value_bytes(input: &[u8], range: DictionaryEntryByteRange) -> &[u8] {
    &input[range.start..range.end]
}

/// Copyable byte-context shared by the field helpers so each can build an
/// [`ObjectStreamMemberExtractionError`] without re-threading the object-stream
/// offset and source length.
#[derive(Clone, Copy)]
struct Ctx {
    object_stream_byte_offset: usize,
    byte_len: usize,
}

impl Ctx {
    const fn error(
        self,
        error_byte_offset: Option<usize>,
        reason: ObjectStreamMemberExtractionRejection,
    ) -> ObjectStreamMemberExtractionError {
        ObjectStreamMemberExtractionError {
            object_stream_byte_offset: self.object_stream_byte_offset,
            byte_len: self.byte_len,
            error_byte_offset,
            reason,
        }
    }
}
