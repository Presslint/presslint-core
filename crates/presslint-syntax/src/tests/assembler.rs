use presslint_types::ByteRange;

use crate::{AssembleError, TokenRef, assemble_operators, serialize_tokens_unmodified, tokenize};

#[test]
fn assembler_groups_operands_with_operator_tokens() -> Result<(), String> {
    let input = b"q\n/DeviceRGB cs\n1 0 0 rg\n";
    let tokens = tokenize(input).map_err(|error| format!("{error:?}"))?;
    let assembled = assemble_operators(&tokens).map_err(|error| format!("{error:?}"))?;

    assert_eq!(assembled.records.len(), 3);
    assert_eq!(
        assembled.records[0].operator.range,
        ByteRange { start: 0, end: 1 }
    );
    assert!(assembled.records[0].operands.is_empty());
    assert_eq!(
        assembled.records[1].operands[0].range,
        ByteRange { start: 2, end: 12 }
    );
    assert_eq!(
        assembled.records[1].operator.range,
        ByteRange { start: 13, end: 15 }
    );
    assert_eq!(assembled.records[1].range, ByteRange { start: 2, end: 15 });
    assert_eq!(
        assembled.records[2]
            .operands
            .iter()
            .map(|operand| operand.range)
            .collect::<Vec<_>>(),
        vec![
            ByteRange { start: 16, end: 17 },
            ByteRange { start: 18, end: 19 },
            ByteRange { start: 20, end: 21 },
        ]
    );
    assert_eq!(
        assembled.records[2].operator.range,
        ByteRange { start: 22, end: 24 }
    );
    Ok(())
}

#[test]
fn assembler_keeps_composite_operands_as_token_ranges() -> Result<(), String> {
    let input = b"[1 [2] << /K (v) >>] BDC\n";
    let tokens = tokenize(input).map_err(|error| format!("{error:?}"))?;
    let assembled = assemble_operators(&tokens).map_err(|error| format!("{error:?}"))?;
    let record = &assembled.records[0];

    assert_eq!(record.operands.len(), 1);
    assert_eq!(record.operands[0].range, ByteRange { start: 0, end: 20 });
    assert_eq!(
        record.operands[0]
            .tokens
            .iter()
            .map(|token| token.range)
            .collect::<Vec<_>>(),
        tokens[..15]
            .iter()
            .map(|token| token.range)
            .collect::<Vec<_>>()
    );
    assert_eq!(record.operator.range, ByteRange { start: 21, end: 24 });
    Ok(())
}

#[test]
fn assembler_preserves_all_token_references_explicitly() -> Result<(), String> {
    let input = b"% lead\nq\n/DeviceRGB cs % mid\n1 0 0 rg\n";
    let tokens = tokenize(input).map_err(|error| format!("{error:?}"))?;
    let assembled = assemble_operators(&tokens).map_err(|error| format!("{error:?}"))?;
    let mut refs = assembled.trivia.clone();
    for record in &assembled.records {
        refs.push(record.operator);
        refs.extend(record.trivia.iter().copied());
        for operand in &record.operands {
            refs.extend(operand.tokens.iter().copied());
        }
    }
    refs.sort_by_key(|token| token.token_index);

    assert_eq!(
        refs,
        tokens
            .iter()
            .enumerate()
            .map(|(token_index, token)| TokenRef {
                token_index,
                range: token.range,
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(
        serialize_tokens_unmodified(input, &tokens).map_err(|error| format!("{error:?}"))?,
        input
    );
    Ok(())
}

#[test]
fn assembler_reports_malformed_ordering() -> Result<(), String> {
    let trailing = tokenize(b"1 0 0 ").map_err(|error| format!("{error:?}"))?;
    let unmatched = tokenize(b"]").map_err(|error| format!("{error:?}"))?;
    let unterminated = tokenize(b"[1 2 rg").map_err(|error| format!("{error:?}"))?;
    let keyword = tokenize(b"obj").map_err(|error| format!("{error:?}"))?;

    assert_eq!(
        assemble_operators(&trailing),
        Err(AssembleError::TrailingOperands {
            token_index: 0,
            range: ByteRange { start: 0, end: 1 },
        })
    );
    assert_eq!(
        assemble_operators(&unmatched),
        Err(AssembleError::UnmatchedArrayClose {
            token_index: 0,
            range: ByteRange { start: 0, end: 1 },
        })
    );
    assert_eq!(
        assemble_operators(&unterminated),
        Err(AssembleError::OperatorInsideCompositeOperand {
            token_index: 5,
            range: ByteRange { start: 5, end: 7 },
        })
    );
    assert_eq!(
        assemble_operators(&keyword),
        Err(AssembleError::UnexpectedKeyword {
            token_index: 0,
            range: ByteRange { start: 0, end: 3 },
        })
    );
    Ok(())
}
