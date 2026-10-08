//! PNG pixel and scanline decoding support.
//!
//! This module contains the internal pixel and scanline decoding routines used by the PNG decoder.

use std::io::{self, Read};

use crate::pixels::pack_rgba;
use crate::simd::PngSimd;

use super::compression::{
    decompress_zlib_chunks,
    decompress_zlib_chunks_reuse,
    decompress_zlib_reader_chunks_reuse,
};
use super::helpers::invalid_data;

use super::{
    filter::{
        unfilter_first_row,
        unfilter_scanline,
    },
    Png,
    PngColorType,
    Transparency,
};

// ============================================================================
// PNG DECODER
// ============================================================================


/// Decode PNG image data directly into packed RGBA pixels.
///
/// Each pixel is represented as:
///
/// ```text
/// 0xRRGGBBAA
/// ```
///
/// This decoder:
///
/// - is dependency free
/// - detects SIMD support once per image
/// - allocates the decompressed image once
/// - inflates directly into the decompressed image buffer
/// - reconstructs scanlines in place
/// - performs no scanline copies
/// - performs no per-row allocations
/// - writes directly into the final `Vec<u32>`
pub fn decode_png_pixels(png: &Png) -> io::Result<Vec<u32>> {
    decode_png_pixels_with_checksum(png, true)
}

/// Lightweight variant allowing the caller to explicitly disable zlib checksum
/// verification. PNG structural validation remains enabled either way.
pub(crate) fn decode_png_pixels_with_checksum(
    png: &Png,
    verify_checksum: bool,
) -> io::Result<Vec<u32>> {
    decode_png_pixels_impl(png, verify_checksum, None)
}

/// PNG decode path that reuses caller-owned cross-decode scratch storage.
pub(crate) fn decode_png_pixels_with_scratch(
    png: &Png,
    verify_checksum: bool,
    scratch: &mut crate::DecodeScratch,
) -> io::Result<Vec<u32>> {
    decode_png_pixels_impl(png, verify_checksum, Some(scratch))
}

fn decode_png_pixels_impl(
    png: &Png,
    verify_checksum: bool,
    mut scratch: Option<&mut crate::DecodeScratch>,
) -> io::Result<Vec<u32>> {
    png.header.validate()?;

    let width = png.header.width as usize;
    let height = png.header.height as usize;
    let row_bytes = png
        .header
        .row_bytes()
        .ok_or_else(|| invalid_data("PNG row size overflow"))?;
    let filter_bpp = png.header.filter_bpp();
    let stride = row_bytes
        .checked_add(1)
        .ok_or_else(|| invalid_data("PNG stride overflow"))?;
    let decompressed_size = height
        .checked_mul(stride)
        .ok_or_else(|| invalid_data("PNG decompressed size overflow"))?;
    let pixel_count = width
        .checked_mul(height)
        .ok_or_else(|| invalid_data("PNG pixel count overflow"))?;

    let mut pixels = vec![0u32; pixel_count];
    let simd = PngSimd::detect();
    let mut y = 0usize;

    if let Some(scratch) = scratch.as_deref_mut() {
        scratch.png_previous_row.resize(row_bytes, 0);
        scratch.png_previous_row.fill(0);
        {
            let previous = &mut scratch.png_previous_row;
            let history = &mut scratch.png_history;
            let chunk_buffer = &mut scratch.png_chunk;
            let mut consume_row = |chunk: &mut [u8]| -> io::Result<()> {
                if chunk.len() != stride || y >= height {
                    return Err(invalid_data("Invalid PNG decompressed scanline length"));
                }
                let filter = chunk[0];
                let row = &mut chunk[1..];
                if y == 0 {
                    unfilter_first_row(filter, row, filter_bpp, simd)?;
                } else {
                    unfilter_scanline(filter, row, previous, filter_bpp, simd)?;
                }
                let output_start = y * width;
                encode_row_rgba(
                    row,
                    png,
                    &mut pixels[output_start..output_start + width],
                    simd,
                )?;
                previous.copy_from_slice(row);
                y += 1;
                Ok(())
            };
            decompress_zlib_chunks_reuse(
                png.image_data.as_slice(),
                decompressed_size,
                stride,
                verify_checksum,
                history.as_mut(),
                chunk_buffer,
                &mut consume_row,
            )?;
        }
        scratch.enforce_retention_limit();
    } else {
        let mut previous = vec![0u8; row_bytes];
        // The DEFLATE layer retains only its 32 KiB history and one filtered row.
        // Rows are reconstructed in that reusable chunk, converted directly into
        // final pixels, then copied into the single previous-row buffer needed by
        // PNG filters. Peak temporary memory is therefore O(row_bytes + 32 KiB).
        decompress_zlib_chunks(
            png.image_data.as_slice(),
            decompressed_size,
            stride,
            verify_checksum,
            |chunk| {
                if chunk.len() != stride || y >= height {
                    return Err(invalid_data("Invalid PNG decompressed scanline length"));
                }
                let filter = chunk[0];
                let row = &mut chunk[1..];
                if y == 0 {
                    unfilter_first_row(filter, row, filter_bpp, simd)?;
                } else {
                    unfilter_scanline(filter, row, &previous, filter_bpp, simd)?;
                }
                let output_start = y * width;
                encode_row_rgba(
                    row,
                    png,
                    &mut pixels[output_start..output_start + width],
                    simd,
                )?;
                previous.copy_from_slice(row);
                y += 1;
                Ok(())
            },
        )?;
    }

    if y != height {
        return Err(invalid_data("PNG decompressed row count mismatch"));
    }
    Ok(pixels)
}

