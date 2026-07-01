//! Public bridge from classic-xref PDF bytes to page-object inventory.

use presslint_inventory::{GraphicsWalkError, Inventory, build_inventory};
use presslint_pdf::{
    ContentStreamDataExtentInspectionError, ContentStreamDataSliceError,
    ContentStreamFilterClassification, ContentStreamFilterClassificationError,
    DocumentPageContentExtentInspection, DocumentPageContentExtentResult,
    DocumentPageContentExtentsInspectionError, FlateDecodeParametersResolution,
    FlateDecodeParametersResolutionError, FlateDecodeStreamError, PageContentExtentInspection,
    SkippedPageContentTargetReason, classify_content_stream_filter, content_stream_data_slice,
    decode_flate_stream, inspect_classic_document_access, inspect_document_page_content_extents,
    resolve_flate_decode_parameters,
};
use presslint_syntax::{AssembleError, TokenizeError, assemble_operators, tokenize};
use presslint_types::{ContentScope, PageIndex};
use serde::{Deserialize, Serialize};

/// Result of building inventory from a classic-xref PDF.
///
/// Page indexes in this report are zero-based document-order ordinals assigned
/// by the page-content-extents pass. Inventoried page entries are merged into
/// `inventory` in the same page order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassicPdfInventory {
    /// Total source length supplied by the caller.
    pub byte_len: usize,
    /// Combined inventory for all pages that this bridge could build.
    pub inventory: Inventory,
    /// One document-ordered result for each enumerated page.
    pub pages: Vec<ClassicPdfInventoryPage>,
}

/// Per-page bridge result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassicPdfInventoryPage {
    /// Zero-based document-order page ordinal.
    pub page_index: PageIndex,
    /// Page inventory result or structured skip.
    pub result: ClassicPdfInventoryPageResult,
}

/// Inventory result for one page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ClassicPdfInventoryPageResult {
    /// The page had exactly one supported content stream and was inventoried.
    Inventoried {
        /// Number of entries emitted for this page.
        entry_count: usize,
    },
    /// The page was intentionally skipped with a structured reason.
    Skipped {
        /// Structured reason the page was not inventoried.
        reason: ClassicPdfInventorySkip,
    },
}

/// Structured page/stream skip reasons for the classic PDF inventory bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "skip", rename_all = "snake_case")]
pub enum ClassicPdfInventorySkip {
    /// The page `/Contents` value could not be inspected.
    ContentsFailed {
        /// Delegated page `/Contents` inspection failure.
        error: presslint_pdf::PageContentsInspectionError,
    },
    /// The page had no content-stream targets.
    NoContentStreams,
    /// The page had more than one content-stream target.
    MultipleContentStreams {
        /// Number of content targets reported for this page.
        stream_count: usize,
    },
    /// The single target could not be resolved through the classic xref table.
    TargetSkipped {
        /// Delegated target skip reason.
        reason: SkippedPageContentTargetReason,
    },
    /// The single resolved target's stream extent could not be located.
    ExtentFailed {
        /// Resolved content-stream object byte offset.
        object_byte_offset: usize,
        /// Delegated stream-extent failure.
        error: ContentStreamDataExtentInspectionError,
    },
    /// The located extent could not be bridged to a borrowed source slice.
    SliceFailed {
        /// Resolved content-stream object byte offset.
        object_byte_offset: usize,
        /// Delegated slice failure.
        error: ContentStreamDataSliceError,
    },
    /// The stream `/Filter` declaration was malformed.
    FilterClassificationFailed {
        /// Resolved content-stream object byte offset.
        object_byte_offset: usize,
        /// Delegated filter-classification failure.
        error: ContentStreamFilterClassificationError,
    },
    /// The stream uses a filter shape this bridge does not decode.
    UnsupportedFilter {
        /// Resolved content-stream object byte offset.
        object_byte_offset: usize,
        /// Delegated filter classification.
        classification: ContentStreamFilterClassification,
    },
    /// The stream `/DecodeParms` declaration was malformed.
    DecodeParmsFailed {
        /// Resolved content-stream object byte offset.
        object_byte_offset: usize,
        /// Delegated `/DecodeParms` failure.
        error: FlateDecodeParametersResolutionError,
    },
    /// The stream uses the `/DecodeParms` array form.
    UnsupportedDecodeParms {
        /// Resolved content-stream object byte offset.
        object_byte_offset: usize,
        /// Delegated `/DecodeParms` resolution.
        resolution: FlateDecodeParametersResolution,
    },
    /// The bounded `/FlateDecode` operation failed.
    DecodeFailed {
        /// Resolved content-stream object byte offset.
        object_byte_offset: usize,
        /// Delegated Flate decode failure.
        error: FlateDecodeStreamError,
    },
    /// The decoded content stream could not be tokenized.
    TokenizeFailed {
        /// Resolved content-stream object byte offset.
        object_byte_offset: usize,
        /// Delegated tokenizer failure.
        error: TokenizeError,
    },
    /// The tokenized content stream could not be assembled into operators.
    AssembleFailed {
        /// Resolved content-stream object byte offset.
        object_byte_offset: usize,
        /// Delegated assembler failure.
        error: AssembleError,
    },
    /// The graphics-state inventory walk failed.
    GraphicsWalkFailed {
        /// Resolved content-stream object byte offset.
        object_byte_offset: usize,
        /// Delegated graphics-state walk failure.
        error: GraphicsWalkError,
    },
}

