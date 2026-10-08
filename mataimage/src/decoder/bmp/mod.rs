//! BMP image decoding implementation.
//!
//! This module parses BMP input, validates format-specific structures, and converts
//! decoded image content into the crate's packed pixel/frame representation.

use crate::decoder::dib::{mask_is_contiguous, row_stride, ChannelMask};
use crate::decoder::byte_reader::{
    read_i32_le, read_u16_le, read_u32_le, read_u8_advance as read_byte,
};
use crate::ImageError;
use crate::pixels::pack_rgba;

// ============================================================================
// BMP compression constants
// ============================================================================

const BI_RGB: u32 = 0;
const BI_RLE8: u32 = 1;
const BI_RLE4: u32 = 2;
const BI_BITFIELDS: u32 = 3;
const BI_JPEG: u32 = 4;
const BI_PNG: u32 = 5;
const BI_ALPHABITFIELDS: u32 = 6;

// ============================================================================
// BMP
// ============================================================================

#[derive(Debug, Clone)]
/// Represents a decoded BMP image.
pub struct Bmp {
    /// Stores the image, frame, or plane width in pixels.
    pub width: u32,
    /// Stores the image, frame, or plane height in pixels.
    pub height: u32,

    /// Decoded pixels in logical `0xRRGGBBAA` order.
    pub data: Vec<u32>,
}

// ============================================================================
// Internal structures
// ============================================================================

#[derive(Debug, Clone, Copy)]
/// Stores channel bit masks used by masked/bitfield BMP pixel formats.
struct ColorMasks {
    /// Stores the BMP bit mask used to extract the red channel.
    red: u32,
    /// Stores the BMP bit mask used to extract the green channel.
    green: u32,
    /// Stores the BMP bit mask used to extract the blue channel.
    blue: u32,
    /// Stores the BMP bit mask used to extract the alpha channel.
    alpha: u32,
}


#[derive(Debug)]
/// Stores parsed BMP header values required by the pixel decoder.
struct BmpHeader {
    /// Stores the image, frame, or plane width in pixels.
    width: u32,
    /// Stores the image, frame, or plane height in pixels.
    height: u32,

    /// Records whether bitmap rows are encoded from top to bottom rather than bottom to top.
    top_down: bool,

    /// Stores the number of encoded bits used for each BMP pixel.
    bits_per_pixel: u16,
    /// Stores the compression mode declared or detected for the image.
    compression: u32,

    /// Stores the byte offset at which BMP pixel data begins.
    pixel_offset: usize,

    /// Stores the byte offset at which the BMP color palette begins.
    palette_start: usize,
    /// Stores the number of bytes occupied by each BMP palette entry.
    palette_entry_size: usize,
    /// Stores the number of palette colors explicitly declared by the BMP header.
    colors_used: u32,

    /// Stores optional channel masks used by BMP bitfield-compressed pixel formats.
    masks: Option<ColorMasks>,
}

// ============================================================================
// BMP implementation
// ============================================================================

impl Bmp {
    /// Borrows decoded packed pixels without allocating.
    #[inline]
    pub fn pixels_ref(&self) -> &[u32] {
        &self.data
    }

    /// Moves decoded packed pixels out of this rich decoder.
    #[inline]
    pub fn into_pixels(self) -> Vec<u32> {
        self.data
    }

    /// Constructs a new decoder value from the supplied encoded byte slice.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        let header = Self::parse_header(bytes)?;

        let pixel_count = (header.width as usize)
            .checked_mul(header.height as usize)
            .ok_or_else(|| ImageError::new("BMP dimensions overflow"))?;

        let mut data = vec![0u32; pixel_count];

        let palette = if header.bits_per_pixel <= 8 {
            Some(Self::read_palette(bytes, &header)?)
        } else {
            None
        };

