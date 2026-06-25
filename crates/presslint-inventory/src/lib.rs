//! Page-object inventory model and graphics-state observations.

#![forbid(unsafe_code)]

use presslint_core::{
    BoundingBox, ByteRange, ColorObservation, ColorSpace, ColorUsage, ContentScope, EditCapability,
    ObjectId, ObjectKind, PageIndex, PdfName, Provenance,
};
use presslint_syntax::OperatorRecord;
use serde::{Deserialize, Serialize};

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
    let mut entries = Vec::new();

    for event in events {
        let GraphicsStateEventKind::XObjectInvoke { name } = &event.kind else {
            continue;
        };
        if !image_xobject_names.contains(name) {
            continue;
        }

        let sequence = usize_to_u32(entries.len());
        let provenance = Provenance {
            page,
            scope: scope.clone(),
            range: Some(event.record_range),
        };
        let colors = vec![image_color_observation()];
        let digest = image_object_digest(page, sequence, scope, event, name, &colors);

        entries.push(InventoryEntry {
            id: ObjectId {
                page,
                sequence,
                digest,
            },
            kind: ObjectKind::Image,
            provenance,
            bounds: None,
            colors,
            capabilities: vec![EditCapability::ReadOnly],
        });
    }

    Inventory { entries }
}

/// Device colour currently held by one side of the graphics state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphicsDeviceColor {
    /// Device colour space selected by the operator stream.
    pub space: ColorSpace,
    /// Components in source-space order.
    pub components: Vec<f64>,
}

impl GraphicsDeviceColor {
    /// Create a graphics-state colour snapshot.
    #[must_use]
    pub const fn new(space: ColorSpace, components: Vec<f64>) -> Self {
        Self { space, components }
    }

    /// Return this colour as an inventory colour observation.
    #[must_use]
    pub fn observation(&self, usage: ColorUsage) -> ColorObservation {
        ColorObservation {
            usage,
            space: self.space.clone(),
            components: self.components.clone(),
            spot_name: None,
        }
    }
}

/// Graphics-state slots tracked by the initial content walker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphicsStateSnapshot {
    /// Current transformation matrix in PDF `[a b c d e f]` layout.
    pub ctm: [f64; 6],
    /// Current stroking device colour.
    pub stroking_color: GraphicsDeviceColor,
    /// Current nonstroking device colour.
    pub nonstroking_color: GraphicsDeviceColor,
    /// Current text rendering mode.
    pub text_rendering_mode: TextRenderingMode,
}

impl GraphicsStateSnapshot {
    /// Return the page-initial graphics state for this walker slice.
    #[must_use]
    pub fn page_default() -> Self {
        Self {
            ctm: IDENTITY_CTM,
            stroking_color: GraphicsDeviceColor::new(ColorSpace::DeviceGray, vec![0.0]),
            nonstroking_color: GraphicsDeviceColor::new(ColorSpace::DeviceGray, vec![0.0]),
            text_rendering_mode: TextRenderingMode::Fill,
        }
    }

    /// Current stroking colour as an inventory observation.
    #[must_use]
    pub fn stroke_observation(&self) -> ColorObservation {
        self.stroking_color.observation(ColorUsage::Stroke)
    }

    /// Current nonstroking colour as an inventory observation.
    #[must_use]
    pub fn fill_observation(&self) -> ColorObservation {
        self.nonstroking_color.observation(ColorUsage::Fill)
    }
}

impl Default for GraphicsStateSnapshot {
    fn default() -> Self {
        Self::page_default()
    }
}

/// Text rendering mode relevant to first-slice text inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextRenderingMode {
    /// Fill glyph outlines with the nonstroking colour (`0 Tr`).
    Fill,
    /// Stroke glyph outlines with the stroking colour (`1 Tr`).
    Stroke,
    /// Fill and stroke glyph outlines (`2 Tr`).
    FillThenStroke,
    /// Neither fill nor stroke glyph outlines (`3 Tr`).
    Invisible,
    /// Rendering modes outside the first supported editable slice.
    Unsupported {
        /// Raw `Tr` mode value.
        value: i32,
    },
}

impl TextRenderingMode {
    /// Map a PDF `Tr` integer into this inventory slice.
    #[must_use]
    pub const fn from_pdf_value(value: i32) -> Self {
        match value {
            0 => Self::Fill,
            1 => Self::Stroke,
            2 => Self::FillThenStroke,
            3 => Self::Invisible,
            _ => Self::Unsupported { value },
        }
    }

    /// Whether this mode uses the stroking colour in the supported slice.
    #[must_use]
    pub const fn uses_stroke(self) -> bool {
        matches!(self, Self::Stroke | Self::FillThenStroke)
    }

    /// Whether this mode uses the nonstroking colour in the supported slice.
    #[must_use]
    pub const fn uses_fill(self) -> bool {
        matches!(self, Self::Fill | Self::FillThenStroke)
    }

    /// Whether this mode can be edited by first-slice text color actions.
    #[must_use]
    pub const fn has_supported_visible_paint(self) -> bool {
        matches!(self, Self::Fill | Self::Stroke | Self::FillThenStroke)
    }
}

/// Text-showing operator observed in a content stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextShowOperator {
    /// `Tj`.
    ShowText,
    /// `TJ`.
    ShowTextAdjusted,
    /// `'`.
    MoveNextLineAndShowText,
    /// `"`.
    SetSpacingMoveNextLineAndShowText,
}

/// Type of path paint operation observed in a content stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathPaintKind {
    /// `S`.
    Stroke,
    /// `s`.
    CloseAndStroke,
    /// `f`.
    FillNonzero,
    /// `F`.
    FillObsolete,
    /// `f*`.
    FillEvenOdd,
    /// `B`.
    FillAndStrokeNonzero,
    /// `B*`.
    FillAndStrokeEvenOdd,
    /// `b`.
    CloseFillAndStrokeNonzero,
    /// `b*`.
    CloseFillAndStrokeEvenOdd,
    /// `n`.
    EndPath,
}

impl PathPaintKind {
    /// Whether this path paint operation uses the stroking colour.
    #[must_use]
    pub const fn uses_stroke(self) -> bool {
        matches!(
            self,
            Self::Stroke
                | Self::CloseAndStroke
                | Self::FillAndStrokeNonzero
                | Self::FillAndStrokeEvenOdd
                | Self::CloseFillAndStrokeNonzero
                | Self::CloseFillAndStrokeEvenOdd
        )
    }

    /// Whether this path paint operation uses the nonstroking colour.
    #[must_use]
    pub const fn uses_fill(self) -> bool {
        matches!(
            self,
            Self::FillNonzero
                | Self::FillObsolete
                | Self::FillEvenOdd
                | Self::FillAndStrokeNonzero
                | Self::FillAndStrokeEvenOdd
                | Self::CloseFillAndStrokeNonzero
                | Self::CloseFillAndStrokeEvenOdd
        )
    }
}

