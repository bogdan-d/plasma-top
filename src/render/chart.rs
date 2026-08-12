//! Pure-stdlib-style raster charts encoded as PNG for the tooltip graphs page.
//!
//! The Python oracle renders these charts directly into RGBA pixels and then
//! wraps them in a PNG data URI. This module mirrors that behavior in Rust:
//! grid, labels, area fill, top line, optional overlay line, and a fixed PNG
//! chunk layout. Only the zlib/DEFLATE step uses a small pure-Rust dependency.

use miniz_oxide::deflate::compress_to_vec_zlib;

/// One RGBA color in 8-bit channels.
pub type RGBA = (u8, u8, u8, u8);

/// Faint grid color that reads on both light and dark tooltips.
pub const GRID: RGBA = (128, 128, 128, 70);
/// Muted y-axis digit color.
pub const LABEL: RGBA = (140, 140, 140, 210);
/// Plasma blue line used by CPU charts.
pub const BLUE_LINE: RGBA = (61, 174, 233, 255);
/// Plasma blue fill used by CPU charts.
pub const BLUE_FILL: RGBA = (61, 174, 233, 70);
/// Purple line used by memory charts.
pub const PURPLE_LINE: RGBA = (163, 102, 255, 255);
/// Purple fill used by memory charts.
pub const PURPLE_FILL: RGBA = (163, 102, 255, 70);
/// Green line used by GPU-usage charts.
pub const GREEN_LINE: RGBA = (46, 204, 113, 255);
/// Green fill used by GPU-usage charts.
pub const GREEN_FILL: RGBA = (46, 204, 113, 70);
/// Orange overlay line used by GPU-decoder charts.
pub const ORANGE_LINE: RGBA = (230, 126, 34, 255);
/// Teal line used by network-download charts.
pub const TEAL_LINE: RGBA = (26, 188, 156, 255);
/// Teal fill used by network-download charts.
pub const TEAL_FILL: RGBA = (26, 188, 156, 70);
/// Red overlay line used by network-upload charts.
pub const RED_LINE: RGBA = (231, 76, 60, 255);

const DIGIT_WIDTH: usize = 3;
const DIGIT_HEIGHT: usize = 5;
const DEFAULT_GRID_LEVELS: &[f64] = &[0.0, 25.0, 50.0, 75.0, 100.0];

const DIGITS: [([[u8; DIGIT_WIDTH]; DIGIT_HEIGHT], char); 10] = [
    ([*b"111", *b"101", *b"101", *b"101", *b"111"], '0'),
    ([*b"110", *b"010", *b"010", *b"010", *b"111"], '1'),
    ([*b"111", *b"001", *b"111", *b"100", *b"111"], '2'),
    ([*b"111", *b"001", *b"111", *b"001", *b"111"], '3'),
    ([*b"101", *b"101", *b"111", *b"001", *b"001"], '4'),
    ([*b"111", *b"100", *b"111", *b"001", *b"111"], '5'),
    ([*b"111", *b"100", *b"111", *b"101", *b"111"], '6'),
    ([*b"111", *b"001", *b"001", *b"010", *b"010"], '7'),
    ([*b"111", *b"101", *b"111", *b"101", *b"111"], '8'),
    ([*b"111", *b"101", *b"111", *b"001", *b"111"], '9'),
];

/// Rendering knobs for [`area_chart_png`].
#[derive(Debug, Clone)]
pub struct AreaChartOptions<'a> {
    /// Maximum visible value; input values are clamped into `0..=vmax`.
    pub vmax: f64,
    /// RGBA color for the main line.
    pub line: RGBA,
    /// RGBA color for the area fill.
    pub fill: RGBA,
    /// RGBA color for horizontal grid lines.
    pub grid: RGBA,
    /// RGBA color for the baked y-axis labels.
    pub label: RGBA,
    /// Left pixel margin reserved for y-axis labels.
    pub left_pad: usize,
    /// Grid levels, in chart-value units, drawn across the plot.
    pub grid_levels: &'a [f64],
    /// Optional second series drawn as a line only.
    pub overlay: Option<&'a [f64]>,
    /// RGBA color for the overlay line.
    pub overlay_line: RGBA,
    /// Whether to draw numeric y-axis labels in the reserved left margin.
    pub label_values: bool,
}

impl Default for AreaChartOptions<'static> {
    fn default() -> Self {
        Self {
            vmax: 100.0,
            line: BLUE_LINE,
            fill: BLUE_FILL,
            grid: GRID,
            label: LABEL,
            left_pad: 0,
            grid_levels: DEFAULT_GRID_LEVELS,
            overlay: None,
            overlay_line: GRID,
            label_values: true,
        }
    }
}

