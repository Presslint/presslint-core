use miniz_oxide::inflate::{TINFLStatus, decompress_to_vec_zlib_with_limit};
use serde::{Deserialize, Serialize};

/// Explicit `/FlateDecode` parameter values supplied by the caller.
///
/// The defaults match PDF `/DecodeParms`: no prediction, one colour component,
/// eight bits per component, and one sample column per row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlateDecodeParameters {
    /// `/Predictor`; `1` means no predictor.
    pub predictor: u16,
    /// `/Colors`; used when `predictor > 1`.
    pub colors: u32,
    /// `/BitsPerComponent`; used when `predictor > 1`.
    pub bits_per_component: u8,
    /// `/Columns`; used when `predictor > 1`.
    pub columns: u32,
}

impl Default for FlateDecodeParameters {
    fn default() -> Self {
        Self {
            predictor: 1,
            colors: 1,
            bits_per_component: 8,
            columns: 1,
        }
    }
}

/// Error returned when a bounded `/FlateDecode` payload cannot be decoded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlateDecodeStreamError {
    /// Caller-supplied compressed byte count.
    pub compressed_len: usize,
    /// Caller-supplied maximum inflated byte count.
    pub output_limit: usize,
    /// Decode parameters used for this attempt.
    pub parameters: FlateDecodeParameters,
    /// Structured failure reason.
    pub reason: FlateDecodeStreamRejection,
}

/// Structured `/FlateDecode` stream rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum FlateDecodeStreamRejection {
    /// Inflating would exceed the caller-supplied output limit.
    OutputLimitExceeded,
    /// The compressed bytes are not a valid zlib-wrapped Flate payload.
    InflateFailed,
    /// The `/Predictor` value is not supported by this helper.
    UnsupportedPredictor {
        /// Unsupported predictor value.
        predictor: u16,
    },
    /// `/Colors`, `/Columns`, or `/BitsPerComponent` is outside the valid set.
    MalformedPredictorParameters,
    /// Predictor row-size arithmetic overflowed `usize`.
    IntegerOverflow,
    /// The inflated byte length does not match complete predictor rows.
    RowGeometryMismatch,
    /// A PNG predictor row used an unknown filter byte.
    UnknownPngFilter {
        /// Filter byte found at the start of the row.
        filter: u8,
    },
}

/// Decode a single zlib-wrapped `/FlateDecode` stream payload.
///
/// The input stays borrowed. The returned decoded byte stream is owned because
/// decompression materializes a new operator byte sequence for downstream
/// tokenizers. `output_limit` bounds the inflated byte buffer before any
/// predictor reversal is attempted.
///
/// # Errors
///
/// Returns [`FlateDecodeStreamError`] for inflate errors, output-limit
/// rejection, unsupported or malformed predictor parameters, row geometry
/// mismatch, integer overflow, or unknown PNG filter bytes.
pub fn decode_flate_stream(
    compressed: &[u8],
    parameters: FlateDecodeParameters,
    output_limit: usize,
) -> Result<Vec<u8>, FlateDecodeStreamError> {
    let mut decoded =
        decompress_to_vec_zlib_with_limit(compressed, output_limit).map_err(|error| {
            let reason = if error.status == TINFLStatus::HasMoreOutput {
                FlateDecodeStreamRejection::OutputLimitExceeded
            } else {
                FlateDecodeStreamRejection::InflateFailed
            };
            flate_error(compressed, parameters, output_limit, reason)
        })?;

    reverse_predictor(&mut decoded, parameters)
        .map_err(|reason| flate_error(compressed, parameters, output_limit, reason))?;
    Ok(decoded)
}

fn reverse_predictor(
    decoded: &mut Vec<u8>,
    parameters: FlateDecodeParameters,
) -> Result<(), FlateDecodeStreamRejection> {
    match parameters.predictor {
        1 => Ok(()),
        2 => reverse_tiff_predictor(decoded, parameters),
        10..=15 => reverse_png_predictor(decoded, parameters),
        predictor => Err(FlateDecodeStreamRejection::UnsupportedPredictor { predictor }),
    }
}

