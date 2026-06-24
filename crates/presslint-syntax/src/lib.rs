//! Byte-preserving content-stream syntax.

#![forbid(unsafe_code)]

use presslint_core::ByteRange;
use serde::{Deserialize, Serialize};

/// Lexical token with source byte range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// Token lexical class.
    pub kind: TokenKind,
    /// Source range in the content stream.
    pub range: ByteRange,
}

impl Token {
    /// Create a token with a lexical class and source range.
    #[must_use]
    pub const fn new(kind: TokenKind, range: ByteRange) -> Self {
        Self { kind, range }
    }

    /// Return this token's exact source bytes when the range is valid.
    #[must_use]
    pub fn source_bytes<'source>(&self, source: &'source [u8]) -> Option<&'source [u8]> {
        if self.range.start > self.range.end {
            return None;
        }
        source.get(self.range.start..self.range.end)
    }
}

/// Source-preserving PDF content-stream token class.
///
/// Variants classify lexical structure only. Parsed values remain in the
/// original byte stream and are addressed by each [`Token`]'s byte range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum TokenKind {
    /// Whitespace or comment trivia.
    Trivia(TriviaKind),
    /// `true` or `false`.
    Boolean,
    /// `null`.
    Null,
    /// Integer or real numeric lexeme.
    Number(NumberKind),
    /// Literal or hexadecimal string.
    String(StringKind),
    /// PDF name beginning with `/`.
    Name,
    /// Structural delimiter.
    Delimiter(Delimiter),
    /// Object or stream keyword.
    Keyword(Keyword),
    /// Content-stream operator.
    Operator,
}

/// Non-semantic lexical trivia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriviaKind {
    /// One or more PDF whitespace bytes.
    Whitespace,
    /// `%` comment through end of line.
    Comment,
}

/// Numeric lexeme family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberKind {
    /// Integer number.
    Integer,
    /// Real number.
    Real,
}

/// String lexeme family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StringKind {
    /// Parenthesized literal string.
    Literal,
    /// Angle-bracket hexadecimal string.
    Hexadecimal,
}

/// PDF syntax delimiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delimiter {
    /// `[`.
    ArrayOpen,
    /// `]`.
    ArrayClose,
    /// `<<`.
    DictionaryOpen,
    /// `>>`.
    DictionaryClose,
}

/// Keyword lexeme outside normal content operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Keyword {
    /// `obj`.
    ObjectBegin,
    /// `endobj`.
    ObjectEnd,
    /// `stream`.
    StreamBegin,
    /// `endstream`.
    StreamEnd,
}

/// Return the input bytes unchanged.
///
/// This placeholder makes the round-trip contract explicit before the real
/// tokenizer lands.
#[must_use]
pub fn serialize_unmodified(input: &[u8]) -> Vec<u8> {
    input.to_vec()
}

#[cfg(test)]
mod tests {
    use super::{
        Delimiter, Keyword, NumberKind, StringKind, Token, TokenKind, TriviaKind,
        serialize_unmodified,
    };
    use presslint_core::ByteRange;

    #[test]
    fn unmodified_serializer_is_byte_identical() {
        let input = b"0 0 0 rg\n10 20 m\n";
        assert_eq!(serialize_unmodified(input), input);
    }

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
}
