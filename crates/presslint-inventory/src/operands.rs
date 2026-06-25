use presslint_core::{ByteRange, ColorSpace, PdfName};
use presslint_syntax::OperatorRecord;

use crate::walker::{GraphicsDeviceColor, GraphicsWalkError, GraphicsWalkErrorKind};

pub fn checked_source(
    source: &[u8],
    range: ByteRange,
    error_range: ByteRange,
) -> Result<&[u8], GraphicsWalkError> {
    source.get(range.start..range.end).ok_or_else(|| {
        GraphicsWalkError::new(GraphicsWalkErrorKind::InvalidSourceRange, error_range)
    })
}

pub fn device_color(
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

pub fn expect_operands(
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

pub fn numeric_operands(
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

pub fn integer_operand(
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

pub fn name_operand(
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
pub fn concat_matrix(m: [f64; 6], n: [f64; 6]) -> [f64; 6] {
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