        match header.compression {
            // ================================================================
            // Uncompressed
            // ================================================================

            BI_RGB => {
                match header.bits_per_pixel {
                    1 | 4 | 8 => {
                        let palette = palette
                            .as_ref()
                            .ok_or_else(|| {
                                ImageError::new(
                                    "Indexed BMP is missing a palette"
                                )
                            })?;

                        Self::decode_indexed_raw(
                            bytes,
                            &header,
                            palette,
                            &mut data,
                        )?;
                    }

                    16 => {
                        // BI_RGB 16-bit BMP defaults to RGB555.
                        let masks = ColorMasks {
                            red: 0x7C00,
                            green: 0x03E0,
                            blue: 0x001F,
                            alpha: 0,
                        };

                        Self::decode_masked(
                            bytes,
                            &header,
                            masks,
                            2,
                            &mut data,
                        )?;
                    }

                    24 => {
                        Self::decode_24bit(
                            bytes,
                            &header,
                            &mut data,
                        )?;
                    }

                    32 => {
                        Self::decode_32bit_rgb(
                            bytes,
                            &header,
                            &mut data,
                        )?;
                    }

                    _ => {
                        return Err(ImageError::new(
                            format!(
                                "Unsupported BI_RGB bit depth: {}",
                                header.bits_per_pixel
                            )
                        ));
                    }
                }
            }

            // ================================================================
            // RLE8
            // ================================================================

            BI_RLE8 => {
                if header.bits_per_pixel != 8 {
                    return Err(ImageError::new(
                        "BI_RLE8 requires an 8-bit BMP"
                    ));
                }

                if header.top_down {
                    return Err(ImageError::new(
                        "Top-down RLE BMPs are not valid"
                    ));
                }

                let palette = palette
                    .as_ref()
                    .ok_or_else(|| {
                        ImageError::new(
                            "RLE8 BMP is missing a palette"
                        )
                    })?;

                Self::fill_with_palette_zero(
                    &mut data,
                    palette,
                );

                Self::decode_rle8(
                    bytes,
                    &header,
                    palette,
                    &mut data,
                )?;
            }

            // ================================================================
            // RLE4
            // ================================================================

            BI_RLE4 => {
                if header.bits_per_pixel != 4 {
                    return Err(ImageError::new(
                        "BI_RLE4 requires a 4-bit BMP"
                    ));
                }

                if header.top_down {
                    return Err(ImageError::new(
                        "Top-down RLE BMPs are not valid"
                    ));
                }

                let palette = palette
                    .as_ref()
                    .ok_or_else(|| {
                        ImageError::new(
                            "RLE4 BMP is missing a palette"
                        )
                    })?;

                Self::fill_with_palette_zero(
                    &mut data,
                    palette,
                );

                Self::decode_rle4(
                    bytes,
                    &header,
                    palette,
                    &mut data,
                )?;
            }

            // ================================================================
            // Bitfields
            // ================================================================

            BI_BITFIELDS | BI_ALPHABITFIELDS => {
                if header.bits_per_pixel != 16
                    && header.bits_per_pixel != 32
                {
                    return Err(ImageError::new(
                        "BI_BITFIELDS is only supported for 16-bit and \
                         32-bit BMPs"
                    ));
                }

                let masks = header.masks.ok_or_else(|| {
                    ImageError::new(
                        "BMP bitfield compression is missing color masks"
                    )
                })?;

                let bytes_per_pixel =
                    (header.bits_per_pixel / 8) as usize;

                Self::decode_masked(
                    bytes,
                    &header,
                    masks,
                    bytes_per_pixel,
                    &mut data,
                )?;
            }

            // ================================================================
            // Embedded JPEG / PNG
            // ================================================================

            BI_JPEG => {
                return Err(ImageError::new(
                    "BMP containing embedded JPEG data is not supported"
                ));
            }

            BI_PNG => {
                return Err(ImageError::new(
                    "BMP containing embedded PNG data is not supported"
                ));
            }

            _ => {
                return Err(ImageError::new(
                    format!(
                        "Unsupported BMP compression mode: {}",
                        header.compression
                    )
                ));
            }
        }

