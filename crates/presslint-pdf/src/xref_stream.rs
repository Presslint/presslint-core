use serde::{Deserialize, Serialize};

use crate::source_utils::{
    is_pdf_delimiter, is_pdf_whitespace, parse_usize_decimal, skip_whitespace_and_comments,
};
use crate::{
    ArrayExtentInspectionRejection, DictionaryEntryByteRange, DictionaryEntrySpan,
    DictionaryValueKind, IndirectObjectDictionaryInspection,
    IndirectObjectDictionaryInspectionRejection,
};

const TYPE_KEY: &[u8] = b"/Type";
const W_KEY: &[u8] = b"/W";
const SIZE_KEY: &[u8] = b"/Size";
const INDEX_KEY: &[u8] = b"/Index";
const XREF_TYPE_VALUE: &[u8] = b"/XRef";
const W_WIDTH_COUNT: usize = 3;

/// One `/Index` subsection pair from a cross-reference-stream dictionary.
///
/// The pair locates a run of entries in the (eventually decoded) cross-reference
/// table: `entry_count` consecutive entries beginning at object number
/// `first_object_number`. This report stores only the two parsed integers and
/// retains no PDF bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct XrefStreamSubsection {
    /// First object number covered by this subsection.
    pub first_object_number: usize,
    /// Number of entries declared for this subsection.
    pub entry_count: usize,
}

/// Parsed geometry fields of a cross-reference-stream (`/Type /XRef`) dictionary.
///
/// This report stores only the delegated object-dictionary inspection, byte
/// ranges, small parsed integers, and the bounded `widths` and
/// `index_subsections` vectors. It does not retain or copy PDF bytes, object
/// bodies, stream bodies, decoded bytes, or source slices, and it decodes no
/// cross-reference stream entry records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XrefStreamDictionaryInspection {
    /// Delegated indirect object dictionary inspection that supplied the
    /// header and top-level entry spans.
    pub object_dictionary: IndirectObjectDictionaryInspection,
    /// Byte range covering the exact top-level raw `/Type` key.
    pub type_key_range: DictionaryEntryByteRange,
    /// Byte range covering the `/Type` value span (the `/XRef` name).
    pub type_value_range: DictionaryEntryByteRange,
    /// Byte range covering the `/W` value span (the widths array).
    pub w_value_range: DictionaryEntryByteRange,
    /// The exactly-three `/W` field widths in source order; a width of `0`
    /// marks an omitted cross-reference field.
    pub widths: Vec<usize>,
    /// Byte range covering the `/Size` value span (the size integer).
    pub size_value_range: DictionaryEntryByteRange,
    /// Parsed `/Size` value (one past the highest object number).
    pub size: usize,
    /// Byte range covering the `/Index` value span, when the key is present.
    pub index_value_range: Option<DictionaryEntryByteRange>,
    /// Ordered `(first_object_number, entry_count)` subsection pairs; a single
    /// `(0, Size)` pair when `/Index` is absent.
    pub index_subsections: Vec<XrefStreamSubsection>,
}

/// Error returned when a cross-reference-stream dictionary cannot be inspected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XrefStreamDictionaryInspectionError {
    /// Caller-supplied byte offset where xref-stream dictionary inspection
    /// began.
    pub byte_offset: usize,
    /// Total source length.
    pub byte_len: usize,
    /// Byte offset where the resolved object header begins, when it was located.
    pub object_header_byte_offset: Option<usize>,
    /// Byte offset where the malformed or unsupported construct was found, when
    /// available.
    pub error_byte_offset: Option<usize>,
    /// Structured failure reason.
    pub reason: XrefStreamDictionaryInspectionRejection,
}

