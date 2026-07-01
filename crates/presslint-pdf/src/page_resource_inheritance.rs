use crate::{
    DictionaryEntryByteRange, DictionaryEntrySpan, DictionaryValueKind,
    IndirectObjectDictionaryInspection, IndirectRef, ObjectLookup, ObjectLookupLocation,
    locate_xref_object, parse_indirect_reference,
};

use crate::SkippedPageXObjectResourceReason;

/// Effective resource dictionary entries while walking the page tree root-down.
///
/// The entries are copied as small byte-range records only. No dictionary bytes,
/// object bodies, streams, decoded data, or source slices are retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InheritedPageResources {
    pub entries: Vec<DictionaryEntrySpan>,
}

impl InheritedPageResources {
    #[must_use]
    pub const fn new(entries: Vec<DictionaryEntrySpan>) -> Self {
        Self { entries }
    }
}

/// Apply PDF page-resource inheritance for this slice.
///
/// A present child `/Resources` dictionary replaces the inherited dictionary.
/// An absent child `/Resources` keeps the inherited dictionary.
#[must_use]
pub fn inherit_or_replace_page_resources(
    inherited: Option<&InheritedPageResources>,
    replacement: Option<Vec<DictionaryEntrySpan>>,
) -> Option<InheritedPageResources> {
    replacement.map_or_else(
        || inherited.cloned(),
        |entries| Some(InheritedPageResources::new(entries)),
    )
}

#[derive(Clone)]
pub struct ResourceContext {
    pub resources: Option<InheritedPageResources>,
    pub skips: Vec<SkippedPageXObjectResourceReason>,
}

impl ResourceContext {
    pub fn from_dictionary(
        input: &[u8],
        lookup: ObjectLookup<'_>,
        dictionary: &IndirectObjectDictionaryInspection,
        inherited: Option<&Self>,
    ) -> Self {
        let inherited_resources = inherited.and_then(|context| context.resources.as_ref());
        let mut skips = inherited.map_or_else(Vec::new, |context| context.skips.clone());
        match resource_replacement(input, lookup, &dictionary.entries) {
            ResourceReplacement::Absent => Self {
                resources: inherit_or_replace_page_resources(inherited_resources, None),
                skips,
            },
            ResourceReplacement::Present(entries) => Self {
                resources: inherit_or_replace_page_resources(inherited_resources, Some(entries)),
                skips,
            },
            ResourceReplacement::Skipped(reason) => {
                skips.push(reason);
                Self {
                    resources: None,
                    skips,
                }
            }
        }
    }
}

enum ResourceReplacement {
    Absent,
    Present(Vec<DictionaryEntrySpan>),
    Skipped(SkippedPageXObjectResourceReason),
}

fn resource_replacement(
    input: &[u8],
    lookup: ObjectLookup<'_>,
    entries: &[DictionaryEntrySpan],
) -> ResourceReplacement {
    let Some(entry) = (match unique_entry(input, entries, b"/Resources") {
        Ok(entry) => entry,
        Err((first_key_range, duplicate_key_range)) => {
            return ResourceReplacement::Skipped(
                SkippedPageXObjectResourceReason::DuplicateResources {
                    first_key_range,
                    duplicate_key_range,
                },
            );
        }
    }) else {
        return ResourceReplacement::Absent;
    };

    match entry.value_kind {
        DictionaryValueKind::Dictionary => {
            match crate::inspect_dictionary_entries(input, entry.value_range.start) {
                Ok(resources) => ResourceReplacement::Present(resources.entries),
                Err(error) => ResourceReplacement::Skipped(
                    SkippedPageXObjectResourceReason::DirectResourcesDictionaryFailed { error },
                ),
            }
        }
        DictionaryValueKind::IndirectReferenceLike => {
            resource_replacement_from_reference(input, lookup, entry)
        }
        value_kind => ResourceReplacement::Skipped(
            SkippedPageXObjectResourceReason::UnsupportedResourcesValue { value_kind },
        ),
    }
}

fn resource_replacement_from_reference(
    input: &[u8],
    lookup: ObjectLookup<'_>,
    entry: DictionaryEntrySpan,
) -> ResourceReplacement {
    let reference = match parse_indirect_reference(input, entry.value_range.start) {
        Ok(reference) => reference.reference,
        Err(error) => {
            return ResourceReplacement::Skipped(
                SkippedPageXObjectResourceReason::MalformedResourcesReference {
                    reference_reason: error.reason,
                },
            );
        }
    };
    let (_, object_byte_offset) = match resolve_reference(lookup, reference) {
        Ok(resolved) => resolved,
        Err(ResolveReferenceError::Unresolved {
            reference,
            location,
        }) => {
            return ResourceReplacement::Skipped(
                SkippedPageXObjectResourceReason::UnresolvedResourcesReference {
                    reference,
                    location,
                },
            );
        }
        Err(ResolveReferenceError::GenerationMismatch {
            reference,
            xref_generation,
        }) => {
            return ResourceReplacement::Skipped(
                SkippedPageXObjectResourceReason::ResourcesGenerationMismatch {
                    reference,
                    xref_generation,
                },
            );
        }
    };

    match crate::inspect_indirect_object_dictionary(input, object_byte_offset) {
        Ok(resources) => ResourceReplacement::Present(resources.entries),
        Err(error) => ResourceReplacement::Skipped(
            SkippedPageXObjectResourceReason::IndirectResourcesDictionaryFailed {
                reference,
                object_byte_offset,
                error,
            },
        ),
    }
}

pub enum ResolveReferenceError {
    Unresolved {
        reference: IndirectRef,
        location: ObjectLookupLocation,
    },
    GenerationMismatch {
        reference: IndirectRef,
        xref_generation: u16,
    },
}

pub fn resolve_reference(
    lookup: ObjectLookup<'_>,
    reference: IndirectRef,
) -> Result<(u16, usize), ResolveReferenceError> {
    let location = locate_xref_object(
        lookup,
        usize::try_from(reference.object_number).map_or(usize::MAX, |value| value),
    );
    let Some((xref_generation, object_byte_offset)) = in_use_offset(location) else {
        return Err(ResolveReferenceError::Unresolved {
            reference,
            location,
        });
    };
    if xref_generation != reference.generation {
        return Err(ResolveReferenceError::GenerationMismatch {
            reference,
            xref_generation,
        });
    }
    Ok((xref_generation, object_byte_offset))
}

const fn in_use_offset(location: ObjectLookupLocation) -> Option<(u16, usize)> {
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
        } => Some((generation, byte_offset)),
        _ => None,
    }
}

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