        Ok(Self {
            width: header.width,
            height: header.height,
            data,
        })
    }

    // ========================================================================
    // Header parsing
    // ========================================================================

    /// Parses header from the supplied encoded or textual input.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn parse_header(
        bytes: &[u8],
    ) -> Result<BmpHeader, ImageError> {
        if bytes.len() < 18 {
            return Err(ImageError::new(
                "File too small for BMP header"
            ));
        }

        if &bytes[..2] != b"BM" {
            return Err(ImageError::new(
                "Not a BMP file"
            ));
        }

        let pixel_offset =
            read_u32_le(bytes, 10)? as usize;

        if pixel_offset > bytes.len() {
            return Err(ImageError::new(
                "BMP pixel offset is outside the file"
            ));
        }

        let dib_size =
            read_u32_le(bytes, 14)? as usize;

        if dib_size < 12 {
            return Err(ImageError::new(
                format!(
                    "Unsupported BMP DIB header size: {}",
                    dib_size
                )
            ));
        }

        let dib_end = 14usize
            .checked_add(dib_size)
            .ok_or_else(|| {
                ImageError::new(
                    "BMP DIB header size overflow"
                )
            })?;

        if dib_end > bytes.len() {
            return Err(ImageError::new(
                "BMP DIB header is truncated"
            ));
        }

        // ====================================================================
        // BITMAPCOREHEADER
        //
        // OS/2 1.x:
        //
        // size   = 12
        // width  = u16
        // height = u16
        // ====================================================================

        if dib_size == 12 {
            let width =
                read_u16_le(bytes, 18)? as u32;

            let height =
                read_u16_le(bytes, 20)? as u32;

            let planes =
                read_u16_le(bytes, 22)?;

            let bits_per_pixel =
                read_u16_le(bytes, 24)?;

            if width == 0 || height == 0 {
                return Err(ImageError::new(
                    "BMP dimensions cannot be zero"
                ));
            }

            if planes != 1 {
                return Err(ImageError::new(
                    "BMP plane count must be 1"
                ));
            }

            let palette_start = 26;

            if pixel_offset < palette_start {
                return Err(ImageError::new(
                    "Invalid BMP pixel offset"
                ));
            }

            return Ok(BmpHeader {
                width,
                height,

                top_down: false,

                bits_per_pixel,
                compression: BI_RGB,

                pixel_offset,

                palette_start,
                palette_entry_size: 3,

                colors_used: 0,

                masks: None,
            });
        }

        // ====================================================================
        // Windows BITMAPINFOHEADER and later
        // ====================================================================

        if dib_size < 40 {
            return Err(ImageError::new(
                format!(
                    "Unsupported BMP DIB header size: {}",
                    dib_size
                )
            ));
        }

        let width_signed =
            read_i32_le(bytes, 18)?;

        let height_signed =
            read_i32_le(bytes, 22)?;

        if width_signed <= 0 {
            return Err(ImageError::new(
                "BMP width must be greater than zero"
            ));
        }

        if height_signed == 0 {
            return Err(ImageError::new(
                "BMP height cannot be zero"
            ));
        }

        let width =
            width_signed as u32;

        let height =
            height_signed
                .checked_abs()
                .ok_or_else(|| {
                    ImageError::new(
                        "BMP height cannot be i32::MIN"
                    )
                })? as u32;

        let top_down =
            height_signed < 0;

        let planes =
            read_u16_le(bytes, 26)?;

        let bits_per_pixel =
            read_u16_le(bytes, 28)?;

        let compression =
            read_u32_le(bytes, 30)?;

        let colors_used =
            read_u32_le(bytes, 46)?;

        if planes != 1 {
            return Err(ImageError::new(
                "BMP plane count must be 1"
            ));
        }

        let mut palette_start =
            dib_end;

        // ====================================================================
        // Bitfield masks
        // ====================================================================

        let masks = if compression == BI_BITFIELDS
            || compression == BI_ALPHABITFIELDS
        {
            if bits_per_pixel != 16
                && bits_per_pixel != 32
            {
                return Err(ImageError::new(
                    "BMP bitfields require 16-bit or 32-bit pixels"
                ));
            }

            let masks = if dib_size >= 52 {
                // Masks are stored inside the DIB header.
                let red =
                    read_u32_le(bytes, 54)?;

                let green =
                    read_u32_le(bytes, 58)?;

                let blue =
                    read_u32_le(bytes, 62)?;

                let alpha = if dib_size >= 56 {
                    read_u32_le(bytes, 66)?
                } else {
                    0
                };

                ColorMasks {
                    red,
                    green,
                    blue,
                    alpha,
                }
            } else {
                // BITMAPINFOHEADER:
                //
                // Masks immediately follow the 40-byte header.

                let mask_start =
                    14 + 40;

                let red =
                    read_u32_le(
                        bytes,
                        mask_start,
                    )?;

                let green =
                    read_u32_le(
                        bytes,
                        mask_start + 4,
                    )?;

                let blue =
                    read_u32_le(
                        bytes,
                        mask_start + 8,
                    )?;

                let alpha =
                    if compression
                        == BI_ALPHABITFIELDS
                    {
                        read_u32_le(
                            bytes,
                            mask_start + 12,
                        )?
                    } else {
                        0
                    };

                palette_start =
                    mask_start
                        + if compression
                            == BI_ALPHABITFIELDS
                        {
                            16
                        } else {
                            12
                        };

                ColorMasks {
                    red,
                    green,
                    blue,
                    alpha,
                }
            };

            validate_masks(masks)?;

            Some(masks)
        } else {
            None
        };

        if pixel_offset < palette_start {
            return Err(ImageError::new(
                "BMP pixel data overlaps its header"
            ));
        }

        Ok(BmpHeader {
            width,
            height,
            top_down,

            bits_per_pixel,
            compression,

            pixel_offset,

            palette_start,
            palette_entry_size: 4,

            colors_used,

            masks,
        })
    }

    // ========================================================================
    // Palette
    // ========================================================================

    /// Reads palette from the supplied input while advancing the relevant cursor or bit position.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn read_palette(
        bytes: &[u8],
        header: &BmpHeader,
    ) -> Result<Vec<[u8; 4]>, ImageError> {
        let bpp =
            header.bits_per_pixel;

        if bpp > 8 {
            return Ok(Vec::new());
        }

        let maximum_colors =
            1usize << bpp;

        let color_count =
            if header.colors_used == 0 {
                maximum_colors
            } else {
                let count =
                    header.colors_used as usize;

                if count > maximum_colors {
                    return Err(ImageError::new(
                        "BMP palette contains more entries than its bit depth allows"
                    ));
                }

                count
            };

        let palette_bytes =
            color_count
                .checked_mul(
                    header.palette_entry_size
                )
                .ok_or_else(|| {
                    ImageError::new(
                        "BMP palette size overflow"
                    )
                })?;

        let palette_end =
            header.palette_start
                .checked_add(
                    palette_bytes
                )
                .ok_or_else(|| {
                    ImageError::new(
                        "BMP palette offset overflow"
                    )
                })?;

        if palette_end > bytes.len()
            || palette_end > header.pixel_offset
        {
            return Err(ImageError::new(
                "BMP palette is truncated"
            ));
        }

        let mut palette =
            Vec::with_capacity(color_count);

        for i in 0..color_count {
            let offset =
                header.palette_start
                    + i * header.palette_entry_size;

            let b =
                bytes[offset];

            let g =
                bytes[offset + 1];

            let r =
                bytes[offset + 2];

            // BMP palette's fourth byte is traditionally reserved,
            // not an alpha byte.
            palette.push([
                r,
                g,
                b,
                255,
            ]);
        }

        Ok(palette)
    }

    // ========================================================================
    // 1 / 4 / 8-bit uncompressed
    // ========================================================================

    /// Decodes indexed raw data into the representation required by the surrounding decoder.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn decode_indexed_raw(
        bytes: &[u8],
        header: &BmpHeader,
        palette: &[[u8; 4]],
        out: &mut [u32],
    ) -> Result<(), ImageError> {
        let width = header.width as usize;
        let height = header.height as usize;
        let bpp = header.bits_per_pixel as usize;
        let row_size = row_stride(header.width, header.bits_per_pixel)?;

        // Pack the palette once. Indexed hot loops can then do one bounds check
        // on the encoded index plus one native u32 load instead of repacking
        // four channels for every destination pixel.
        if palette.len() > 256 {
            return Err(ImageError::new("BMP palette contains too many entries"));
        }
        let mut packed_palette = [0u32; 256];
        for (dst, color) in packed_palette.iter_mut().zip(palette) {
            *dst = pack_rgba(color[0], color[1], color[2], color[3]);
        }

        for y in 0..height {
            let source_y = source_row(y, height, header.top_down);
            let row_start = header.pixel_offset
                .checked_add(source_y.checked_mul(row_size)
                    .ok_or_else(|| ImageError::new("BMP row offset overflow"))?)
                .ok_or_else(|| ImageError::new("BMP row offset overflow"))?;
            let row_end = row_start.checked_add(row_size)
                .ok_or_else(|| ImageError::new("BMP row size overflow"))?;
            let row = bytes.get(row_start..row_end)
                .ok_or_else(|| ImageError::new("Unexpected EOF in indexed BMP pixel data"))?;
            let dst = &mut out[y * width..(y + 1) * width];

            match bpp {
                8 => {
                    for (pixel, &index) in dst.iter_mut().zip(&row[..width]) {
                        let index = index as usize;
                        if index >= palette.len() {
                            return Err(ImageError::new(format!(
                                "BMP palette index {index} is out of range"
                            )));
                        }
                        *pixel = packed_palette[index];
                    }
                }
                4 => {
                    let mut x = 0usize;
                    for &byte in &row[..width.div_ceil(2)] {
                        let high = (byte >> 4) as usize;
                        if high >= palette.len() {
                            return Err(ImageError::new("BMP palette index is out of range"));
                        }
                        dst[x] = packed_palette[high];
                        x += 1;
                        if x == width { break; }
                        let low = (byte & 0x0f) as usize;
                        if low >= palette.len() {
                            return Err(ImageError::new("BMP palette index is out of range"));
                        }
                        dst[x] = packed_palette[low];
                        x += 1;
                    }
                }
                1 => {
                    let mut x = 0usize;
                    for &byte in &row[..width.div_ceil(8)] {
                        for shift in (0..8).rev() {
                            if x == width { break; }
                            let index = ((byte >> shift) & 1) as usize;
                            if index >= palette.len() {
                                return Err(ImageError::new("BMP palette index is out of range"));
                            }
                            dst[x] = packed_palette[index];
                            x += 1;
                        }
                    }
                }
                _ => return Err(ImageError::new("Unsupported indexed BMP bit depth")),
            }
        }

        Ok(())
    }

    // ========================================================================
    // 24-bit BGR
    // ========================================================================

    /// Decodes 24bit data into the representation required by the surrounding decoder.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn decode_24bit(
        bytes: &[u8],
        header: &BmpHeader,
        out: &mut [u32],
    ) -> Result<(), ImageError> {
        let width =
            header.width as usize;

        let height =
            header.height as usize;

        let row_size =
            row_stride(
                header.width,
                24,
            )?;

        for y in 0..height {
            let source_y =
                source_row(
                    y,
                    height,
                    header.top_down,
                );

            let row_start =
                header.pixel_offset
                    .checked_add(
                        source_y
                            .checked_mul(row_size)
                            .ok_or_else(|| {
                                ImageError::new(
                                    "BMP row offset overflow"
                                )
                            })?
                    )
                    .ok_or_else(|| {
                        ImageError::new(
                            "BMP row offset overflow"
                        )
                    })?;

            let required =
                width
                    .checked_mul(3)
                    .ok_or_else(|| {
                        ImageError::new(
                            "BMP row size overflow"
                        )
                    })?;

            if row_start
                .checked_add(required)
                .is_none_or(|end| end > bytes.len())
            {
                return Err(ImageError::new(
                    "Unexpected EOF in 24-bit BMP data"
                ));
            }

            for x in 0..width {
                let src =
                    row_start + x * 3;

                let dst = y * width + x;

                let b = bytes[src];
                let g = bytes[src + 1];
                let r = bytes[src + 2];

                out[dst] = pack_rgba(r, g, b, 255);
            }
        }

        Ok(())
    }

    // ========================================================================
    // 32-bit BI_RGB
    //
    // This is BGRX, not necessarily BGRA.
    //
    // The fourth byte in a traditional BI_RGB BMP is reserved. Treating a
    // zero reserved byte as alpha=0 would make many normal 32-bit BMPs
    // completely transparent.
    // ========================================================================

    /// Decodes 32bit rgb data into the representation required by the surrounding decoder.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn decode_32bit_rgb(
        bytes: &[u8],
        header: &BmpHeader,
        out: &mut [u32],
    ) -> Result<(), ImageError> {
        let width =
            header.width as usize;

        let height =
            header.height as usize;

        let row_size =
            row_stride(
                header.width,
                32,
            )?;

        for y in 0..height {
            let source_y =
                source_row(
                    y,
                    height,
                    header.top_down,
                );

            let row_start =
                header.pixel_offset
                    .checked_add(
                        source_y
                            .checked_mul(row_size)
                            .ok_or_else(|| {
                                ImageError::new(
                                    "BMP row offset overflow"
                                )
                            })?
                    )
                    .ok_or_else(|| {
                        ImageError::new(
                            "BMP row offset overflow"
                        )
                    })?;

            let required =
                width
                    .checked_mul(4)
                    .ok_or_else(|| {
                        ImageError::new(
                            "BMP row size overflow"
                        )
                    })?;

            if row_start
                .checked_add(required)
                .is_none_or(|end| end > bytes.len())
            {
                return Err(ImageError::new(
                    "Unexpected EOF in 32-bit BMP data"
                ));
            }

            for x in 0..width {
                let src =
                    row_start + x * 4;

                let dst = y * width + x;

                let b = bytes[src];
                let g = bytes[src + 1];
                let r = bytes[src + 2];

                // BI_RGB's fourth byte is not reliably alpha.
                out[dst] = pack_rgba(r, g, b, 255);
            }
        }

        Ok(())
    }

    // ========================================================================
    // 16 / 32-bit masked pixels
    // ========================================================================

    /// Decodes masked data into the representation required by the surrounding decoder.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn decode_masked(
        bytes: &[u8],
        header: &BmpHeader,
        masks: ColorMasks,
        bytes_per_pixel: usize,
        out: &mut [u32],
    ) -> Result<(), ImageError> {
        let width =
            header.width as usize;

        let height =
            header.height as usize;

        let row_size =
            row_stride(
                header.width,
                header.bits_per_pixel,
            )?;

        // Mask shape work is invariant for the entire image. Hoist trailing-zero
        // counts and scale ranges out of the pixel loop, with exact fast paths
        // for the standard 4/5/6/8-bit channel widths.
        let red = ChannelMask::new(masks.red);
        let green = ChannelMask::new(masks.green);
        let blue = ChannelMask::new(masks.blue);
        let alpha = ChannelMask::new(masks.alpha);

        for y in 0..height {
            let source_y =
                source_row(
                    y,
                    height,
                    header.top_down,
                );

            let row_start =
                header.pixel_offset
                    .checked_add(
                        source_y
                            .checked_mul(row_size)
                            .ok_or_else(|| {
                                ImageError::new(
                                    "BMP row offset overflow"
                                )
                            })?
                    )
                    .ok_or_else(|| {
                        ImageError::new(
                            "BMP row offset overflow"
                        )
                    })?;

            let required =
                width
                    .checked_mul(bytes_per_pixel)
                    .ok_or_else(|| {
                        ImageError::new(
                            "BMP row size overflow"
                        )
                    })?;

            if row_start
                .checked_add(required)
                .is_none_or(|end| end > bytes.len())
            {
                return Err(ImageError::new(
                    "Unexpected EOF in BMP bitfield data"
                ));
            }

            for x in 0..width {
                let src =
                    row_start
                        + x * bytes_per_pixel;

                let value =
                    match bytes_per_pixel {
                        2 => {
                            u16::from_le_bytes([
                                bytes[src],
                                bytes[src + 1],
                            ]) as u32
                        }

                        4 => {
                            u32::from_le_bytes([
                                bytes[src],
                                bytes[src + 1],
                                bytes[src + 2],
                                bytes[src + 3],
                            ])
                        }

                        _ => unreachable!(),
                    };

                let r = red.extract(value);
                let g = green.extract(value);
                let b = blue.extract(value);
                let a = if masks.alpha == 0 { 255 } else { alpha.extract(value) };

                let dst = y * width + x;
                out[dst] = pack_rgba(r, g, b, a);
            }
        }

        Ok(())
    }

    // ========================================================================
    // RLE8
    // ========================================================================

    /// Decodes rle8 data into the representation required by the surrounding decoder.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn decode_rle8(
        bytes: &[u8],
        header: &BmpHeader,
        palette: &[[u8; 4]],
        out: &mut [u32],
    ) -> Result<(), ImageError> {
        let width =
            header.width as usize;

        let height =
            header.height as usize;

        let mut offset =
            header.pixel_offset;

        let mut x = 0usize;

        // BMP RLE starts at the bottom row.
        let mut encoded_y =
            0usize;

        while offset < bytes.len()
            && encoded_y < height
        {
            let count =
                read_byte(bytes, &mut offset)?;

            let value =
                read_byte(bytes, &mut offset)?;

            if count != 0 {
                // ------------------------------------------------------------
                // Encoded mode
                // ------------------------------------------------------------

                let count =
                    count as usize;

                if x + count > width {
                    return Err(ImageError::new(
                        "RLE8 run exceeds BMP row width"
                    ));
                }

                for _ in 0..count {
                    Self::write_rle_pixel(
                        out,
                        width,
                        height,
                        x,
                        encoded_y,
                        value as usize,
                        palette,
                    )?;

                    x += 1;
                }

                continue;
            }

            match value {
                // ------------------------------------------------------------
                // End of line
                // ------------------------------------------------------------

                0 => {
                    x = 0;

                    encoded_y += 1;
                }

                // ------------------------------------------------------------
                // End of bitmap
                // ------------------------------------------------------------

                1 => {
                    break;
                }

                // ------------------------------------------------------------
                // Delta
                // ------------------------------------------------------------

                2 => {
                    let dx =
                        read_byte(
                            bytes,
                            &mut offset,
                        )? as usize;

                    let dy =
                        read_byte(
                            bytes,
                            &mut offset,
                        )? as usize;

                    x =
                        x.checked_add(dx)
                            .ok_or_else(|| {
                                ImageError::new(
                                    "RLE8 X delta overflow"
                                )
                            })?;

                    encoded_y =
                        encoded_y
                            .checked_add(dy)
                            .ok_or_else(|| {
                                ImageError::new(
                                    "RLE8 Y delta overflow"
                                )
                            })?;

                    if x > width
                        || encoded_y >= height
                    {
                        return Err(ImageError::new(
                            "RLE8 delta moves outside the image"
                        ));
                    }
                }

                // ------------------------------------------------------------
                // Absolute mode
                // ------------------------------------------------------------

                literal_count => {
                    let literal_count =
                        literal_count as usize;

                    if x + literal_count > width {
                        return Err(ImageError::new(
                            "RLE8 absolute run exceeds BMP row width"
                        ));
                    }

                    if offset + literal_count
                        > bytes.len()
                    {
                        return Err(ImageError::new(
                            "Unexpected EOF in RLE8 absolute data"
                        ));
                    }

                    for _ in 0..literal_count {
                        let index =
                            bytes[offset] as usize;

                        offset += 1;

                        Self::write_rle_pixel(
                            out,
                            width,
                            height,
                            x,
                            encoded_y,
                            index,
                            palette,
                        )?;

                        x += 1;
                    }

                    // Absolute RLE data is word aligned.
                    if literal_count & 1 != 0 {
                        if offset >= bytes.len() {
                            return Err(ImageError::new(
                                "Missing RLE8 padding byte"
                            ));
                        }

                        offset += 1;
                    }
                }
            }
        }

        Ok(())
    }

    // ========================================================================
    // RLE4
    // ========================================================================

    /// Decodes rle4 data into the representation required by the surrounding decoder.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn decode_rle4(
        bytes: &[u8],
        header: &BmpHeader,
        palette: &[[u8; 4]],
        out: &mut [u32],
    ) -> Result<(), ImageError> {
        let width =
            header.width as usize;

        let height =
            header.height as usize;

        let mut offset =
            header.pixel_offset;

        let mut x =
            0usize;

        let mut encoded_y =
            0usize;

        while offset < bytes.len()
            && encoded_y < height
        {
            let count =
                read_byte(
                    bytes,
                    &mut offset,
                )?;

            let value =
                read_byte(
                    bytes,
                    &mut offset,
                )?;

            if count != 0 {
                // ------------------------------------------------------------
                // Encoded mode
                //
                // The two nibbles alternate.
                // ------------------------------------------------------------

                let count =
                    count as usize;

                if x + count > width {
                    return Err(ImageError::new(
                        "RLE4 run exceeds BMP row width"
                    ));
                }

                let high =
                    (value >> 4) as usize;

                let low =
                    (value & 0x0F) as usize;

                for i in 0..count {
                    let index =
                        if i & 1 == 0 {
                            high
                        } else {
                            low
                        };

                    Self::write_rle_pixel(
                        out,
                        width,
                        height,
                        x,
                        encoded_y,
                        index,
                        palette,
                    )?;

                    x += 1;
                }

                continue;
            }

            match value {
                // ------------------------------------------------------------
                // End of line
                // ------------------------------------------------------------

                0 => {
                    x = 0;

                    encoded_y += 1;
                }

                // ------------------------------------------------------------
                // End of bitmap
                // ------------------------------------------------------------

                1 => {
                    break;
                }

                // ------------------------------------------------------------
                // Delta
                // ------------------------------------------------------------

                2 => {
                    let dx =
                        read_byte(
                            bytes,
                            &mut offset,
                        )? as usize;

                    let dy =
                        read_byte(
                            bytes,
                            &mut offset,
                        )? as usize;

                    x =
                        x.checked_add(dx)
                            .ok_or_else(|| {
                                ImageError::new(
                                    "RLE4 X delta overflow"
                                )
                            })?;

                    encoded_y =
                        encoded_y
                            .checked_add(dy)
                            .ok_or_else(|| {
                                ImageError::new(
                                    "RLE4 Y delta overflow"
                                )
                            })?;

                    if x > width
                        || encoded_y >= height
                    {
                        return Err(ImageError::new(
                            "RLE4 delta moves outside the image"
                        ));
                    }
                }

                // ------------------------------------------------------------
                // Absolute mode
                // ------------------------------------------------------------

                pixel_count => {
                    let pixel_count =
                        pixel_count as usize;

                    if x + pixel_count > width {
                        return Err(ImageError::new(
                            "RLE4 absolute run exceeds BMP row width"
                        ));
                    }

                    let byte_count =
                        (pixel_count + 1) / 2;

                    if offset + byte_count
                        > bytes.len()
                    {
                        return Err(ImageError::new(
                            "Unexpected EOF in RLE4 absolute data"
                        ));
                    }

                    for i in 0..pixel_count {
                        let byte =
                            bytes[
                                offset + i / 2
                            ];

                        let index =
                            if i & 1 == 0 {
                                byte >> 4
                            } else {
                                byte & 0x0F
                            };

                        Self::write_rle_pixel(
                            out,
                            width,
                            height,
                            x,
                            encoded_y,
                            index as usize,
                            palette,
                        )?;

                        x += 1;
                    }

                    offset += byte_count;

                    // Absolute data is padded to a WORD boundary.
                    if byte_count & 1 != 0 {
                        if offset >= bytes.len() {
                            return Err(ImageError::new(
                                "Missing RLE4 padding byte"
                            ));
                        }

                        offset += 1;
                    }
                }
            }
        }

        Ok(())
    }

    // ========================================================================
    // RLE helpers
    // ========================================================================

    /// Writes rle pixel into the destination buffer using the format required by this decoder.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn write_rle_pixel(
        out: &mut [u32],
        width: usize,
        height: usize,
        x: usize,
        encoded_y: usize,
        palette_index: usize,
        palette: &[[u8; 4]],
    ) -> Result<(), ImageError> {
        if x >= width
            || encoded_y >= height
        {
            return Err(ImageError::new(
                "RLE pixel is outside BMP dimensions"
            ));
        }

        let color =
            palette
                .get(palette_index)
                .ok_or_else(|| {
                    ImageError::new(
                        format!(
                            "BMP palette index {} is out of range",
                            palette_index
                        )
                    )
                })?;

        // RLE BMP data is bottom-up.
        let y =
            height - 1 - encoded_y;

        let dst = y * width + x;
        out[dst] = pack_rgba(color[0], color[1], color[2], color[3]);

        Ok(())
    }

    /// Fills with palette zero in the destination image according to the active decoding or rendering state.
    fn fill_with_palette_zero(
        out: &mut [u32],
        palette: &[[u8; 4]],
    ) {
        let color =
            palette
                .first()
                .copied()
                .unwrap_or([
                    0,
                    0,
                    0,
                    255,
                ]);

        out.fill(pack_rgba(color[0], color[1], color[2], color[3]));
    }
}