/// Error returned when the classic PDF inventory bridge cannot establish the
/// document path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassicPdfInventoryError {
    /// Total source length supplied by the caller.
    pub byte_len: usize,
    /// Structured top-level failure.
    pub reason: ClassicPdfInventoryRejection,
}

/// Top-level bridge failures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum ClassicPdfInventoryRejection {
    /// The classic-xref document-access spine failed.
    ClassicDocumentAccess {
        /// Delegated document-access failure.
        error: Box<presslint_pdf::ClassicDocumentAccessError>,
    },
    /// The page-content-extents aggregate could not be established.
    PageContentExtents {
        /// Delegated page-content-extents failure.
        error: Box<DocumentPageContentExtentsInspectionError>,
    },
    /// A document-order page ordinal did not fit in [`PageIndex`].
    PageIndexOutOfRange {
        /// Zero-based document-order ordinal that exceeded `u32`.
        ordinal: usize,
    },
}

/// Build page-object inventory from a classic-xref PDF byte slice.
///
/// The bridge accepts borrowed PDF bytes and never mutates them. It establishes
/// the existing classic-xref document-access path, locates page content-stream
/// data extents, and inventories only pages with exactly one located content
/// stream whose decode path is raw or a single `/FlateDecode` with resolved
/// non-array `/DecodeParms`.
///
/// Raw streams are passed to syntax and inventory as borrowed slices. Flate
/// streams allocate only the bounded decoded buffer returned by
/// [`presslint_pdf::decode_flate_stream`]. Resource dictionaries are not
/// inspected in this slice, so empty image and form `XObject` name lists are
/// supplied to the combined inventory builder.
///
/// Page indexes in the returned report are zero-based document-order ordinals.
///
/// # Errors
///
/// Returns [`ClassicPdfInventoryError`] only for failures that prevent the
/// classic document/page-content path from being established. Unsupported page
/// and stream shapes are represented as structured page skips.
pub fn build_classic_pdf_inventory(
    input: &[u8],
    max_decoded_stream_bytes: usize,
) -> Result<ClassicPdfInventory, ClassicPdfInventoryError> {
    let access = inspect_classic_document_access(input).map_err(|error| {
        inventory_error(
            input,
            ClassicPdfInventoryRejection::ClassicDocumentAccess {
                error: Box::new(error),
            },
        )
    })?;
    let extents = inspect_document_page_content_extents(
        input,
        &access.xref_table,
        access.page_tree_root.object_byte_offset,
    )
    .map_err(|error| {
        inventory_error(
            input,
            ClassicPdfInventoryRejection::PageContentExtents {
                error: Box::new(error),
            },
        )
    })?;

    let mut inventory = Inventory::default();
    let mut pages = Vec::with_capacity(extents.pages.len());
    for page in &extents.pages {
        let page_index = page_index_error(input, page.ordinal)?;
        let result = match build_page_inventory(input, page, page_index, max_decoded_stream_bytes) {
            Ok(page_inv) => {
                let entry_count = page_inv.len();
                inventory.entries.extend(page_inv.entries);
                ClassicPdfInventoryPageResult::Inventoried { entry_count }
            }
            Err(reason) => ClassicPdfInventoryPageResult::Skipped {
                reason: reason.into(),
            },
        };
        pages.push(ClassicPdfInventoryPage { page_index, result });
    }

    Ok(ClassicPdfInventory {
        byte_len: input.len(),
        inventory,
        pages,
    })
}

