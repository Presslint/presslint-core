use serde::{Deserialize, Serialize};

use crate::object_stream::{LengthEntryIssue, find_length_entry};
use crate::{
    ClassicXrefTableInspection, ContentStreamStartInspectionRejection, DictionaryEntryByteRange,
    DictionaryValueKind, DirectLengthContentStreamDataExtentInspection,
    DirectLengthContentStreamDataExtentInspectionRejection,
    IndirectLengthContentStreamDataExtentInspection,
    IndirectLengthContentStreamDataExtentInspectionRejection, inspect_content_stream_start,
    inspect_direct_length_content_stream_data_extent,
    inspect_indirect_length_content_stream_data_extent,
};

/// Located byte extent for a content stream whose top-level `/Length` is either
/// direct or resolved through a caller-supplied classic xref table.
///
/// This report stores only one fixed-size delegated focused-helper report. It
/// does not retain or copy stream bytes, decoded bytes, object bodies,
/// dictionaries, source slices, or PDF payload bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "length_kind", rename_all = "snake_case")]
pub enum ContentStreamDataExtentInspection {
    /// Extent located by the direct integer `/Length` helper.
    DirectLength(DirectLengthContentStreamDataExtentInspection),
    /// Extent located by the indirect-reference `/Length` helper.
    IndirectLength(IndirectLengthContentStreamDataExtentInspection),
}

impl ContentStreamDataExtentInspection {
    /// Resolved stream-data length in bytes.
    #[must_use]
    pub const fn length(&self) -> usize {
        match self {
            Self::DirectLength(report) => report.length,
            Self::IndirectLength(report) => report.length,
        }
    }

    /// Byte offset where stream data begins.
    #[must_use]
    pub const fn stream_data_start_byte_offset(&self) -> usize {
        match self {
            Self::DirectLength(report) => report.stream_data_start_byte_offset,
            Self::IndirectLength(report) => report.stream_data_start_byte_offset,
        }
    }

    /// Exclusive byte offset immediately after the stream data.
    #[must_use]
    pub const fn stream_data_end_byte_offset(&self) -> usize {
        match self {
            Self::DirectLength(report) => report.stream_data_end_byte_offset,
            Self::IndirectLength(report) => report.stream_data_end_byte_offset,
        }
    }
}

/// Error returned when a direct-or-indirect content-stream data extent cannot
/// be located.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentStreamDataExtentInspectionError {
    /// Caller-supplied object byte offset where inspection began.
    pub byte_offset: usize,
    /// Total source length.
    pub byte_len: usize,
    /// Byte offset where the malformed or unsupported construct was found,
    /// when available.
    pub error_byte_offset: Option<usize>,
    /// Structured failure reason.
    pub reason: ContentStreamDataExtentInspectionRejection,
}

/// Structured direct-or-indirect content-stream extent rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ContentStreamDataExtentInspectionRejection {
    /// A delegated content-stream start inspection failed during dispatch.
    StreamStart {
        /// Underlying content-stream start rejection reason.
        stream_start_reason: ContentStreamStartInspectionRejection,
    },
    /// The stream dictionary has no exact top-level raw `/Length` key.
    MissingLength,
    /// The stream dictionary has more than one exact top-level raw `/Length`
    /// key.
    DuplicateLength {
        /// First `/Length` key range observed in source order.
        first_key_range: DictionaryEntryByteRange,
        /// Duplicate `/Length` key range observed in source order.
        duplicate_key_range: DictionaryEntryByteRange,
    },
    /// The `/Length` value is an indirect reference but no classic xref table
    /// was supplied.
    IndirectLengthRequiresXrefTable,
    /// The `/Length` value kind is neither number-like nor indirect-reference
    /// like.
    UnsupportedLengthValueKind {
        /// Shallow value kind reported by dictionary entry inspection.
        value_kind: DictionaryValueKind,
    },
    /// Delegated direct-length extent inspection failed.
    DirectLength {
        /// Underlying direct-length rejection reason.
        direct_length_reason: DirectLengthContentStreamDataExtentInspectionRejection,
    },
    /// Delegated indirect-length extent inspection failed.
    IndirectLength {
        /// Underlying indirect-length rejection reason.
        indirect_length_reason: IndirectLengthContentStreamDataExtentInspectionRejection,
    },
}