/// Semantic event emitted for one assembled operator record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphicsStateEventKind {
    /// `q` saved the current graphics state.
    Save,
    /// `Q` restored the most recently saved graphics state.
    Restore,
    /// `cm` concatenated a matrix onto the CTM.
    ConcatMatrix {
        /// Operand matrix in PDF `[a b c d e f]` layout.
        matrix: [f64; 6],
    },
    /// A stroking device-colour operator changed state.
    SetStrokingDeviceColor {
        /// Updated stroking colour.
        color: GraphicsDeviceColor,
    },
    /// A nonstroking device-colour operator changed state.
    SetNonstrokingDeviceColor {
        /// Updated nonstroking colour.
        color: GraphicsDeviceColor,
    },
    /// A path paint operator observed the current state.
    PathPaint {
        /// Path paint operation.
        paint: PathPaintKind,
    },
    /// `Tr` changed the active text rendering mode.
    SetTextRenderingMode {
        /// Updated text rendering mode.
        mode: TextRenderingMode,
    },
    /// A text-showing operator observed the current text state.
    TextShow {
        /// Text-showing operator.
        operator: TextShowOperator,
        /// Active text rendering mode for this text-showing operation.
        rendering_mode: TextRenderingMode,
    },
    /// `Do` invoked an `XObject` resource by name.
    XObjectInvoke {
        /// Resource name operand without the leading slash.
        name: PdfName,
    },
    /// Operator outside this walker slice; state is unchanged.
    NoOp,
}

/// Ordered graphics-state event tied to source byte provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphicsStateEvent {
    /// Zero-based operator-record index.
    pub index: usize,
    /// Source range for the operator token.
    pub operator_range: ByteRange,
    /// Source range for operands plus operator.
    pub record_range: ByteRange,
    /// Semantic event for this operator.
    pub kind: GraphicsStateEventKind,
    /// Graphics-state snapshot after the operator was applied.
    pub state: GraphicsStateSnapshot,
}

/// Structured graphics-state walker failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphicsWalkError {
    /// Error class.
    pub kind: GraphicsWalkErrorKind,
    /// Source range to highlight for the failing operator record.
    pub range: ByteRange,
}

impl GraphicsWalkError {
    /// Create a walker error.
    #[must_use]
    pub const fn new(kind: GraphicsWalkErrorKind, range: ByteRange) -> Self {
        Self { kind, range }
    }
}

/// Structured graphics-state walker error class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphicsWalkErrorKind {
    /// A source range from an operator record does not address the source bytes.
    InvalidSourceRange,
    /// `Q` appeared while the graphics-state stack was empty.
    GraphicsStateStackUnderflow,
    /// A supported operator had the wrong number of operands.
    MalformedOperandCount {
        /// Operator name as source bytes.
        operator: Vec<u8>,
        /// Expected operand count.
        expected: usize,
        /// Observed operand count.
        got: usize,
    },
    /// A supported operator operand was not a single numeric lexeme.
    MalformedNumericOperand {
        /// Operator name as source bytes.
        operator: Vec<u8>,
        /// Zero-based operand index.
        operand_index: usize,
    },
    /// A supported operator operand was not a single PDF name lexeme.
    MalformedNameOperand {
        /// Operator name as source bytes.
        operator: Vec<u8>,
        /// Zero-based operand index.
        operand_index: usize,
    },
    /// A supported operator numeric operand parsed as NaN or infinity.
    NonFiniteNumericOperand {
        /// Operator name as source bytes.
        operator: Vec<u8>,
        /// Zero-based operand index.
        operand_index: usize,
    },
}

/// Stateful walker over assembled content-stream operator records.
#[derive(Debug, Clone)]
pub struct GraphicsStateWalker {
    state: GraphicsStateSnapshot,
    stack: Vec<GraphicsStateSnapshot>,
}

