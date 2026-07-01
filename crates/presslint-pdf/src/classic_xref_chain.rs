use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::xref_section::classify_xref_section;
use crate::xref_stream::{parse_non_negative_integer, unique_entry};
use crate::{
    ClassicXrefEntry, ClassicXrefEntryState, ClassicXrefObjectLocation, ClassicXrefTableInspection,
    ClassicXrefTableInspectionError, ClassicXrefTrailerPrevInspectionError,
    ClassicXrefTrailerRootInspectionError, IndirectRef, PdfSourceDiagnostic, XrefSection,
    inspect_classic_xref_table, inspect_classic_xref_trailer_prev,
    inspect_classic_xref_trailer_root,
};

const SIZE_KEY: &[u8] = b"/Size";

/// Maximum number of classic cross-reference sections followed through a `/Prev`
/// chain.
pub const MAX_CLASSIC_XREF_CHAIN_SECTIONS: usize = 64;

/// Maximum number of merged entries retained across a classic cross-reference
/// `/Prev` chain.
pub const MAX_CLASSIC_XREF_CHAIN_ENTRIES: usize = 1_000_000;

/// Newest-wins object map built from a same-type classic-table `/Prev` chain.
///
/// The `startxref` section is the newest section; `/Prev` is followed
/// newest-to-oldest. The merge is **newest-wins including free-entry shadowing**:
/// a newer entry (in-use or free) for an object number shadows any older entry
/// for the same number, and earlier sections only fill object numbers not already
/// present. Within a single section, the **first** entry for an object number in
/// source order wins (intra-section-first). Classic free-list fields are
/// preserved exactly as parsed; the chain reports parsed structure only and does
/// not validate free-list integrity.
///
/// This report stores only structural metadata: bounded section byte offsets,
/// the newest `/Root` reference, an effective `/Size`, and the small `Copy`
/// merged entries. It retains or copies no PDF source bytes, trailer bytes,
/// object bodies, or stream bodies. Final entries are deterministic and sorted
/// ascending by object number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassicXrefChain {
    /// Caller-supplied byte offset of the newest classic cross-reference section.
    pub startxref_byte_offset: usize,
    /// Followed classic cross-reference section byte offsets, newest to oldest.
    pub section_byte_offsets: Vec<usize>,
    /// `/Root` reference read from the newest section trailer only.
    pub root_reference: IndirectRef,
    /// Effective `/Size`, the maximum direct `/Size` observed across the merged
    /// section trailers. Because classic object location uses byte offsets rather
    /// than `/Size`, a section trailer without a readable direct `/Size` simply
    /// does not contribute; the field is best-effort and does not gate the chain.
    pub effective_size: usize,
    /// Newest-wins entries in ascending object-number order.
    pub entries: Vec<ClassicXrefEntry>,
}

/// Error returned when a classic cross-reference `/Prev` chain cannot be built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassicXrefChainError {
    /// Newest classic cross-reference byte offset supplied by the caller.
    pub startxref_byte_offset: usize,
    /// Total source length.
    pub byte_len: usize,
    /// Structured chain stop reason.
    pub reason: ClassicXrefChainRejection,
}

/// Structured classic cross-reference chain rejection reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum ClassicXrefChainRejection {
    /// The current section offset is outside the input bounds.
    OffsetOutOfBounds {
        /// Out-of-bounds classic cross-reference byte offset.
        byte_offset: usize,
    },
    /// Following `/Prev` revisited a section offset already seen.
    Cycle {
        /// Revisited classic cross-reference byte offset.
        byte_offset: usize,
    },
    /// The bounded maximum number of sections was reached before the chain
    /// ended.
    SectionLimitExceeded {
        /// Configured section limit.
        max_sections: usize,
    },
    /// The bounded maximum number of merged entries was exceeded.
    EntryLimitExceeded {
        /// Configured entry limit.
        max_entries: usize,
    },
    /// A `/Prev` target could not be classified as an xref section.
    PrevSectionUnclassified {
        /// `/Prev` byte offset that failed classification.
        byte_offset: usize,
        /// Delegated source diagnostic.
        diagnostic: Box<PdfSourceDiagnostic>,
    },
    /// A `/Prev` target classified as a cross-reference stream. Mixed
    /// classic/xref-stream chains are deferred to a later slice, so this is a
    /// structured stop rather than a silent drop or a panic.
    PrevSectionNotClassicXref {
        /// `/Prev` byte offset that was not a classic table.
        byte_offset: usize,
    },
    /// Inspecting one classic xref table in the chain failed.
    SectionTable {
        /// Classic cross-reference section byte offset.
        byte_offset: usize,
        /// Delegated classic xref table inspection failure.
        error: Box<ClassicXrefTableInspectionError>,
    },
    /// Reading `/Root` from the newest section trailer failed.
    TrailerRoot {
        /// Newest classic cross-reference section byte offset.
        byte_offset: usize,
        /// Delegated trailer `/Root` inspection failure.
        error: Box<ClassicXrefTrailerRootInspectionError>,
    },
    /// Reading `/Prev` from one section trailer failed.
    TrailerPrev {
        /// Classic cross-reference section byte offset.
        byte_offset: usize,
        /// Delegated trailer `/Prev` inspection failure.
        error: Box<ClassicXrefTrailerPrevInspectionError>,
    },
}