pub fn build_page_inventory(
    input: &[u8],
    page: &DocumentPageContentExtentInspection,
    page_index: PageIndex,
    max_decoded_stream_bytes: usize,
) -> Result<Inventory, InventoryPageSkip> {
    let extents = match &page.result {
        DocumentPageContentExtentResult::Inspected { extents, .. } => extents,
        DocumentPageContentExtentResult::ContentsFailed { error } => {
            return Err(InventoryPageSkip::ContentsFailed {
                error: error.clone(),
            });
        }
    };

    if extents.entries.is_empty() {
        return Err(InventoryPageSkip::NoContentStreams);
    }
    if extents.entries.len() > 1 {
        return Err(InventoryPageSkip::MultipleContentStreams {
            stream_count: extents.entries.len(),
        });
    }

    let stream = match &extents.entries[0] {
        PageContentExtentInspection::Located {
            object_byte_offset,
            extent,
            ..
        } => (*object_byte_offset, extent),
        PageContentExtentInspection::Skipped { reason, .. } => {
            return Err(InventoryPageSkip::TargetSkipped {
                reason: reason.clone(),
            });
        }
        PageContentExtentInspection::Failed {
            object_byte_offset,
            error,
            ..
        } => {
            return Err(InventoryPageSkip::ExtentFailed {
                object_byte_offset: *object_byte_offset,
                error: error.clone(),
            });
        }
    };

    let content = decode_content(input, stream.0, stream.1, max_decoded_stream_bytes)?;
    let source = content.as_slice();
    let tokens = tokenize(source).map_err(|error| InventoryPageSkip::TokenizeFailed {
        object_byte_offset: stream.0,
        error,
    })?;
    let assembled =
        assemble_operators(&tokens).map_err(|error| InventoryPageSkip::AssembleFailed {
            object_byte_offset: stream.0,
            error,
        })?;
    let inventory = build_inventory(
        source,
        &assembled.records,
        page_index,
        &ContentScope::Page,
        &[],
        &[],
    )
    .map_err(|error| InventoryPageSkip::GraphicsWalkFailed {
        object_byte_offset: stream.0,
        error,
    })?;

    Ok(inventory)
}

enum ContentBytes<'input> {
    Borrowed(&'input [u8]),
    Owned(Vec<u8>),
}

impl ContentBytes<'_> {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }
}

