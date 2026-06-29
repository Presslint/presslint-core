use crate::source_utils::{
    consume_keyword, consume_line_end, count_leading_digits, is_pdf_whitespace,
    parse_usize_decimal, skip_whitespace,
};
use crate::{
    ClassicXrefEntry, ClassicXrefEntryState, ClassicXrefSubsection, ClassicXrefTableInspection,
    ClassicXrefTableInspectionError, ClassicXrefTableInspectionRejection,
};

const XREF_KEYWORD: &[u8] = b"xref";
const TRAILER_KEYWORD: &[u8] = b"trailer";

/// Inspect a classic xref table at a caller-supplied byte offset.
/// # Errors
///
/// Returns [`ClassicXrefTableInspectionError`] when the offset is not a
/// classic table, a subsection header or entry is malformed, a numeric field is
/// out of range, or the following `trailer` keyword is missing.
pub fn inspect_classic_xref_table(
    input: &[u8],
    byte_offset: usize,
) -> Result<ClassicXrefTableInspection, ClassicXrefTableInspectionError> {
    if byte_offset >= input.len() {
        return Err(classic_xref_error(
            input,
            byte_offset,
            ClassicXrefTableInspectionRejection::OffsetOutOfBounds,
        ));
    }

    let table_byte_offset = byte_offset + skip_whitespace(&input[byte_offset..]);
    let Some(after_xref) = input
        .get(table_byte_offset..)
        .and_then(|content| consume_keyword(content, XREF_KEYWORD))
    else {
        return Err(classic_xref_error(
            input,
            byte_offset,
            ClassicXrefTableInspectionRejection::NotXrefTable,
        ));
    };

    let mut cursor = table_byte_offset + after_xref;
    let mut subsections = Vec::new();

    loop {
        cursor += skip_whitespace(&input[cursor..]);
        let content = &input[cursor..];

        if consume_keyword(content, TRAILER_KEYWORD).is_some() {
            return Ok(ClassicXrefTableInspection {
                table_byte_offset,
                subsections,
                trailer_byte_offset: cursor,
            });
        }

        if content.is_empty() {
            return Err(classic_xref_error_at(
                input,
                byte_offset,
                ClassicXrefTableInspectionRejection::MissingTrailer,
                cursor,
            ));
        }

        let (subsection, next_cursor) = parse_classic_xref_subsection(input, byte_offset, cursor)?;
        subsections.push(subsection);
        cursor = next_cursor;
    }
}

fn parse_classic_xref_subsection(
    input: &[u8],
    table_offset: usize,
    header_offset: usize,
) -> Result<(ClassicXrefSubsection, usize), ClassicXrefTableInspectionError> {
    let content = &input[header_offset..];
    let first_digits = count_leading_digits(content);
    if first_digits == 0 {
        return Err(malformed_header_error(input, table_offset, header_offset));
    }

    let after_first = &content[first_digits..];
    let gap = skip_whitespace(after_first);
    if gap == 0 {
        return Err(malformed_header_error(input, table_offset, header_offset));
    }

    let count_field = &after_first[gap..];
    let count_digits = count_leading_digits(count_field);
    if count_digits == 0 {
        return Err(malformed_header_error(input, table_offset, header_offset));
    }

    let first_object_number = parse_usize_decimal(&content[..first_digits])
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| {
            classic_xref_error_at(
                input,
                table_offset,
                ClassicXrefTableInspectionRejection::SubsectionObjectNumberOutOfRange,
                header_offset,
            )
        })?;
    let entry_count = parse_usize_decimal(&count_field[..count_digits])
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| {
            classic_xref_error_at(
                input,
                table_offset,
                ClassicXrefTableInspectionRejection::SubsectionEntryCountOutOfRange,
                header_offset,
            )
        })?;

    if entry_count > 0 && first_object_number.checked_add(entry_count - 1).is_none() {
        return Err(classic_xref_error_at(
            input,
            table_offset,
            ClassicXrefTableInspectionRejection::SubsectionObjectRangeOutOfRange,
            header_offset,
        ));
    }

    let mut cursor = header_offset + first_digits + gap + count_digits;
    cursor = consume_line_end(input, cursor, true)
        .ok_or_else(|| malformed_header_error(input, table_offset, header_offset))?;

    let mut entries = Vec::new();
    for index in 0..entry_count {
        let object_number = first_object_number + index;
        let (entry, next_cursor) =
            parse_classic_xref_entry(input, table_offset, cursor, object_number)?;
        entries.push(entry);
        cursor = next_cursor;
    }

    Ok((
        ClassicXrefSubsection {
            first_object_number,
            entry_count,
            entries,
        },
        cursor,
    ))
}