/// Build a bounded newest-wins classic-table object map by following `/Prev`.
///
/// The builder classifies and inspects one classic cross-reference table at a
/// time with [`inspect_classic_xref_table`], reads the newest `/Root` with
/// [`inspect_classic_xref_trailer_root`], follows the `/Prev` byte offset read by
/// [`inspect_classic_xref_trailer_prev`], and merges entries newest-to-oldest
/// into a deterministic [`BTreeMap`], keeping only the merged section offsets and
/// small `Copy` entry records.
///
/// Cross-section duplicate object numbers are expected and resolved newest-wins;
/// intra-section duplicates keep the first entry in source order. Free entries
/// shadow older in-use entries. The work is bounded by a visited-offset set for
/// cycles, [`MAX_CLASSIC_XREF_CHAIN_SECTIONS`], and
/// [`MAX_CLASSIC_XREF_CHAIN_ENTRIES`], so a malformed `/Prev` graph cannot cause
/// unbounded work or allocation.
///
/// A `/Prev` target that classifies as a cross-reference stream stops with
/// [`ClassicXrefChainRejection::PrevSectionNotClassicXref`]; mixing classic and
/// xref-stream sections is deferred.
///
/// # Errors
///
/// Returns [`ClassicXrefChainError`] for any bounded chain stop: out-of-bounds
/// offsets, cycles, over-long chains, non-classic `/Prev` targets, classification
/// failures, table/trailer inspection failures, and merged-entry bound overflow.
/// No partial chain is returned.
pub fn build_classic_xref_chain(
    input: &[u8],
    startxref_byte_offset: usize,
) -> Result<ClassicXrefChain, ClassicXrefChainError> {
    let mut visited = BTreeSet::new();
    let mut section_byte_offsets = Vec::new();
    let mut merged = BTreeMap::<u32, ClassicXrefEntry>::new();
    let mut next_offset = Some(startxref_byte_offset);
    let mut root_reference = None;
    let mut effective_size = 0usize;

    while let Some(byte_offset) = next_offset {
        if section_byte_offsets.len() >= MAX_CLASSIC_XREF_CHAIN_SECTIONS {
            return Err(chain_error(
                input,
                startxref_byte_offset,
                ClassicXrefChainRejection::SectionLimitExceeded {
                    max_sections: MAX_CLASSIC_XREF_CHAIN_SECTIONS,
                },
            ));
        }

        let table = open_chain_section(input, startxref_byte_offset, byte_offset, &mut visited)?;

        if root_reference.is_none() {
            let root = inspect_classic_xref_trailer_root(input, table.trailer_byte_offset)
                .map_err(|error| {
                    chain_error(
                        input,
                        startxref_byte_offset,
                        ClassicXrefChainRejection::TrailerRoot {
                            byte_offset,
                            error: Box::new(error),
                        },
                    )
                })?;
            root_reference = Some(root.root_reference);
        }

        let prev = inspect_classic_xref_trailer_prev(input, table.trailer_byte_offset).map_err(
            |error| {
                chain_error(
                    input,
                    startxref_byte_offset,
                    ClassicXrefChainRejection::TrailerPrev {
                        byte_offset,
                        error: Box::new(error),
                    },
                )
            },
        )?;

        if let Some(size) = read_trailer_size(input, table.trailer_byte_offset) {
            effective_size = effective_size.max(size);
        }

        merge_section_entries(input, startxref_byte_offset, &mut merged, &table)?;

        section_byte_offsets.push(byte_offset);
        next_offset = prev.map(|prev| prev.prev_byte_offset);
    }

    let Some(root_reference) = root_reference else {
        return Err(chain_error(
            input,
            startxref_byte_offset,
            ClassicXrefChainRejection::OffsetOutOfBounds {
                byte_offset: startxref_byte_offset,
            },
        ));
    };

    Ok(ClassicXrefChain {
        startxref_byte_offset,
        section_byte_offsets,
        root_reference,
        effective_size,
        entries: merged.into_values().collect(),
    })
}