/// Structured cross-reference-stream dictionary inspection rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum XrefStreamDictionaryInspectionRejection {
    /// A delegated indirect object dictionary inspection failed.
    ObjectDictionary {
        /// Underlying object dictionary rejection reason.
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
    NonNameTypeValue {
        /// Shallow value kind reported by dictionary entry inspection.
        value_kind: DictionaryValueKind,
    },
    /// The `/Type` value is a name other than `/XRef`.
    UnexpectedTypeName,
    /// The dictionary has no exact top-level raw `/W` key.
    MissingW,
    /// The dictionary has more than one exact top-level raw `/W` key.
    DuplicateW {
        /// First `/W` key range observed in source order.
        first_key_range: DictionaryEntryByteRange,
        /// Duplicate `/W` key range observed in source order.
        duplicate_key_range: DictionaryEntryByteRange,
    },
    /// The `/W` value is not shaped as an array.
    NonArrayWValue {
        /// Shallow value kind reported by dictionary entry inspection.
        value_kind: DictionaryValueKind,
    },
    /// A delegated `/W` array extent could not be located.
    MalformedWArray {
        /// Underlying array extent rejection reason.
        array_reason: ArrayExtentInspectionRejection,
    },
    /// A `/W` array element is not a non-negative decimal integer.
    MalformedWElement,
    /// A `/W` array width does not fit `usize`.
    WidthOutOfRange,
    /// The `/W` array does not contain exactly three integers.
    WrongWLength {
        /// Number of integer elements scanned in the `/W` array.
        width_count: usize,
    },
    /// The dictionary has no exact top-level raw `/Size` key.
    MissingSize,
    /// The dictionary has more than one exact top-level raw `/Size` key.
    DuplicateSize {
        /// First `/Size` key range observed in source order.
        first_key_range: DictionaryEntryByteRange,
        /// Duplicate `/Size` key range observed in source order.
        duplicate_key_range: DictionaryEntryByteRange,
    },
    /// The `/Size` value is not a direct non-negative integer.
    NonIntegerSizeValue {
        /// Shallow value kind reported by dictionary entry inspection.
        value_kind: DictionaryValueKind,
    },
    /// The `/Size` non-negative integer does not fit `usize`.
    SizeOutOfRange,
    /// The dictionary has more than one exact top-level raw `/Index` key.
    DuplicateIndex {
        /// First `/Index` key range observed in source order.
        first_key_range: DictionaryEntryByteRange,
        /// Duplicate `/Index` key range observed in source order.
        duplicate_key_range: DictionaryEntryByteRange,
    },
    /// The `/Index` value is not shaped as an array.
    NonArrayIndexValue {
        /// Shallow value kind reported by dictionary entry inspection.
        value_kind: DictionaryValueKind,
    },
    /// A delegated `/Index` array extent could not be located.
    MalformedIndexArray {
        /// Underlying array extent rejection reason.
        array_reason: ArrayExtentInspectionRejection,
    },
    /// An `/Index` array element is not a non-negative decimal integer.
    MalformedIndexElement,
    /// The `/Index` array does not contain an even number of integers.
    OddIndexLength {
        /// Number of integer elements scanned in the `/Index` array.
        integer_count: usize,
    },
    /// An `/Index` array integer does not fit `usize`.
    IndexIntegerOutOfRange,
}

