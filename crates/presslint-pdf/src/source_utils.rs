pub fn count_leading_digits(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count()
}

pub fn parse_usize_decimal(bytes: &[u8]) -> Option<usize> {
    let mut value = 0usize;
    for byte in bytes {
        let digit = usize::from(byte - b'0');
        value = value.checked_mul(10)?.checked_add(digit)?;
    }
    Some(value)
}

pub fn consume_keyword(bytes: &[u8], keyword: &[u8]) -> Option<usize> {
    let after_keyword = bytes.strip_prefix(keyword)?;
    if after_keyword
        .first()
        .is_some_and(|byte| !is_pdf_whitespace(*byte) && !is_pdf_delimiter(*byte))
    {
        return None;
    }
    Some(keyword.len())
}

pub fn consume_line_end(input: &[u8], mut cursor: usize, allow_now: bool) -> Option<usize> {
    let mut allow_line_end = allow_now;
    while let Some(byte) = input.get(cursor) {
        match *byte {
            b'\r' if allow_line_end || input.get(cursor + 1) == Some(&b'\n') => {
                let after_cr = cursor + 1;
                return Some(if input.get(after_cr) == Some(&b'\n') {
                    after_cr + 1
                } else {
                    after_cr
                });
            }
            b'\n' if allow_line_end => return Some(cursor + 1),
            byte if is_pdf_whitespace(byte) && !matches!(byte, b'\r' | b'\n') => {
                cursor += 1;
                allow_line_end = true;
            }
            _ => return None,
        }
    }
    None
}

pub fn skip_whitespace(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|byte| !is_pdf_whitespace(*byte))
        .unwrap_or(bytes.len())
}

pub const fn is_pdf_whitespace(byte: u8) -> bool {
    matches!(byte, b'\0' | b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
}

const fn is_pdf_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

pub fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub fn rfind_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}
