use crate::model::{SerializeError, Token};

/// Serialize an unmodified token stream by copying exact source bytes.
///
/// This function is intentionally limited to the unmodified path. It proves
/// that token ranges can re-emit the original byte stream without syntax
/// normalization.
///
/// # Errors
///
/// Returns a [`SerializeError`] if token ranges are invalid or non-contiguous.
pub fn serialize_tokens_unmodified(
    source: &[u8],
    tokens: &[Token],
) -> Result<Vec<u8>, SerializeError> {
    let mut output = Vec::with_capacity(source.len());
    let mut expected_start = 0;

    for (token_index, token) in tokens.iter().enumerate() {
        if token.range.start != expected_start {
            return Err(SerializeError::NonContiguousRange {
                token_index,
                expected_start,
                actual_start: token.range.start,
            });
        }

        let Some(bytes) = token.source_bytes(source) else {
            return Err(SerializeError::InvalidRange {
                token_index,
                range: token.range,
            });
        };

        output.extend_from_slice(bytes);
        expected_start = token.range.end;
    }

    if expected_start != source.len() {
        return Err(SerializeError::NonContiguousRange {
            token_index: tokens.len(),
            expected_start,
            actual_start: source.len(),
        });
    }

    Ok(output)
}

/// Return the input bytes unchanged.
///
/// This placeholder makes the round-trip contract explicit before the real
/// tokenizer lands.
#[must_use]
pub fn serialize_unmodified(input: &[u8]) -> Vec<u8> {
    input.to_vec()
}
