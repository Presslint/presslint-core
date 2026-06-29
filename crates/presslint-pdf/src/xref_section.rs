use crate::source_utils::{count_leading_digits, parse_usize_decimal, skip_whitespace};
use crate::{PdfSourceDiagnostic, PdfXrefSectionIssue, XREF_SECTION_SCAN_LIMIT, XrefSection};

const XREF_KEYWORD: &[u8] = b"xref";
const OBJ_KEYWORD: &[u8] = b"obj";

pub fn classify_xref_section(
    input: &[u8],
    byte_offset: usize,
) -> Result<XrefSection, PdfSourceDiagnostic> {
    let byte_len = input.len();
    if byte_offset >= byte_len {
        return Err(xref_section_diagnostic(
            PdfXrefSectionIssue::OffsetOutOfBounds,
            byte_offset,
            byte_len,
        ));
    }

    let window_end = byte_offset
        .saturating_add(XREF_SECTION_SCAN_LIMIT)
        .min(byte_len);
    let window = &input[byte_offset..window_end];
    let content = &window[skip_whitespace(window)..];

    if content.starts_with(XREF_KEYWORD) {
        return Ok(XrefSection::Table);
    }

    classify_indirect_object_header(content, byte_offset, byte_len)
}

fn classify_indirect_object_header(
    content: &[u8],
    byte_offset: usize,
    byte_len: usize,
) -> Result<XrefSection, PdfSourceDiagnostic> {
    let unrecognized =
        || xref_section_diagnostic(PdfXrefSectionIssue::Unrecognized, byte_offset, byte_len);

    let object_digits = count_leading_digits(content);
    if object_digits == 0 {
        return Err(unrecognized());
    }
    let after_object = &content[object_digits..];

    let object_generation_gap = skip_whitespace(after_object);
    if object_generation_gap == 0 {
        return Err(unrecognized());
    }
    let generation_field = &after_object[object_generation_gap..];

    let generation_digits = count_leading_digits(generation_field);
    if generation_digits == 0 {
        return Err(unrecognized());
    }
    let after_generation = &generation_field[generation_digits..];

    let generation_keyword_gap = skip_whitespace(after_generation);
    if generation_keyword_gap == 0 {
        return Err(unrecognized());
    }
    if !after_generation[generation_keyword_gap..].starts_with(OBJ_KEYWORD) {
        return Err(unrecognized());
    }

    let object_number = parse_usize_decimal(&content[..object_digits])
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| {
            xref_section_diagnostic(
                PdfXrefSectionIssue::ObjectNumberOutOfRange,
                byte_offset,
                byte_len,
            )
        })?;
    let generation = parse_usize_decimal(&generation_field[..generation_digits])
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| {
            xref_section_diagnostic(
                PdfXrefSectionIssue::GenerationOutOfRange,
                byte_offset,
                byte_len,
            )
        })?;

    Ok(XrefSection::Stream {
        object_number,
        generation,
    })
}

const fn xref_section_diagnostic(
    reason: PdfXrefSectionIssue,
    byte_offset: usize,
    byte_len: usize,
) -> PdfSourceDiagnostic {
    PdfSourceDiagnostic::XrefSectionUnclassified {
        reason,
        byte_offset,
        byte_len,
    }
}