impl GraphicsStateWalker {
    /// Create a walker with the page-initial graphics state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: GraphicsStateSnapshot::page_default(),
            stack: Vec::new(),
        }
    }

    /// Return the current graphics-state snapshot.
    #[must_use]
    pub const fn state(&self) -> &GraphicsStateSnapshot {
        &self.state
    }

    /// Apply one operator record and emit its post-operator event.
    ///
    /// # Errors
    ///
    /// Returns a structured error for stack underflow, invalid source ranges,
    /// malformed operand counts, malformed numeric operands, or non-finite
    /// numeric operands in the supported operator set.
    pub fn step(
        &mut self,
        source: &[u8],
        index: usize,
        record: &OperatorRecord,
    ) -> Result<GraphicsStateEvent, GraphicsWalkError> {
        checked_source(source, record.range, record.range)?;
        let operator = checked_source(source, record.operator.range, record.range)?;
        let kind = self.event_kind(source, operator, record)?;
        Ok(GraphicsStateEvent {
            index,
            operator_range: record.operator.range,
            record_range: record.range,
            kind,
            state: self.state.clone(),
        })
    }

    fn event_kind(
        &mut self,
        source: &[u8],
        operator: &[u8],
        record: &OperatorRecord,
    ) -> Result<GraphicsStateEventKind, GraphicsWalkError> {
        match operator {
            b"q" => {
                expect_operands(operator, record, 0)?;
                self.stack.push(self.state.clone());
                Ok(GraphicsStateEventKind::Save)
            }
            b"Q" => {
                expect_operands(operator, record, 0)?;
                let Some(previous) = self.stack.pop() else {
                    return Err(GraphicsWalkError::new(
                        GraphicsWalkErrorKind::GraphicsStateStackUnderflow,
                        record.range,
                    ));
                };
                self.state = previous;
                Ok(GraphicsStateEventKind::Restore)
            }
            b"cm" => {
                let matrix = numeric_operands(source, operator, record, 6)?;
                self.state.ctm = concat_matrix(matrix, self.state.ctm);
                Ok(GraphicsStateEventKind::ConcatMatrix { matrix })
            }
            b"G" => {
                self.set_stroking_device_color(source, operator, record, ColorSpace::DeviceGray, 1)
            }
            b"g" => self.set_nonstroking_device_color(
                source,
                operator,
                record,
                ColorSpace::DeviceGray,
                1,
            ),
            b"RG" => {
                self.set_stroking_device_color(source, operator, record, ColorSpace::DeviceRgb, 3)
            }
            b"rg" => self.set_nonstroking_device_color(
                source,
                operator,
                record,
                ColorSpace::DeviceRgb,
                3,
            ),
            b"K" => {
                self.set_stroking_device_color(source, operator, record, ColorSpace::DeviceCmyk, 4)
            }
            b"k" => self.set_nonstroking_device_color(
                source,
                operator,
                record,
                ColorSpace::DeviceCmyk,
                4,
            ),
            b"Tr" => self.set_text_rendering_mode(source, operator, record),
            b"S" => Self::path_paint(operator, record, PathPaintKind::Stroke),
            b"s" => Self::path_paint(operator, record, PathPaintKind::CloseAndStroke),
            b"f" => Self::path_paint(operator, record, PathPaintKind::FillNonzero),
            b"F" => Self::path_paint(operator, record, PathPaintKind::FillObsolete),
            b"f*" => Self::path_paint(operator, record, PathPaintKind::FillEvenOdd),
            b"B" => Self::path_paint(operator, record, PathPaintKind::FillAndStrokeNonzero),
            b"B*" => Self::path_paint(operator, record, PathPaintKind::FillAndStrokeEvenOdd),
            b"b" => Self::path_paint(operator, record, PathPaintKind::CloseFillAndStrokeNonzero),
            b"b*" => Self::path_paint(operator, record, PathPaintKind::CloseFillAndStrokeEvenOdd),
            b"n" => Self::path_paint(operator, record, PathPaintKind::EndPath),
            b"Tj" => Self::text_show(
                operator,
                record,
                TextShowOperator::ShowText,
                1,
                self.state.text_rendering_mode,
            ),
            b"TJ" => Self::text_show(
                operator,
                record,
                TextShowOperator::ShowTextAdjusted,
                1,
                self.state.text_rendering_mode,
            ),
            b"'" => Self::text_show(
                operator,
                record,
                TextShowOperator::MoveNextLineAndShowText,
                1,
                self.state.text_rendering_mode,
            ),
            b"\"" => Self::text_show(
                operator,
                record,
                TextShowOperator::SetSpacingMoveNextLineAndShowText,
                3,
                self.state.text_rendering_mode,
            ),
            b"Do" => Ok(GraphicsStateEventKind::XObjectInvoke {
                name: name_operand(source, operator, record)?,
            }),
            _ => Ok(GraphicsStateEventKind::NoOp),
        }
    }

    fn set_stroking_device_color(
        &mut self,
        source: &[u8],
        operator: &[u8],
        record: &OperatorRecord,
        space: ColorSpace,
        count: usize,
    ) -> Result<GraphicsStateEventKind, GraphicsWalkError> {
        let color = device_color(source, operator, record, space, count)?;
        self.state.stroking_color = color.clone();
        Ok(GraphicsStateEventKind::SetStrokingDeviceColor { color })
    }

    fn set_nonstroking_device_color(
        &mut self,
        source: &[u8],
        operator: &[u8],
        record: &OperatorRecord,
        space: ColorSpace,
        count: usize,
    ) -> Result<GraphicsStateEventKind, GraphicsWalkError> {
        let color = device_color(source, operator, record, space, count)?;
        self.state.nonstroking_color = color.clone();
        Ok(GraphicsStateEventKind::SetNonstrokingDeviceColor { color })
    }

    fn set_text_rendering_mode(
        &mut self,
        source: &[u8],
        operator: &[u8],
        record: &OperatorRecord,
    ) -> Result<GraphicsStateEventKind, GraphicsWalkError> {
        let value = integer_operand(source, operator, record)?;
        let mode = TextRenderingMode::from_pdf_value(value);
        self.state.text_rendering_mode = mode;
        Ok(GraphicsStateEventKind::SetTextRenderingMode { mode })
    }

    fn path_paint(
        operator: &[u8],
        record: &OperatorRecord,
        paint: PathPaintKind,
    ) -> Result<GraphicsStateEventKind, GraphicsWalkError> {
        expect_operands(operator, record, 0)?;
        Ok(GraphicsStateEventKind::PathPaint { paint })
    }

    fn text_show(
        operator: &[u8],
        record: &OperatorRecord,
        show_operator: TextShowOperator,
        expected_operands: usize,
        rendering_mode: TextRenderingMode,
    ) -> Result<GraphicsStateEventKind, GraphicsWalkError> {
        expect_operands(operator, record, expected_operands)?;
        Ok(GraphicsStateEventKind::TextShow {
            operator: show_operator,
            rendering_mode,
        })
    }
}

impl Default for GraphicsStateWalker {
    fn default() -> Self {
        Self::new()
    }
}

/// Walk assembled operator records into ordered graphics-state events.
///
/// Unsupported operators emit explicit no-op events and leave state unchanged.
///
/// # Errors
///
/// Returns a structured walker error for malformed records in the supported
/// operator set or invalid source ranges.
pub fn walk_graphics_state(
    source: &[u8],
    records: &[OperatorRecord],
) -> Result<Vec<GraphicsStateEvent>, GraphicsWalkError> {
    let mut walker = GraphicsStateWalker::new();
    records
        .iter()
        .enumerate()
        .map(|(index, record)| walker.step(source, index, record))
        .collect()
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
    }
}

fn vector_object_digest(
    page: PageIndex,
    sequence: u32,
    scope: &ContentScope,
    event: &GraphicsStateEvent,
    paint: PathPaintKind,
    colors: &[ColorObservation],
) -> [u8; 32] {
    let mut digest = StableDigest::new();
    digest.push_bytes(b"presslint.vector.v1");
    digest.push_u32(page.0);
    digest.push_u32(sequence);
    digest.push_scope(scope);
    digest.push_usize(event.index);
    digest.push_range(event.record_range);
    digest.push_range(event.operator_range);
    digest.push_u8(path_paint_tag(paint));
    for color in colors {
        digest.push_color_observation(color);
    }
    digest.finish()
}

fn text_object_digest(
    page: PageIndex,
    sequence: u32,
    scope: &ContentScope,
    event: &GraphicsStateEvent,
    operator: TextShowOperator,
    rendering_mode: TextRenderingMode,
    colors: &[ColorObservation],
) -> [u8; 32] {
    let mut digest = StableDigest::new();
    digest.push_bytes(b"presslint.text.v1");
    digest.push_u32(page.0);
    digest.push_u32(sequence);
    digest.push_scope(scope);
    digest.push_usize(event.index);
    digest.push_range(event.record_range);
    digest.push_range(event.operator_range);
    digest.push_u8(text_show_operator_tag(operator));
    digest.push_text_rendering_mode(rendering_mode);
    for color in colors {
        digest.push_color_observation(color);
    }
    digest.finish()
}