/// Inspect the geometry fields of a cross-reference-stream dictionary.
///
/// The helper delegates the object header and top-level entry spans to
/// [`crate::inspect_indirect_object_dictionary`] (so it reimplements no header,
/// body-token, dictionary-open, or entry-span scanning), then matches only the
/// exact raw top-level keys `/Type`, `/W`, `/Size`, and `/Index` the same way
/// [`crate::inspect_classic_xref_trailer_root`] matches `/Root`. It requires:
///
/// - exactly one `/Type` key whose value is the name `/XRef`;
/// - exactly one `/W` key whose value is an array of exactly three non-negative
///   integers (a width of `0` marks an omitted field), located with
///   [`crate::inspect_array_extent`] and scanned as whitespace-separated decimal
///   integer elements;
/// - exactly one `/Size` key whose value is a direct non-negative integer that
///   fits `usize`;
/// - an optional `/Index` key whose value is an array of an even count of
///   non-negative integers parsed as `(first_object_number, entry_count)`
///   subsection pairs; when `/Index` is absent it defaults to a single
///   `(0, Size)` subsection.
///
/// It decodes no cross-reference stream body bytes, parses no `/W`-width entry
/// records, builds no object offset map, and reads neither `/Root` nor `/Prev`.
/// The report retains or copies no PDF bytes, object bodies, stream bodies, or
/// source slices; its only owned allocations are the bounded `widths` and
/// `index_subsections` vectors.
///
/// # Errors
///
/// Returns [`XrefStreamDictionaryInspectionError`] for a delegated
/// object-dictionary failure, a missing/duplicate/non-name/non-`/XRef` `/Type`,
/// a missing/duplicate/non-array/malformed/wrong-length `/W`, a
/// missing/duplicate/non-integer/out-of-range `/Size`, a
/// duplicate/non-array/malformed/odd-length `/Index`, or an integer/array
/// overflow. It never returns partial geometry on error.
pub fn inspect_xref_stream_dictionary(
    input: &[u8],
    object_byte_offset: usize,
) -> Result<XrefStreamDictionaryInspection, XrefStreamDictionaryInspectionError> {
    let object_dictionary = crate::inspect_indirect_object_dictionary(input, object_byte_offset)
        .map_err(|error| {
            ErrorContext {
                byte_offset: object_byte_offset,
                byte_len: input.len(),
                object_header_byte_offset: error.header_byte_offset,
            }
            .error(
                XrefStreamDictionaryInspectionRejection::ObjectDictionary {
                    object_dictionary_reason: error.reason,
                },
                error.error_byte_offset,
            )
        })?;

    let ctx = ErrorContext {
        byte_offset: object_byte_offset,
        byte_len: input.len(),
        object_header_byte_offset: Some(object_dictionary.header_range.start),
    };
    let close_offset = object_dictionary.dictionary_close_byte_offset;

    let type_entry = require_type(input, &object_dictionary, close_offset, ctx)?;
    let (w_entry, widths) = require_widths(input, &object_dictionary, close_offset, ctx)?;
    let (size_entry, size) = require_size(input, &object_dictionary, close_offset, ctx)?;
    let index = require_index(input, &object_dictionary, size, ctx)?;

    Ok(XrefStreamDictionaryInspection {
        object_dictionary,
        type_key_range: type_entry.key_range,
        type_value_range: type_entry.value_range,
        w_value_range: w_entry.value_range,
        widths,
        size_value_range: size_entry.value_range,
        size,
        index_value_range: index.value_range,
        index_subsections: index.subsections,
    })
}

/// Locate the single exact `/Type` key and confirm its value is the `/XRef`
/// name.
fn require_type(
    input: &[u8],
    object_dictionary: &IndirectObjectDictionaryInspection,
    close_offset: usize,
    ctx: ErrorContext,
) -> Result<DictionaryEntrySpan, XrefStreamDictionaryInspectionError> {
    let entry = unique_entry(input, &object_dictionary.entries, TYPE_KEY)
        .map_err(|(first_key_range, duplicate_key_range)| {
            ctx.error(
                XrefStreamDictionaryInspectionRejection::DuplicateType {
                    first_key_range,
                    duplicate_key_range,
                },
                Some(duplicate_key_range.start),
            )
        })?
        .ok_or_else(|| {
            ctx.error(
                XrefStreamDictionaryInspectionRejection::MissingType,
                Some(close_offset),
            )
        })?;

    if entry.value_kind != DictionaryValueKind::Name {
        return Err(ctx.error(
            XrefStreamDictionaryInspectionRejection::NonNameTypeValue {
                value_kind: entry.value_kind,
            },
            Some(entry.value_range.start),
        ));
    }

    if value_bytes(input, entry.value_range) != XREF_TYPE_VALUE {
        return Err(ctx.error(
            XrefStreamDictionaryInspectionRejection::UnexpectedTypeName,
            Some(entry.value_range.start),
        ));
    }

    Ok(entry)
}

