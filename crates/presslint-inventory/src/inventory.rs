use presslint_core::{
    BoundingBox, ColorObservation, ColorSpace, ColorUsage, ContentScope, EditCapability, ObjectId,
    ObjectKind, PageIndex, PdfName, Provenance,
};
use presslint_syntax::OperatorRecord;
use serde::{Deserialize, Serialize};

use crate::digest::{
    form_object_digest, image_object_digest, text_object_digest, usize_to_u32, vector_object_digest,
};
use crate::walker::{
    GraphicsStateEvent, GraphicsStateEventKind, GraphicsStateSnapshot, GraphicsWalkError,
    PathPaintKind, TextRenderingMode, walk_graphics_state,
};

/// One queryable page object discovered by the inventory pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventoryEntry {
    /// Stable object identity.
    pub id: ObjectId,
    /// Object class.
    pub kind: ObjectKind,
    /// Source location that enables later action planning.
    pub provenance: Provenance,
    /// Optional object bounds.
    pub bounds: Option<BoundingBox>,
    /// Color observations associated with the object.
    pub colors: Vec<ColorObservation>,
    /// Edit capabilities known at inventory time.
    pub capabilities: Vec<EditCapability>,
}

/// Deterministic document inventory.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Inventory {
    /// Entries in page order and then content order.
    pub entries: Vec<InventoryEntry>,
}

