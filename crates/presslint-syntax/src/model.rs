use presslint_types::ByteRange;
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

/// Tokenization failure with source range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenizeError {
    /// Error class.
    pub kind: TokenizeErrorKind,
    /// Source range that caused the error.
    pub range: ByteRange,
}

/// Structured tokenizer error class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenizeErrorKind {
    /// A literal string opened with `(` but did not close.
    UnterminatedLiteralString,
    /// A hexadecimal string opened with `<` but did not close.
    UnterminatedHexString,
}

/// Reference to one token in the source token stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenRef {
    /// Index in the token slice passed to [`crate::assemble_operators`].
    pub token_index: usize,
    /// Source range of the referenced token.
    pub range: ByteRange,
}

/// Operand represented by the original token ranges that formed it.
///
/// This is deliberately lexical. Numeric values, names, strings, arrays, and
/// dictionaries are not decoded or normalized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperandRecord {
    /// Exact tokens that form the operand. Composite operands include their
    /// delimiter and interior trivia tokens.
    pub tokens: Vec<TokenRef>,
    /// Source range from the first operand token through the last one.
    pub range: ByteRange,
}

/// One content-stream operator grouped with its preceding operands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorRecord {
    /// Operator token.
    pub operator: TokenRef,
    /// Operands immediately preceding the operator at top level.
    pub operands: Vec<OperandRecord>,
    /// Top-level trivia between operands and their operator.
    pub trivia: Vec<TokenRef>,
    /// Source range from the first operand token, or the operator token for
    /// zero-operand records, through the operator token.
    pub range: ByteRange,
}

/// Assembled content stream with non-record trivia preserved explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssembledContentStream {
    /// Operator records in source order.
    pub records: Vec<OperatorRecord>,
    /// Top-level trivia not owned by an operator record.
    pub trivia: Vec<TokenRef>,
}

/// Operator assembly failure with source location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssembleError {
    /// A token range is reversed.
    InvalidTokenRange {
        /// Token index in the provided token stream.
        token_index: usize,
        /// Invalid source range.
        range: ByteRange,
    },
    /// A top-level operand was not followed by an operator.
    TrailingOperands {
        /// First unconsumed operand token.
        token_index: usize,
        /// Source range of the first unconsumed operand.
        range: ByteRange,
    },
    /// `]` appeared without a matching `[`.
    UnmatchedArrayClose {
        /// Token index of the unmatched close delimiter.
        token_index: usize,
        /// Source range of the unmatched close delimiter.
        range: ByteRange,
    },
    /// `>>` appeared without a matching `<<`.
    UnmatchedDictionaryClose {
        /// Token index of the unmatched close delimiter.
        token_index: usize,
        /// Source range of the unmatched close delimiter.
        range: ByteRange,
    },
    /// A composite operand closed with the wrong delimiter.
    MismatchedDelimiter {
        /// Token index of the mismatched close delimiter.
        token_index: usize,
        /// Source range of the mismatched close delimiter.
        range: ByteRange,
    },
    /// The token stream ended before a composite operand closed.
    UnterminatedCompositeOperand {
        /// Token index of the opening delimiter.
        token_index: usize,
        /// Source range of the opening delimiter.
        range: ByteRange,
    },
    /// An operator token appeared inside an array or dictionary operand.
    OperatorInsideCompositeOperand {
        /// Token index of the operator token.
        token_index: usize,
        /// Source range of the operator token.
        range: ByteRange,
    },
    /// Object/stream keywords are not supported as content-stream operands.
    UnexpectedKeyword {
        /// Token index of the keyword token.
        token_index: usize,
        /// Source range of the keyword token.
        range: ByteRange,
    },
}

/// Serialization failure for unmodified token streams.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SerializeError {
    /// A token range is reversed or outside the source bytes.
    InvalidRange {
        /// Token index in the provided token stream.
        token_index: usize,
        /// Invalid source range.
        range: ByteRange,
    },
    /// Token ranges do not form one contiguous source span.
    NonContiguousRange {
        /// Token index in the provided token stream.
        token_index: usize,
        /// Expected range start.
        expected_start: usize,
        /// Actual range start.
        actual_start: usize,
    },
}