fn reverse_tiff_predictor(
    decoded: &mut [u8],
    parameters: FlateDecodeParameters,
) -> Result<(), FlateDecodeStreamRejection> {
    let geometry = predictor_geometry(parameters)?;
    if geometry.row_bytes == 0 || !decoded.len().is_multiple_of(geometry.row_bytes) {
        return Err(FlateDecodeStreamRejection::RowGeometryMismatch);
    }
    let colors = usize::try_from(parameters.colors)
        .map_err(|_| FlateDecodeStreamRejection::IntegerOverflow)?;

    for row in decoded.chunks_exact_mut(geometry.row_bytes) {
        match parameters.bits_per_component {
            1 | 2 | 4 => reverse_tiff_sub_byte_row(
                row,
                colors,
                parameters.bits_per_component,
                geometry.component_count,
            ),
            8 => reverse_tiff_byte_row(row, colors),
            16 => reverse_tiff_u16_row(row, colors, geometry.component_count),
            _ => return Err(FlateDecodeStreamRejection::MalformedPredictorParameters),
        }
    }
    Ok(())
}

fn reverse_png_predictor(
    decoded: &mut Vec<u8>,
    parameters: FlateDecodeParameters,
) -> Result<(), FlateDecodeStreamRejection> {
    let geometry = predictor_geometry(parameters)?;
    let encoded_row_bytes = geometry
        .row_bytes
        .checked_add(1)
        .ok_or(FlateDecodeStreamRejection::IntegerOverflow)?;
    if !decoded.len().is_multiple_of(encoded_row_bytes) {
        return Err(FlateDecodeStreamRejection::RowGeometryMismatch);
    }

    let row_count = decoded.len() / encoded_row_bytes;
    let mut write_offset = 0usize;
    for row_index in 0..row_count {
        let read_offset = row_index
            .checked_mul(encoded_row_bytes)
            .ok_or(FlateDecodeStreamRejection::IntegerOverflow)?;
        let filter = decoded[read_offset];
        let row_start = read_offset + 1;

        for col in 0..geometry.row_bytes {
            let raw = decoded[row_start + col];
            let left = if col >= geometry.png_bytes_per_pixel {
                decoded[write_offset + col - geometry.png_bytes_per_pixel]
            } else {
                0
            };
            let up = if row_index == 0 {
                0
            } else {
                decoded[write_offset - geometry.row_bytes + col]
            };
            let up_left = if row_index == 0 || col < geometry.png_bytes_per_pixel {
                0
            } else {
                decoded[write_offset - geometry.row_bytes + col - geometry.png_bytes_per_pixel]
            };
            decoded[write_offset + col] = match filter {
                0 => raw,
                1 => raw.wrapping_add(left),
                2 => raw.wrapping_add(up),
                3 => raw.wrapping_add(png_average(left, up)),
                4 => raw.wrapping_add(paeth(left, up, up_left)),
                filter => return Err(FlateDecodeStreamRejection::UnknownPngFilter { filter }),
            };
        }

        write_offset += geometry.row_bytes;
    }

    decoded.truncate(write_offset);
    Ok(())
}

fn reverse_tiff_byte_row(row: &mut [u8], colors: usize) {
    for index in colors..row.len() {
        row[index] = row[index].wrapping_add(row[index - colors]);
    }
}

fn reverse_tiff_u16_row(row: &mut [u8], colors: usize, component_count: usize) {
    for component in colors..component_count {
        let prior = read_component_u16(row, component - colors);
        let residual = read_component_u16(row, component);
        write_component_u16(row, component, residual.wrapping_add(prior));
    }
}

fn reverse_tiff_sub_byte_row(
    row: &mut [u8],
    colors: usize,
    bits_per_component: u8,
    component_count: usize,
) {
    let mask = (1u16 << bits_per_component) - 1;
    for component in colors..component_count {
        let prior = get_component_bits(row, component - colors, bits_per_component);
        let residual = get_component_bits(row, component, bits_per_component);
        set_component_bits(
            row,
            component,
            bits_per_component,
            (residual + prior) & mask,
        );
    }
}

fn read_component_u16(row: &[u8], component: usize) -> u16 {
    let offset = component * 2;
    u16::from_be_bytes([row[offset], row[offset + 1]])
}

