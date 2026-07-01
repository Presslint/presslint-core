use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::{
    ClassicXrefTableInspection, IndirectObjectHeaderInspectionRejection, IndirectRef, ObjectLookup,
    ObjectLookupLocation, ObjectStreamMemberExtractionRejection, extract_object_stream_member,
    inspect_indirect_object_header, locate_xref_object,
};

/// In-use object location resolved from a cross-reference backend.
///
/// This is the backend-neutral success currency of object resolution. A classic
/// xref table produces it today through [`resolve_classic_xref_object_offset`];
/// a future cross-reference-stream backend can produce the same report without
/// changing consumers.
///
/// This report stores only structural metadata. It does not retain or copy PDF
/// bytes, object bodies, stream bodies, dictionaries, or referenced-object bytes,
/// and it does not read the resolved object body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedObject {
    /// Requested indirect reference, proven to match both the cross-reference
    /// entry generation and the indirect object header at the resolved offset.
    pub reference: IndirectRef,
    /// Resolved in-use object byte offset.
    pub object_byte_offset: usize,
    /// Generation number reported by the matching in-use cross-reference entry.
    pub xref_generation: u16,
}

/// Error returned when an indirect reference cannot be resolved to an in-use
/// object byte offset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectResolutionError {
    /// Requested indirect reference.
    pub reference: IndirectRef,
    /// Total source length.
    pub byte_len: usize,
    /// Resolved in-use object byte offset, when cross-reference resolution
    /// reached one before a later check failed.
    pub object_byte_offset: Option<usize>,
    /// Byte offset where delegated object-header inspection found a malformed
    /// construct, when available.
    pub error_byte_offset: Option<usize>,
    /// Structured failure reason.
    pub reason: ObjectResolutionRejection,
}

/// Structured object-resolution rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ObjectResolutionRejection {
    /// The cross-reference result was not an uncompressed in-use object entry.
    UnresolvedXrefLocation {
        /// Locate-only cross-reference result for the requested object number.
        location: ObjectLookupLocation,
    },
    /// The cross-reference stream reports the object as compressed inside an
    /// object stream. This resolver does not extract object streams.
    UnsupportedCompressedXrefStreamEntry {
        /// Requested object number.
        object_number: usize,
        /// Object number of the containing object stream.
        object_stream_number: usize,
        /// Index of this object inside the object stream.
        index_within_object_stream: usize,
    },
    /// The cross-reference stream reports a reserved or future entry type.
    UnsupportedReservedXrefStreamEntry {
        /// Requested object number.
        object_number: usize,
        /// Raw type field value.
        entry_type: u64,
        /// Raw second field value.
        field2: u64,
        /// Raw third field value.
        field3: u64,
    },
    /// The in-use cross-reference entry generation did not match the requested
    /// reference generation.
    GenerationMismatch {
        /// Generation number from the requested indirect reference.
        requested_generation: u16,
        /// Generation number from the matching in-use cross-reference entry.
        xref_generation: u16,
    },
    /// The indirect object header at the resolved byte offset could not be
    /// parsed.
    ObjectHeader {
        /// Underlying object-header rejection reason.
        header_reason: IndirectObjectHeaderInspectionRejection,
    },
    /// The indirect object header parsed but its object/generation did not match
    /// the requested reference.
    ObjectHeaderReferenceMismatch {
        /// Indirect reference parsed from the object header at the resolved
        /// offset.
        header_reference: IndirectRef,
    },
    /// The requested compressed object's generation was not `0`. Objects stored
    /// in object streams always have generation `0` (PDF 32000 §7.5.7).
    CompressedObjectGenerationNotZero {
        /// Requested object number.
        object_number: usize,
        /// Object number of the containing object stream.
        object_stream_number: usize,
        /// Index of this object inside the object stream.
        index_within_object_stream: usize,
        /// Requested (non-zero) generation.
        requested_generation: u16,
    },
    /// The containing object stream is itself reported as a type-2 compressed
    /// entry; an object stream cannot be compressed inside another object stream.
    ObjectStreamIsCompressed {
        /// Requested object number.
        object_number: usize,
        /// Object number of the containing object stream.
        object_stream_number: usize,
        /// Index of this object inside the object stream.
        index_within_object_stream: usize,
    },
    /// The containing object stream object could not be resolved to an in-use
    /// uncompressed byte offset for a reason other than being compressed (see
    /// [`Self::ObjectStreamIsCompressed`]). The outer error's byte offsets carry
    /// the delegated anchor when one was available.
    ObjectStreamObjectUnresolved {
        /// Requested object number.
        object_number: usize,
        /// Object number of the containing object stream.
        object_stream_number: usize,
        /// Index of this object inside the object stream.
        index_within_object_stream: usize,
    },
    /// The containing object stream resolved but its `/ObjStm` body could not be
    /// validated or the requested compressed member could not be extracted. The
    /// object stream object byte offset is carried by
    /// [`ObjectResolutionError::object_byte_offset`].
    ObjectStreamMemberExtraction {
        /// Delegated `/ObjStm` member extraction rejection reason.
        extraction_reason: ObjectStreamMemberExtractionRejection,
    },
}

