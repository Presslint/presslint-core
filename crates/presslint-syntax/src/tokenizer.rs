use presslint_core::ByteRange;

use crate::model::{
    Delimiter, Keyword, NumberKind, StringKind, Token, TokenKind, TokenizeError, TokenizeErrorKind,
    TriviaKind,
};

/// Tokenize a PDF content byte stream into source-preserving lexical tokens.
///
/// This is a lexical scanner only. Token values remain encoded in the source
/// bytes and are addressed through each token's byte range.
///
/// # Errors
///
/// Returns a [`TokenizeError`] when a string token is unterminated.
pub fn tokenize(input: &[u8]) -> Result<Vec<Token>, TokenizeError> {
    let mut tokens = Vec::new();
    let mut offset = 0;

    while offset < input.len() {
        let byte = input[offset];
        if is_whitespace(byte) {
            let start = offset;
            offset += 1;
            while offset < input.len() && is_whitespace(input[offset]) {
                offset += 1;
            }
            tokens.push(Token::new(
                TokenKind::Trivia(TriviaKind::Whitespace),
                ByteRange { start, end: offset },
            ));
        } else if byte == b'%' {
            let start = offset;
            offset += 1;
            while offset < input.len() && input[offset] != b'\r' && input[offset] != b'\n' {
                offset += 1;
            }
            tokens.push(Token::new(
                TokenKind::Trivia(TriviaKind::Comment),
                ByteRange { start, end: offset },
            ));
        } else if byte == b'(' {
            let end = scan_literal_string(input, offset)?;
            tokens.push(Token::new(
                TokenKind::String(StringKind::Literal),
                ByteRange { start: offset, end },
            ));
            offset = end;
        } else if byte == b'<' && input.get(offset + 1) != Some(&b'<') {
            let end = scan_hex_string(input, offset)?;
            tokens.push(Token::new(
                TokenKind::String(StringKind::Hexadecimal),
                ByteRange { start: offset, end },
            ));
            offset = end;
        } else if let Some((kind, end)) = scan_delimiter(input, offset) {
            tokens.push(Token::new(
                TokenKind::Delimiter(kind),
                ByteRange { start: offset, end },
            ));
            offset = end;
        } else {
            let start = offset;
            offset += 1;
            while offset < input.len()
                && !is_whitespace(input[offset])
                && !is_delimiter(input[offset])
            {
                offset += 1;
            }
            tokens.push(Token::new(
                classify_regular_token(&input[start..offset]),
                ByteRange { start, end: offset },
            ));
        }
    }

    Ok(tokens)
}

fn scan_literal_string(input: &[u8], start: usize) -> Result<usize, TokenizeError> {
    let mut offset = start + 1;
    let mut depth = 1_u32;

    while offset < input.len() {
        match input[offset] {
            b'\\' => {
                offset = offset.saturating_add(2);
            }
            b'(' => {
                depth += 1;
                offset += 1;
            }
            b')' => {
                depth -= 1;
                offset += 1;
                if depth == 0 {
                    return Ok(offset);
                }
            }
            _ => offset += 1,
        }
    }

    Err(TokenizeError {
        kind: TokenizeErrorKind::UnterminatedLiteralString,
        range: ByteRange {
            start,
            end: input.len(),
        },
    })
}

fn scan_hex_string(input: &[u8], start: usize) -> Result<usize, TokenizeError> {
    let mut offset = start + 1;
    while offset < input.len() {
        if input[offset] == b'>' {
            return Ok(offset + 1);
        }
        offset += 1;
    }

    Err(TokenizeError {
        kind: TokenizeErrorKind::UnterminatedHexString,
        range: ByteRange {
            start,
            end: input.len(),
        },
    })
}

fn scan_delimiter(input: &[u8], offset: usize) -> Option<(Delimiter, usize)> {
    match input[offset] {
        b'[' => Some((Delimiter::ArrayOpen, offset + 1)),
        b']' => Some((Delimiter::ArrayClose, offset + 1)),
        b'<' if input.get(offset + 1) == Some(&b'<') => {
            Some((Delimiter::DictionaryOpen, offset + 2))
        }
        b'>' if input.get(offset + 1) == Some(&b'>') => {
            Some((Delimiter::DictionaryClose, offset + 2))
        }
        _ => None,
    }
}

fn classify_regular_token(bytes: &[u8]) -> TokenKind {
    match bytes {
        b"true" | b"false" => TokenKind::Boolean,
        b"null" => TokenKind::Null,
        b"obj" => TokenKind::Keyword(Keyword::ObjectBegin),
        b"endobj" => TokenKind::Keyword(Keyword::ObjectEnd),
        b"stream" => TokenKind::Keyword(Keyword::StreamBegin),
        b"endstream" => TokenKind::Keyword(Keyword::StreamEnd),
        _ if bytes.first() == Some(&b'/') => TokenKind::Name,
        _ if is_number_lexeme(bytes) => {
            if bytes.contains(&b'.') {
                TokenKind::Number(NumberKind::Real)
            } else {
                TokenKind::Number(NumberKind::Integer)
            }
        }
        _ => TokenKind::Operator,
    }
}

fn is_number_lexeme(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    let mut offset = 0;
    if matches!(bytes[offset], b'+' | b'-') {
        offset += 1;
    }
    if offset == bytes.len() {
        return false;
    }

    let mut saw_digit = false;
    let mut saw_dot = false;
    while offset < bytes.len() {
        match bytes[offset] {
            b'0'..=b'9' => saw_digit = true,
            b'.' if !saw_dot => saw_dot = true,
            _ => return false,
        }
        offset += 1;
    }

    saw_digit
}

const fn is_whitespace(byte: u8) -> bool {
    matches!(byte, 0x00 | b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

const fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}