/// Locate the single exact `/W` key and scan its array into exactly three
/// non-negative integer widths.
fn require_widths(
    input: &[u8],
    object_dictionary: &IndirectObjectDictionaryInspection,
    close_offset: usize,
    ctx: ErrorContext,
) -> Result<(DictionaryEntrySpan, Vec<usize>), XrefStreamDictionaryInspectionError> {
    let entry = unique_entry(input, &object_dictionary.entries, W_KEY)
        .map_err(|(first_key_range, duplicate_key_range)| {
            ctx.error(
                XrefStreamDictionaryInspectionRejection::DuplicateW {
                    first_key_range,
                    duplicate_key_range,
                },
                Some(duplicate_key_range.start),
            )
        })?
        .ok_or_else(|| {
            ctx.error(
                XrefStreamDictionaryInspectionRejection::MissingW,
                Some(close_offset),
            )
        })?;

    if entry.value_kind != DictionaryValueKind::Array {
        return Err(ctx.error(
            XrefStreamDictionaryInspectionRejection::NonArrayWValue {
                value_kind: entry.value_kind,
            },
            Some(entry.value_range.start),
        ));
    }

    let widths =
        scan_integer_array(input, entry.value_range.start).map_err(|element| match element {
            IntegerArrayError::Array {
                array_reason,
                error_byte_offset,
            } => ctx.error(
                XrefStreamDictionaryInspectionRejection::MalformedWArray { array_reason },
                error_byte_offset,
            ),
            IntegerArrayError::MalformedElement { error_byte_offset } => ctx.error(
                XrefStreamDictionaryInspectionRejection::MalformedWElement,
                Some(error_byte_offset),
            ),
            IntegerArrayError::ElementOutOfRange { error_byte_offset } => ctx.error(
                XrefStreamDictionaryInspectionRejection::WidthOutOfRange,
                Some(error_byte_offset),
            ),
        })?;

    if widths.len() != W_WIDTH_COUNT {
        return Err(ctx.error(
            XrefStreamDictionaryInspectionRejection::WrongWLength {
                width_count: widths.len(),
            },
            Some(entry.value_range.start),
        ));
    }

    Ok((entry, widths))
}

/// Locate the single exact `/Size` key and parse its value as a direct
/// non-negative integer.
fn require_size(
    input: &[u8],
    object_dictionary: &IndirectObjectDictionaryInspection,
    close_offset: usize,
    ctx: ErrorContext,
) -> Result<(DictionaryEntrySpan, usize), XrefStreamDictionaryInspectionError> {
    let entry = unique_entry(input, &object_dictionary.entries, SIZE_KEY)
        .map_err(|(first_key_range, duplicate_key_range)| {
            ctx.error(
                XrefStreamDictionaryInspectionRejection::DuplicateSize {
                    first_key_range,
                    duplicate_key_range,
                },
                Some(duplicate_key_range.start),
            )
        })?
        .ok_or_else(|| {
            ctx.error(
                XrefStreamDictionaryInspectionRejection::MissingSize,
                Some(close_offset),
            )
        })?;

    let size = match parse_non_negative_integer(value_bytes(input, entry.value_range)) {
        Ok(size) => size,
        Err(IntegerError::Malformed) => {
            return Err(ctx.error(
                XrefStreamDictionaryInspectionRejection::NonIntegerSizeValue {
                    value_kind: entry.value_kind,
                },
                Some(entry.value_range.start),
            ));
        }
        Err(IntegerError::OutOfRange) => {
            return Err(ctx.error(
                XrefStreamDictionaryInspectionRejection::SizeOutOfRange,
                Some(entry.value_range.start),
            ));
        }
    };

    Ok((entry, size))
}

