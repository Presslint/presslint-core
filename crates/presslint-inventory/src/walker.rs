use presslint_core::{ByteRange, ColorObservation, ColorSpace, ColorUsage, PdfName};
use presslint_syntax::OperatorRecord;
use serde::{Deserialize, Serialize};

use crate::operands::{
    checked_source, concat_matrix, device_color, expect_operands, integer_operand, name_operand,
    numeric_operands,
};

const IDENTITY_CTM: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

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