/// Resolve an indirect reference to an in-use object byte offset through a
/// parsed classic xref table.
///
/// The resolution accepts the reference only when every check holds:
///
/// - [`resolve_classic_xref_object`] reports exactly one in-use entry for the
///   object number (free, not-found, and ambiguous results are rejected);
/// - the in-use entry generation matches the requested reference generation;
/// - the indirect object header at the resolved byte offset parses and its
///   object number and generation match the requested reference.
///
/// The generation is therefore validated twice: once against the cross-reference
/// entry and once against the object header at the resolved offset.
///
/// It performs no `/Prev` traversal, incremental-section merging, object-stream
/// extraction, object-body reading, caching, or object-map construction; it only
/// scans the already-parsed xref table and reads the short header at the resolved
/// offset.
///
/// # Errors
///
/// Returns [`ObjectResolutionError`] when the cross-reference result is not a
/// single in-use entry, the entry generation does not match, the object header
/// fails to parse, or the parsed header reference does not match the requested
/// reference.
pub fn resolve_classic_xref_object_offset(
    input: &[u8],
    xref: &ClassicXrefTableInspection,
    reference: IndirectRef,
) -> Result<ResolvedObject, ObjectResolutionError> {
    resolve_xref_object_offset(input, ObjectLookup::ClassicXref(xref), reference)
}

/// Resolve an indirect reference to an in-use object byte offset through a
/// borrowed xref backend.
///
/// Classic and cross-reference-stream type-1 entries share the same success
/// currency and the same object-header validation. Cross-reference-stream
/// type-2 compressed entries and reserved/future entry types are reported as
/// structured unsupported paths and are never treated as not found or fabricated
/// into byte offsets.
///
/// # Errors
///
/// Returns [`ObjectResolutionError`] when lookup does not produce an
/// uncompressed in-use object entry, the xref generation does not match, the
/// object header fails to parse, or the parsed header reference does not match
/// the requested reference.
pub fn resolve_xref_object_offset(
    input: &[u8],
    lookup: ObjectLookup<'_>,
    reference: IndirectRef,
) -> Result<ResolvedObject, ObjectResolutionError> {
    let (xref_generation, object_byte_offset) = resolve_lookup_location(input, lookup, reference)?;

    if xref_generation != reference.generation {
        return Err(object_resolution_error(
            input,
            reference,
            Some(object_byte_offset),
            None,
            ObjectResolutionRejection::GenerationMismatch {
                requested_generation: reference.generation,
                xref_generation,
            },
        ));
    }

    let header = inspect_indirect_object_header(input, object_byte_offset).map_err(|error| {
        object_resolution_error(
            input,
            reference,
            Some(object_byte_offset),
            error.error_byte_offset,
            ObjectResolutionRejection::ObjectHeader {
                header_reason: error.reason,
            },
        )
    })?;

    if header.reference != reference {
        return Err(object_resolution_error(
            input,
            reference,
            Some(object_byte_offset),
            Some(header.header_range.start),
            ObjectResolutionRejection::ObjectHeaderReferenceMismatch {
                header_reference: header.reference,
            },
        ));
    }

    Ok(ResolvedObject {
        reference,
        object_byte_offset,
        xref_generation,
    })
}

