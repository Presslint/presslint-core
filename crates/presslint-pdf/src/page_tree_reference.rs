use serde::{Deserialize, Serialize};

use crate::{
    ClassicXrefObjectLocation, ClassicXrefTableInspection, IndirectRef, PageTreeNodeTypeInspection,
    PageTreeNodeTypeInspectionRejection,
};

/// Resolved and classified target of one page-tree indirect reference.
///
/// This report stores only the requested reference, resolved in-use xref
/// metadata, and the delegated node-type inspection. It does not retain or copy
/// PDF bytes, object bodies, stream bodies, page dictionaries, page-tree
/// dictionaries, contents streams, or referenced-object bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageTreeReferenceTargetInspection {
    /// Requested page-tree indirect reference.
    pub reference: IndirectRef,
    /// In-use object byte offset resolved from the classic xref table.
    pub object_byte_offset: usize,
    /// Generation number reported by the matching xref entry.
    pub xref_generation: u16,
    /// Delegated classification of the referenced object's `/Type`.
    pub node_type: PageTreeNodeTypeInspection,
}

/// Error returned when a page-tree reference target cannot be resolved and
/// classified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageTreeReferenceTargetInspectionError {
    /// Requested page-tree indirect reference.
    pub reference: IndirectRef,
    /// Total source length.
    pub byte_len: usize,
    /// Resolved in-use object byte offset, when xref resolution reached one.
    pub object_byte_offset: Option<usize>,
    /// Byte offset where delegated node-type inspection found a malformed or
    /// unsupported construct, when available.
    pub error_byte_offset: Option<usize>,
    /// Structured failure reason.
    pub reason: PageTreeReferenceTargetInspectionRejection,
}

/// Structured page-tree reference target rejection reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum PageTreeReferenceTargetInspectionRejection {
    /// The xref result was not exactly one in-use entry.
    UnresolvedXrefLocation {
        /// Locate-only xref result for the requested object number.
        location: ClassicXrefObjectLocation,
    },
    /// The xref entry generation did not match the requested reference
    /// generation.
    GenerationMismatch {
        /// Generation number from the requested indirect reference.
        requested_generation: u16,
        /// Generation number from the matching in-use xref entry.
        xref_generation: u16,
    },
    /// Delegated page-tree node-type inspection failed.
    NodeType {
        /// Underlying page-tree node-type rejection reason.
        node_type_reason: PageTreeNodeTypeInspectionRejection,
    },
}

/// Resolve one page-tree indirect reference through a classic xref inspection
/// and classify the referenced object's `/Type`.
///
/// The helper composes existing bounded helpers only: it resolves the requested
/// object number through [`crate::resolve_classic_xref_object`], accepts only a
/// single in-use xref entry whose generation matches the requested
/// [`IndirectRef`], then delegates object classification to
/// [`crate::inspect_page_tree_node_type`] at the resolved byte offset.
///
/// It does not implement page-tree traversal, recurse through `/Kids`, validate
/// `/Count`, inspect page contents/resources/annotations, parse streams, follow
/// `/Prev`, or add any cache/index around the xref table.
///
/// # Errors
///
/// Returns [`PageTreeReferenceTargetInspectionError`] when xref resolution is
/// free, not found, or ambiguous; when the in-use xref generation does not match
/// the requested reference generation; or when delegated node-type inspection
/// fails.
pub fn inspect_page_tree_reference_target(
    input: &[u8],
    xref: &ClassicXrefTableInspection,
    reference: IndirectRef,
) -> Result<PageTreeReferenceTargetInspection, PageTreeReferenceTargetInspectionError> {
    let location = crate::resolve_classic_xref_object(xref, reference.object_number);
    let ClassicXrefObjectLocation::InUse {
        generation: xref_generation,
        byte_offset: object_byte_offset,
        ..
    } = location
    else {
        return Err(page_tree_reference_target_error(
            input,
            reference,
            None,
            None,
            PageTreeReferenceTargetInspectionRejection::UnresolvedXrefLocation { location },
        ));
    };

    if xref_generation != reference.generation {
        return Err(page_tree_reference_target_error(
            input,
            reference,
            Some(object_byte_offset),
            None,
            PageTreeReferenceTargetInspectionRejection::GenerationMismatch {
                requested_generation: reference.generation,
                xref_generation,
            },
        ));
    }

    let node_type =
        crate::inspect_page_tree_node_type(input, object_byte_offset).map_err(|error| {
            page_tree_reference_target_error(
                input,
                reference,
                Some(object_byte_offset),
                error.error_byte_offset,
                PageTreeReferenceTargetInspectionRejection::NodeType {
                    node_type_reason: error.reason,
                },
            )
        })?;

    Ok(PageTreeReferenceTargetInspection {
        reference,
        object_byte_offset,
        xref_generation,
        node_type,
    })
}

const fn page_tree_reference_target_error(
    input: &[u8],
    reference: IndirectRef,
    object_byte_offset: Option<usize>,
    error_byte_offset: Option<usize>,
    reason: PageTreeReferenceTargetInspectionRejection,
) -> PageTreeReferenceTargetInspectionError {
    PageTreeReferenceTargetInspectionError {
        reference,
        byte_len: input.len(),
        object_byte_offset,
        error_byte_offset,
        reason,
    }
}