/// Decodes PNG pixels while consuming the zlib stream directly from a `Read`
/// implementation such as the streaming IDAT chunk reader.
pub(crate) fn decode_png_reader_pixels<R: Read>(
    png: &Png,
    idat: R,
    verify_checksum: bool,
    mut scratch: Option<&mut crate::DecodeScratch>,
) -> io::Result<Vec<u32>> {
    png.header.validate()?;

    let width = png.header.width as usize;
    let height = png.header.height as usize;
    let row_bytes = png.header.row_bytes().ok_or_else(|| invalid_data("PNG row size overflow"))?;
    let filter_bpp = png.header.filter_bpp();
    let stride = row_bytes.checked_add(1).ok_or_else(|| invalid_data("PNG stride overflow"))?;
    let decompressed_size = height.checked_mul(stride)
        .ok_or_else(|| invalid_data("PNG decompressed size overflow"))?;
    let pixel_count = width.checked_mul(height)
        .ok_or_else(|| invalid_data("PNG pixel count overflow"))?;

    let mut pixels = vec![0u32; pixel_count];
    let simd = PngSimd::detect();
    let mut y = 0usize;

    if let Some(scratch) = scratch.as_deref_mut() {
        scratch.png_previous_row.resize(row_bytes, 0);
        scratch.png_previous_row.fill(0);
        let previous = &mut scratch.png_previous_row;
        let history = &mut scratch.png_history;
        let chunk_buffer = &mut scratch.png_chunk;
        let mut consume_row = |chunk: &mut [u8]| -> io::Result<()> {
            if chunk.len() != stride || y >= height {
                return Err(invalid_data("Invalid PNG decompressed scanline length"));
            }
            let filter = chunk[0];
            let row = &mut chunk[1..];
            if y == 0 {
                unfilter_first_row(filter, row, filter_bpp, simd)?;
            } else {
                unfilter_scanline(filter, row, previous, filter_bpp, simd)?;
            }
            let output_start = y * width;
            encode_row_rgba(row, png, &mut pixels[output_start..output_start + width], simd)?;
            previous.copy_from_slice(row);
            y += 1;
            Ok(())
        };
        decompress_zlib_reader_chunks_reuse(
            idat,
            decompressed_size,
            stride,
            verify_checksum,
            history.as_mut(),
            chunk_buffer,
            &mut consume_row,
        )?;
        drop(consume_row);
        scratch.enforce_retention_limit();
    } else {
        let mut history = Box::new([0u8; 32_768]);
        let mut current = Vec::with_capacity(stride);
        let mut previous = vec![0u8; row_bytes];
        let mut consume_row = |chunk: &mut [u8]| -> io::Result<()> {
            if chunk.len() != stride || y >= height {
                return Err(invalid_data("Invalid PNG decompressed scanline length"));
            }
            let filter = chunk[0];
            let row = &mut chunk[1..];
            if y == 0 {
                unfilter_first_row(filter, row, filter_bpp, simd)?;
            } else {
                unfilter_scanline(filter, row, &previous, filter_bpp, simd)?;
            }
            let output_start = y * width;
            encode_row_rgba(row, png, &mut pixels[output_start..output_start + width], simd)?;
            previous.copy_from_slice(row);
            y += 1;
            Ok(())
        };
        decompress_zlib_reader_chunks_reuse(
            idat,
            decompressed_size,
            stride,
            verify_checksum,
            history.as_mut(),
            &mut current,
            &mut consume_row,
        )?;
    }

    if y != height {
        return Err(invalid_data("PNG decompressed row count mismatch"));
    }
    Ok(pixels)
}

