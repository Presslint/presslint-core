use presslint_core::ByteRange;

use crate::{
    SerializeError, Token, TokenKind, serialize_tokens_unmodified, serialize_unmodified, tokenize,
};

#[test]
fn unmodified_serializer_is_byte_identical() {
    let input = b"0 0 0 rg\n10 20 m\n";
    assert_eq!(serialize_unmodified(input), input);
}

#[test]
fn token_stream_serializer_is_byte_identical() -> Result<(), String> {
    let input = b"q\n/DeviceRGB cs [1 -2.5 .75]\n";
    let tokens = tokenize(input).map_err(|error| format!("{error:?}"))?;

    assert_eq!(
        serialize_tokens_unmodified(input, &tokens).map_err(|error| format!("{error:?}"))?,
        input
    );
    Ok(())
}

#[test]
fn token_stream_serializer_rejects_bad_ranges() {
    let source = b"abc";
    let invalid = [Token::new(
        TokenKind::Operator,
        ByteRange { start: 0, end: 4 },
    )];
    let gapped = [
        Token::new(TokenKind::Operator, ByteRange { start: 0, end: 1 }),
        Token::new(TokenKind::Operator, ByteRange { start: 2, end: 3 }),
    ];
    let overlapping = [
        Token::new(TokenKind::Operator, ByteRange { start: 0, end: 2 }),
        Token::new(TokenKind::Operator, ByteRange { start: 1, end: 3 }),
    ];

    assert_eq!(
        serialize_tokens_unmodified(source, &invalid),
        Err(SerializeError::InvalidRange {
            token_index: 0,
            range: ByteRange { start: 0, end: 4 },
        })
    );
    assert_eq!(
        serialize_tokens_unmodified(source, &gapped),
        Err(SerializeError::NonContiguousRange {
            token_index: 1,
            expected_start: 1,
            actual_start: 2,
        })
    );
    assert_eq!(
        serialize_tokens_unmodified(source, &overlapping),
        Err(SerializeError::NonContiguousRange {
            token_index: 1,
            expected_start: 2,
            actual_start: 1,
        })
    );
}
