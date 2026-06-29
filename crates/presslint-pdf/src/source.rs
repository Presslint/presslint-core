use serde::{Deserialize, Serialize};

use crate::source_utils::find_bytes;
use crate::startxref::inspect_startxref;
use crate::xref_section::classify_xref_section;

const PDF_HEADER_MARKER: &[u8] = b"%PDF-";

/// Maximum leading bytes inspected while looking for a PDF header.
pub const PDF_HEADER_SCAN_LIMIT: usize = 1024;

/// Maximum trailing bytes inspected while looking for the final `startxref`.
pub const STARTXREF_SCAN_LIMIT: usize = 4096;

/// Maximum bytes inspected at the `startxref` offset during classification.
///
/// This window only needs to see the `xref` keyword or a short `N G obj`
/// header, never the section body.
pub const XREF_SECTION_SCAN_LIMIT: usize = 64;

/// Small, source-oriented report over caller-provided PDF bytes.
///
/// This report deliberately stores only document facts and diagnostics. It does
/// not retain or copy the source bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfSourceInspection {
    /// Total number of bytes in the caller-provided source slice.
    pub byte_len: usize,
    /// Header discovered in the bounded leading source window.
    pub header: PdfHeader,
    /// Final `startxref` value discovered in the bounded trailing source window.
    pub startxref: Option<PdfStartXref>,
    /// Cross-reference section style classified at the resolved `startxref`
    /// offset. Only populated when a `startxref` offset was resolved and the
    /// bounded window at that offset matched a known section shape.
    pub xref_section: Option<XrefSection>,
    /// Non-fatal facts that could not be discovered by this bounded slice.
    pub diagnostics: Vec<PdfSourceDiagnostic>,
}

impl PdfSourceInspection {
    /// PDF header version as a `(major, minor)` pair.
    #[must_use]
    pub const fn pdf_version(&self) -> (u8, u8) {
        (self.header.version.major, self.header.version.minor)
    }
}

/// PDF header found near the beginning of the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfHeader {
    /// Byte offset where `%PDF-` begins.
    pub byte_offset: usize,
    /// Header version.
    pub version: PdfVersion,
}

/// PDF version from a `%PDF-M.N` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfVersion {
    /// Major version digit.
    pub major: u8,
    /// Minor version digit.
    pub minor: u8,
}

/// Parsed `startxref` record from the bounded trailing source window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfStartXref {
    /// Byte offset where the `startxref` keyword begins.
    pub marker_byte_offset: usize,
    /// Decimal byte offset declared after `startxref`.
    pub byte_offset: usize,
}

/// Style of the cross-reference section found at the final `startxref` offset.
///
/// This classification only inspects the leading bytes of the section. It does
/// not read table entries, the trailer dictionary, the stream dictionary, or
/// any object body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "style", rename_all = "snake_case")]
pub enum XrefSection {
    /// Classic cross-reference table: the section begins (after optional PDF
    /// whitespace) with the `xref` keyword.
    Table,
    /// Cross-reference stream: the section begins (after optional PDF
    /// whitespace) with an `N G obj` indirect object header.
    Stream {
        /// Object number parsed from the indirect object header.
        object_number: u32,
        /// Generation number parsed from the indirect object header.
        generation: u16,
    },
}

/// Rejection returned when the source cannot be identified as a PDF source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfSourceInspectionError {
    /// Total source length.
    pub byte_len: usize,
    /// Structured rejection reason.
    pub reason: PdfSourceRejection,
}

/// Fatal source-inspection rejection reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum PdfSourceRejection {
    /// No `%PDF-M.N` header was found in the bounded leading window.
    MissingHeader {
        /// First source byte inspected.
        searched_from: usize,
        /// End-exclusive source byte inspected.
        searched_to: usize,
    },
    /// A `%PDF-` marker was found, but it was not followed by `M.N` digits.
    MalformedHeader {
        /// Byte offset where `%PDF-` begins.
        header_byte_offset: usize,
    },
}