// ============================================================================
// ROW CONVERSION
// ============================================================================

#[inline]
/// Converts one reconstructed PNG scanline to packed `0xRRGGBBAA` output pixels.
///
/// Returns an error when the supplied data cannot be processed as required.
fn encode_row_rgba(
    row: &[u8],
    png: &Png,
    output: &mut [u32],
    simd: PngSimd,
) -> io::Result<()> {
    // let width =
    //     output.len();

    let bit_depth =
        png.header.bit_depth;

    // ------------------------------------------------------------------------
    // Common SIMD fast paths
    // ------------------------------------------------------------------------

    match (
        png.header.color_type,
        bit_depth,
    ) {
        (
            PngColorType::Truecolor,
            8,
        ) => {
            // tRNS requires per-pixel alpha testing, so only use the SIMD
            // conversion when no truecolor transparent key exists.
            if !matches!(
                &png.transparency,
                Some(
                    Transparency::Truecolor {
                        ..
                    }
                )
            ) {
                simd.rgb8_to_u32(
                    row,
                    output,
                );

                return Ok(());
            }
        }

        (
            PngColorType::TruecolorWithAlpha,
            8,
        ) => {
            simd.rgba8_to_u32(
                row,
                output,
            );

            return Ok(());
        }

        _ => {}
    }

    match png.header.color_type {
        // ====================================================================
        // GRAYSCALE
        // ====================================================================

        PngColorType::Greyscale => {
            encode_grayscale(
                row,
                output,
                bit_depth,
                &png.transparency,
            )?;
        }

        // ====================================================================
        // TRUECOLOR
        // ====================================================================

        PngColorType::Truecolor => {
            encode_truecolor(
                row,
                output,
                bit_depth,
                &png.transparency,
            )?;
        }

        // ====================================================================
        // INDEXED
        // ====================================================================

        PngColorType::IndexedColor => {
            encode_indexed(
                row,
                output,
                bit_depth,
                png,
            )?;
        }

        // ====================================================================
        // GRAYSCALE + ALPHA
        // ====================================================================

        PngColorType::GreyscaleWithAlpha => {
            encode_grayscale_alpha(
                row,
                output,
                bit_depth,
            )?;
        }

        // ====================================================================
        // TRUECOLOR + ALPHA
        // ====================================================================

        PngColorType::TruecolorWithAlpha => {
            encode_truecolor_alpha(
                row,
                output,
                bit_depth,
            )?;
        }
    }

    Ok(())
}

// ============================================================================
// GRAYSCALE
// ============================================================================

/// Expands a grayscale PNG scanline into packed RGBA output pixels.
///
/// Returns an error when the supplied data cannot be processed as required.
fn encode_grayscale(
    row: &[u8],
    output: &mut [u32],
    bit_depth: u8,
    transparency: &Option<Transparency>,
) -> io::Result<()> {
    let transparent =
        match transparency {
            Some(
                Transparency::Greyscale(
                    value
                )
            ) => Some(*value),

            _ => None,
        };

    match bit_depth {
        // --------------------------------------------------------------------
        // Packed grayscale
        // --------------------------------------------------------------------

        1 | 2 | 4 => {
            let max =
                (
                    1u16
                        << bit_depth
                ) - 1;

            let mut x =
                0usize;

            while x
                < output.len()
            {
                let sample =
                    read_packed_sample(
                        row,
                        x,
                        bit_depth,
                    );

                let gray =
                    (
                        sample as u16
                            * 255
                            / max
                    ) as u8;

                let alpha =
                    if transparent
                        == Some(
                            sample as u16
                        )
                    {
                        0
                    } else {
                        255
                    };

                output[x] =
                    pack_rgba(
                        gray,
                        gray,
                        gray,
                        alpha,
                    );

                x += 1;
            }
        }

        // --------------------------------------------------------------------
        // 8-bit grayscale
        // --------------------------------------------------------------------

        8 => {
            for (
                destination,
                &gray,
            ) in
                output
                    .iter_mut()
                    .zip(
                        row.iter()
                    )
            {
                let alpha =
                    if transparent
                        == Some(
                            gray as u16
                        )
                    {
                        0
                    } else {
                        255
                    };

                *destination =
                    pack_rgba(
                        gray,
                        gray,
                        gray,
                        alpha,
                    );
            }
        }

        // --------------------------------------------------------------------
        // 16-bit grayscale
        // --------------------------------------------------------------------

        16 => {
            for (
                destination,
                sample,
            ) in
                output
                    .iter_mut()
                    .zip(
                        row
                            .chunks_exact(2)
                    )
            {
                let raw =
                    u16::from_be_bytes([
                        sample[0],
                        sample[1],
                    ]);

                let alpha =
                    if transparent
                        == Some(raw)
                    {
                        0
                    } else {
                        255
                    };

                let gray =
                    sample[0];

                *destination =
                    pack_rgba(
                        gray,
                        gray,
                        gray,
                        alpha,
                    );
            }
        }

        _ => {
            return Err(
                unsupported(
                    "Unsupported grayscale bit depth"
                )
            );
        }
    }

    Ok(())
}