fn image_object_digest(
    page: PageIndex,
    sequence: u32,
    scope: &ContentScope,
    event: &GraphicsStateEvent,
    name: &PdfName,
    colors: &[ColorObservation],
) -> [u8; 32] {
    let mut digest = StableDigest::new();
    digest.push_bytes(b"presslint.image.v1");
    digest.push_u32(page.0);
    digest.push_u32(sequence);
    digest.push_scope(scope);
    digest.push_usize(event.index);
    digest.push_range(event.record_range);
    digest.push_range(event.operator_range);
    digest.push_bytes(&name.0);
    for color in colors {
        digest.push_color_observation(color);
    }
    digest.finish()
}

#[derive(Debug, Clone)]
struct StableDigest {
    lanes: [u64; 4],
}

impl StableDigest {
    const fn new() -> Self {
        Self {
            lanes: [
                0xcbf2_9ce4_8422_2325,
                0x8422_2325_cbf2_9ce4,
                0x9e37_79b1_85eb_ca87,
                0xc2b2_ae3d_27d4_eb4f,
            ],
        }
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        self.push_usize(bytes.len());
        for byte in bytes {
            self.push_u8(*byte);
        }
    }

    fn push_u8(&mut self, value: u8) {
        for (index, lane) in self.lanes.iter_mut().enumerate() {
            *lane ^= u64::from(value).wrapping_add((index as u64) << 8);
            *lane = lane.wrapping_mul(0x0100_0000_01b3);
            *lane ^= *lane >> 32;
        }
    }

    fn push_u32(&mut self, value: u32) {
        for byte in value.to_le_bytes() {
            self.push_u8(byte);
        }
    }

    fn push_u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.push_u8(byte);
        }
    }

    fn push_usize(&mut self, value: usize) {
        self.push_u64(usize_to_u64(value));
    }

    fn push_f64(&mut self, value: f64) {
        self.push_u64(value.to_bits());
    }

    fn push_range(&mut self, range: ByteRange) {
        self.push_usize(range.start);
        self.push_usize(range.end);
    }

    fn push_scope(&mut self, scope: &ContentScope) {
        match scope {
            ContentScope::Page => self.push_u8(0),
            ContentScope::FormXObject { name } => {
                self.push_u8(1);
                self.push_bytes(&name.0);
            }
            ContentScope::AnnotationAppearance => self.push_u8(2),
        }
    }

    fn push_color_observation(&mut self, color: &ColorObservation) {
        self.push_u8(color_usage_tag(color.usage));
        self.push_u8(color_space_tag(&color.space));
        if let ColorSpace::Resource(name) = &color.space {
            self.push_bytes(&name.0);
        }
        self.push_usize(color.components.len());
        for component in &color.components {
            self.push_f64(*component);
        }
        match &color.spot_name {
            Some(name) => {
                self.push_u8(1);
                self.push_bytes(&name.0);
            }
            None => self.push_u8(0),
        }
    }

    fn push_text_rendering_mode(&mut self, mode: TextRenderingMode) {
        match mode {
            TextRenderingMode::Fill => self.push_u8(0),
            TextRenderingMode::Stroke => self.push_u8(1),
            TextRenderingMode::FillThenStroke => self.push_u8(2),
            TextRenderingMode::Invisible => self.push_u8(3),
            TextRenderingMode::Unsupported { value } => {
                self.push_u8(4);
                self.push_i32(value);
            }
        }
    }

    fn push_i32(&mut self, value: i32) {
        for byte in value.to_le_bytes() {
            self.push_u8(byte);
        }
    }

    fn finish(self) -> [u8; 32] {
        let mut out = [0; 32];
        for (chunk, lane) in out.chunks_exact_mut(8).zip(self.lanes) {
            chunk.copy_from_slice(&lane.to_le_bytes());
        }
        out
    }
}

fn usize_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

const fn path_paint_tag(paint: PathPaintKind) -> u8 {
    match paint {
        PathPaintKind::Stroke => 0,
        PathPaintKind::CloseAndStroke => 1,
        PathPaintKind::FillNonzero => 2,
        PathPaintKind::FillObsolete => 3,
        PathPaintKind::FillEvenOdd => 4,
        PathPaintKind::FillAndStrokeNonzero => 5,
        PathPaintKind::FillAndStrokeEvenOdd => 6,
        PathPaintKind::CloseFillAndStrokeNonzero => 7,
        PathPaintKind::CloseFillAndStrokeEvenOdd => 8,
        PathPaintKind::EndPath => 9,
    }
}

const fn text_show_operator_tag(operator: TextShowOperator) -> u8 {
    match operator {
        TextShowOperator::ShowText => 0,
        TextShowOperator::ShowTextAdjusted => 1,
        TextShowOperator::MoveNextLineAndShowText => 2,
        TextShowOperator::SetSpacingMoveNextLineAndShowText => 3,
    }
}

const fn color_usage_tag(usage: ColorUsage) -> u8 {
    match usage {
        ColorUsage::Fill => 0,
        ColorUsage::Stroke => 1,
        ColorUsage::Image => 2,
        ColorUsage::Shading => 3,
    }
}

const fn color_space_tag(space: &ColorSpace) -> u8 {
    match space {
        ColorSpace::DeviceGray => 0,
        ColorSpace::DeviceRgb => 1,
        ColorSpace::DeviceCmyk => 2,
        ColorSpace::IccBased => 3,
        ColorSpace::Lab => 4,
        ColorSpace::CalGray => 5,
        ColorSpace::CalRgb => 6,
        ColorSpace::Indexed => 7,
        ColorSpace::Separation => 8,
        ColorSpace::DeviceN => 9,
        ColorSpace::Pattern => 10,
        ColorSpace::Resource(_) => 11,
        ColorSpace::Unknown => 12,
    }
}

const IDENTITY_CTM: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn checked_source(
    source: &[u8],
    range: ByteRange,
    error_range: ByteRange,
) -> Result<&[u8], GraphicsWalkError> {
    source.get(range.start..range.end).ok_or_else(|| {
        GraphicsWalkError::new(GraphicsWalkErrorKind::InvalidSourceRange, error_range)
    })
}

fn device_color(
    source: &[u8],
    operator: &[u8],
    record: &OperatorRecord,
    space: ColorSpace,
    count: usize,
) -> Result<GraphicsDeviceColor, GraphicsWalkError> {
    Ok(GraphicsDeviceColor::new(
        space,
        numeric_operands_vec(source, operator, record, count)?,
    ))
}

fn expect_operands(
    operator: &[u8],
    record: &OperatorRecord,
    expected: usize,
) -> Result<(), GraphicsWalkError> {
    let got = record.operands.len();
    if got == expected {
        Ok(())
    } else {
        Err(GraphicsWalkError::new(
            GraphicsWalkErrorKind::MalformedOperandCount {
                operator: operator.to_vec(),
                expected,
                got,
            },
            record.range,
        ))
    }
}

