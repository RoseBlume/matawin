//! Wireless Bitmap (WBMP) Type 0 decoding.

use crate::ImageError;


const WBMP_WHITE: u32 = 0xffff_ffff;
const WBMP_BLACK: u32 = 0x0000_00ff;

/// Expands one encoded WBMP byte into eight final packed pixels.  The table is
/// generated at compile time, feature-gated with the WBMP decoder, and removes
/// eight mask/branch operations per full input byte from the hot path.
const WBMP_BYTE_PIXELS: [[u32; 8]; 256] = build_wbmp_byte_pixels();

const fn build_wbmp_byte_pixels() -> [[u32; 8]; 256] {
    let mut table = [[0u32; 8]; 256];
    let mut value = 0usize;
    while value < 256 {
        let mut bit = 0usize;
        while bit < 8 {
            table[value][bit] = if (value as u8) & (0x80u8 >> bit) == 0 {
                WBMP_WHITE
            } else {
                WBMP_BLACK
            };
            bit += 1;
        }
        value += 1;
    }
    table
}

/// Represents a decoded Wireless Bitmap image.
pub struct Wbmp {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Decoded RGBA8 bytes retained for the rich decoder API.
    pub data: Vec<u8>,
}

impl Wbmp {
    /// Borrows the rich decoder's RGBA8 output bytes without allocating.
    #[inline]
    pub fn rgba8_ref(&self) -> &[u8] {
        &self.data
    }

    /// Moves the rich decoder's RGBA8 output bytes without copying.
    #[inline]
    pub fn into_rgba8(self) -> Vec<u8> {
        self.data
    }

    /// Consumes the rich decoder and converts its RGBA8 output to the crate's
    /// canonical packed `0xRRGGBBAA` representation.
    #[inline]
    pub fn into_pixels(self) -> Vec<u32> {
        crate::simd::rgba8_to_vec(&self.data)
    }

    /// Decodes a Type 0 WBMP.
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        let (width, height, pixels) = decode_packed(bytes, None)?;
        let mut data = Vec::with_capacity(pixels.len() * 4);
        for pixel in pixels {
            data.extend_from_slice(&[
                (pixel >> 24) as u8,
                (pixel >> 16) as u8,
                (pixel >> 8) as u8,
                pixel as u8,
            ]);
        }
        Ok(Self { width, height, data })
    }

    /// Reads a WBMP multi-byte integer (7 payload bits per byte).
    fn read_mbint(bytes: &[u8], offset: &mut usize) -> Result<u32, ImageError> {
        let mut result = 0u32;
        for _ in 0..5 {
            let byte = *bytes
                .get(*offset)
                .ok_or_else(|| ImageError::new("Unexpected EOF in WBMP header"))?;
            *offset += 1;
            if result > (u32::MAX >> 7) {
                return Err(ImageError::new("WBMP integer overflow"));
            }
            result = (result << 7) | u32::from(byte & 0x7f);
            if byte & 0x80 == 0 {
                return Ok(result);
            }
        }
        Err(ImageError::new("WBMP integer is too long"))
    }
}

/// Lightweight WBMP path with one packed output allocation and no per-pixel
/// division/modulo.
pub(crate) fn decode_image(
    bytes: &[u8],
    options: &crate::DecodeOptions,
) -> Result<crate::Image, ImageError> {
    let (width, height, pixels) = decode_packed(bytes, Some(options))?;
    crate::Image::from_packed_pixels(width, height, pixels, options)
}

fn decode_packed(
    bytes: &[u8],
    options: Option<&crate::DecodeOptions>,
) -> Result<(u32, u32, Vec<u32>), ImageError> {
    if bytes.len() < 4 {
        return Err(ImageError::new("File too small for WBMP"));
    }
    if bytes[0] != 0 || bytes[1] != 0 {
        return Err(ImageError::new("Unsupported WBMP type (only Type 0 supported)"));
    }
    let mut offset = 2usize;
    let width = Wbmp::read_mbint(bytes, &mut offset)?;
    let height = Wbmp::read_mbint(bytes, &mut offset)?;
    if width == 0 || height == 0 {
        return Err(ImageError::new("Invalid WBMP dimensions"));
    }

    let row_bytes = (width as usize).div_ceil(8);
    let encoded_size = row_bytes
        .checked_mul(height as usize)
        .ok_or_else(|| ImageError::new("WBMP encoded size overflow"))?;
    let source = bytes
        .get(offset..offset.saturating_add(encoded_size))
        .ok_or_else(|| ImageError::new("WBMP data truncated"))?;
    let pixel_count = match options {
        Some(options) => options.checked_pixel_count(width, height)?,
        None => (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| ImageError::new("WBMP pixel count overflow"))?,
    };

    let mut pixels = vec![0u32; pixel_count];
    let width_usize = width as usize;

    for y in 0..height as usize {
        let encoded_row = &source[y * row_bytes..(y + 1) * row_bytes];
        let output_row = &mut pixels[y * width_usize..(y + 1) * width_usize];
        let mut x = 0usize;
        for &byte in encoded_row {
            let remaining = output_row.len() - x;
            let count = remaining.min(8);
            output_row[x..x + count].copy_from_slice(&WBMP_BYTE_PIXELS[byte as usize][..count]);
            x += count;
            if x == output_row.len() {
                break;
            }
        }
    }

    Ok((width, height, pixels))
}

super::impl_rgba8_decoder!(Wbmp);