/// Open one classic cross-reference section at `byte_offset`: bounds-check the
/// offset, guard against a `/Prev` cycle, classify it as a classic table (never
/// an xref stream), and inspect the table.
fn open_chain_section(
    input: &[u8],
    startxref_byte_offset: usize,
    byte_offset: usize,
    visited: &mut BTreeSet<usize>,
) -> Result<ClassicXrefTableInspection, ClassicXrefChainError> {
    if byte_offset >= input.len() {
        return Err(chain_error(
            input,
            startxref_byte_offset,
            ClassicXrefChainRejection::OffsetOutOfBounds { byte_offset },
        ));
    }
    if !visited.insert(byte_offset) {
        return Err(chain_error(
            input,
            startxref_byte_offset,
            ClassicXrefChainRejection::Cycle { byte_offset },
        ));
    }

    match classify_xref_section(input, byte_offset).map_err(|diagnostic| {
        chain_error(
            input,
            startxref_byte_offset,
            ClassicXrefChainRejection::PrevSectionUnclassified {
                byte_offset,
                diagnostic: Box::new(diagnostic),
            },
        )
    })? {
        XrefSection::Table => {}
        XrefSection::Stream { .. } => {
            return Err(chain_error(
                input,
                startxref_byte_offset,
                ClassicXrefChainRejection::PrevSectionNotClassicXref { byte_offset },
            ));
        }
    }

    inspect_classic_xref_table(input, byte_offset).map_err(|error| {
        chain_error(
            input,
            startxref_byte_offset,
            ClassicXrefChainRejection::SectionTable {
                byte_offset,
                error: Box::new(error),
            },
        )
    })
}

/// Merge one section's entries newest-wins into `merged`: the first entry for an
/// object number (newest section, first-in-section) is kept, so free entries
/// shadow older in-use entries and earlier sections only fill unseen numbers.
fn merge_section_entries(
    input: &[u8],
    startxref_byte_offset: usize,
    merged: &mut BTreeMap<u32, ClassicXrefEntry>,
    table: &ClassicXrefTableInspection,
) -> Result<(), ClassicXrefChainError> {
    for subsection in &table.subsections {
        for entry in &subsection.entries {
            if !merged.contains_key(&entry.object_number)
                && merged.len() >= MAX_CLASSIC_XREF_CHAIN_ENTRIES
            {
                return Err(chain_error(
                    input,
                    startxref_byte_offset,
                    ClassicXrefChainRejection::EntryLimitExceeded {
                        max_entries: MAX_CLASSIC_XREF_CHAIN_ENTRIES,
                    },
                ));
            }
            merged.entry(entry.object_number).or_insert(*entry);
        }
    }
    Ok(())
}

/// Resolve an object number against a merged classic `/Prev` chain.
///
/// The chain entries are already newest-wins deduplicated and sorted ascending
/// by object number, so this binary-searches them and reports the same
/// in-use/free currency as the single-table resolver. There is no ambiguous
/// result: cross-section and intra-section duplicates were resolved during the
/// merge.
#[must_use]
pub fn resolve_classic_xref_chain_object(
    chain: &ClassicXrefChain,
    object_number: u32,
) -> ClassicXrefObjectLocation {
    chain
        .entries
        .binary_search_by_key(&object_number, |entry| entry.object_number)
        .map_or(
            ClassicXrefObjectLocation::NotFound { object_number },
            |index| location_for_entry(chain.entries[index]),
        )
}

/// Map a merged classic entry to its locate-only object location, mirroring the
/// single-table entry-to-location rule.
const fn location_for_entry(entry: ClassicXrefEntry) -> ClassicXrefObjectLocation {
    match entry.state {
        ClassicXrefEntryState::InUse => ClassicXrefObjectLocation::InUse {
            object_number: entry.object_number,
            generation: entry.generation,
            byte_offset: entry.byte_offset,
        },
        ClassicXrefEntryState::Free => ClassicXrefObjectLocation::Free {
            object_number: entry.object_number,
            generation: entry.generation,
            next_free_object_number: entry.byte_offset,
        },
    }
}

/// Best-effort read of a single direct non-negative `/Size` from a section
/// trailer.
///
/// Returns `None` when the trailer has no readable single direct `/Size`
/// (absent, duplicate, non-integer, or overflowing). Because classic object
/// location uses byte offsets rather than `/Size`, a missing `/Size` is not a
/// chain error. The trailer dictionary and entries were already validated by
/// [`inspect_classic_xref_trailer_prev`] earlier in the same section.
fn read_trailer_size(input: &[u8], trailer_keyword_offset: usize) -> Option<usize> {
    let trailer_dictionary =
        crate::inspect_classic_xref_trailer_dictionary(input, trailer_keyword_offset).ok()?;
    let entries =
        crate::inspect_dictionary_entries(input, trailer_dictionary.dictionary_open_byte_offset)
            .ok()?;
    let entry = unique_entry(input, &entries.entries, SIZE_KEY).ok()??;
    parse_non_negative_integer(&input[entry.value_range.start..entry.value_range.end]).ok()
}

const fn chain_error(
    input: &[u8],
    startxref_byte_offset: usize,
    reason: ClassicXrefChainRejection,
) -> ClassicXrefChainError {
    ClassicXrefChainError {
        startxref_byte_offset,
        byte_len: input.len(),
        reason,
    }
}
