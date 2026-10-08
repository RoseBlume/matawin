//! Truevision TGA image decoding.

use crate::{ImageError, pixels::pack_rgba};

/// Represents a decoded TGA image.
pub struct Tga {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Decoded RGBA8 channel bytes retained by the rich API.
    pub data: Vec<u8>,
}

impl Tga {
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

    /// Decodes true-color uncompressed or RLE TGA data.
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

/// Lightweight TGA decode that writes packed output directly.
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
    if bytes.len() < 18 {
        return Err(ImageError::new("File too small for TGA header"));
    }
    let id_length = bytes[0] as usize;
    if bytes[1] != 0 {
        return Err(ImageError::new("Color-mapped TGA not supported"));
    }
    let image_type = bytes[2];
    let width = u16::from_le_bytes([bytes[12], bytes[13]]) as u32;
    let height = u16::from_le_bytes([bytes[14], bytes[15]]) as u32;
    if width == 0 || height == 0 {
        return Err(ImageError::new("TGA dimensions cannot be zero"));
    }
    let pixel_size = match bytes[16] {
        24 => 3usize,
        32 => 4usize,
        _ => return Err(ImageError::new("Only 24/32-bit TGA is supported")),
    };
    let descriptor = bytes[17];
    let origin_top = descriptor & 0x20 != 0;
    let origin_right = descriptor & 0x10 != 0;
    let mut offset = 18usize
        .checked_add(id_length)
        .ok_or_else(|| ImageError::new("TGA data offset overflow"))?;
    if offset > bytes.len() {
        return Err(ImageError::new("TGA ID field is truncated"));
    }

    let pixel_count = match options {
        Some(options) => options.checked_pixel_count(width, height)?,
        None => (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| ImageError::new("TGA image size overflow"))?,
    };
    let mut pixels = vec![0u32; pixel_count];

    match image_type {
        2 => decode_raw(
            bytes,
            &mut offset,
            width as usize,
            height as usize,
            pixel_size,
            origin_top,
            origin_right,
            &mut pixels,
        )?,
        10 => decode_rle(
            bytes,
            &mut offset,
            width as usize,
            height as usize,
            pixel_size,
            origin_top,
            origin_right,
            &mut pixels,
        )?,
        _ => return Err(ImageError::new(format!("Unsupported TGA type {image_type}"))),
    }

    Ok((width, height, pixels))
}

#[inline(always)]
fn read_pixel(bytes: &[u8], offset: &mut usize, pixel_size: usize) -> Result<u32, ImageError> {
    let end = offset
        .checked_add(pixel_size)
        .ok_or_else(|| ImageError::new("TGA pixel offset overflow"))?;
    let pixel = bytes
        .get(*offset..end)
        .ok_or_else(|| ImageError::new("Unexpected EOF in TGA pixel data"))?;
    *offset = end;
    let alpha = if pixel_size == 4 { pixel[3] } else { 255 };
    Ok(pack_rgba(pixel[2], pixel[1], pixel[0], alpha))
}

#[inline(always)]
fn destination_index(
    encoded_index: usize,
    width: usize,
    height: usize,
    origin_top: bool,
    origin_right: bool,
) -> usize {
    let encoded_y = encoded_index / width;
    let encoded_x = encoded_index - encoded_y * width;
    let y = if origin_top { encoded_y } else { height - 1 - encoded_y };
    let x = if origin_right { width - 1 - encoded_x } else { encoded_x };
    y * width + x
}

fn decode_raw(
    bytes: &[u8],
    offset: &mut usize,
    width: usize,
    height: usize,
    pixel_size: usize,
    origin_top: bool,
    origin_right: bool,
    out: &mut [u32],
) -> Result<(), ImageError> {
    let required = out
        .len()
        .checked_mul(pixel_size)
        .ok_or_else(|| ImageError::new("TGA raw byte count overflow"))?;
    if bytes.len().saturating_sub(*offset) < required {
        return Err(ImageError::new("Unexpected EOF in TGA data"));
    }

    // The overwhelmingly common top-left, left-to-right layout can write one
    // contiguous destination slice without coordinate arithmetic per pixel.
    if origin_top && !origin_right {
        let source = &bytes[*offset..*offset + required];
        if pixel_size == 4 {
            crate::simd::bgra8_to_u32(source, out);
        } else {
            for (dst, pixel) in out.iter_mut().zip(source.chunks_exact(3)) {
                *dst = pack_rgba(pixel[2], pixel[1], pixel[0], 255);
            }
        }
        *offset += required;
        return Ok(());
    }

    for encoded_index in 0..out.len() {
        let pixel = read_pixel(bytes, offset, pixel_size)?;
        let index = destination_index(encoded_index, width, height, origin_top, origin_right);
        out[index] = pixel;
    }
    Ok(())
}

fn decode_rle(
    bytes: &[u8],
    offset: &mut usize,
    width: usize,
    height: usize,
    pixel_size: usize,
    origin_top: bool,
    origin_right: bool,
    out: &mut [u32],
) -> Result<(), ImageError> {
    let mut encoded_index = 0usize;
    while encoded_index < out.len() {
        let header = *bytes
            .get(*offset)
            .ok_or_else(|| ImageError::new("Unexpected EOF in TGA RLE data"))?;
        *offset += 1;
        let count = usize::from(header & 0x7f) + 1;
        let end_index = encoded_index
            .checked_add(count)
            .ok_or_else(|| ImageError::new("TGA RLE run overflow"))?;
        if end_index > out.len() {
            return Err(ImageError::new("TGA RLE packet exceeds image dimensions"));
        }

        if header & 0x80 != 0 {
            let pixel = read_pixel(bytes, offset, pixel_size)?;
            // A natural orientation gives a contiguous output range and can use
            // slice::fill for repeated packets.
            if origin_top && !origin_right {
                out[encoded_index..end_index].fill(pixel);
            } else {
                for logical in encoded_index..end_index {
                    let index = destination_index(logical, width, height, origin_top, origin_right);
                    out[index] = pixel;
                }
            }
        } else if origin_top && !origin_right {
            let byte_count = count
                .checked_mul(pixel_size)
                .ok_or_else(|| ImageError::new("TGA raw RLE packet size overflow"))?;
            let end = offset
                .checked_add(byte_count)
                .ok_or_else(|| ImageError::new("TGA raw RLE packet offset overflow"))?;
            let source = bytes
                .get(*offset..end)
                .ok_or_else(|| ImageError::new("Unexpected EOF in TGA raw RLE packet"))?;
            let destination = &mut out[encoded_index..end_index];
            if pixel_size == 4 {
                crate::simd::bgra8_to_u32(source, destination);
            } else {
                for (dst, pixel) in destination.iter_mut().zip(source.chunks_exact(3)) {
                    *dst = pack_rgba(pixel[2], pixel[1], pixel[0], 255);
                }
            }
            *offset = end;
        } else {
            for logical in encoded_index..end_index {
                let pixel = read_pixel(bytes, offset, pixel_size)?;
                let index = destination_index(logical, width, height, origin_top, origin_right);
                out[index] = pixel;
            }
        }
        encoded_index = end_index;
    }
    Ok(())
}

super::impl_rgba8_decoder!(Tga);