/// Encodes `series` as a tooltip PNG area chart.
///
/// The output mirrors `src/chart.py`: transparent background, filled area,
/// antialiased top line, optional overlay line, horizontal grid, and baked
/// 3×5 digit labels. PNG chunk order is fixed as `IHDR`, `IDAT`, `IEND`.
#[must_use]
pub fn area_chart_png(
    series: &[f64],
    width: usize,
    height: usize,
    options: AreaChartOptions<'_>,
) -> Vec<u8> {
    let mut pixels = vec![0; width.saturating_mul(height).saturating_mul(4)];
    let top_pad = 2usize;
    let floor_y = height.saturating_sub(1);
    let span = height.saturating_sub(1 + top_pad);
    let plot_x0 = options.left_pad.min(width);
    let plot_width = width.saturating_sub(plot_x0);

    if !series.is_empty() {
        let curve = curve_yf(series, plot_width, floor_y, span, options.vmax);
        fill_area(
            &mut pixels,
            width,
            height,
            plot_x0,
            floor_y,
            &curve,
            options.fill,
        );
        draw_line(&mut pixels, width, height, plot_x0, &curve, options.line);
    }

    if let Some(overlay) = options.overlay.filter(|overlay| !overlay.is_empty()) {
        let curve = curve_yf(overlay, plot_width, floor_y, span, options.vmax);
        draw_line(
            &mut pixels,
            width,
            height,
            plot_x0,
            &curve,
            options.overlay_line,
        );
    }

    for &level in options.grid_levels {
        let gy = round_half_even(value_yf(level, options.vmax, floor_y, span));
        draw_horizontal_grid(&mut pixels, width, height, plot_x0, gy, options.grid);
        if options.left_pad > 0 && options.label_values {
            draw_digits(
                &mut pixels,
                width,
                height,
                &level_to_string(level),
                options.left_pad.saturating_sub(2) as isize,
                gy,
                options.label,
            );
        }
    }

    encode_png(width, height, &pixels)
}

fn level_to_string(level: f64) -> String {
    if level.fract() == 0.0 {
        format!("{level:.0}")
    } else {
        level.to_string()
    }
}

fn encode_png(width: usize, height: usize, pixels: &[u8]) -> Vec<u8> {
    let stride = width.saturating_mul(4);
    let mut raw = Vec::with_capacity(height.saturating_mul(stride + 1));
    for row in pixels.chunks(stride) {
        raw.push(0);
        raw.extend_from_slice(row);
    }

    let compressed = compress_to_vec_zlib(&raw, 9);
    let mut out = Vec::with_capacity(8 + 12 + 13 + 12 + compressed.len() + 12);
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    append_chunk(&mut out, *b"IHDR", &ihdr_bytes(width, height));
    append_chunk(&mut out, *b"IDAT", &compressed);
    append_chunk(&mut out, *b"IEND", b"");
    out
}

fn ihdr_bytes(width: usize, height: usize) -> [u8; 13] {
    let width = width as u32;
    let height = height as u32;
    let mut data = [0u8; 13];
    data[0..4].copy_from_slice(&width.to_be_bytes());
    data[4..8].copy_from_slice(&height.to_be_bytes());
    data[8] = 8;
    data[9] = 6;
    data[10] = 0;
    data[11] = 0;
    data[12] = 0;
    data
}

fn append_chunk(out: &mut Vec<u8>, tag: [u8; 4], data: &[u8]) {
    let len = data.len() as u32;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&tag);
    out.extend_from_slice(data);

    let mut crc_input = Vec::with_capacity(tag.len() + data.len());
    crc_input.extend_from_slice(&tag);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn curve_yf(series: &[f64], plot_width: usize, floor_y: usize, span: usize, vmax: f64) -> Vec<f64> {
    let mut curve = Vec::with_capacity(plot_width);
    let series_last = series.len().saturating_sub(1);
    for offset in 0..plot_width {
        let t = if series.len() > 1 && plot_width > 1 {
            (offset as f64 / (plot_width - 1) as f64) * series_last as f64
        } else {
            0.0
        };
        let index = t.floor() as usize;
        let frac = t - index as f64;
        let value = if index + 1 >= series.len() {
            series[index]
        } else {
            series[index] * (1.0 - frac) + series[index + 1] * frac
        };
        curve.push(value_yf(value, vmax, floor_y, span));
    }
    curve
}

fn fill_area(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    plot_x0: usize,
    floor_y: usize,
    curve: &[f64],
    fill: RGBA,
) {
    let stride = width.saturating_mul(4);
    for (offset, &y) in curve.iter().enumerate() {
        let x = plot_x0 + offset;
        let y0 = y.floor() as usize;
        set_px(pixels, width, height, x, y0, fill, (y0 + 1) as f64 - y);

        if y0 + 1 > floor_y {
            continue;
        }

        let mut cursor = ((y0 + 1) * width + x) * 4;
        for _ in (y0 + 1)..=floor_y {
            pixels[cursor] = fill.0;
            pixels[cursor + 1] = fill.1;
            pixels[cursor + 2] = fill.2;
            pixels[cursor + 3] = fill.3;
            cursor += stride;
        }
    }
}

