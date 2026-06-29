use presslint_core::ByteRange;

use crate::{Delimiter, Keyword, NumberKind, StringKind, Token, TokenKind, TriviaKind};

#[test]
fn token_returns_exact_source_bytes() {
    let input = b"/DeviceRGB 1 0 obj";
    let token = Token::new(TokenKind::Name, ByteRange { start: 0, end: 10 });

    assert_eq!(token.kind, TokenKind::Name);
    assert_eq!(token.source_bytes(input), Some(&input[..10]));
}

#[test]
fn token_rejects_invalid_ranges_without_panicking() {
    let input = b"q";
    let reversed = Token::new(TokenKind::Operator, ByteRange { start: 1, end: 0 });
    let out_of_bounds = Token::new(TokenKind::Operator, ByteRange { start: 0, end: 2 });

    assert_eq!(reversed.source_bytes(input), None);
    assert_eq!(out_of_bounds.source_bytes(input), None);
}

#[test]
fn token_kind_covers_initial_pdf_lexical_classes() {
    let kinds = [
        TokenKind::Trivia(TriviaKind::Whitespace),
        TokenKind::Trivia(TriviaKind::Comment),
        TokenKind::Boolean,
        TokenKind::Null,
        TokenKind::Number(NumberKind::Integer),
        TokenKind::Number(NumberKind::Real),
        TokenKind::String(StringKind::Literal),
        TokenKind::String(StringKind::Hexadecimal),
        TokenKind::Name,
        TokenKind::Delimiter(Delimiter::ArrayOpen),
        TokenKind::Delimiter(Delimiter::ArrayClose),
        TokenKind::Delimiter(Delimiter::DictionaryOpen),
        TokenKind::Delimiter(Delimiter::DictionaryClose),
        TokenKind::Keyword(Keyword::ObjectBegin),
        TokenKind::Keyword(Keyword::ObjectEnd),
        TokenKind::Keyword(Keyword::StreamBegin),
        TokenKind::Keyword(Keyword::StreamEnd),
        TokenKind::Operator,
    ];

    assert_eq!(kinds.len(), 18);
}