const fn malformed_header_error(
    input: &[u8],
    table_offset: usize,
    header_offset: usize,
) -> ClassicXrefTableInspectionError {
    classic_xref_error_at(
        input,
        table_offset,
        ClassicXrefTableInspectionRejection::MalformedSubsectionHeader,
        header_offset,
    )
}

fn parse_classic_xref_entry(
    input: &[u8],
    table_offset: usize,
    entry_offset: usize,
    object_number: u32,
) -> Result<(ClassicXrefEntry, usize), ClassicXrefTableInspectionError> {
    let Some(line) = input.get(entry_offset..) else {
        return Err(malformed_entry_error(
            input,
            table_offset,
            entry_offset,
            object_number,
        ));
    };

    if line.len() < 19
        || !line[..10].iter().all(u8::is_ascii_digit)
        || !is_pdf_whitespace(line[10])
        || !line[11..16].iter().all(u8::is_ascii_digit)
        || !is_pdf_whitespace(line[16])
        || !matches!(line[17], b'f' | b'n')
        || !is_pdf_whitespace(line[18])
    {
        return Err(malformed_entry_error(
            input,
            table_offset,
            entry_offset,
            object_number,
        ));
    }

    let byte_offset = parse_usize_decimal(&line[..10]).ok_or_else(|| {
        classic_xref_entry_error(
            input,
            table_offset,
            ClassicXrefTableInspectionRejection::EntryByteOffsetOutOfRange,
            entry_offset,
            object_number,
        )
    })?;
    let generation = parse_usize_decimal(&line[11..16])
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| {
            classic_xref_entry_error(
                input,
                table_offset,
                ClassicXrefTableInspectionRejection::EntryGenerationOutOfRange,
                entry_offset,
                object_number,
            )
        })?;
    let state = if line[17] == b'f' {
        ClassicXrefEntryState::Free
    } else {
        ClassicXrefEntryState::InUse
    };
    let next_cursor = consume_line_end(input, entry_offset + 18, false)
        .ok_or_else(|| malformed_entry_error(input, table_offset, entry_offset, object_number))?;

    Ok((
        ClassicXrefEntry {
            object_number,
            generation,
            byte_offset,
            state,
        },
        next_cursor,
    ))
}

const fn malformed_entry_error(
    input: &[u8],
    table_offset: usize,
    byte_offset: usize,
    object_number: u32,
) -> ClassicXrefTableInspectionError {
    classic_xref_entry_error(
        input,
        table_offset,
        ClassicXrefTableInspectionRejection::MalformedEntry,
        byte_offset,
        object_number,
    )
}

const fn classic_xref_error(
    input: &[u8],
    byte_offset: usize,
    reason: ClassicXrefTableInspectionRejection,
) -> ClassicXrefTableInspectionError {
    ClassicXrefTableInspectionError {
        byte_offset,
        byte_len: input.len(),
        error_byte_offset: None,
        object_number: None,
        reason,
    }
}

const fn classic_xref_error_at(
    input: &[u8],
    byte_offset: usize,
    reason: ClassicXrefTableInspectionRejection,
    error_byte_offset: usize,
) -> ClassicXrefTableInspectionError {
    ClassicXrefTableInspectionError {
        byte_offset,
        byte_len: input.len(),
        error_byte_offset: Some(error_byte_offset),
        object_number: None,
        reason,
    }
}

const fn classic_xref_entry_error(
    input: &[u8],
    byte_offset: usize,
    reason: ClassicXrefTableInspectionRejection,
    error_byte_offset: usize,
    object_number: u32,
) -> ClassicXrefTableInspectionError {
    ClassicXrefTableInspectionError {
        byte_offset,
        byte_len: input.len(),
        error_byte_offset: Some(error_byte_offset),
        object_number: Some(object_number),
        reason,
    }
}
