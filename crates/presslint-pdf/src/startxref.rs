use crate::source_utils::{
    count_leading_digits, is_pdf_whitespace, parse_usize_decimal, rfind_bytes, skip_whitespace,
};
use crate::{PdfSourceDiagnostic, PdfStartXref, PdfStartXrefIssue, STARTXREF_SCAN_LIMIT};

const STARTXREF_MARKER: &[u8] = b"startxref";
const EOF_MARKER: &[u8] = b"%%EOF";

pub fn inspect_startxref(input: &[u8]) -> Result<PdfStartXref, PdfSourceDiagnostic> {
    let searched_from = input.len().saturating_sub(STARTXREF_SCAN_LIMIT);
    let searched_to = input.len();
    let trailing = &input[searched_from..searched_to];

    let Some(relative_marker_offset) = rfind_bytes(trailing, STARTXREF_MARKER) else {
        return Err(startxref_diagnostic(
            PdfStartXrefIssue::MissingMarker,
            searched_from,
            searched_to,
            None,
        ));
    };
    let marker_byte_offset = searched_from + relative_marker_offset;
    let after_marker = relative_marker_offset + STARTXREF_MARKER.len();
    let remainder = &trailing[after_marker..];
    let offset_start = skip_whitespace(remainder);
    let digits = &remainder[offset_start..];
    let digit_count = count_leading_digits(digits);

    if digit_count == 0 {
        return Err(startxref_diagnostic(
            PdfStartXrefIssue::MissingOffset,
            searched_from,
            searched_to,
            Some(marker_byte_offset),
        ));
    }

    let byte_offset = parse_usize_decimal(&digits[..digit_count]).ok_or_else(|| {
        startxref_diagnostic(
            PdfStartXrefIssue::InvalidOffset,
            searched_from,
            searched_to,
            Some(marker_byte_offset),
        )
    })?;
    let after_digits = &digits[digit_count..];
    let eof_search_start = skip_whitespace(after_digits);
    let eof_candidate = &after_digits[eof_search_start..];

    if !eof_candidate.starts_with(EOF_MARKER) {
        return Err(startxref_diagnostic(
            PdfStartXrefIssue::MissingEofMarker,
            searched_from,
            searched_to,
            Some(marker_byte_offset),
        ));
    }

    if !eof_candidate[EOF_MARKER.len()..]
        .iter()
        .all(|byte| is_pdf_whitespace(*byte))
    {
        return Err(startxref_diagnostic(
            PdfStartXrefIssue::TrailingBytesAfterEof,
            searched_from,
            searched_to,
            Some(marker_byte_offset),
        ));
    }

    Ok(PdfStartXref {
        marker_byte_offset,
        byte_offset,
    })
}

const fn startxref_diagnostic(
    reason: PdfStartXrefIssue,
    searched_from: usize,
    searched_to: usize,
    marker_byte_offset: Option<usize>,
) -> PdfSourceDiagnostic {
    PdfSourceDiagnostic::StartXrefUnavailable {
        reason,
        searched_from,
        searched_to,
        marker_byte_offset,
    }
}