/// Locate the stream-data byte range for a dictionary-bodied content stream.
///
/// The top-level `/Length` may be either a direct non-negative integer or an
/// indirect reference resolved through a caller-supplied classic xref table.
///
/// The helper first inspects the stream start to classify exactly one exact raw
/// top-level `/Length` entry by [`DictionaryValueKind`]. `NumberLike` dispatches
/// to [`inspect_direct_length_content_stream_data_extent`].
/// `IndirectReferenceLike` dispatches to
/// [`inspect_indirect_length_content_stream_data_extent`] when an xref table is
/// supplied, or rejects as [`IndirectLengthRequiresXrefTable`](ContentStreamDataExtentInspectionRejection::IndirectLengthRequiresXrefTable)
/// without attempting resolution. Other value kinds reject as
/// [`UnsupportedLengthValueKind`](ContentStreamDataExtentInspectionRejection::UnsupportedLengthValueKind).
///
/// It does not parse `/Length`, resolve indirect objects, validate
/// `endstream`, read/copy/decode/decompress stream bytes, inspect filters,
/// tokenize content streams, or validate page/content semantics itself; those
/// operations remain delegated to the focused helpers selected by dispatch.
///
/// # Errors
///
/// Returns [`ContentStreamDataExtentInspectionError`] for dispatch-time
/// stream-start failures, missing or duplicate `/Length`, an indirect `/Length`
/// without a supplied classic xref table, unsupported `/Length` value kinds,
/// or delegated direct/indirect focused-helper failures with the underlying
/// rejection reason preserved.
pub fn inspect_content_stream_data_extent(
    input: &[u8],
    xref_table: Option<&ClassicXrefTableInspection>,
    object_offset: usize,
) -> Result<ContentStreamDataExtentInspection, ContentStreamDataExtentInspectionError> {
    let stream_start = inspect_content_stream_start(input, object_offset).map_err(|error| {
        content_stream_data_extent_error(
            input,
            object_offset,
            error.error_byte_offset,
            ContentStreamDataExtentInspectionRejection::StreamStart {
                stream_start_reason: error.reason,
            },
        )
    })?;

    let length_entry = find_length_entry(input, &stream_start).map_err(|issue| {
        content_stream_data_extent_entry_error(input, object_offset, &stream_start, issue)
    })?;

    match length_entry.value_kind {
        DictionaryValueKind::NumberLike => {
            inspect_direct_length_content_stream_data_extent(input, object_offset)
                .map(ContentStreamDataExtentInspection::DirectLength)
                .map_err(|error| {
                    content_stream_data_extent_error(
                        input,
                        object_offset,
                        error.error_byte_offset,
                        ContentStreamDataExtentInspectionRejection::DirectLength {
                            direct_length_reason: error.reason,
                        },
                    )
                })
        }
        DictionaryValueKind::IndirectReferenceLike => {
            let Some(xref_table) = xref_table else {
                return Err(content_stream_data_extent_error(
                    input,
                    object_offset,
                    Some(length_entry.value_range.start),
                    ContentStreamDataExtentInspectionRejection::IndirectLengthRequiresXrefTable,
                ));
            };

            inspect_indirect_length_content_stream_data_extent(input, xref_table, object_offset)
                .map(ContentStreamDataExtentInspection::IndirectLength)
                .map_err(|error| {
                    content_stream_data_extent_error(
                        input,
                        object_offset,
                        error.error_byte_offset,
                        ContentStreamDataExtentInspectionRejection::IndirectLength {
                            indirect_length_reason: error.reason,
                        },
                    )
                })
        }
        value_kind => Err(content_stream_data_extent_error(
            input,
            object_offset,
            Some(length_entry.value_range.start),
            ContentStreamDataExtentInspectionRejection::UnsupportedLengthValueKind { value_kind },
        )),
    }
}

const fn content_stream_data_extent_entry_error(
    input: &[u8],
    byte_offset: usize,
    stream_start: &crate::ContentStreamStartInspection,
    issue: LengthEntryIssue,
) -> ContentStreamDataExtentInspectionError {
    match issue {
        LengthEntryIssue::Missing => content_stream_data_extent_error(
            input,
            byte_offset,
            Some(stream_start.dictionary.dictionary_close_byte_offset),
            ContentStreamDataExtentInspectionRejection::MissingLength,
        ),
        LengthEntryIssue::Duplicate(first_key_range, duplicate_key_range) => {
            content_stream_data_extent_error(
                input,
                byte_offset,
                Some(duplicate_key_range.start),
                ContentStreamDataExtentInspectionRejection::DuplicateLength {
                    first_key_range,
                    duplicate_key_range,
                },
            )
        }
    }
}

const fn content_stream_data_extent_error(
    input: &[u8],
    byte_offset: usize,
    error_byte_offset: Option<usize>,
    reason: ContentStreamDataExtentInspectionRejection,
) -> ContentStreamDataExtentInspectionError {
    ContentStreamDataExtentInspectionError {
        byte_offset,
        byte_len: input.len(),
        error_byte_offset,
        reason,
    }
}