fn write_component_u16(row: &mut [u8], component: usize, value: u16) {
    let offset = component * 2;
    let bytes = value.to_be_bytes();
    row[offset] = bytes[0];
    row[offset + 1] = bytes[1];
}

fn get_component_bits(row: &[u8], component: usize, bits_per_component: u8) -> u16 {
    let bit_offset = component * usize::from(bits_per_component);
    let byte_offset = bit_offset / 8;
    let bit_in_byte = bit_offset % 8;
    let shift = 8 - bit_in_byte - usize::from(bits_per_component);
    u16::from((row[byte_offset] >> shift) & ((1u8 << bits_per_component) - 1))
}

fn set_component_bits(row: &mut [u8], component: usize, bits_per_component: u8, value: u16) {
    let bit_offset = component * usize::from(bits_per_component);
    let byte_offset = bit_offset / 8;
    let bit_in_byte = bit_offset % 8;
    let shift = 8 - bit_in_byte - usize::from(bits_per_component);
    let mask = ((1u8 << bits_per_component) - 1) << shift;
    let value = u8::try_from(value).unwrap_or(u8::MAX);
    row[byte_offset] = (row[byte_offset] & !mask) | ((value << shift) & mask);
}

fn paeth(left: u8, up: u8, up_left: u8) -> u8 {
    let left_value = i16::from(left);
    let up_value = i16::from(up);
    let up_left_value = i16::from(up_left);
    let estimate = left_value + up_value - up_left_value;
    let left_distance = (estimate - left_value).abs();
    let up_distance = (estimate - up_value).abs();
    let up_left_distance = (estimate - up_left_value).abs();

    if left_distance <= up_distance && left_distance <= up_left_distance {
        left
    } else if up_distance <= up_left_distance {
        up
    } else {
        up_left
    }
}

fn png_average(left: u8, up: u8) -> u8 {
    u8::try_from(u16::midpoint(u16::from(left), u16::from(up))).unwrap_or(u8::MAX)
}

#[derive(Debug, Clone, Copy)]
struct PredictorGeometry {
    row_bytes: usize,
    png_bytes_per_pixel: usize,
    component_count: usize,
}

fn predictor_geometry(
    parameters: FlateDecodeParameters,
) -> Result<PredictorGeometry, FlateDecodeStreamRejection> {
    if parameters.colors == 0 || parameters.columns == 0 {
        return Err(FlateDecodeStreamRejection::MalformedPredictorParameters);
    }
    if !matches!(parameters.bits_per_component, 1 | 2 | 4 | 8 | 16) {
        return Err(FlateDecodeStreamRejection::MalformedPredictorParameters);
    }

    let colors = usize::try_from(parameters.colors)
        .map_err(|_| FlateDecodeStreamRejection::IntegerOverflow)?;
    let columns = usize::try_from(parameters.columns)
        .map_err(|_| FlateDecodeStreamRejection::IntegerOverflow)?;
    let bits_per_component = usize::from(parameters.bits_per_component);
    let component_count = columns
        .checked_mul(colors)
        .ok_or(FlateDecodeStreamRejection::IntegerOverflow)?;
    let row_bits = component_count
        .checked_mul(bits_per_component)
        .ok_or(FlateDecodeStreamRejection::IntegerOverflow)?;
    let row_bytes = row_bits
        .checked_add(7)
        .ok_or(FlateDecodeStreamRejection::IntegerOverflow)?
        / 8;
    let pixel_bits = colors
        .checked_mul(bits_per_component)
        .ok_or(FlateDecodeStreamRejection::IntegerOverflow)?;
    let png_bytes_per_pixel = pixel_bits
        .checked_add(7)
        .ok_or(FlateDecodeStreamRejection::IntegerOverflow)?
        / 8;

    Ok(PredictorGeometry {
        row_bytes,
        png_bytes_per_pixel,
        component_count,
    })
}

const fn flate_error(
    compressed: &[u8],
    parameters: FlateDecodeParameters,
    output_limit: usize,
    reason: FlateDecodeStreamRejection,
) -> FlateDecodeStreamError {
    FlateDecodeStreamError {
        compressed_len: compressed.len(),
        output_limit,
        parameters,
        reason,
    }
}