// ============================================================================
// Decoder
// ============================================================================

/// Decodes BMP directly into the lightweight image representation.
pub(crate) fn decode_image(
    bytes: &[u8],
    options: &crate::DecodeOptions,
) -> Result<crate::Image, ImageError> {
    let info = crate::ImageInfo::read(bytes)?;
    options.checked_rgba_bytes(info.width, info.height)?;
    let decoded = Bmp::new(bytes)?;
    crate::Image::from_packed_pixels(decoded.width, decoded.height, decoded.data, options)
}

impl super::Decoder for Bmp {
    #[inline]
    fn width(&self) -> u32 { self.width }
    #[inline]
    fn height(&self) -> u32 { self.height }

    fn into_image(self: Box<Self>) -> Result<crate::Image, ImageError> {
        let this = *self;
        crate::Image::from_packed_pixels(
            this.width,
            this.height,
            this.data,
            &crate::DecodeOptions::default(),
        )
    }
}

// ============================================================================
// Helpers
// ============================================================================

#[inline(always)]
/// Maps an output row to the corresponding stored BMP row, accounting for top-down and bottom-up images.
fn source_row(
    output_y: usize,
    height: usize,
    top_down: bool,
) -> usize {
    if top_down {
        output_y
    } else {
        height - 1 - output_y
    }
}