// ============================================================================
// TRUECOLOR
// ============================================================================

/// Expands an RGB PNG scanline into packed RGBA output pixels.
///
/// Returns an error when the supplied data cannot be processed as required.
fn encode_truecolor(
    row: &[u8],
    output: &mut [u32],
    bit_depth: u8,
    transparency: &Option<Transparency>,
) -> io::Result<()> {
    let transparent =
        match transparency {
            Some(
                Transparency::Truecolor {
                    red,
                    green,
                    blue,
                }
            ) => {
                Some((
                    *red,
                    *green,
                    *blue,
                ))
            }

            _ => None,
        };

    match bit_depth {
        // --------------------------------------------------------------------
        // RGB8
        // --------------------------------------------------------------------

        8 => {
            for (
                destination,
                sample,
            ) in
                output
                    .iter_mut()
                    .zip(
                        row
                            .chunks_exact(3)
                    )
            {
                let r =
                    sample[0];

                let g =
                    sample[1];

                let b =
                    sample[2];

                let alpha =
                    if transparent
                        == Some((
                            r as u16,
                            g as u16,
                            b as u16,
                        ))
                    {
                        0
                    } else {
                        255
                    };

                *destination =
                    pack_rgba(
                        r,
                        g,
                        b,
                        alpha,
                    );
            }
        }

        // --------------------------------------------------------------------
        // RGB16
        // --------------------------------------------------------------------

        16 => {
            for (
                destination,
                sample,
            ) in
                output
                    .iter_mut()
                    .zip(
                        row
                            .chunks_exact(6)
                    )
            {
                let raw_r =
                    u16::from_be_bytes([
                        sample[0],
                        sample[1],
                    ]);

                let raw_g =
                    u16::from_be_bytes([
                        sample[2],
                        sample[3],
                    ]);

                let raw_b =
                    u16::from_be_bytes([
                        sample[4],
                        sample[5],
                    ]);

                let alpha =
                    if transparent
                        == Some((
                            raw_r,
                            raw_g,
                            raw_b,
                        ))
                    {
                        0
                    } else {
                        255
                    };

                *destination =
                    pack_rgba(
                        sample[0],
                        sample[2],
                        sample[4],
                        alpha,
                    );
            }
        }

        _ => {
            return Err(
                unsupported(
                    "Unsupported truecolor bit depth"
                )
            );
        }
    }

    Ok(())
}

// ============================================================================
// INDEXED
// ============================================================================

/// Expands palette indices from a PNG scanline into packed RGBA output pixels.
///
/// Returns an error when the supplied data cannot be processed as required.
fn encode_indexed(
    row: &[u8],
    output: &mut [u32],
    bit_depth: u8,
    png: &Png,
) -> io::Result<()> {
    let palette =
        png.palette
            .as_ref()
            .ok_or_else(|| {
                invalid_data(
                    "Indexed PNG has no PLTE chunk"
                )
            })?;

    let alpha =
        match &png.transparency {
            Some(
                Transparency::IndexedColor(
                    values
                )
            ) => {
                Some(
                    values.as_slice()
                )
            }

            _ => None,
        };

    if bit_depth == 8 {
        for (
            destination,
            &index_byte,
        ) in
            output
                .iter_mut()
                .zip(
                    row.iter()
                )
        {
            let index =
                index_byte as usize;

            let &(
                r,
                g,
                b,
            ) =
                palette
                    .get(index)
                    .ok_or_else(|| {
                        invalid_data(
                            "Palette index out of bounds"
                        )
                    })?;

            let a =
                alpha
                    .and_then(
                        |values| {
                            values.get(
                                index
                            )
                        }
                    )
                    .copied()
                    .unwrap_or(255);

            *destination =
                pack_rgba(
                    r,
                    g,
                    b,
                    a,
                );
        }

        return Ok(());
    }

    let mut x =
        0usize;

    while x
        < output.len()
    {
        let index =
            read_packed_sample(
                row,
                x,
                bit_depth,
            ) as usize;

        let &(
            r,
            g,
            b,
        ) =
            palette
                .get(index)
                .ok_or_else(|| {
                    invalid_data(
                        "Palette index out of bounds"
                    )
                })?;

        let a =
            alpha
                .and_then(
                    |values| {
                        values.get(
                            index
                        )
                    }
                )
                .copied()
                .unwrap_or(255);

        output[x] =
            pack_rgba(
                r,
                g,
                b,
                a,
            );

        x += 1;
    }

    Ok(())
}

