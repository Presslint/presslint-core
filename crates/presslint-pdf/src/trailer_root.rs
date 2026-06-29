use serde::{Deserialize, Serialize};

use crate::{
    ClassicXrefTrailerDictionaryInspection, ClassicXrefTrailerDictionaryInspectionRejection,
    DictionaryEntryByteRange, DictionaryEntryInspectionRejection, DictionaryValueKind, IndirectRef,
    IndirectReferenceInspectionRejection,
};

const ROOT_KEY: &[u8] = b"/Root";

/// Parsed `/Root` indirect reference from a classic xref trailer dictionary.
///
/// This report stores only structural metadata. It does not retain or copy
/// trailer dictionary bytes, object bodies, stream bodies, catalog dictionaries,
/// page trees, or referenced-object bytes, and it does not resolve the parsed
/// reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassicXrefTrailerRootInspection {
    /// Balanced trailer dictionary inspection that supplied the scanned range.
    pub trailer_dictionary: ClassicXrefTrailerDictionaryInspection,
    /// Byte range covering the exact top-level raw `/Root` key.
    pub root_key_range: DictionaryEntryByteRange,
    /// Byte range covering the `/Root` value span.
    pub root_value_range: DictionaryEntryByteRange,
    /// Parsed document catalog indirect reference.
    pub root_reference: IndirectRef,
}

/// Error returned when a classic xref trailer `/Root` reference cannot be
/// inspected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassicXrefTrailerRootInspectionError {
    /// Caller-supplied byte offset where trailer-root inspection began.
    pub byte_offset: usize,
    /// Total source length.
    pub byte_len: usize,
    /// Byte offset where the malformed or unsupported construct was found, when
    /// available.
    pub error_byte_offset: Option<usize>,
    /// Structured failure reason.
    pub reason: ClassicXrefTrailerRootInspectionRejection,
}

/// Structured classic xref trailer `/Root` inspection rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ClassicXrefTrailerRootInspectionRejection {
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
    /// The trailer dictionary has no exact top-level raw `/Root` key.
    MissingRoot,
    /// The trailer dictionary has more than one exact top-level raw `/Root` key.
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
    /// The `/Root` value was shaped as an indirect reference but did not parse.
    MalformedRootReference {
        /// Underlying indirect reference rejection reason.
        reference_reason: IndirectReferenceInspectionRejection,
    },
}

/// Inspect the top-level `/Root` indirect reference from a classic xref trailer.
///
/// The helper composes existing bounded inspectors: it locates the trailer
/// dictionary with [`crate::inspect_classic_xref_trailer_dictionary`], scans
/// top-level entries with [`crate::inspect_dictionary_entries`], matches only
/// the exact raw key bytes `/Root`, and parses that value through
/// [`crate::parse_indirect_reference`].
///
/// It does not decode PDF name escapes, interpret nested dictionaries, resolve
/// the parsed reference, or inspect catalog/page-tree/object bodies.
///
/// # Errors
///
/// Returns [`ClassicXrefTrailerRootInspectionError`] for delegated trailer or
/// dictionary entry inspection failures, missing or duplicate exact `/Root`
/// keys, non-reference `/Root` values, or malformed `/Root` references.
pub fn inspect_classic_xref_trailer_root(
    input: &[u8],
    trailer_keyword_offset: usize,
) -> Result<ClassicXrefTrailerRootInspection, ClassicXrefTrailerRootInspectionError> {
    let trailer_dictionary =
        crate::inspect_classic_xref_trailer_dictionary(input, trailer_keyword_offset).map_err(
            |error| {
                trailer_root_error(
                    input,
                    trailer_keyword_offset,
                    ClassicXrefTrailerRootInspectionRejection::TrailerDictionary {
                        trailer_reason: error.reason,
                    },
                    error.error_byte_offset,
                )
            },
        )?;

    let entries =
        crate::inspect_dictionary_entries(input, trailer_dictionary.dictionary_open_byte_offset)
            .map_err(|error| {
                trailer_root_error(
                    input,
                    trailer_keyword_offset,
                    ClassicXrefTrailerRootInspectionRejection::DictionaryEntries {
                        dictionary_entries_reason: error.reason,
                    },
                    error.error_byte_offset,
                )
            })?;

    let mut root_entry: Option<crate::DictionaryEntrySpan> = None;
    for entry in entries.entries {
        if !is_exact_root_key(input, entry.key_range) {
            continue;
        }

        if let Some(first) = root_entry {
            return Err(trailer_root_error(
                input,
                trailer_keyword_offset,
                ClassicXrefTrailerRootInspectionRejection::DuplicateRoot {
                    first_key_range: first.key_range,
                    duplicate_key_range: entry.key_range,
                },
                Some(entry.key_range.start),
            ));
        }

        root_entry = Some(entry);
    }

    let root_entry = root_entry.ok_or_else(|| {
        trailer_root_error(
            input,
            trailer_keyword_offset,
            ClassicXrefTrailerRootInspectionRejection::MissingRoot,
            Some(trailer_dictionary.dictionary_close_byte_offset),
        )
    })?;

    if !matches!(
        root_entry.value_kind,
        DictionaryValueKind::IndirectReferenceLike | DictionaryValueKind::OtherScalar
    ) {
        return Err(trailer_root_error(
            input,
            trailer_keyword_offset,
            ClassicXrefTrailerRootInspectionRejection::NonReferenceRootValue {
                value_kind: root_entry.value_kind,
            },
            Some(root_entry.value_range.start),
        ));
    }

    let root_reference = crate::parse_indirect_reference(input, root_entry.value_range.start)
        .map_err(|error| {
            trailer_root_error(
                input,
                trailer_keyword_offset,
                ClassicXrefTrailerRootInspectionRejection::MalformedRootReference {
                    reference_reason: error.reason,
                },
                error.error_byte_offset,
            )
        })?;

    if root_reference.after_keyword_offset != root_entry.value_range.end {
        return Err(trailer_root_error(
            input,
            trailer_keyword_offset,
            ClassicXrefTrailerRootInspectionRejection::MalformedRootReference {
                reference_reason: IndirectReferenceInspectionRejection::MalformedReference,
            },
            Some(root_reference.after_keyword_offset),
        ));
    }

    Ok(ClassicXrefTrailerRootInspection {
        trailer_dictionary,
        root_key_range: root_entry.key_range,
        root_value_range: root_entry.value_range,
        root_reference: root_reference.reference,
    })
}

fn is_exact_root_key(input: &[u8], key_range: DictionaryEntryByteRange) -> bool {
    input.get(key_range.start..key_range.end) == Some(ROOT_KEY)
}

const fn trailer_root_error(
    input: &[u8],
    byte_offset: usize,
    reason: ClassicXrefTrailerRootInspectionRejection,
    error_byte_offset: Option<usize>,
) -> ClassicXrefTrailerRootInspectionError {
    ClassicXrefTrailerRootInspectionError {
        byte_offset,
        byte_len: input.len(),
        error_byte_offset,
        reason,
    }
}