/// Optional `/Index` location: a value byte range when present and the ordered
/// subsection pairs (defaulted to `(0, Size)` when absent).
struct IndexResult {
    value_range: Option<DictionaryEntryByteRange>,
    subsections: Vec<XrefStreamSubsection>,
}

/// Locate the optional single exact `/Index` key and scan its array into an
/// even-length run of `(first_object_number, entry_count)` subsection pairs,
/// defaulting to a single `(0, Size)` subsection when the key is absent.
fn require_index(
    input: &[u8],
    object_dictionary: &IndirectObjectDictionaryInspection,
    size: usize,
    ctx: ErrorContext,
) -> Result<IndexResult, XrefStreamDictionaryInspectionError> {
    let Some(entry) = unique_entry(input, &object_dictionary.entries, INDEX_KEY).map_err(
        |(first_key_range, duplicate_key_range)| {
            ctx.error(
                XrefStreamDictionaryInspectionRejection::DuplicateIndex {
                    first_key_range,
                    duplicate_key_range,
                },
                Some(duplicate_key_range.start),
            )
        },
    )?
    else {
        return Ok(IndexResult {
            value_range: None,
            subsections: vec![XrefStreamSubsection {
                first_object_number: 0,
                entry_count: size,
            }],
        });
    };

    if entry.value_kind != DictionaryValueKind::Array {
        return Err(ctx.error(
            XrefStreamDictionaryInspectionRejection::NonArrayIndexValue {
                value_kind: entry.value_kind,
            },
            Some(entry.value_range.start),
        ));
    }

    let integers =
        scan_integer_array(input, entry.value_range.start).map_err(|element| match element {
            IntegerArrayError::Array {
                array_reason,
                error_byte_offset,
            } => ctx.error(
                XrefStreamDictionaryInspectionRejection::MalformedIndexArray { array_reason },
                error_byte_offset,
            ),
            IntegerArrayError::MalformedElement { error_byte_offset } => ctx.error(
                XrefStreamDictionaryInspectionRejection::MalformedIndexElement,
                Some(error_byte_offset),
            ),
            IntegerArrayError::ElementOutOfRange { error_byte_offset } => ctx.error(
                XrefStreamDictionaryInspectionRejection::IndexIntegerOutOfRange,
                Some(error_byte_offset),
            ),
        })?;

    if integers.len() % 2 != 0 {
        return Err(ctx.error(
            XrefStreamDictionaryInspectionRejection::OddIndexLength {
                integer_count: integers.len(),
            },
            Some(entry.value_range.start),
        ));
    }

    let subsections = integers
        .chunks_exact(2)
        .map(|pair| XrefStreamSubsection {
            first_object_number: pair[0],
            entry_count: pair[1],
        })
        .collect();

    Ok(IndexResult {
        value_range: Some(entry.value_range),
        subsections,
    })
}

/// Failure reason for [`scan_integer_array`].
enum IntegerArrayError {
    /// The balanced array extent could not be located.
    Array {
        array_reason: ArrayExtentInspectionRejection,
        error_byte_offset: Option<usize>,
    },
    /// An array element is not a non-negative decimal integer token.
    MalformedElement { error_byte_offset: usize },
    /// An array element integer does not fit `usize`.
    ElementOutOfRange { error_byte_offset: usize },
}