fn numeric_operands(
    source: &[u8],
    operator: &[u8],
    record: &OperatorRecord,
    expected: usize,
) -> Result<[f64; 6], GraphicsWalkError> {
    let operands = numeric_operands_vec(source, operator, record, expected)?;
    Ok([
        operands[0],
        operands[1],
        operands[2],
        operands[3],
        operands[4],
        operands[5],
    ])
}

fn integer_operand(
    source: &[u8],
    operator: &[u8],
    record: &OperatorRecord,
) -> Result<i32, GraphicsWalkError> {
    let operands = numeric_operands_vec(source, operator, record, 1)?;
    let value = operands[0];
    if value.fract() != 0.0 || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        return Err(GraphicsWalkError::new(
            GraphicsWalkErrorKind::MalformedNumericOperand {
                operator: operator.to_vec(),
                operand_index: 0,
            },
            record.operands[0].range,
        ));
    }
    #[allow(clippy::cast_possible_truncation)]
    Ok(value as i32)
}

fn name_operand(
    source: &[u8],
    operator: &[u8],
    record: &OperatorRecord,
) -> Result<PdfName, GraphicsWalkError> {
    expect_operands(operator, record, 1)?;
    let operand = &record.operands[0];
    if operand.tokens.len() != 1 {
        return Err(GraphicsWalkError::new(
            GraphicsWalkErrorKind::MalformedNameOperand {
                operator: operator.to_vec(),
                operand_index: 0,
            },
            operand.range,
        ));
    }
    let bytes = checked_source(source, operand.range, operand.range)?;
    if bytes.len() <= 1 || bytes[0] != b'/' {
        return Err(GraphicsWalkError::new(
            GraphicsWalkErrorKind::MalformedNameOperand {
                operator: operator.to_vec(),
                operand_index: 0,
            },
            operand.range,
        ));
    }
    Ok(PdfName(bytes[1..].to_vec()))
}

fn numeric_operands_vec(
    source: &[u8],
    operator: &[u8],
    record: &OperatorRecord,
    expected: usize,
) -> Result<Vec<f64>, GraphicsWalkError> {
    expect_operands(operator, record, expected)?;
    record
        .operands
        .iter()
        .enumerate()
        .map(|(operand_index, operand)| {
            if operand.tokens.len() != 1 {
                return Err(GraphicsWalkError::new(
                    GraphicsWalkErrorKind::MalformedNumericOperand {
                        operator: operator.to_vec(),
                        operand_index,
                    },
                    operand.range,
                ));
            }
            let bytes = checked_source(source, operand.range, operand.range)?;
            let Ok(text) = core::str::from_utf8(bytes) else {
                return Err(GraphicsWalkError::new(
                    GraphicsWalkErrorKind::MalformedNumericOperand {
                        operator: operator.to_vec(),
                        operand_index,
                    },
                    operand.range,
                ));
            };
            let Ok(value) = text.parse::<f64>() else {
                return Err(GraphicsWalkError::new(
                    GraphicsWalkErrorKind::MalformedNumericOperand {
                        operator: operator.to_vec(),
                        operand_index,
                    },
                    operand.range,
                ));
            };
            if !value.is_finite() {
                return Err(GraphicsWalkError::new(
                    GraphicsWalkErrorKind::NonFiniteNumericOperand {
                        operator: operator.to_vec(),
                        operand_index,
                    },
                    operand.range,
                ));
            }
            Ok(value)
        })
        .collect()
}

#[allow(clippy::suboptimal_flops)]
fn concat_matrix(m: [f64; 6], n: [f64; 6]) -> [f64; 6] {
    let [a1, b1, c1, d1, e1, f1] = m;
    let [a2, b2, c2, d2, e2, f2] = n;
    [
        a1 * a2 + b1 * c2,
        a1 * b2 + b1 * d2,
        c1 * a2 + d1 * c2,
        c1 * b2 + d1 * d2,
        e1 * a2 + f1 * c2 + e2,
        e1 * b2 + f1 * d2 + f2,
    ]
}

#[cfg(test)]
mod tests {
    use presslint_core::{
        ByteRange, ColorSpace, ColorUsage, ContentScope, EditCapability, ObjectKind, PageIndex,
        PdfName,
    };
    use presslint_syntax::{OperatorRecord, TokenRef, assemble_operators, tokenize};

    use super::{
        GraphicsStateEventKind, GraphicsStateWalker, GraphicsWalkError, GraphicsWalkErrorKind,
        Inventory, PathPaintKind, TextRenderingMode, TextShowOperator, build_image_inventory,
        build_text_inventory, build_vector_inventory, walk_graphics_state,
    };

    fn walk(input: &[u8]) -> Result<Vec<super::GraphicsStateEvent>, GraphicsWalkError> {
        let tokens = tokenize(input).map_err(|error| {
            GraphicsWalkError::new(GraphicsWalkErrorKind::InvalidSourceRange, error.range)
        })?;
        let assembled = assemble_operators(&tokens).map_err(|error| {
            let range = match error {
                presslint_syntax::AssembleError::InvalidTokenRange { range, .. }
                | presslint_syntax::AssembleError::TrailingOperands { range, .. }
                | presslint_syntax::AssembleError::UnmatchedArrayClose { range, .. }
                | presslint_syntax::AssembleError::UnmatchedDictionaryClose { range, .. }
                | presslint_syntax::AssembleError::MismatchedDelimiter { range, .. }
                | presslint_syntax::AssembleError::UnterminatedCompositeOperand { range, .. }
                | presslint_syntax::AssembleError::OperatorInsideCompositeOperand {
                    range, ..
                }
                | presslint_syntax::AssembleError::UnexpectedKeyword { range, .. } => range,
            };
            GraphicsWalkError::new(GraphicsWalkErrorKind::InvalidSourceRange, range)
        })?;
        walk_graphics_state(input, &assembled.records)
    }

    fn vector_inventory(input: &[u8], scope: &ContentScope) -> Result<Inventory, String> {
        let tokens = tokenize(input).map_err(|error| format!("{error:?}"))?;
        let assembled = assemble_operators(&tokens).map_err(|error| format!("{error:?}"))?;
        build_vector_inventory(input, &assembled.records, PageIndex(2), scope)
            .map_err(|error| format!("{error:?}"))
    }

    fn text_inventory(input: &[u8], scope: &ContentScope) -> Result<Inventory, String> {
        let tokens = tokenize(input).map_err(|error| format!("{error:?}"))?;
        let assembled = assemble_operators(&tokens).map_err(|error| format!("{error:?}"))?;
        build_text_inventory(input, &assembled.records, PageIndex(2), scope)
            .map_err(|error| format!("{error:?}"))
    }

