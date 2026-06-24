//! Page-object inventory model and graphics-state observations.

#![forbid(unsafe_code)]

use presslint_core::{
    BoundingBox, ByteRange, ColorObservation, ColorSpace, ColorUsage, EditCapability, ObjectId,
    ObjectKind, Provenance,
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
}

impl GraphicsStateSnapshot {
    /// Return the page-initial graphics state for this walker slice.
    #[must_use]
    pub fn page_default() -> Self {
        Self {
            ctm: IDENTITY_CTM,
            stroking_color: GraphicsDeviceColor::new(ColorSpace::DeviceGray, vec![0.0]),
            nonstroking_color: GraphicsDeviceColor::new(ColorSpace::DeviceGray, vec![0.0]),
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

    fn path_paint(
        operator: &[u8],
        record: &OperatorRecord,
        paint: PathPaintKind,
    ) -> Result<GraphicsStateEventKind, GraphicsWalkError> {
        expect_operands(operator, record, 0)?;
        Ok(GraphicsStateEventKind::PathPaint { paint })
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
    use presslint_syntax::{OperatorRecord, TokenRef, assemble_operators, tokenize};

    use super::{
        ColorSpace, ColorUsage, GraphicsStateEventKind, GraphicsStateWalker, GraphicsWalkError,
        GraphicsWalkErrorKind, PathPaintKind, walk_graphics_state,
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
}
