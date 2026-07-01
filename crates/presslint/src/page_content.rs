use presslint_pdf::{
    ContentStreamFilterClassification, FlateDecodeParameters, FlateDecodeParametersResolution,
    FlateDecodeStreamError, FlateDecodeStreamRejection, PageContentExtentInspection,
    classify_content_stream_filter, content_stream_data_slice, decode_flate_stream,
    resolve_flate_decode_parameters,
};

use crate::document_inventory::InventoryPageSkip;

const STREAM_SEPARATOR: u8 = b'\n';

pub enum PageContentBytes<'input> {
    Borrowed(&'input [u8]),
    Owned(Vec<u8>),
}

impl PageContentBytes<'_> {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }
}

pub fn page_content_bytes<'input>(
    input: &'input [u8],
    entries: &[PageContentExtentInspection],
    max_decoded_stream_bytes: usize,
) -> Result<PageContentBytes<'input>, InventoryPageSkip> {
    if entries.len() == 1 {
        let (object_byte_offset, extent) = located_stream(&entries[0])?;
        return decode_content(input, object_byte_offset, extent, max_decoded_stream_bytes);
    }

    let mut joined = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let (object_byte_offset, extent) = located_stream(entry)?;
        if index > 0 {
            push_bounded_separator(&mut joined, object_byte_offset, max_decoded_stream_bytes)?;
        }

        let remaining = max_decoded_stream_bytes
            .checked_sub(joined.len())
            .ok_or_else(|| content_limit_error(object_byte_offset, 0, max_decoded_stream_bytes))?;
        let content = decode_content(input, object_byte_offset, extent, remaining)?;
        let bytes = content.as_slice();
        if bytes.len() > remaining {
            return Err(content_limit_error(
                object_byte_offset,
                bytes.len(),
                max_decoded_stream_bytes,
            ));
        }
        joined.extend_from_slice(bytes);
    }

    Ok(PageContentBytes::Owned(joined))
}

fn located_stream(
    entry: &PageContentExtentInspection,
) -> Result<(usize, &presslint_pdf::ContentStreamDataExtentInspection), InventoryPageSkip> {
    match entry {
        PageContentExtentInspection::Located {
            object_byte_offset,
            extent,
            ..
        } => Ok((*object_byte_offset, extent)),
        PageContentExtentInspection::Skipped { reason, .. } => {
            Err(InventoryPageSkip::TargetSkipped {
                reason: reason.clone(),
            })
        }
        PageContentExtentInspection::Failed {
            object_byte_offset,
            error,
            ..
        } => Err(InventoryPageSkip::ExtentFailed {
            object_byte_offset: *object_byte_offset,
            error: error.clone(),
        }),
    }
}

fn decode_content<'input>(
    input: &'input [u8],
    object_byte_offset: usize,
    extent: &presslint_pdf::ContentStreamDataExtentInspection,
    max_decoded_stream_bytes: usize,
) -> Result<PageContentBytes<'input>, InventoryPageSkip> {
    let stream_data = content_stream_data_slice(input, extent).map_err(|error| {
        InventoryPageSkip::SliceFailed {
            object_byte_offset,
            error,
        }
    })?;

    match classify_content_stream_filter(input, object_byte_offset).map_err(|error| {
        InventoryPageSkip::FilterClassificationFailed {
            object_byte_offset,
            error,
        }
    })? {
        ContentStreamFilterClassification::Uncompressed => {
            Ok(PageContentBytes::Borrowed(stream_data))
        }
        ContentStreamFilterClassification::Flate => {
            let resolution =
                resolve_flate_decode_parameters(input, object_byte_offset).map_err(|error| {
                    InventoryPageSkip::DecodeParmsFailed {
                        object_byte_offset,
                        error,
                    }
                })?;
            let FlateDecodeParametersResolution::Resolved { parameters, .. } = resolution else {
                return Err(InventoryPageSkip::UnsupportedDecodeParms {
                    object_byte_offset,
                    resolution,
                });
            };
            let decoded = decode_flate_stream(stream_data, parameters, max_decoded_stream_bytes)
                .map_err(|error| InventoryPageSkip::DecodeFailed {
                    object_byte_offset,
                    error,
                })?;
            Ok(PageContentBytes::Owned(decoded))
        }
        classification @ (ContentStreamFilterClassification::UnsupportedFilter { .. }
        | ContentStreamFilterClassification::UnsupportedFilterChain { .. }) => {
            Err(InventoryPageSkip::UnsupportedFilter {
                object_byte_offset,
                classification,
            })
        }
    }
}

fn push_bounded_separator(
    joined: &mut Vec<u8>,
    object_byte_offset: usize,
    max_decoded_stream_bytes: usize,
) -> Result<(), InventoryPageSkip> {
    if joined.len() == max_decoded_stream_bytes {
        return Err(content_limit_error(
            object_byte_offset,
            1,
            max_decoded_stream_bytes,
        ));
    }
    joined.push(STREAM_SEPARATOR);
    Ok(())
}

fn content_limit_error(
    object_byte_offset: usize,
    decoded_len: usize,
    output_limit: usize,
) -> InventoryPageSkip {
    InventoryPageSkip::DecodeFailed {
        object_byte_offset,
        error: FlateDecodeStreamError {
            compressed_len: decoded_len,
            output_limit,
            parameters: FlateDecodeParameters::default(),
            reason: FlateDecodeStreamRejection::OutputLimitExceeded,
        },
    }
}
