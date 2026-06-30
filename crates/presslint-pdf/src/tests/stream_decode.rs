use miniz_oxide::deflate::compress_to_vec_zlib;

use crate::{FlateDecodeParameters, FlateDecodeStreamRejection, decode_flate_stream};

fn compress(data: &[u8]) -> Vec<u8> {
    compress_to_vec_zlib(data, 6)
}

#[test]
fn decodes_raw_flate_stream() {
    let decoded = b"q\n0 0 1 rg\nf\nQ";
    let compressed = compress(decoded);

    let result = decode_flate_stream(&compressed, FlateDecodeParameters::default(), 1024)
        .expect("raw FlateDecode stream should decode");

    assert_eq!(result, decoded);
}

#[test]
fn rejects_inflate_output_over_limit() {
    let compressed = compress(b"0123456789");

    let error = decode_flate_stream(&compressed, FlateDecodeParameters::default(), 4)
        .expect_err("over-limit inflate should reject");

    assert_eq!(
        error.reason,
        FlateDecodeStreamRejection::OutputLimitExceeded
    );
    assert_eq!(error.output_limit, 4);
}

#[test]
fn reverses_tiff_predictor_two() {
    let residual = [
        10, 20, 30, // first RGB sample
        3, 4, 5, // second sample residuals
        10, 10, 10, // third sample residuals
    ];
    let compressed = compress(&residual);
    let parameters = FlateDecodeParameters {
        predictor: 2,
        colors: 3,
        bits_per_component: 8,
        columns: 3,
    };

    let result =
        decode_flate_stream(&compressed, parameters, 1024).expect("TIFF predictor should reverse");

    assert_eq!(result, [10, 20, 30, 13, 24, 35, 23, 34, 45]);
}

#[test]
fn reverses_png_none_filter() {
    let residual = [0, 10, 20, 30];
    let compressed = compress(&residual);
    let parameters = png_parameters(3);

    let result =
        decode_flate_stream(&compressed, parameters, 1024).expect("PNG None filter should reverse");

    assert_eq!(result, [10, 20, 30]);
}

#[test]
fn reverses_png_sub_filter() {
    let residual = [1, 10, 3, 10];
    let compressed = compress(&residual);
    let parameters = png_parameters(3);

    let result =
        decode_flate_stream(&compressed, parameters, 1024).expect("PNG Sub filter should reverse");

    assert_eq!(result, [10, 13, 23]);
}

#[test]
fn reverses_png_up_filter() {
    let residual = [
        0, 10, 20, 30, //
        2, 1, 2, 3,
    ];
    let compressed = compress(&residual);
    let parameters = png_parameters(3);

    let result =
        decode_flate_stream(&compressed, parameters, 1024).expect("PNG Up filter should reverse");

    assert_eq!(result, [10, 20, 30, 11, 22, 33]);
}

#[test]
fn reverses_png_average_filter() {
    let residual = [
        0, 10, 20, 30, //
        3, 6, 7, 7,
    ];
    let compressed = compress(&residual);
    let parameters = png_parameters(3);

    let result = decode_flate_stream(&compressed, parameters, 1024)
        .expect("PNG Average filter should reverse");

    assert_eq!(result, [10, 20, 30, 11, 22, 33]);
}

#[test]
fn reverses_png_paeth_filter() {
    let residual = [
        0, 10, 20, 30, //
        4, 1, 2, 3,
    ];
    let compressed = compress(&residual);
    let parameters = png_parameters(3);

    let result = decode_flate_stream(&compressed, parameters, 1024)
        .expect("PNG Paeth filter should reverse");

    assert_eq!(result, [10, 20, 30, 11, 22, 33]);
}

#[test]
fn rejects_unsupported_predictor() {
    let compressed = compress(b"abc");
    let parameters = FlateDecodeParameters {
        predictor: 9,
        ..FlateDecodeParameters::default()
    };

    let error = decode_flate_stream(&compressed, parameters, 1024)
        .expect_err("unsupported predictor should reject");

    assert_eq!(
        error.reason,
        FlateDecodeStreamRejection::UnsupportedPredictor { predictor: 9 }
    );
}

#[test]
fn rejects_invalid_png_row_geometry() {
    let compressed = compress(&[0, 1, 2]);
    let parameters = png_parameters(3);

    let error = decode_flate_stream(&compressed, parameters, 1024)
        .expect_err("partial PNG predictor row should reject");

    assert_eq!(
        error.reason,
        FlateDecodeStreamRejection::RowGeometryMismatch
    );
}

#[test]
fn rejects_unknown_png_filter() {
    let compressed = compress(&[5, 1, 2, 3]);
    let parameters = png_parameters(3);

    let error = decode_flate_stream(&compressed, parameters, 1024)
        .expect_err("unknown PNG predictor filter should reject");

    assert_eq!(
        error.reason,
        FlateDecodeStreamRejection::UnknownPngFilter { filter: 5 }
    );
}

fn png_parameters(columns: u32) -> FlateDecodeParameters {
    FlateDecodeParameters {
        predictor: 15,
        colors: 1,
        bits_per_component: 8,
        columns,
    }
}