fn draw_line(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    plot_x0: usize,
    curve: &[f64],
    color: RGBA,
) {
    let mut previous: Option<(usize, f64)> = None;
    for (offset, &y) in curve.iter().enumerate() {
        let x = plot_x0 + offset;
        if let Some((prev_x, prev_y)) = previous {
            let dy = y - prev_y;
            if (-1.0..=1.0).contains(&dy) {
                let yi = y.floor() as usize;
                let frac = y - yi as f64;
                set_px(pixels, width, height, x, yi, color, 1.0 - frac);
                set_px(pixels, width, height, x, yi + 1, color, frac);
            } else {
                let step: isize = if dy > 0.0 { 1 } else { -1 };
                let mut yi = round_half_even(prev_y) as isize;
                let y_end = round_half_even(y) as isize;
                loop {
                    let mut t = (yi as f64 - prev_y) / dy;
                    t = t.clamp(0.0, 1.0);
                    let xf = prev_x as f64 + t;
                    let xb = xf.floor() as usize;
                    let frac = xf - xb as f64;
                    set_px(pixels, width, height, xb, yi as usize, color, 1.0 - frac);
                    set_px(pixels, width, height, xb + 1, yi as usize, color, frac);
                    if yi == y_end {
                        break;
                    }
                    yi += step;
                }
            }
        } else {
            let yi = y.floor() as usize;
            let frac = y - yi as f64;
            set_px(pixels, width, height, x, yi, color, 1.0 - frac);
            set_px(pixels, width, height, x, yi + 1, color, frac);
        }
        previous = Some((x, y));
    }
}

fn draw_horizontal_grid(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    plot_x0: usize,
    y: usize,
    color: RGBA,
) {
    for x in plot_x0..width {
        set_px(pixels, width, height, x, y, color, 1.0);
    }
}

fn draw_digits(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    text: &str,
    right_x: isize,
    center_y: usize,
    color: RGBA,
) {
    let max_top = height.saturating_sub(DIGIT_HEIGHT);
    let mut top = center_y.saturating_sub(DIGIT_HEIGHT / 2);
    if top > max_top {
        top = max_top;
    }

    let mut x = right_x;
    for ch in text.chars().rev() {
        x -= DIGIT_WIDTH as isize;
        if let Some(glyph) = digit_glyph(ch) {
            for (row_index, row) in glyph.iter().enumerate() {
                for (col_index, bit) in row.iter().enumerate() {
                    if *bit == b'1' {
                        set_px_signed(
                            pixels,
                            width,
                            height,
                            x + col_index as isize,
                            (top + row_index) as isize,
                            color,
                            1.0,
                        );
                    }
                }
            }
        }
        x -= 1;
    }
}

fn digit_glyph(ch: char) -> Option<&'static [[u8; DIGIT_WIDTH]; DIGIT_HEIGHT]> {
    DIGITS
        .iter()
        .find_map(|(rows, digit)| if *digit == ch { Some(rows) } else { None })
}

fn value_yf(value: f64, vmax: f64, floor_y: usize, span: usize) -> f64 {
    let clamped = if value < 0.0 {
        0.0
    } else if value > vmax {
        vmax
    } else {
        value
    };
    floor_y as f64 - (clamped / vmax) * span as f64
}

fn set_px(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    color: RGBA,
    coverage: f64,
) {
    if x >= width || y >= height {
        return;
    }

    let alpha = (f64::from(color.3) * coverage) as i32;
    if alpha <= 0 {
        return;
    }
    let alpha = alpha as u32;
    let offset = (y * width + x) * 4;
    let dst_alpha = u32::from(pixels[offset + 3]);

    if alpha >= 255 || dst_alpha == 0 {
        pixels[offset] = color.0;
        pixels[offset + 1] = color.1;
        pixels[offset + 2] = color.2;
        pixels[offset + 3] = alpha.min(255) as u8;
        return;
    }

    let inv = 255 - alpha;
    let dst_alpha_scaled = dst_alpha * inv / 255;
    let out_alpha = alpha + dst_alpha_scaled;
    if out_alpha == 0 {
        return;
    }

    let red =
        (u32::from(color.0) * alpha + u32::from(pixels[offset]) * dst_alpha_scaled) / out_alpha;
    let green =
        (u32::from(color.1) * alpha + u32::from(pixels[offset + 1]) * dst_alpha_scaled) / out_alpha;
    let blue =
        (u32::from(color.2) * alpha + u32::from(pixels[offset + 2]) * dst_alpha_scaled) / out_alpha;

    pixels[offset] = red.min(255) as u8;
    pixels[offset + 1] = green.min(255) as u8;
    pixels[offset + 2] = blue.min(255) as u8;
    pixels[offset + 3] = out_alpha.min(255) as u8;
}

fn set_px_signed(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    x: isize,
    y: isize,
    color: RGBA,
    coverage: f64,
) {
    if x < 0 || y < 0 {
        return;
    }
    set_px(
        pixels, width, height, x as usize, y as usize, color, coverage,
    );
}

fn round_half_even(value: f64) -> usize {
    if !value.is_finite() {
        return 0;
    }
    if value <= 0.0 {
        return 0;
    }

    let floor = value.floor();
    let frac = value - floor;
    let rounded = if frac < 0.5 {
        floor
    } else if frac > 0.5 {
        floor + 1.0
    } else if (floor as i64) % 2 == 0 {
        floor
    } else {
        floor + 1.0
    };
    rounded as usize
}

#[cfg(test)]
mod tests;
