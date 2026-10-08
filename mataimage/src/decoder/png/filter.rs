//! PNG scanline filter reconstruction support.
//!
//! This module contains the internal scanline filter reconstruction routines used by the PNG decoder.

use std::io;

use crate::simd::PngSimd;

// ============================================================================
// FILTER DISPATCH
// ============================================================================

/// Reverse a PNG filter on the first scanline.
///
/// There is no previous scanline, which allows several filters to be reduced
/// to cheaper forms.
#[inline]
pub(crate) fn unfilter_first_row(
    filter: u8,
    row: &mut [u8],
    bpp: usize,
    simd: PngSimd,
) -> io::Result<()> {
    match filter {
        // --------------------------------------------------------------------
        // None
        // --------------------------------------------------------------------

        0 => {}

        // --------------------------------------------------------------------
        // Sub
        // --------------------------------------------------------------------

        1 => {
            simd.sub(
                row,
                bpp,
            );
        }

        // --------------------------------------------------------------------
        // Up
        //
        // Above is zero everywhere, therefore no operation is required.
        // --------------------------------------------------------------------

        2 => {}

        // --------------------------------------------------------------------
        // Average
        // --------------------------------------------------------------------

        3 => {
            unfilter_average_first(
                row,
                bpp,
            );
        }

        // --------------------------------------------------------------------
        // Paeth
        //
        // With above = 0 and upper-left = 0, Paeth always chooses left.
        // Therefore Paeth becomes Sub.
        // --------------------------------------------------------------------

        4 => {
            simd.sub(
                row,
                bpp,
            );
        }

        _ => {
            return Err(
                invalid_filter()
            );
        }
    }

    Ok(())
}

/// Reverse a PNG filter on a normal scanline.
#[inline]
pub(crate) fn unfilter_scanline(
    filter: u8,
    row: &mut [u8],
    previous: &[u8],
    bpp: usize,
    simd: PngSimd,
) -> io::Result<()> {
    debug_assert_eq!(
        row.len(),
        previous.len()
    );

    match filter {
        0 => {}

        1 => {
            simd.sub(
                row,
                bpp,
            );
        }

        2 => {
            simd.up(
                row,
                previous,
            );
        }

        3 => {
            unfilter_average(
                row,
                previous,
                bpp,
            );
        }

        4 => {
            unfilter_paeth(
                row,
                previous,
                bpp,
            );
        }

        _ => {
            return Err(
                invalid_filter()
            );
        }
    }

    Ok(())
}

// ============================================================================
// AVERAGE
// ============================================================================

/// Reverse Average on the first scanline.
///
/// For the first pixel, both left and above are zero.
///
/// After the first pixel:
///
/// ```text
/// predictor = left / 2
/// ```
#[inline]
fn unfilter_average_first(
    row: &mut [u8],
    bpp: usize,
) {
    let len =
        row.len();

    let mut i =
        bpp.min(len);

    // SAFETY:
    //
    // i begins >= bpp, so i - bpp is valid.
    // i is checked against len before every access.
    unsafe {
        let ptr =
            row.as_mut_ptr();

        while i < len {
            let left =
                *ptr.add(
                    i - bpp
                );

            let predictor =
                left >> 1;

            let current =
                ptr.add(i);

            *current =
                (*current)
                    .wrapping_add(
                        predictor
                    );

            i += 1;
        }
    }
}

/// Reverse the PNG Average filter.
///
/// The first pixel is separated from the main loop so there is no
/// `i >= bpp` branch in the hot path.
#[inline]
fn unfilter_average(
    row: &mut [u8],
    previous: &[u8],
    bpp: usize,
) {
    let len =
        row.len();

    let prefix =
        bpp.min(len);

    unsafe {
        let row_ptr =
            row.as_mut_ptr();

        let previous_ptr =
            previous.as_ptr();

        let mut i =
            0usize;

        // --------------------------------------------------------------------
        // First pixel
        // --------------------------------------------------------------------

        while i < prefix {
            let above =
                *previous_ptr.add(i);

            let current =
                row_ptr.add(i);

            *current =
                (*current)
                    .wrapping_add(
                        above >> 1
                    );

            i += 1;
        }

        // --------------------------------------------------------------------
        // Main loop
        // --------------------------------------------------------------------

        while i < len {
            let left =
                *row_ptr.add(
                    i - bpp
                );

            let above =
                *previous_ptr.add(i);

            let predictor =
                (
                    left as u16
                        + above as u16
                ) >> 1;

            let current =
                row_ptr.add(i);

            *current =
                (*current)
                    .wrapping_add(
                        predictor as u8
                    );

            i += 1;
        }
    }
}

// ============================================================================
// PAETH
// ============================================================================

/// Reverse the PNG Paeth filter.
#[inline]
fn unfilter_paeth(
    row: &mut [u8],
    previous: &[u8],
    bpp: usize,
) {
    let len =
        row.len();

    let prefix =
        bpp.min(len);

    unsafe {
        let row_ptr =
            row.as_mut_ptr();

        let previous_ptr =
            previous.as_ptr();

        let mut i =
            0usize;

        // --------------------------------------------------------------------
        // First pixel
        //
        // With no left or upper-left sample, Paeth chooses above.
        // --------------------------------------------------------------------

        while i < prefix {
            let above =
                *previous_ptr.add(i);

            let current =
                row_ptr.add(i);

            *current =
                (*current)
                    .wrapping_add(
                        above
                    );

            i += 1;
        }

        // --------------------------------------------------------------------
        // Main loop
        // --------------------------------------------------------------------

        while i < len {
            let left =
                *row_ptr.add(
                    i - bpp
                );

            let above =
                *previous_ptr.add(i);

            let upper_left =
                *previous_ptr.add(
                    i - bpp
                );

            let predictor =
                paeth_predictor(
                    left,
                    above,
                    upper_left,
                );

            let current =
                row_ptr.add(i);

            *current =
                (*current)
                    .wrapping_add(
                        predictor
                    );

            i += 1;
        }
    }
}

// ============================================================================
// PAETH PREDICTOR
// ============================================================================

#[inline(always)]
/// Computes the PNG Paeth predictor from the left, upper, and upper-left neighboring bytes.
fn paeth_predictor(
    left: u8,
    above: u8,
    upper_left: u8,
) -> u8 {
    let a =
        left as i16;

    let b =
        above as i16;

    let c =
        upper_left as i16;

    // Equivalent to:
    //
    // p  = a + b - c
    // pa = abs(p - a)
    // pb = abs(p - b)
    // pc = abs(p - c)

    let pa =
        (b - c).abs();

    let pb =
        (a - c).abs();

    let pc =
        (
            a
                + b
                - c
                - c
        )
        .abs();

    if pa <= pb
        && pa <= pc
    {
        left
    } else if pb <= pc {
        above
    } else {
        upper_left
    }
}

// ============================================================================
// ERROR
// ============================================================================

#[inline(never)]
/// Constructs an invalid-data error for an unsupported PNG scanline filter type.
fn invalid_filter(
) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Invalid PNG filter type",
    )
}