/// Non-fatal source-inspection diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PdfSourceDiagnostic {
    /// A final `startxref` record was not found in the bounded trailing window.
    StartXrefUnavailable {
        /// Why the marker could not be reported.
        reason: PdfStartXrefIssue,
        /// First source byte inspected.
        searched_from: usize,
        /// End-exclusive source byte inspected.
        searched_to: usize,
        /// Byte offset of the `startxref` keyword when one was found.
        marker_byte_offset: Option<usize>,
    },
    /// The cross-reference section at the resolved `startxref` offset could not
    /// be classified as a table or a stream.
    XrefSectionUnclassified {
        /// Why the section could not be classified.
        reason: PdfXrefSectionIssue,
        /// Resolved `startxref` offset where classification began.
        byte_offset: usize,
        /// Total source length, for out-of-bounds context.
        byte_len: usize,
    },
}

/// Reasons the cross-reference section at the `startxref` offset could not be
/// classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PdfXrefSectionIssue {
    /// The `startxref` offset lies beyond the source length.
    OffsetOutOfBounds,
    /// The leading bytes match neither an `xref` table nor an `N G obj` header.
    Unrecognized,
    /// An `N G obj` header was found, but the object number does not fit `u32`.
    ObjectNumberOutOfRange,
    /// An `N G obj` header was found, but the generation number does not fit
    /// `u16`.
    GenerationOutOfRange,
}

/// Reasons the bounded trailing source window could not report `startxref`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PdfStartXrefIssue {
    /// No `startxref` keyword was found in the trailing scan window.
    MissingMarker,
    /// The keyword was present, but no decimal offset followed it.
    MissingOffset,
    /// The decimal offset could not fit in `usize`.
    InvalidOffset,
    /// No following `%%EOF` marker was found.
    MissingEofMarker,
    /// Non-whitespace bytes followed the final `%%EOF` marker.
    TrailingBytesAfterEof,
}

/// Inspect caller-provided PDF bytes without parsing objects or streams.
///
/// # Errors
///
/// Returns [`PdfSourceInspectionError`] when no valid `%PDF-M.N` header can be
/// found in the bounded leading source window.
pub fn inspect_pdf_source(input: &[u8]) -> Result<PdfSourceInspection, PdfSourceInspectionError> {
    let byte_len = input.len();
    let header =
        inspect_header(input).map_err(|reason| PdfSourceInspectionError { byte_len, reason })?;

    let mut diagnostics = Vec::new();
    let startxref = match inspect_startxref(input) {
        Ok(startxref) => Some(startxref),
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            None
        }
    };

    let mut xref_section = None;
    if let Some(startxref) = startxref {
        match classify_xref_section(input, startxref.byte_offset) {
            Ok(section) => xref_section = Some(section),
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }

    Ok(PdfSourceInspection {
        byte_len,
        header,
        startxref,
        xref_section,
        diagnostics,
    })
}

fn inspect_header(input: &[u8]) -> Result<PdfHeader, PdfSourceRejection> {
    let searched_to = input.len().min(PDF_HEADER_SCAN_LIMIT);
    let leading = &input[..searched_to];

    let Some(marker_offset) = find_bytes(leading, PDF_HEADER_MARKER) else {
        return Err(PdfSourceRejection::MissingHeader {
            searched_from: 0,
            searched_to,
        });
    };

    let version_start = marker_offset + PDF_HEADER_MARKER.len();
    let version = leading
        .get(version_start..version_start + 3)
        .and_then(parse_version)
        .ok_or(PdfSourceRejection::MalformedHeader {
            header_byte_offset: marker_offset,
        })?;

    Ok(PdfHeader {
        byte_offset: marker_offset,
        version,
    })
}

fn parse_version(bytes: &[u8]) -> Option<PdfVersion> {
    let [major, b'.', minor] = bytes else {
        return None;
    };

    if !major.is_ascii_digit() || !minor.is_ascii_digit() {
        return None;
    }

    Some(PdfVersion {
        major: major - b'0',
        minor: minor - b'0',
    })
}