// ============================================================================
// Bitfield decoding
// ============================================================================

/// Validates masks and reports whether it satisfies the format's structural requirements.
///
/// Returns an error when the supplied data cannot be processed as required.
fn validate_masks(
    masks: ColorMasks,
) -> Result<(), ImageError> {
    if masks.red == 0
        || masks.green == 0
        || masks.blue == 0
    {
        return Err(ImageError::new(
            "BMP RGB masks cannot be zero"
        ));
    }

    if masks.red & masks.green != 0
        || masks.red & masks.blue != 0
        || masks.green & masks.blue != 0
    {
        return Err(ImageError::new(
            "BMP RGB masks overlap"
        ));
    }

    if masks.alpha != 0
        && (
            masks.alpha & masks.red != 0
                || masks.alpha & masks.green != 0
                || masks.alpha & masks.blue != 0
        )
    {
        return Err(ImageError::new(
            "BMP alpha mask overlaps an RGB mask"
        ));
    }

    for mask in [
        masks.red,
        masks.green,
        masks.blue,
        masks.alpha,
    ] {
        if mask != 0
            && !mask_is_contiguous(mask)
        {
            return Err(ImageError::new(
                "BMP color masks must contain contiguous bits"
            ));
        }
    }

    Ok(())
}

// ============================================================================
// Byte readers
// ============================================================================
