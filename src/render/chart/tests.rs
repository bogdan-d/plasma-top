#![allow(clippy::expect_used)]

use super::*;
use miniz_oxide::inflate::decompress_to_vec_zlib;

#[derive(Debug)]
struct DecodedPng {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

fn decode_png(bytes: &[u8]) -> DecodedPng {
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

    let mut cursor = 8usize;
    let mut width = 0usize;
    let mut height = 0usize;
    let mut idat = Vec::new();

    while cursor + 12 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[cursor..cursor + 4].try_into().expect("len"));
        let len = len as usize;
        let tag = &bytes[cursor + 4..cursor + 8];
        let data = &bytes[cursor + 8..cursor + 8 + len];
        let crc = u32::from_be_bytes(
            bytes[cursor + 8 + len..cursor + 12 + len]
                .try_into()
                .expect("crc"),
        );

        let mut crc_input = Vec::with_capacity(4 + len);
        crc_input.extend_from_slice(tag);
        crc_input.extend_from_slice(data);
        assert_eq!(crc32(&crc_input), crc);

        match tag {
            b"IHDR" => {
                width = u32::from_be_bytes(data[0..4].try_into().expect("width")) as usize;
                height = u32::from_be_bytes(data[4..8].try_into().expect("height")) as usize;
                assert_eq!(&data[8..13], &[8, 6, 0, 0, 0]);
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }

        cursor += 12 + len;
    }

    let raw = decompress_to_vec_zlib(&idat).expect("zlib stream should decode");
    let stride = width * 4;
    let mut pixels = Vec::with_capacity(width * height * 4);
    let mut raw_cursor = 0usize;
    for _ in 0..height {
        assert_eq!(raw[raw_cursor], 0);
        raw_cursor += 1;
        pixels.extend_from_slice(&raw[raw_cursor..raw_cursor + stride]);
        raw_cursor += stride;
    }

    DecodedPng {
        width,
        height,
        pixels,
    }
}

fn pixel(decoded: &DecodedPng, x: usize, y: usize) -> RGBA {
    let offset = (y * decoded.width + x) * 4;
    (
        decoded.pixels[offset],
        decoded.pixels[offset + 1],
        decoded.pixels[offset + 2],
        decoded.pixels[offset + 3],
    )
}

#[test]
fn png_encoder_round_trips_rgba_rows() {
    let pixels = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

    let decoded = decode_png(&encode_png(2, 2, &pixels));

    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.pixels, pixels);
}

#[test]
fn empty_series_draws_grid_and_labels_only() {
    let decoded = decode_png(&area_chart_png(
        &[],
        12,
        8,
        AreaChartOptions {
            left_pad: 4,
            ..AreaChartOptions::default()
        },
    ));

    assert_eq!(decoded.width, 12);
    assert_eq!(decoded.height, 8);
    assert_eq!(crc32(&decoded.pixels), 0xe512_ff19);
    assert_eq!(pixel(&decoded, 1, 0), LABEL);
    assert_eq!(pixel(&decoded, 4, 7), GRID);
    assert_eq!(pixel(&decoded, 11, 3), GRID);
}

#[test]
fn repeated_calls_are_byte_stable() {
    let options = AreaChartOptions {
        left_pad: 3,
        overlay: Some(&[5.0, 10.0, 5.0, 0.0]),
        overlay_line: ORANGE_LINE,
        ..AreaChartOptions::default()
    };

    let first = area_chart_png(&[0.0, 20.0, 50.0, 100.0], 10, 8, options.clone());
    let second = area_chart_png(&[0.0, 20.0, 50.0, 100.0], 10, 8, options);

    assert_eq!(first, second);
}

#[test]
fn overlay_and_label_suppression_preserve_reserved_margin() {
    let decoded = decode_png(&area_chart_png(
        &[0.0, 50.0, 100.0, 50.0, 0.0],
        14,
        8,
        AreaChartOptions {
            left_pad: 5,
            grid_levels: &[0.0],
            overlay: Some(&[100.0, 75.0, 50.0, 25.0, 0.0]),
            overlay_line: RED_LINE,
            label_values: false,
            ..AreaChartOptions::default()
        },
    ));

    assert_eq!(crc32(&decoded.pixels), 0x9b1f_9252);
    assert_eq!(pixel(&decoded, 0, 0).3, 0);
    assert_eq!(pixel(&decoded, 5, 7), (79, 161, 204, 255));
    assert_eq!(pixel(&decoded, 9, 2), BLUE_LINE);
}

#[test]
fn single_point_chart_matches_python_pixels() {
    let decoded = decode_png(&area_chart_png(
        &[100.0],
        10,
        8,
        AreaChartOptions {
            left_pad: 3,
            ..AreaChartOptions::default()
        },
    ));

    assert_eq!(crc32(&decoded.pixels), 0xca87_e05d);
    assert_eq!(pixel(&decoded, 3, 2), (79, 161, 204, 255));
    assert_eq!(pixel(&decoded, 3, 7), (100, 147, 171, 120));
    assert_eq!(pixel(&decoded, 9, 7), (100, 147, 171, 120));
}

#[test]
fn constant_series_chart_matches_python_pixels() {
    let decoded = decode_png(&area_chart_png(
        &[25.0, 25.0, 25.0, 25.0],
        10,
        8,
        AreaChartOptions {
            left_pad: 3,
            ..AreaChartOptions::default()
        },
    ));

    assert_eq!(crc32(&decoded.pixels), 0x9a17_b9c5);
    assert_eq!(pixel(&decoded, 3, 6), (82, 159, 199, 220));
    assert_eq!(pixel(&decoded, 5, 6), (82, 159, 199, 220));
    assert_eq!(pixel(&decoded, 9, 7), (100, 147, 171, 120));
}