fn decode_content<'input>(
    input: &'input [u8],
    object_byte_offset: usize,
    extent: &presslint_pdf::ContentStreamDataExtentInspection,
    max_decoded_stream_bytes: usize,
) -> Result<ContentBytes<'input>, InventoryPageSkip> {
    let stream_data = content_stream_data_slice(input, extent).map_err(|error| {
        InventoryPageSkip::SliceFailed {
            object_byte_offset,
            error,
        }
    })?;

    match classify_content_stream_filter(input, object_byte_offset).map_err(|error| {
        InventoryPageSkip::FilterClassificationFailed {
            object_byte_offset,
            error,
        }
    })? {
        ContentStreamFilterClassification::Uncompressed => Ok(ContentBytes::Borrowed(stream_data)),
        ContentStreamFilterClassification::Flate => {
            let resolution =
                resolve_flate_decode_parameters(input, object_byte_offset).map_err(|error| {
                    InventoryPageSkip::DecodeParmsFailed {
                        object_byte_offset,
                        error,
                    }
                })?;
            let FlateDecodeParametersResolution::Resolved { parameters, .. } = resolution else {
                return Err(InventoryPageSkip::UnsupportedDecodeParms {
                    object_byte_offset,
                    resolution,
                });
            };
            let decoded = decode_flate_stream(stream_data, parameters, max_decoded_stream_bytes)
                .map_err(|error| InventoryPageSkip::DecodeFailed {
                    object_byte_offset,
                    error,
                })?;
            Ok(ContentBytes::Owned(decoded))
        }
        classification @ (ContentStreamFilterClassification::UnsupportedFilter { .. }
        | ContentStreamFilterClassification::UnsupportedFilterChain { .. }) => {
            Err(InventoryPageSkip::UnsupportedFilter {
                object_byte_offset,
                classification,
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryPageSkip {
    ContentsFailed {
        error: presslint_pdf::PageContentsInspectionError,
    },
    NoContentStreams,
    MultipleContentStreams {
        stream_count: usize,
    },
    TargetSkipped {
        reason: SkippedPageContentTargetReason,
    },
    ExtentFailed {
        object_byte_offset: usize,
        error: ContentStreamDataExtentInspectionError,
    },
    SliceFailed {
        object_byte_offset: usize,
        error: ContentStreamDataSliceError,
    },
    FilterClassificationFailed {
        object_byte_offset: usize,
        error: ContentStreamFilterClassificationError,
    },
    UnsupportedFilter {
        object_byte_offset: usize,
        classification: ContentStreamFilterClassification,
    },
    DecodeParmsFailed {
        object_byte_offset: usize,
        error: FlateDecodeParametersResolutionError,
    },
    UnsupportedDecodeParms {
        object_byte_offset: usize,
        resolution: FlateDecodeParametersResolution,
    },
    DecodeFailed {
        object_byte_offset: usize,
        error: FlateDecodeStreamError,
    },
    TokenizeFailed {
        object_byte_offset: usize,
        error: TokenizeError,
    },
    AssembleFailed {
        object_byte_offset: usize,
        error: AssembleError,
    },
    GraphicsWalkFailed {
        object_byte_offset: usize,
        error: GraphicsWalkError,
    },
}

impl From<InventoryPageSkip> for ClassicPdfInventorySkip {
    fn from(skip: InventoryPageSkip) -> Self {
        match skip {
            InventoryPageSkip::ContentsFailed { error } => Self::ContentsFailed { error },
            InventoryPageSkip::NoContentStreams => Self::NoContentStreams,
            InventoryPageSkip::MultipleContentStreams { stream_count } => {
                Self::MultipleContentStreams { stream_count }
            }
            InventoryPageSkip::TargetSkipped { reason } => Self::TargetSkipped { reason },
            InventoryPageSkip::ExtentFailed {
                object_byte_offset,
                error,
            } => Self::ExtentFailed {
                object_byte_offset,
                error,
            },
            InventoryPageSkip::SliceFailed {
                object_byte_offset,
                error,
            } => Self::SliceFailed {
                object_byte_offset,
                error,
            },
            InventoryPageSkip::FilterClassificationFailed {
                object_byte_offset,
                error,
            } => Self::FilterClassificationFailed {
                object_byte_offset,
                error,
            },
            InventoryPageSkip::UnsupportedFilter {
                object_byte_offset,
                classification,
            } => Self::UnsupportedFilter {
                object_byte_offset,
                classification,
            },
            InventoryPageSkip::DecodeParmsFailed {
                object_byte_offset,
                error,
            } => Self::DecodeParmsFailed {
                object_byte_offset,
                error,
            },
            InventoryPageSkip::UnsupportedDecodeParms {
                object_byte_offset,
                resolution,
            } => Self::UnsupportedDecodeParms {
                object_byte_offset,
                resolution,
            },
            InventoryPageSkip::DecodeFailed {
                object_byte_offset,
                error,
            } => Self::DecodeFailed {
                object_byte_offset,
                error,
            },
            InventoryPageSkip::TokenizeFailed {
                object_byte_offset,
                error,
            } => Self::TokenizeFailed {
                object_byte_offset,
                error,
            },
            InventoryPageSkip::AssembleFailed {
                object_byte_offset,
                error,
            } => Self::AssembleFailed {
                object_byte_offset,
                error,
            },
            InventoryPageSkip::GraphicsWalkFailed {
                object_byte_offset,
                error,
            } => Self::GraphicsWalkFailed {
                object_byte_offset,
                error,
            },
        }
    }
}

pub fn page_index(ordinal: usize) -> Result<PageIndex, usize> {
    u32::try_from(ordinal).map(PageIndex).map_err(|_| ordinal)
}

fn page_index_error(input: &[u8], ordinal: usize) -> Result<PageIndex, ClassicPdfInventoryError> {
    page_index(ordinal).map_err(|ordinal| {
        inventory_error(
            input,
            ClassicPdfInventoryRejection::PageIndexOutOfRange { ordinal },
        )
    })
}

const fn inventory_error(
    input: &[u8],
    reason: ClassicPdfInventoryRejection,
) -> ClassicPdfInventoryError {
    ClassicPdfInventoryError {
        byte_len: input.len(),
        reason,
    }
}
