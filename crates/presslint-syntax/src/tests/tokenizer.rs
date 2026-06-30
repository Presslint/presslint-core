use presslint_types::ByteRange;

use crate::{
    Delimiter, Keyword, NumberKind, StringKind, TokenKind, TokenizeErrorKind, TriviaKind, tokenize,
};

#[test]
fn tokenizer_preserves_ranges_for_common_content_tokens() -> Result<(), String> {
    let input = b"q\n/DeviceRGB cs [1 -2.5 .75] << /K (v\\)x) /H <4E6F> >> Do\n% note\n";
    let tokens = tokenize(input).map_err(|error| format!("{error:?}"))?;
    let observed: Option<Vec<(TokenKind, &[u8])>> = tokens
        .iter()
        .map(|token| Some((token.kind, token.source_bytes(input)?)))
        .collect();

    assert_eq!(
        observed,
        Some(vec![
            (TokenKind::Operator, &b"q"[..]),
            (TokenKind::Trivia(TriviaKind::Whitespace), &b"\n"[..]),
            (TokenKind::Name, &b"/DeviceRGB"[..]),
            (TokenKind::Trivia(TriviaKind::Whitespace), &b" "[..]),
            (TokenKind::Operator, &b"cs"[..]),
            (TokenKind::Trivia(TriviaKind::Whitespace), &b" "[..]),
            (TokenKind::Delimiter(Delimiter::ArrayOpen), &b"["[..]),
            (TokenKind::Number(NumberKind::Integer), &b"1"[..]),
            (TokenKind::Trivia(TriviaKind::Whitespace), &b" "[..]),
            (TokenKind::Number(NumberKind::Real), &b"-2.5"[..]),
            (TokenKind::Trivia(TriviaKind::Whitespace), &b" "[..]),
            (TokenKind::Number(NumberKind::Real), &b".75"[..]),
            (TokenKind::Delimiter(Delimiter::ArrayClose), &b"]"[..]),
            (TokenKind::Trivia(TriviaKind::Whitespace), &b" "[..]),
            (TokenKind::Delimiter(Delimiter::DictionaryOpen), &b"<<"[..]),
            (TokenKind::Trivia(TriviaKind::Whitespace), &b" "[..]),
            (TokenKind::Name, &b"/K"[..]),
            (TokenKind::Trivia(TriviaKind::Whitespace), &b" "[..]),
            (TokenKind::String(StringKind::Literal), &b"(v\\)x)"[..]),
            (TokenKind::Trivia(TriviaKind::Whitespace), &b" "[..]),
            (TokenKind::Name, &b"/H"[..]),
            (TokenKind::Trivia(TriviaKind::Whitespace), &b" "[..]),
            (TokenKind::String(StringKind::Hexadecimal), &b"<4E6F>"[..]),
            (TokenKind::Trivia(TriviaKind::Whitespace), &b" "[..]),
            (TokenKind::Delimiter(Delimiter::DictionaryClose), &b">>"[..]),
            (TokenKind::Trivia(TriviaKind::Whitespace), &b" "[..]),
            (TokenKind::Operator, &b"Do"[..]),
            (TokenKind::Trivia(TriviaKind::Whitespace), &b"\n"[..]),
            (TokenKind::Trivia(TriviaKind::Comment), &b"% note"[..]),
            (TokenKind::Trivia(TriviaKind::Whitespace), &b"\n"[..]),
        ])
    );
    Ok(())
}

#[test]
fn tokenizer_recognizes_boolean_null_and_object_keywords() -> Result<(), String> {
    let input = b"true false null obj endobj stream endstream";
    let kinds: Vec<TokenKind> = tokenize(input)
        .map_err(|error| format!("{error:?}"))?
        .into_iter()
        .filter(|token| !matches!(token.kind, TokenKind::Trivia(_)))
        .map(|token| token.kind)
        .collect();

    assert_eq!(
        kinds,
        vec![
            TokenKind::Boolean,
            TokenKind::Boolean,
            TokenKind::Null,
            TokenKind::Keyword(Keyword::ObjectBegin),
            TokenKind::Keyword(Keyword::ObjectEnd),
            TokenKind::Keyword(Keyword::StreamBegin),
            TokenKind::Keyword(Keyword::StreamEnd),
        ]
    );
    Ok(())
}

#[test]
fn tokenizer_reports_unterminated_strings() -> Result<(), String> {
    let Err(literal) = tokenize(b"(unterminated") else {
        return Err("literal string unexpectedly tokenized".to_owned());
    };
    let Err(hex) = tokenize(b"<4E6F") else {
        return Err("hex string unexpectedly tokenized".to_owned());
    };

    assert_eq!(literal.kind, TokenizeErrorKind::UnterminatedLiteralString);
    assert_eq!(literal.range, ByteRange { start: 0, end: 13 });
    assert_eq!(hex.kind, TokenizeErrorKind::UnterminatedHexString);
    assert_eq!(hex.range, ByteRange { start: 0, end: 5 });
    Ok(())
}
