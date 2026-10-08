//! Portable Graymap (PGM) decoding.

use crate::{ImageError, pixels::pack_rgba};

/// Represents a decoded Portable Graymap image.
pub struct Pgm {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Decoded RGBA8 bytes retained for compatibility with the rich decoder API.
    pub data: Vec<u8>,
}

impl Pgm {
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

    /// Decodes a P2/P5 PGM into the rich RGBA8 representation.
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
}

/// Lightweight path: parse numeric tokens directly from byte slices and write
/// packed pixels without constructing per-sample strings or an RGBA byte buffer.
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
    let mut position = 0usize;
    let magic = next_token(bytes, &mut position)
        .ok_or_else(|| ImageError::new("Missing PGM magic number"))?;
    if magic != b"P2" && magic != b"P5" {
        return Err(ImageError::new("Unsupported PGM format (expected P2 or P5)"));
    }

    let width = parse_u32(
        next_token(bytes, &mut position).ok_or_else(|| ImageError::new("Missing PGM width"))?,
    )?;
    let height = parse_u32(
        next_token(bytes, &mut position).ok_or_else(|| ImageError::new("Missing PGM height"))?,
    )?;
    let maxval = parse_u32(
        next_token(bytes, &mut position).ok_or_else(|| ImageError::new("Missing PGM maxval"))?,
    )?;
    if maxval == 0 || maxval > 65_535 {
        return Err(ImageError::new("Unsupported PGM maxval"));
    }

    let pixel_count = match options {
        Some(options) => options.checked_pixel_count(width, height)?,
        None => (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| ImageError::new("PGM image size overflow"))?,
    };
    let mut pixels = vec![0u32; pixel_count];

    if magic == b"P2" {
        for pixel in &mut pixels {
            let value = parse_u32(
                next_token(bytes, &mut position)
                    .ok_or_else(|| ImageError::new("Unexpected EOF in PGM ASCII data"))?,
            )?;
            if value > maxval {
                return Err(ImageError::new("PGM sample exceeds maxval"));
            }
            let gray = scale_to_u8(value, maxval);
            *pixel = pack_rgba(gray, gray, gray, 255);
        }
    } else {
        // Token scanning stops before the delimiter after maxval. Consume the
        // required whitespace separator, including CRLF as one logical separator.
        if !bytes.get(position).is_some_and(u8::is_ascii_whitespace) {
            return Err(ImageError::new("PGM binary header is missing whitespace separator"));
        }
        if bytes[position] == b'\r' && bytes.get(position + 1) == Some(&b'\n') {
            position += 2;
        } else {
            position += 1;
        }

        if maxval <= 255 {
            let source = bytes
                .get(position..position.saturating_add(pixel_count))
                .ok_or_else(|| ImageError::new("Not enough PGM pixel data"))?;
            if maxval == 255 {
                crate::simd::gray8_to_u32(source, &mut pixels);
            } else {
                for (out, &value) in pixels.iter_mut().zip(source) {
                    if value as u32 > maxval {
                        return Err(ImageError::new("PGM sample exceeds maxval"));
                    }
                    let gray = scale_to_u8(value as u32, maxval);
                    *out = pack_rgba(gray, gray, gray, 255);
                }
            }
        } else {
            let needed = pixel_count
                .checked_mul(2)
                .ok_or_else(|| ImageError::new("PGM 16-bit data size overflow"))?;
            let source = bytes
                .get(position..position.saturating_add(needed))
                .ok_or_else(|| ImageError::new("Not enough 16-bit PGM pixel data"))?;
            // Unroll four big-endian samples at a time.  This exposes four
            // independent scale operations to the optimizer and avoids the
            // iterator/chunk bookkeeping on the common 16-bit P5 path.
            let mut source_offset = 0usize;
            let mut pixel_offset = 0usize;
            while source_offset + 8 <= source.len() {
                let values = [
                    u16::from_be_bytes([source[source_offset], source[source_offset + 1]]) as u32,
                    u16::from_be_bytes([source[source_offset + 2], source[source_offset + 3]]) as u32,
                    u16::from_be_bytes([source[source_offset + 4], source[source_offset + 5]]) as u32,
                    u16::from_be_bytes([source[source_offset + 6], source[source_offset + 7]]) as u32,
                ];
                for (slot, value) in pixels[pixel_offset..pixel_offset + 4]
                    .iter_mut()
                    .zip(values)
                {
                    if value > maxval {
                        return Err(ImageError::new("PGM sample exceeds maxval"));
                    }
                    let gray = scale_to_u8(value, maxval);
                    *slot = pack_rgba(gray, gray, gray, 255);
                }
                source_offset += 8;
                pixel_offset += 4;
            }
            while pixel_offset < pixels.len() {
                let value = u16::from_be_bytes([
                    source[source_offset],
                    source[source_offset + 1],
                ]) as u32;
                if value > maxval {
                    return Err(ImageError::new("PGM sample exceeds maxval"));
                }
                let gray = scale_to_u8(value, maxval);
                pixels[pixel_offset] = pack_rgba(gray, gray, gray, 255);
                source_offset += 2;
                pixel_offset += 1;
            }
        }
    }

    Ok((width, height, pixels))
}

#[inline]
fn scale_to_u8(value: u32, maxval: u32) -> u8 {
    if maxval == 255 {
        value as u8
    } else {
        ((value * 255 + maxval / 2) / maxval) as u8
    }
}

fn next_token<'a>(bytes: &'a [u8], position: &mut usize) -> Option<&'a [u8]> {
    loop {
        while bytes.get(*position).is_some_and(u8::is_ascii_whitespace) {
            *position += 1;
        }
        if bytes.get(*position) == Some(&b'#') {
            while let Some(&byte) = bytes.get(*position) {
                *position += 1;
                if byte == b'\n' {
                    break;
                }
            }
            continue;
        }
        break;
    }

    let start = *position;
    while bytes
        .get(*position)
        .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'#')
    {
        *position += 1;
    }
    (start != *position).then_some(&bytes[start..*position])
}

fn parse_u32(bytes: &[u8]) -> Result<u32, ImageError> {
    if bytes.is_empty() {
        return Err(ImageError::new("Missing PGM integer"));
    }
    let mut value = 0u32;
    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return Err(ImageError::new("Invalid PGM integer"));
        }
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(u32::from(byte - b'0')))
            .ok_or_else(|| ImageError::new("PGM integer overflow"))?;
    }
    Ok(value)
}

super::impl_rgba8_decoder!(Pgm);
