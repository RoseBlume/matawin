//! PNG header parsing and validation support.
//!
//! This module contains the internal header parsing and validation routines used by the PNG decoder.

use super::helpers::invalid_data;
use std::io;

use super::PngColorType;

// ============================================================================
// PNG HEADER
// ============================================================================

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
/// Stores the values parsed from the PNG `IHDR` chunk.
pub struct PngHeader {
    /// Stores the image, frame, or plane width in pixels.
    pub width: u32,

    /// Stores the image, frame, or plane height in pixels.
    pub height: u32,

    /// Stores the number of bits used by each PNG sample.
    pub bit_depth: u8,

    /// Stores the PNG color model declared by the `IHDR` chunk.
    pub color_type: PngColorType,

    /// Stores the PNG compression-method identifier from `IHDR`.
    pub compression_method: u8,

    /// Stores the PNG filtering-method identifier from `IHDR`.
    pub filter_method: u8,

    /// Stores the PNG interlace-method identifier from `IHDR`.
    pub interlace_method: u8,
}

impl PngHeader {
    /// Parse the 13-byte IHDR payload.
    #[inline]
    pub fn parse(
        data: &[u8],
    ) -> Option<Self> {
        if data.len() != 13 {
            return None;
        }

        let width =
            u32::from_be_bytes([
                data[0],
                data[1],
                data[2],
                data[3],
            ]);

        let height =
            u32::from_be_bytes([
                data[4],
                data[5],
                data[6],
                data[7],
            ]);

        let bit_depth =
            data[8];

        let color_type =
            PngColorType::try_from(
                data[9]
            )
            .ok()?;

        Some(Self {
            width,
            height,
            bit_depth,
            color_type,
            compression_method: data[10],
            filter_method: data[11],
            interlace_method: data[12],
        })
    }

    /// Validate the parts of IHDR needed by this decoder.
    pub(crate) fn validate(
        &self,
    ) -> io::Result<()> {
        if self.width == 0
            || self.height == 0
        {
            return Err(
                invalid_data(
                    "PNG dimensions must be non-zero"
                )
            );
        }

        if self.compression_method
            != 0
        {
            return Err(
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Unsupported PNG compression method",
                )
            );
        }

        if self.filter_method != 0 {
            return Err(
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Unsupported PNG filter method",
                )
            );
        }

        if self.interlace_method
            != 0
        {
            return Err(
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Adam7 interlaced PNGs are not supported",
                )
            );
        }

        let valid_bit_depth =
            match self.color_type {
                PngColorType::Greyscale => {
                    matches!(
                        self.bit_depth,
                        1
                            | 2
                            | 4
                            | 8
                            | 16
                    )
                }

                PngColorType::Truecolor => {
                    matches!(
                        self.bit_depth,
                        8 | 16
                    )
                }

                PngColorType::IndexedColor => {
                    matches!(
                        self.bit_depth,
                        1
                            | 2
                            | 4
                            | 8
                    )
                }

                PngColorType::GreyscaleWithAlpha => {
                    matches!(
                        self.bit_depth,
                        8 | 16
                    )
                }

                PngColorType::TruecolorWithAlpha => {
                    matches!(
                        self.bit_depth,
                        8 | 16
                    )
                }
            };

        if !valid_bit_depth {
            return Err(
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Unsupported PNG bit depth for color type",
                )
            );
        }

        Ok(())
    }

    /// Number of samples making up one pixel.
    #[inline(always)]
    pub(crate) const fn channels(
        &self,
    ) -> usize {
        match self.color_type {
            PngColorType::Greyscale => 1,

            PngColorType::Truecolor => 3,

            PngColorType::IndexedColor => 1,

            PngColorType::GreyscaleWithAlpha => 2,

            PngColorType::TruecolorWithAlpha => 4,
        }
    }

    /// Number of bits required for one pixel.
    #[inline(always)]
    pub(crate) const fn bits_per_pixel(
        &self,
    ) -> usize {
        self.channels()
            * self.bit_depth as usize
    }

    /// Number of bytes used by the PNG filtering algorithm for one pixel.
    #[inline(always)]
    pub(crate) const fn filter_bpp(
        &self,
    ) -> usize {
        let bits =
            self.bits_per_pixel();

        let bytes =
            (bits + 7) / 8;

        if bytes == 0 {
            1
        } else {
            bytes
        }
    }

    /// Number of encoded bytes making up one scanline, excluding the filter
    /// byte.
    #[inline]
    pub(crate) fn row_bytes(
        &self,
    ) -> Option<usize> {
        let width =
            self.width as usize;

        let bits =
            width.checked_mul(
                self.bits_per_pixel()
            )?;

        let rounded =
            bits.checked_add(7)?;

        Some(
            rounded / 8
        )
    }
}
