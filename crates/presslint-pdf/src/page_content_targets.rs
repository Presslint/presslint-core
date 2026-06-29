use serde::{Deserialize, Serialize};

use crate::{
    ClassicXrefObjectLocation, ClassicXrefTableInspection, PageContentReference,
    PageContentsInspection,
};

/// Locate-only resolution report for a page object's direct `/Contents`
/// references.
///
/// This report stores only the caller-visible source length and one
/// source-ordered result per direct content reference reported by
/// [`crate::inspect_page_contents`]. It does not retain or copy PDF bytes,
/// object bodies, stream bodies, decoded streams, or source slices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageContentTargetsInspection {
    /// Total source length supplied by the caller.
    pub byte_len: usize,
    /// Source-ordered target resolution entries.
    pub entries: Vec<PageContentTargetInspection>,
}

/// Locate-only resolution result for one direct page-content reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PageContentTargetInspection {
    /// The content reference resolved to exactly one matching in-use xref entry.
    Resolved {
        /// Original direct `/Contents` reference reported by page inspection.
        content_reference: PageContentReference,
        /// In-use object byte offset from the matching classic xref entry.
        object_byte_offset: usize,
        /// Generation number reported by the matching xref entry.
        xref_generation: u16,
    },
    /// The content reference was intentionally skipped.
    Skipped {
        /// Original direct `/Contents` reference reported by page inspection.
        content_reference: PageContentReference,
        /// Structured skip reason.
        reason: SkippedPageContentTargetReason,
    },
}

/// Structured reason why one direct page-content reference was not resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum SkippedPageContentTargetReason {
    /// The xref result was free, missing, or ambiguous rather than exactly one
    /// in-use entry.
    UnresolvedXrefLocation {
        /// Locate-only xref result for the requested object number.
        location: ClassicXrefObjectLocation,
    },
    /// The xref entry generation did not match the requested content reference
    /// generation.
    GenerationMismatch {
        /// Generation number from the requested indirect reference.
        requested_generation: u16,
        /// Generation number from the matching in-use xref entry.
        xref_generation: u16,
        /// In-use object byte offset from the generation-mismatched xref entry.
        object_byte_offset: usize,
    },
}

/// Resolve direct page `/Contents` references through an existing classic xref
/// inspection.
///
/// The helper performs a deterministic locate-only pass over
/// [`PageContentsInspection::contents`]. Each content object number is resolved
/// through [`crate::resolve_classic_xref_object`], and only a single in-use xref
/// entry whose generation matches the requested reference is reported as
/// resolved. Free, missing, ambiguous, and generation-mismatched entries become
/// source-ordered structured skips so later references are still processed.
///
/// It does not inspect content stream dictionaries, locate `stream` or
/// `endstream`, decode streams, concatenate stream bytes, tokenize content
/// bytes, mutate PDF bytes, follow `/Prev`, or build a cache/index around the
/// xref table.
#[must_use]
pub fn inspect_page_content_targets(
    input: &[u8],
    xref: &ClassicXrefTableInspection,
    page_contents: &PageContentsInspection,
) -> PageContentTargetsInspection {
    let entries = page_contents
        .contents
        .iter()
        .copied()
        .map(|content_reference| resolve_content_reference(xref, content_reference))
        .collect();

    PageContentTargetsInspection {
        byte_len: input.len(),
        entries,
    }
}

fn resolve_content_reference(
    xref: &ClassicXrefTableInspection,
    content_reference: PageContentReference,
) -> PageContentTargetInspection {
    let location =
        crate::resolve_classic_xref_object(xref, content_reference.reference.object_number);
    let ClassicXrefObjectLocation::InUse {
        generation: xref_generation,
        byte_offset: object_byte_offset,
        ..
    } = location
    else {
        return PageContentTargetInspection::Skipped {
            content_reference,
            reason: SkippedPageContentTargetReason::UnresolvedXrefLocation { location },
        };
    };

    if xref_generation != content_reference.reference.generation {
        return PageContentTargetInspection::Skipped {
            content_reference,
            reason: SkippedPageContentTargetReason::GenerationMismatch {
                requested_generation: content_reference.reference.generation,
                xref_generation,
                object_byte_offset,
            },
        };
    }

    PageContentTargetInspection::Resolved {
        content_reference,
        object_byte_offset,
        xref_generation,
    }
}