impl Inventory {
    /// Return the number of entries.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return true when no entries were discovered.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Build vector inventory entries from assembled content-stream operators.
///
/// This slice records path paint operations that actually paint. Geometry is
/// intentionally not inferred yet, so vector bounds are left unset.
///
/// # Errors
///
/// Returns a structured graphics-state walker error for malformed records in
/// the supported operator set or invalid source ranges.
pub fn build_vector_inventory(
    source: &[u8],
    records: &[OperatorRecord],
    page: PageIndex,
    scope: &ContentScope,
) -> Result<Inventory, GraphicsWalkError> {
    let events = walk_graphics_state(source, records)?;
    Ok(vector_inventory_from_graphics_events(page, scope, &events))
}

/// Build text inventory entries from assembled content-stream operators.
///
/// This slice recognizes text-showing operators and records the active text
/// rendering mode. It does not decode glyph strings or infer text geometry.
///
/// # Errors
///
/// Returns a structured graphics-state walker error for malformed records in
/// the supported operator set or invalid source ranges.
pub fn build_text_inventory(
    source: &[u8],
    records: &[OperatorRecord],
    page: PageIndex,
    scope: &ContentScope,
) -> Result<Inventory, GraphicsWalkError> {
    let events = walk_graphics_state(source, records)?;
    Ok(text_inventory_from_graphics_events(page, scope, &events))
}

/// Build image inventory entries from assembled content-stream operators.
///
/// This slice recognizes `Do` `XObject` invocations but emits image entries
/// only for resource names the caller has already classified as image
/// `XObjects`.
/// Resource dictionaries, image streams, filters, and bounds are intentionally
/// not inspected here.
///
/// # Errors
///
/// Returns a structured graphics-state walker error for malformed records in
/// the supported operator set or invalid source ranges.
pub fn build_image_inventory(
    source: &[u8],
    records: &[OperatorRecord],
    page: PageIndex,
    scope: &ContentScope,
    image_xobject_names: &[PdfName],
) -> Result<Inventory, GraphicsWalkError> {
    let events = walk_graphics_state(source, records)?;
    Ok(image_inventory_from_graphics_events(
        page,
        scope,
        &events,
        image_xobject_names,
    ))
}

/// Build form `XObject` invocation inventory entries from assembled
/// content-stream operators.
///
/// This slice recognizes `Do` `XObject` invocations but emits form entries
/// only for resource names the caller has already classified as form
/// `XObjects`.
/// Resource dictionaries, nested form streams, bounds, and colors are
/// intentionally not inspected here.
///
/// # Errors
///
/// Returns a structured graphics-state walker error for malformed records in
/// the supported operator set or invalid source ranges.
pub fn build_form_inventory(
    source: &[u8],
    records: &[OperatorRecord],
    page: PageIndex,
    scope: &ContentScope,
    form_xobject_names: &[PdfName],
) -> Result<Inventory, GraphicsWalkError> {
    let events = walk_graphics_state(source, records)?;
    Ok(form_inventory_from_graphics_events(
        page,
        scope,
        &events,
        form_xobject_names,
    ))
}

/// Build vector inventory entries from graphics-state events.
///
/// Only path paint events that use stroke or fill color become inventory
/// entries. Path-ending and unsupported no-op events are skipped.
#[must_use]
pub fn vector_inventory_from_graphics_events(
    page: PageIndex,
    scope: &ContentScope,
    events: &[GraphicsStateEvent],
) -> Inventory {
    let mut entries = Vec::new();

    for event in events {
        let GraphicsStateEventKind::PathPaint { paint } = &event.kind else {
            continue;
        };
        let paint = *paint;
        let colors = color_observations(paint, &event.state);
        if colors.is_empty() {
            continue;
        }

        let sequence = usize_to_u32(entries.len());
        let provenance = Provenance {
            page,
            scope: scope.clone(),
            range: Some(event.record_range),
        };
        let digest = vector_object_digest(page, sequence, scope, event, paint, &colors);

        entries.push(InventoryEntry {
            id: ObjectId {
                page,
                sequence,
                digest,
            },
            kind: ObjectKind::Vector,
            provenance,
            bounds: None,
            colors,
            capabilities: vec![EditCapability::RewriteColorOperand],
        });
    }

    Inventory { entries }
}

/// Build text inventory entries from graphics-state events.
///
/// Text-showing events always become `ObjectKind::Text` entries. Supported
/// visible rendering modes carry color observations and edit capabilities;
/// invisible or unsupported modes remain conservative.
#[must_use]
pub fn text_inventory_from_graphics_events(
    page: PageIndex,
    scope: &ContentScope,
    events: &[GraphicsStateEvent],
) -> Inventory {
    let mut entries = Vec::new();

    for event in events {
        let GraphicsStateEventKind::TextShow {
            operator,
            rendering_mode,
        } = &event.kind
        else {
            continue;
        };
        let operator = *operator;
        let rendering_mode = *rendering_mode;
        let colors = text_color_observations(rendering_mode, &event.state);

        let sequence = usize_to_u32(entries.len());
        let provenance = Provenance {
            page,
            scope: scope.clone(),
            range: Some(event.record_range),
        };
        let capabilities = text_capabilities(&colors);
        let digest = text_object_digest(
            page,
            sequence,
            scope,
            event,
            operator,
            rendering_mode,
            &colors,
        );

        entries.push(InventoryEntry {
            id: ObjectId {
                page,
                sequence,
                digest,
            },
            kind: ObjectKind::Text,
            provenance,
            bounds: None,
            colors,
            capabilities,
        });
    }

    Inventory { entries }
}

/// Build image inventory entries from graphics-state events.
///
/// Only `Do` invocations whose resource names appear in `image_xobject_names`
/// become `ObjectKind::Image` entries. Other `XObject` invocations are
/// preserved as walker events but skipped by this inventory slice.
#[must_use]
pub fn image_inventory_from_graphics_events(
    page: PageIndex,
    scope: &ContentScope,
    events: &[GraphicsStateEvent],
    image_xobject_names: &[PdfName],
) -> Inventory {
    xobject_inventory_from_graphics_events(
        page,
        scope,
        events,
        image_xobject_names,
        |sequence, event, name| {
            let colors = vec![image_color_observation()];
            let digest = image_object_digest(page, sequence, scope, event, name, &colors);

            (ObjectKind::Image, colors, digest)
        },
    )
}

/// Build form `XObject` invocation inventory entries from graphics-state events.
///
/// Only `Do` invocations whose resource names appear in `form_xobject_names`
/// become `ObjectKind::FormXObject` entries. Other `XObject` invocations are
/// preserved as walker events but skipped by this inventory slice.
#[must_use]
pub fn form_inventory_from_graphics_events(
    page: PageIndex,
    scope: &ContentScope,
    events: &[GraphicsStateEvent],
    form_xobject_names: &[PdfName],
) -> Inventory {
    xobject_inventory_from_graphics_events(
        page,
        scope,
        events,
        form_xobject_names,
        |sequence, event, name| {
            let digest = form_object_digest(page, sequence, scope, event, name);

            (ObjectKind::FormXObject, Vec::new(), digest)
        },
    )
}

fn xobject_inventory_from_graphics_events(
    page: PageIndex,
    scope: &ContentScope,
    events: &[GraphicsStateEvent],
    xobject_names: &[PdfName],
    mut entry_parts: impl FnMut(
        u32,
        &GraphicsStateEvent,
        &PdfName,
    ) -> (ObjectKind, Vec<ColorObservation>, [u8; 32]),
) -> Inventory {
    let entries = events
        .iter()
        .filter_map(|event| {
            let GraphicsStateEventKind::XObjectInvoke { name } = &event.kind else {
                return None;
            };
            xobject_names.contains(name).then_some((event, name))
        })
        .enumerate()
        .map(|(sequence, (event, name))| {
            let sequence = usize_to_u32(sequence);
            let (kind, colors, digest) = entry_parts(sequence, event, name);
            InventoryEntry {
                id: ObjectId {
                    page,
                    sequence,
                    digest,
                },
                kind,
                provenance: Provenance {
                    page,
                    scope: scope.clone(),
                    range: Some(event.record_range),
                },
                bounds: None,
                colors,
                capabilities: vec![EditCapability::ReadOnly],
            }
        })
        .collect();

    Inventory { entries }
}

fn color_observations(
    paint: PathPaintKind,
    state: &GraphicsStateSnapshot,
) -> Vec<ColorObservation> {
    let mut colors = Vec::with_capacity(2);
    if paint.uses_stroke() {
        colors.push(state.stroke_observation());
    }
    if paint.uses_fill() {
        colors.push(state.fill_observation());
    }
    colors
}

fn text_color_observations(
    mode: TextRenderingMode,
    state: &GraphicsStateSnapshot,
) -> Vec<ColorObservation> {
    let mut colors = Vec::with_capacity(2);
    if mode.uses_stroke() {
        colors.push(state.stroke_observation());
    }
    if mode.uses_fill() {
        colors.push(state.fill_observation());
    }
    colors
}

fn text_capabilities(colors: &[ColorObservation]) -> Vec<EditCapability> {
    if colors.is_empty() {
        Vec::new()
    } else {
        vec![
            EditCapability::RewriteColorOperand,
            EditCapability::AddTextSpreadStroke,
        ]
    }
}

const fn image_color_observation() -> ColorObservation {
    ColorObservation {
        usage: ColorUsage::Image,
        space: ColorSpace::Unknown,
        components: Vec::new(),
        spot_name: None,
        // Synthesized observation: no color-setting operator produced it.
        source: None,
    }
}
