//! Byte-preserving content-stream syntax.

#![forbid(unsafe_code)]

mod assembler;
mod model;
mod serializer;
mod tokenizer;

#[cfg(test)]
mod tests;

pub use assembler::assemble_operators;
pub use model::{
    AssembleError, AssembledContentStream, Delimiter, Keyword, NumberKind, OperandRecord,
    OperatorRecord, SerializeError, StringKind, Token, TokenKind, TokenRef, TokenizeError,
    TokenizeErrorKind, TriviaKind,
};
pub use serializer::{serialize_tokens_unmodified, serialize_unmodified};
pub use tokenizer::tokenize;