    fn image_inventory(
        input: &[u8],
        scope: &ContentScope,
        image_names: &[PdfName],
    ) -> Result<Inventory, String> {
        let tokens = tokenize(input).map_err(|error| format!("{error:?}"))?;
        let assembled = assemble_operators(&tokens).map_err(|error| format!("{error:?}"))?;
        build_image_inventory(input, &assembled.records, PageIndex(2), scope, image_names)
            .map_err(|error| format!("{error:?}"))
    }

    fn assert_ctm_near(actual: [f64; 6], expected: [f64; 6]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn save_restore_returns_to_saved_colour_state() -> Result<(), String> {
        let events = walk(b"1 0 0 rg q 0.5 g Q S").map_err(|error| format!("{error:?}"))?;
        let final_event = events.last().ok_or("missing final event")?;

        assert_eq!(
            final_event.state.nonstroking_color.space,
            ColorSpace::DeviceRgb
        );
        assert_eq!(
            final_event.state.nonstroking_color.components,
            vec![1.0, 0.0, 0.0]
        );
        assert_eq!(
            final_event.kind,
            GraphicsStateEventKind::PathPaint {
                paint: PathPaintKind::Stroke,
            }
        );
        Ok(())
    }

    #[test]
    fn cm_concatenates_current_transformation_matrix() -> Result<(), String> {
        let events =
            walk(b"1 0 0 1 10 0 cm 1 0 0 1 0 5 cm").map_err(|error| format!("{error:?}"))?;
        let final_event = events.last().ok_or("missing final event")?;

        assert_ctm_near(final_event.state.ctm, [1.0, 0.0, 0.0, 1.0, 10.0, 5.0]);
        Ok(())
    }

    #[test]
    fn device_colour_observations_track_stroke_and_fill() -> Result<(), String> {
        let events =
            walk(b"0.1 0.2 0.3 RG 0.4 0.5 0.6 0.7 k B").map_err(|error| format!("{error:?}"))?;
        let final_event = events.last().ok_or("missing final event")?;
        let stroke = final_event.state.stroke_observation();
        let fill = final_event.state.fill_observation();

        assert_eq!(stroke.usage, ColorUsage::Stroke);
        assert_eq!(stroke.space, ColorSpace::DeviceRgb);
        assert_eq!(stroke.components, vec![0.1, 0.2, 0.3]);
        assert_eq!(fill.usage, ColorUsage::Fill);
        assert_eq!(fill.space, ColorSpace::DeviceCmyk);
        assert_eq!(fill.components, vec![0.4, 0.5, 0.6, 0.7]);
        Ok(())
    }

    #[test]
    fn path_paint_event_carries_post_operator_snapshot_and_provenance() -> Result<(), String> {
        let events = walk(b"0.25 g 2 0 0 2 8 9 cm f*").map_err(|error| format!("{error:?}"))?;
        let event = events.last().ok_or("missing path event")?;

        assert_eq!(
            event.kind,
            GraphicsStateEventKind::PathPaint {
                paint: PathPaintKind::FillEvenOdd,
            }
        );
        assert_ctm_near(event.state.ctm, [2.0, 0.0, 0.0, 2.0, 8.0, 9.0]);
        assert_eq!(
            event.state.nonstroking_color,
            super::GraphicsDeviceColor::new(ColorSpace::DeviceGray, vec![0.25])
        );
        assert_eq!(event.record_range.start, 22);
        assert_eq!(event.operator_range.end, 24);
        Ok(())
    }

    #[test]
    fn unsupported_operator_emits_noop_event() -> Result<(), String> {
        let events = walk(b"10 20 m").map_err(|error| format!("{error:?}"))?;

        assert_eq!(events[0].kind, GraphicsStateEventKind::NoOp);
        assert_eq!(
            events[0].state,
            super::GraphicsStateSnapshot::page_default()
        );
        Ok(())
    }

    #[test]
    fn do_operator_emits_xobject_invocation_event() -> Result<(), String> {
        let events = walk(b"/Im1 Do").map_err(|error| format!("{error:?}"))?;

        assert_eq!(
            events[0].kind,
            GraphicsStateEventKind::XObjectInvoke {
                name: PdfName(b"Im1".to_vec()),
            }
        );
        assert_eq!(events[0].record_range, ByteRange { start: 0, end: 7 });
        assert_eq!(
            events[0].state,
            super::GraphicsStateSnapshot::page_default()
        );
        Ok(())
    }

    #[test]
    fn invalid_record_range_returns_structured_error() -> Result<(), String> {
        let mut walker = GraphicsStateWalker::new();
        let record = OperatorRecord {
            operator: TokenRef {
                token_index: 0,
                range: presslint_core::ByteRange { start: 0, end: 1 },
            },
            operands: Vec::new(),
            trivia: Vec::new(),
            range: presslint_core::ByteRange { start: 2, end: 1 },
        };

        let Err(err) = walker.step(b"m", 0, &record) else {
            return Err("invalid record range should fail".to_string());
        };

        assert_eq!(
            err,
            GraphicsWalkError::new(
                GraphicsWalkErrorKind::InvalidSourceRange,
                presslint_core::ByteRange { start: 2, end: 1 },
            )
        );
        Ok(())
    }

    #[test]
    fn stack_underflow_returns_structured_error() -> Result<(), String> {
        let Err(err) = walk(b"Q") else {
            return Err("Q without q should fail".to_string());
        };

        assert_eq!(
            err,
            GraphicsWalkError::new(
                GraphicsWalkErrorKind::GraphicsStateStackUnderflow,
                presslint_core::ByteRange { start: 0, end: 1 },
            )
        );
        Ok(())
    }

    #[test]
    fn malformed_operand_count_returns_structured_error() -> Result<(), String> {
        let Err(err) = walk(b"1 2 RG") else {
            return Err("RG with two operands should fail".to_string());
        };

        assert_eq!(
            err.kind,
            GraphicsWalkErrorKind::MalformedOperandCount {
                operator: b"RG".to_vec(),
                expected: 3,
                got: 2,
            }
        );
        Ok(())
    }

    #[test]
    fn malformed_numeric_operand_returns_structured_error() -> Result<(), String> {
        let Err(err) = walk(b"/Name g") else {
            return Err("name operand should fail".to_string());
        };

        assert_eq!(
            err.kind,
            GraphicsWalkErrorKind::MalformedNumericOperand {
                operator: b"g".to_vec(),
                operand_index: 0,
            }
        );
        Ok(())
    }

    #[test]
    fn malformed_do_operand_count_returns_structured_error() -> Result<(), String> {
        let Err(err) = walk(b"/Im1 /Im2 Do") else {
            return Err("Do with two operands should fail".to_string());
        };

        assert_eq!(
            err.kind,
            GraphicsWalkErrorKind::MalformedOperandCount {
                operator: b"Do".to_vec(),
                expected: 1,
                got: 2,
            }
        );
        Ok(())
    }

    #[test]
    fn malformed_do_name_operand_returns_structured_error() -> Result<(), String> {
        let Err(err) = walk(b"42 Do") else {
            return Err("Do with non-name operand should fail".to_string());
        };

        assert_eq!(
            err.kind,
            GraphicsWalkErrorKind::MalformedNameOperand {
                operator: b"Do".to_vec(),
                operand_index: 0,
            }
        );
        assert_eq!(err.range, ByteRange { start: 0, end: 2 });
        Ok(())
    }

    #[test]
    fn vector_inventory_attaches_color_observations_by_paint_usage() -> Result<(), String> {
        let inventory = vector_inventory(
            b"0.1 0.2 0.3 RG S 0.4 g f 0 0 0 1 K 0.5 0.6 0.7 rg B",
            &ContentScope::Page,
        )?;

        assert_eq!(inventory.entries.len(), 3);
        assert_eq!(inventory.entries[0].kind, ObjectKind::Vector);
        assert_eq!(inventory.entries[0].colors.len(), 1);
        assert_eq!(inventory.entries[0].colors[0].usage, ColorUsage::Stroke);
        assert_eq!(inventory.entries[0].colors[0].space, ColorSpace::DeviceRgb);
        assert_eq!(
            inventory.entries[0].colors[0].components,
            vec![0.1, 0.2, 0.3]
        );

        assert_eq!(inventory.entries[1].colors.len(), 1);
        assert_eq!(inventory.entries[1].colors[0].usage, ColorUsage::Fill);
        assert_eq!(inventory.entries[1].colors[0].space, ColorSpace::DeviceGray);
        assert_eq!(inventory.entries[1].colors[0].components, vec![0.4]);

        assert_eq!(inventory.entries[2].colors.len(), 2);
        assert_eq!(inventory.entries[2].colors[0].usage, ColorUsage::Stroke);
        assert_eq!(inventory.entries[2].colors[0].space, ColorSpace::DeviceCmyk);
        assert_eq!(
            inventory.entries[2].colors[0].components,
            vec![0.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(inventory.entries[2].colors[1].usage, ColorUsage::Fill);
        assert_eq!(inventory.entries[2].colors[1].space, ColorSpace::DeviceRgb);
        assert_eq!(
            inventory.entries[2].colors[1].components,
            vec![0.5, 0.6, 0.7]
        );
        Ok(())
    }

    #[test]
    fn vector_inventory_carries_provenance_and_edit_capability() -> Result<(), String> {
        let scope = ContentScope::FormXObject {
            name: PdfName(b"Logo".to_vec()),
        };
        let inventory = vector_inventory(b"0.25 g f", &scope)?;
        let entry = inventory.entries.first().ok_or("missing vector entry")?;

        assert_eq!(entry.provenance.page, PageIndex(2));
        assert_eq!(entry.provenance.scope, scope);
        assert_eq!(entry.provenance.range, Some(ByteRange { start: 7, end: 8 }));
        assert_eq!(entry.bounds, None);
        assert_eq!(
            entry.capabilities,
            vec![EditCapability::RewriteColorOperand]
        );
        Ok(())
    }

    #[test]
    fn vector_inventory_object_ids_are_deterministic() -> Result<(), String> {
        let first = vector_inventory(b"S f B", &ContentScope::Page)?;
        let second = vector_inventory(b"S f B", &ContentScope::Page)?;

        assert_eq!(first, second);
        assert_eq!(first.entries[0].id.page, PageIndex(2));
        assert_eq!(first.entries[0].id.sequence, 0);
        assert_eq!(first.entries[1].id.sequence, 1);
        assert_eq!(first.entries[2].id.sequence, 2);
        assert_ne!(first.entries[0].id.digest, first.entries[1].id.digest);
        assert_ne!(first.entries[1].id.digest, first.entries[2].id.digest);
        Ok(())
    }

    #[test]
    fn vector_inventory_skips_noop_and_end_path_events() -> Result<(), String> {
        let inventory = vector_inventory(b"10 20 m n", &ContentScope::Page)?;

        assert!(inventory.is_empty());
        Ok(())
    }

    #[test]
    fn default_filled_text_inventory_uses_nonstroking_color() -> Result<(), String> {
        let scope = ContentScope::FormXObject {
            name: PdfName(b"Body".to_vec()),
        };
        let inventory = text_inventory(b"0.2 0.3 0.4 rg (Hello) Tj", &scope)?;
        let entry = inventory.entries.first().ok_or("missing text entry")?;

        assert_eq!(inventory.entries.len(), 1);
        assert_eq!(entry.kind, ObjectKind::Text);
        assert_eq!(entry.provenance.page, PageIndex(2));
        assert_eq!(entry.provenance.scope, scope);
        assert_eq!(
            entry.provenance.range,
            Some(ByteRange { start: 15, end: 25 })
        );
        assert_eq!(entry.bounds, None);
        assert_eq!(entry.colors.len(), 1);
        assert_eq!(entry.colors[0].usage, ColorUsage::Fill);
        assert_eq!(entry.colors[0].space, ColorSpace::DeviceRgb);
        assert_eq!(entry.colors[0].components, vec![0.2, 0.3, 0.4]);
        assert_eq!(
            entry.capabilities,
            vec![
                EditCapability::RewriteColorOperand,
                EditCapability::AddTextSpreadStroke,
            ]
        );
        Ok(())
    }

    #[test]
    fn stroked_text_rendering_mode_uses_stroking_color() -> Result<(), String> {
        let inventory = text_inventory(b"0.7 G 1 Tr (Outline) Tj", &ContentScope::Page)?;
        let entry = inventory.entries.first().ok_or("missing text entry")?;

        assert_eq!(entry.colors.len(), 1);
        assert_eq!(entry.colors[0].usage, ColorUsage::Stroke);
        assert_eq!(entry.colors[0].space, ColorSpace::DeviceGray);
        assert_eq!(entry.colors[0].components, vec![0.7]);
        assert_eq!(
            entry.capabilities,
            vec![
                EditCapability::RewriteColorOperand,
                EditCapability::AddTextSpreadStroke,
            ]
        );
        Ok(())
    }

    #[test]
    fn fill_and_stroke_text_rendering_mode_uses_both_colors() -> Result<(), String> {
        let inventory = text_inventory(
            b"0.1 0.2 0.3 RG 0.4 0.5 0.6 rg 2 Tr [(Hi) 20 (There)] TJ",
            &ContentScope::Page,
        )?;
        let entry = inventory.entries.first().ok_or("missing text entry")?;

        assert_eq!(entry.colors.len(), 2);
        assert_eq!(entry.colors[0].usage, ColorUsage::Stroke);
        assert_eq!(entry.colors[0].space, ColorSpace::DeviceRgb);
        assert_eq!(entry.colors[0].components, vec![0.1, 0.2, 0.3]);
        assert_eq!(entry.colors[1].usage, ColorUsage::Fill);
        assert_eq!(entry.colors[1].space, ColorSpace::DeviceRgb);
        assert_eq!(entry.colors[1].components, vec![0.4, 0.5, 0.6]);
        Ok(())
    }

    #[test]
    fn invisible_text_is_represented_without_color_edit_capability() -> Result<(), String> {
        let inventory = text_inventory(b"3 Tr (Hidden) Tj", &ContentScope::Page)?;
        let entry = inventory.entries.first().ok_or("missing text entry")?;

        assert_eq!(entry.kind, ObjectKind::Text);
        assert!(entry.colors.is_empty());
        assert!(entry.capabilities.is_empty());
        Ok(())
    }

    #[test]
    fn unsupported_text_rendering_mode_is_conservative() -> Result<(), String> {
        let events = walk(b"4 Tr (ClipFill) Tj").map_err(|error| format!("{error:?}"))?;
        assert_eq!(
            events[0].kind,
            GraphicsStateEventKind::SetTextRenderingMode {
                mode: TextRenderingMode::Unsupported { value: 4 },
            }
        );
        assert_eq!(
            events[1].kind,
            GraphicsStateEventKind::TextShow {
                operator: TextShowOperator::ShowText,
                rendering_mode: TextRenderingMode::Unsupported { value: 4 },
            }
        );

        let inventory = text_inventory(b"4 Tr (ClipFill) Tj", &ContentScope::Page)?;
        let entry = inventory.entries.first().ok_or("missing text entry")?;

        assert_eq!(entry.kind, ObjectKind::Text);
        assert!(entry.colors.is_empty());
        assert!(entry.capabilities.is_empty());
        Ok(())
    }

    #[test]
    fn quoted_text_showing_operators_are_inventoried() -> Result<(), String> {
        let inventory = text_inventory(b"(Next) ' 4 2 (Spaced) \"", &ContentScope::Page)?;

        assert_eq!(inventory.entries.len(), 2);
        assert_eq!(inventory.entries[0].id.sequence, 0);
        assert_eq!(
            inventory.entries[0].provenance.range,
            Some(ByteRange { start: 0, end: 8 })
        );
        assert_eq!(inventory.entries[1].id.sequence, 1);
        assert_eq!(
            inventory.entries[1].provenance.range,
            Some(ByteRange { start: 9, end: 23 })
        );
        Ok(())
    }

    #[test]
    fn text_inventory_object_ids_are_deterministic() -> Result<(), String> {
        let first = text_inventory(b"(A) Tj 1 Tr (B) Tj 2 Tr [(C)] TJ", &ContentScope::Page)?;
        let second = text_inventory(b"(A) Tj 1 Tr (B) Tj 2 Tr [(C)] TJ", &ContentScope::Page)?;

        assert_eq!(first, second);
        assert_eq!(first.entries[0].id.page, PageIndex(2));
        assert_eq!(first.entries[0].id.sequence, 0);
        assert_eq!(first.entries[1].id.sequence, 1);
        assert_eq!(first.entries[2].id.sequence, 2);
        assert_ne!(first.entries[0].id.digest, first.entries[1].id.digest);
        assert_ne!(first.entries[1].id.digest, first.entries[2].id.digest);
        Ok(())
    }

    #[test]
    fn image_inventory_includes_only_declared_image_xobject_names() -> Result<(), String> {
        let inventory = image_inventory(
            b"/Im1 Do /Fm1 Do /Im2 Do",
            &ContentScope::Page,
            &[
                PdfName(b"Im2".to_vec()),
                PdfName(b"Missing".to_vec()),
                PdfName(b"Im1".to_vec()),
            ],
        )?;

        assert_eq!(inventory.entries.len(), 2);
        assert_eq!(inventory.entries[0].kind, ObjectKind::Image);
        assert_eq!(inventory.entries[0].id.sequence, 0);
        assert_eq!(
            inventory.entries[0].provenance.range,
            Some(ByteRange { start: 0, end: 7 })
        );
        assert_eq!(inventory.entries[1].id.sequence, 1);
        assert_eq!(
            inventory.entries[1].provenance.range,
            Some(ByteRange { start: 16, end: 23 })
        );
        Ok(())
    }

    #[test]
    fn image_inventory_carries_conservative_observation_and_read_only_capability()
    -> Result<(), String> {
        let scope = ContentScope::AnnotationAppearance;
        let inventory = image_inventory(b"q /Photo Do Q", &scope, &[PdfName(b"Photo".to_vec())])?;
        let entry = inventory.entries.first().ok_or("missing image entry")?;

        assert_eq!(entry.provenance.page, PageIndex(2));
        assert_eq!(entry.provenance.scope, scope);
        assert_eq!(
            entry.provenance.range,
            Some(ByteRange { start: 2, end: 11 })
        );
        assert_eq!(entry.bounds, None);
        assert_eq!(entry.colors.len(), 1);
        assert_eq!(entry.colors[0].usage, ColorUsage::Image);
        assert_eq!(entry.colors[0].space, ColorSpace::Unknown);
        assert!(entry.colors[0].components.is_empty());
        assert_eq!(entry.colors[0].spot_name, None);
        assert_eq!(entry.capabilities, vec![EditCapability::ReadOnly]);
        Ok(())
    }

    #[test]
    fn non_image_xobject_invocations_are_skipped_by_image_inventory() -> Result<(), String> {
        let inventory = image_inventory(
            b"/Form Do /PostScript Do",
            &ContentScope::Page,
            &[PdfName(b"Image".to_vec())],
        )?;

        assert!(inventory.is_empty());
        Ok(())
    }

    #[test]
    fn image_inventory_object_ids_are_deterministic() -> Result<(), String> {
        let names = [PdfName(b"Im1".to_vec()), PdfName(b"Im2".to_vec())];
        let first = image_inventory(b"/Im1 Do /Other Do /Im2 Do", &ContentScope::Page, &names)?;
        let second = image_inventory(b"/Im1 Do /Other Do /Im2 Do", &ContentScope::Page, &names)?;

        assert_eq!(first, second);
        assert_eq!(first.entries[0].id.page, PageIndex(2));
        assert_eq!(first.entries[0].id.sequence, 0);
        assert_eq!(first.entries[1].id.sequence, 1);
        assert_ne!(first.entries[0].id.digest, first.entries[1].id.digest);
        Ok(())
    }
}
