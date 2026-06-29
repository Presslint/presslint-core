use presslint_core::ByteRange;

use crate::model::{
    AssembleError, AssembledContentStream, Delimiter, OperandRecord, OperatorRecord, Token,
    TokenKind, TokenRef,
};

/// Assemble lexical tokens into content-stream operator records.
///
/// The assembler keeps operands as source token ranges and performs only
/// content-stream ordering checks. It does not decode operand values or
/// evaluate graphics state.
///
/// # Errors
///
/// Returns an [`AssembleError`] when operands are left without an operator,
/// delimiters are malformed, object/stream keywords appear in the content
/// stream, or token ranges are invalid.
pub fn assemble_operators(tokens: &[Token]) -> Result<AssembledContentStream, AssembleError> {
    let mut records = Vec::new();
    let mut stream_trivia = Vec::new();
    let mut pending_operands: Vec<OperandRecord> = Vec::new();
    let mut pending_trivia = Vec::new();
    let mut index = 0;

    while index < tokens.len() {
        let token_ref = checked_token_ref(tokens, index)?;
        match tokens[index].kind {
            TokenKind::Trivia(_) => {
                if pending_operands.is_empty() {
                    stream_trivia.push(token_ref);
                } else {
                    pending_trivia.push(token_ref);
                }
                index += 1;
            }
            TokenKind::Operator => {
                let range = pending_operands
                    .first()
                    .map_or(token_ref.range, |first| ByteRange {
                        start: first.range.start,
                        end: token_ref.range.end,
                    });
                records.push(OperatorRecord {
                    operator: token_ref,
                    operands: core::mem::take(&mut pending_operands),
                    trivia: core::mem::take(&mut pending_trivia),
                    range,
                });
                index += 1;
            }
            TokenKind::Delimiter(Delimiter::ArrayClose) => {
                return Err(AssembleError::UnmatchedArrayClose {
                    token_index: index,
                    range: token_ref.range,
                });
            }
            TokenKind::Delimiter(Delimiter::DictionaryClose) => {
                return Err(AssembleError::UnmatchedDictionaryClose {
                    token_index: index,
                    range: token_ref.range,
                });
            }
            TokenKind::Keyword(_) => {
                return Err(AssembleError::UnexpectedKeyword {
                    token_index: index,
                    range: token_ref.range,
                });
            }
            _ => pending_operands.push(parse_operand_record(tokens, &mut index)?),
        }
    }

    if let Some(first) = pending_operands.first() {
        return Err(AssembleError::TrailingOperands {
            token_index: first.tokens[0].token_index,
            range: first.range,
        });
    }

    Ok(AssembledContentStream {
        records,
        trivia: stream_trivia,
    })
}

fn parse_operand_record(
    tokens: &[Token],
    index: &mut usize,
) -> Result<OperandRecord, AssembleError> {
    let first = checked_token_ref(tokens, *index)?;
    match tokens[*index].kind {
        TokenKind::Boolean
        | TokenKind::Null
        | TokenKind::Number(_)
        | TokenKind::String(_)
        | TokenKind::Name => {
            *index += 1;
            Ok(OperandRecord {
                tokens: vec![first],
                range: first.range,
            })
        }
        TokenKind::Delimiter(Delimiter::ArrayOpen) => {
            parse_composite_operand(tokens, index, Delimiter::ArrayClose)
        }
        TokenKind::Delimiter(Delimiter::DictionaryOpen) => {
            parse_composite_operand(tokens, index, Delimiter::DictionaryClose)
        }
        TokenKind::Delimiter(Delimiter::ArrayClose | Delimiter::DictionaryClose)
        | TokenKind::Operator
        | TokenKind::Keyword(_)
        | TokenKind::Trivia(_) => unreachable!("top-level parser handles non-operand tokens"),
    }
}

fn parse_composite_operand(
    tokens: &[Token],
    index: &mut usize,
    expected_close: Delimiter,
) -> Result<OperandRecord, AssembleError> {
    let mut refs = Vec::new();
    append_composite_operand(tokens, index, expected_close, &mut refs)?;
    Ok(operand_from_refs(refs))
}

fn append_composite_operand(
    tokens: &[Token],
    index: &mut usize,
    expected_close: Delimiter,
    refs: &mut Vec<TokenRef>,
) -> Result<(), AssembleError> {
    let open = checked_token_ref(tokens, *index)?;
    refs.push(open);
    *index += 1;

    while *index < tokens.len() {
        let token_ref = checked_token_ref(tokens, *index)?;
        match tokens[*index].kind {
            TokenKind::Trivia(_)
            | TokenKind::Boolean
            | TokenKind::Null
            | TokenKind::Number(_)
            | TokenKind::String(_)
            | TokenKind::Name => {
                refs.push(token_ref);
                *index += 1;
            }
            TokenKind::Delimiter(Delimiter::ArrayOpen) => {
                append_composite_operand(tokens, index, Delimiter::ArrayClose, refs)?;
            }
            TokenKind::Delimiter(Delimiter::DictionaryOpen) => {
                append_composite_operand(tokens, index, Delimiter::DictionaryClose, refs)?;
            }
            TokenKind::Delimiter(close) if close == expected_close => {
                refs.push(token_ref);
                *index += 1;
                return Ok(());
            }
            TokenKind::Delimiter(Delimiter::ArrayClose | Delimiter::DictionaryClose) => {
                return Err(AssembleError::MismatchedDelimiter {
                    token_index: *index,
                    range: token_ref.range,
                });
            }
            TokenKind::Operator => {
                return Err(AssembleError::OperatorInsideCompositeOperand {
                    token_index: *index,
                    range: token_ref.range,
                });
            }
            TokenKind::Keyword(_) => {
                return Err(AssembleError::UnexpectedKeyword {
                    token_index: *index,
                    range: token_ref.range,
                });
            }
        }
    }

    Err(AssembleError::UnterminatedCompositeOperand {
        token_index: open.token_index,
        range: open.range,
    })
}

fn operand_from_refs(refs: Vec<TokenRef>) -> OperandRecord {
    let first = refs[0];
    let last = refs[refs.len() - 1];
    OperandRecord {
        tokens: refs,
        range: ByteRange {
            start: first.range.start,
            end: last.range.end,
        },
    }
}

fn checked_token_ref(tokens: &[Token], token_index: usize) -> Result<TokenRef, AssembleError> {
    let range = tokens[token_index].range;
    if range.start > range.end {
        return Err(AssembleError::InvalidTokenRange { token_index, range });
    }
    Ok(TokenRef { token_index, range })
}