/// Locate the balanced array at `value_start` and scan its body into a vector
/// of non-negative decimal integers.
///
/// The only new logic over the existing extent helpers: a bounded forward scan
/// of whitespace-separated (and `%`-comment-separated) decimal integer elements
/// inside the already-located array extent. A non-digit token, a delimiter where
/// an element was expected, or an over-`usize` value is a distinct structured
/// failure; no PDF bytes are retained.
fn scan_integer_array(input: &[u8], value_start: usize) -> Result<Vec<usize>, IntegerArrayError> {
    let array = crate::inspect_array_extent(input, value_start).map_err(|err| {
        IntegerArrayError::Array {
            array_reason: err.reason,
            error_byte_offset: err.error_byte_offset,
        }
    })?;

    let body_end = array.close_byte_offset;
    let mut cursor = array.open_byte_offset + 1;
    let mut values = Vec::new();

    loop {
        cursor = skip_whitespace_and_comments(input, cursor, body_end);
        if cursor >= body_end {
            break;
        }

        let token_start = cursor;
        while cursor < body_end
            && !is_pdf_whitespace(input[cursor])
            && !is_pdf_delimiter(input[cursor])
        {
            cursor += 1;
        }

        let token = &input[token_start..cursor];
        if token.is_empty() || !token.iter().all(u8::is_ascii_digit) {
            return Err(IntegerArrayError::MalformedElement {
                error_byte_offset: token_start,
            });
        }

        let value = parse_usize_decimal(token).ok_or(IntegerArrayError::ElementOutOfRange {
            error_byte_offset: token_start,
        })?;
        values.push(value);
    }

    Ok(values)
}

/// Failure reason for [`parse_non_negative_integer`].
pub enum IntegerError {
    /// The bytes are empty or are not all ASCII digits.
    Malformed,
    /// The non-negative integer does not fit `usize`.
    OutOfRange,
}

/// Parse a value span as a pure non-negative decimal integer.
///
/// Shared with the cross-reference-stream trailer inspector so its `/Prev` byte
/// offset is parsed with the same non-negative-decimal-integer-fitting-`usize`
/// rule this module applies to `/Size`.
pub fn parse_non_negative_integer(bytes: &[u8]) -> Result<usize, IntegerError> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(IntegerError::Malformed);
    }
    parse_usize_decimal(bytes).ok_or(IntegerError::OutOfRange)
}

/// Select the single dictionary entry whose exact raw key bytes match `key`.
///
/// Returns `Ok(Some(entry))` for exactly one match, `Ok(None)` for zero, and
/// `Err((first_key_range, duplicate_key_range))` when more than one matches.
///
/// Shared with the cross-reference-stream trailer inspector so it matches
/// `/Root` and `/Prev` with the same exact-key, missing/duplicate semantics this
/// module uses for the geometry fields.
pub fn unique_entry(
    input: &[u8],
    entries: &[DictionaryEntrySpan],
    key: &[u8],
) -> Result<Option<DictionaryEntrySpan>, (DictionaryEntryByteRange, DictionaryEntryByteRange)> {
    let mut found: Option<DictionaryEntrySpan> = None;
    for entry in entries {
        if input.get(entry.key_range.start..entry.key_range.end) != Some(key) {
            continue;
        }
        if let Some(first) = found {
            return Err((first.key_range, entry.key_range));
        }
        found = Some(*entry);
    }
    Ok(found)
}

fn value_bytes(input: &[u8], range: DictionaryEntryByteRange) -> &[u8] {
    &input[range.start..range.end]
}

/// Copyable byte-context shared by the field-requirement helpers so each can
/// build an [`XrefStreamDictionaryInspectionError`] without re-threading the
/// caller offset, source length, and resolved object-header offset.
#[derive(Clone, Copy)]
struct ErrorContext {
    byte_offset: usize,
    byte_len: usize,
    object_header_byte_offset: Option<usize>,
}

impl ErrorContext {
    const fn error(
        self,
        reason: XrefStreamDictionaryInspectionRejection,
        error_byte_offset: Option<usize>,
    ) -> XrefStreamDictionaryInspectionError {
        XrefStreamDictionaryInspectionError {
            byte_offset: self.byte_offset,
            byte_len: self.byte_len,
            object_header_byte_offset: self.object_header_byte_offset,
            error_byte_offset,
            reason,
        }
    }
}