/// Body-aware object resolution currency.
///
/// This is the opt-in superset of [`resolve_xref_object_offset`]: an
/// uncompressed object is reported by its zero-copy byte offset, while a
/// cross-reference-stream type-2 compressed object is extracted into a bounded
/// decoded object-stream buffer plus the byte span of its member body inside
/// that buffer. The [`Compressed`](Self::Compressed) variant owns the decoded
/// buffer because compressed member bodies live only in decoded stream bytes,
/// not in the original source; no source PDF bytes are retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedObjectData {
    /// The object is an ordinary uncompressed indirect object located by byte
    /// offset.
    Uncompressed {
        /// Offset-only resolution result.
        resolved: ResolvedObject,
    },
    /// The object is a type-2 compressed member of an object stream.
    Compressed {
        /// Requested indirect reference (generation `0`).
        reference: IndirectRef,
        /// Object number of the containing object stream.
        object_stream_number: usize,
        /// Index of this object inside the object stream.
        index_within_object_stream: usize,
        /// Bounded decoded object-stream body buffer owning the member bytes.
        decoded_object_stream: Vec<u8>,
        /// Byte span of the member body within `decoded_object_stream`.
        object_body_span: Range<usize>,
    },
}

/// Resolve an indirect reference to body-aware object data through a borrowed
/// xref backend.
///
/// Uncompressed and classic in-use objects delegate to
/// [`resolve_xref_object_offset`] and are returned as
/// [`ResolvedObjectData::Uncompressed`], so this path stays byte-for-byte
/// compatible with the offset-only resolver. A cross-reference-stream type-2
/// compressed entry is resolved into [`ResolvedObjectData::Compressed`]:
///
/// - the requested compressed-object generation must be `0`;
/// - the containing object stream object is resolved through
///   [`resolve_xref_object_offset`]; if it is itself compressed the failure is
///   reported as
///   [`ObjectStreamIsCompressed`](ObjectResolutionRejection::ObjectStreamIsCompressed);
/// - the member is validated and extracted with
///   [`extract_object_stream_member`], decoding at most
///   `max_decoded_object_stream_bytes` of the `/ObjStm` body.
///
/// This slice does not follow `/Extends`, cache object streams, or thread
/// compressed data through document navigation.
///
/// # Errors
///
/// Returns [`ObjectResolutionError`] for every offset-only failure of
/// [`resolve_xref_object_offset`] plus the compressed-object failures: a
/// non-zero compressed-object generation, a compressed or otherwise unresolved
/// containing object stream, or a delegated `/ObjStm` member extraction failure.
pub fn resolve_object(
    input: &[u8],
    lookup: ObjectLookup<'_>,
    reference: IndirectRef,
    max_decoded_object_stream_bytes: usize,
) -> Result<ResolvedObjectData, ObjectResolutionError> {
    let object_number = usize::try_from(reference.object_number).unwrap_or(usize::MAX);
    match locate_xref_object(lookup, object_number) {
        ObjectLookupLocation::XrefStreamCompressed {
            object_number,
            object_stream_number,
            index_within_object_stream,
        } => resolve_compressed_object(
            input,
            lookup,
            reference,
            object_number,
            object_stream_number,
            index_within_object_stream,
            max_decoded_object_stream_bytes,
        ),
        _ => resolve_xref_object_offset(input, lookup, reference)
            .map(|resolved| ResolvedObjectData::Uncompressed { resolved }),
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_compressed_object(
    input: &[u8],
    lookup: ObjectLookup<'_>,
    reference: IndirectRef,
    object_number: usize,
    object_stream_number: usize,
    index_within_object_stream: usize,
    max_decoded_object_stream_bytes: usize,
) -> Result<ResolvedObjectData, ObjectResolutionError> {
    if reference.generation != 0 {
        return Err(object_resolution_error(
            input,
            reference,
            None,
            None,
            ObjectResolutionRejection::CompressedObjectGenerationNotZero {
                object_number,
                object_stream_number,
                index_within_object_stream,
                requested_generation: reference.generation,
            },
        ));
    }

    let Ok(object_stream_object_number) = u32::try_from(object_stream_number) else {
        return Err(object_resolution_error(
            input,
            reference,
            None,
            None,
            ObjectResolutionRejection::ObjectStreamObjectUnresolved {
                object_number,
                object_stream_number,
                index_within_object_stream,
            },
        ));
    };

    let object_stream_reference = IndirectRef {
        object_number: object_stream_object_number,
        generation: 0,
    };
    let object_stream = resolve_xref_object_offset(input, lookup, object_stream_reference)
        .map_err(|error| {
            let reason = match error.reason {
                ObjectResolutionRejection::UnsupportedCompressedXrefStreamEntry { .. } => {
                    ObjectResolutionRejection::ObjectStreamIsCompressed {
                        object_number,
                        object_stream_number,
                        index_within_object_stream,
                    }
                }
                _ => ObjectResolutionRejection::ObjectStreamObjectUnresolved {
                    object_number,
                    object_stream_number,
                    index_within_object_stream,
                },
            };
            object_resolution_error(
                input,
                reference,
                error.object_byte_offset,
                error.error_byte_offset,
                reason,
            )
        })?;

    let extracted = extract_object_stream_member(
        input,
        object_stream.object_byte_offset,
        reference.object_number,
        index_within_object_stream,
        max_decoded_object_stream_bytes,
    )
    .map_err(|error| {
        object_resolution_error(
            input,
            reference,
            Some(object_stream.object_byte_offset),
            error.error_byte_offset,
            ObjectResolutionRejection::ObjectStreamMemberExtraction {
                extraction_reason: error.reason,
            },
        )
    })?;

    Ok(ResolvedObjectData::Compressed {
        reference,
        object_stream_number,
        index_within_object_stream,
        decoded_object_stream: extracted.decoded_object_stream,
        object_body_span: extracted.object_body_span,
    })
}

fn resolve_lookup_location(
    input: &[u8],
    lookup: ObjectLookup<'_>,
    reference: IndirectRef,
) -> Result<(u16, usize), ObjectResolutionError> {
    let location = locate_xref_object(
        lookup,
        usize::try_from(reference.object_number).map_or(usize::MAX, |value| value),
    );
    match location {
        ObjectLookupLocation::ClassicInUse {
            generation,
            byte_offset,
            ..
        }
        | ObjectLookupLocation::XrefStreamUncompressed {
            generation,
            byte_offset,
            ..
        } => Ok((generation, byte_offset)),
        ObjectLookupLocation::XrefStreamCompressed {
            object_number,
            object_stream_number,
            index_within_object_stream,
        } => Err(object_resolution_error(
            input,
            reference,
            None,
            None,
            ObjectResolutionRejection::UnsupportedCompressedXrefStreamEntry {
                object_number,
                object_stream_number,
                index_within_object_stream,
            },
        )),
        ObjectLookupLocation::XrefStreamReserved {
            object_number,
            entry_type,
            field2,
            field3,
        } => Err(object_resolution_error(
            input,
            reference,
            None,
            None,
            ObjectResolutionRejection::UnsupportedReservedXrefStreamEntry {
                object_number,
                entry_type,
                field2,
                field3,
            },
        )),
        _ => Err(object_resolution_error(
            input,
            reference,
            None,
            None,
            ObjectResolutionRejection::UnresolvedXrefLocation { location },
        )),
    }
}

const fn object_resolution_error(
    input: &[u8],
    reference: IndirectRef,
    object_byte_offset: Option<usize>,
    error_byte_offset: Option<usize>,
    reason: ObjectResolutionRejection,
) -> ObjectResolutionError {
    ObjectResolutionError {
        reference,
        byte_len: input.len(),
        object_byte_offset,
        error_byte_offset,
        reason,
    }
}