// ============================================================================
// GRAYSCALE + ALPHA
// ============================================================================

/// Expands grayscale-plus-alpha PNG samples into packed RGBA output pixels.
///
/// Returns an error when the supplied data cannot be processed as required.
fn encode_grayscale_alpha(
    row: &[u8],
    output: &mut [u32],
    bit_depth: u8,
) -> io::Result<()> {
    match bit_depth {
        8 => {
            for (
                destination,
                sample,
            ) in
                output
                    .iter_mut()
                    .zip(
                        row
                            .chunks_exact(2)
                    )
            {
                let gray =
                    sample[0];

                *destination =
                    pack_rgba(
                        gray,
                        gray,
                        gray,
                        sample[1],
                    );
            }
        }

        16 => {
            for (
                destination,
                sample,
            ) in
                output
                    .iter_mut()
                    .zip(
                        row
                            .chunks_exact(4)
                    )
            {
                let gray =
                    sample[0];

                *destination =
                    pack_rgba(
                        gray,
                        gray,
                        gray,
                        sample[2],
                    );
            }
        }

        _ => {
            return Err(
                unsupported(
                    "Unsupported grayscale-alpha bit depth"
                )
            );
        }
    }

    Ok(())
}

// ============================================================================
// TRUECOLOR + ALPHA
// ============================================================================

/// Converts RGBA PNG samples into the decoder's packed output representation.
///
/// Returns an error when the supplied data cannot be processed as required.
fn encode_truecolor_alpha(
    row: &[u8],
    output: &mut [u32],
    bit_depth: u8,
) -> io::Result<()> {
    match bit_depth {
        // RGBA8 is normally handled by the SIMD fast path.
        8 => {
            for (
                destination,
                sample,
            ) in
                output
                    .iter_mut()
                    .zip(
                        row
                            .chunks_exact(4)
                    )
            {
                *destination =
                    pack_rgba(
                        sample[0],
                        sample[1],
                        sample[2],
                        sample[3],
                    );
            }
        }

        16 => {
            for (
                destination,
                sample,
            ) in
                output
                    .iter_mut()
                    .zip(
                        row
                            .chunks_exact(8)
                    )
            {
                *destination =
                    pack_rgba(
                        sample[0],
                        sample[2],
                        sample[4],
                        sample[6],
                    );
            }
        }

        _ => {
            return Err(
                unsupported(
                    "Unsupported RGBA bit depth"
                )
            );
        }
    }

    Ok(())
}

// ============================================================================
// PACKED SAMPLES
// ============================================================================

#[inline(always)]
/// Reads packed sample from the supplied input while advancing the relevant cursor or bit position.
fn read_packed_sample(
    row: &[u8],
    index: usize,
    bit_depth: u8,
) -> u8 {
    debug_assert!(
        matches!(
            bit_depth,
            1 | 2 | 4
        )
    );

    let depth =
        bit_depth as usize;

    let samples_per_byte =
        8 / depth;

    let byte =
        row[
            index
                / samples_per_byte
        ];

    let shift =
        8
            - depth
            - (
                index
                    % samples_per_byte
            ) * depth;

    let mask =
        (1u8 << depth) - 1;

    (byte >> shift)
        & mask
}

// ============================================================================
// RGBA
// ============================================================================

// ============================================================================
// ERRORS
// ============================================================================

#[inline(never)]
/// Constructs an unsupported-feature I/O error with the supplied message.
fn unsupported(
    message: &'static str,
) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        message,
    )
}
