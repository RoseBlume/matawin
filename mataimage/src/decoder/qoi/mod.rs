//! Quite OK Image (QOI) decoding.

use remata_macros::FromPrimitive;
use std::io::{Read, Seek};

use crate::{ImageError, pixels::pack_rgba};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents a decoded QOI image.
pub struct Qoi {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Encoded channel-count declaration.
    pub channels: Option<Channel>,
    /// Encoded color-space hint.
    pub colorspace: Option<ColorSpace>,
    /// Decoded RGBA8 pixel bytes.
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive)]
pub enum Channel {
    #[value(3)]
    /// RGB with implicit opaque alpha.
    RGB,
    #[value(4)]
    /// RGBA.
    RGBA,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive)]
pub enum ColorSpace {
    #[value(0)]
    /// sRGB with linear alpha.
    SRGB,
    #[value(1)]
    /// Linear color and alpha.
    Linear,
}

impl Qoi {
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

    /// Decodes a borrowed QOI stream.
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        let decoded = decode_packed(bytes, None)?;
        Ok(Self::from_decoded(decoded))
    }

    /// Decodes an owned encoded buffer without an additional source copy.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, ImageError> {
        let decoded = decode_packed(&data, None)?;
        Ok(Self::from_decoded(decoded))
    }

    /// Reads a QOI stream and decodes it.
    pub fn parse<R: Read + Seek>(reader: &mut R) -> Result<Self, ImageError> {
        let mut encoded = Vec::new();
        reader
            .read_to_end(&mut encoded)
            .map_err(|error| ImageError::new(format!("Failed to read QOI: {error}")))?;
        Self::from_bytes(encoded)
    }

    fn from_decoded(decoded: DecodedQoi) -> Self {
        let mut data = Vec::with_capacity(decoded.pixels.len() * 4);
        for pixel in decoded.pixels {
            data.extend_from_slice(&[
                (pixel >> 24) as u8,
                (pixel >> 16) as u8,
                (pixel >> 8) as u8,
                pixel as u8,
            ]);
        }
        Self {
            width: decoded.width,
            height: decoded.height,
            channels: Some(decoded.channels),
            colorspace: Some(decoded.colorspace),
            data,
        }
    }
}

struct DecodedQoi {
    width: u32,
    height: u32,
    channels: Channel,
    colorspace: ColorSpace,
    pixels: Vec<u32>,
}

/// Lightweight QOI path that writes the final packed image directly.
pub(crate) fn decode_image(
    bytes: &[u8],
    options: &crate::DecodeOptions,
) -> Result<crate::Image, ImageError> {
    let decoded = decode_packed(bytes, Some(options))?;
    crate::Image::from_packed_pixels(decoded.width, decoded.height, decoded.pixels, options)
}

fn decode_packed(
    bytes: &[u8],
    options: Option<&crate::DecodeOptions>,
) -> Result<DecodedQoi, ImageError> {
    if bytes.len() < 22 || bytes.get(..4) != Some(b"qoif") {
        return Err(ImageError::new("Invalid or truncated QOI header"));
    }
    let width = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let height = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    if width == 0 || height == 0 {
        return Err(ImageError::new("QOI dimensions cannot be zero"));
    }
    let channels = Channel::try_from(bytes[12])
        .map_err(|_| ImageError::new("Invalid QOI channel count"))?;
    let colorspace = ColorSpace::try_from(bytes[13])
        .map_err(|_| ImageError::new("Invalid QOI color-space flag"))?;
    let pixel_count = match options {
        Some(options) => options.checked_pixel_count(width, height)?,
        None => (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| ImageError::new("QOI pixel count overflow"))?,
    };

    let mut output = vec![0u32; pixel_count];
    let mut index = [[0u8; 4]; 64];
    let mut pixel = [0u8, 0u8, 0u8, 255u8];
    let mut source = 14usize;
    let mut destination = 0usize;

    while destination < pixel_count {
        let byte = *bytes
            .get(source)
            .ok_or_else(|| ImageError::new("Unexpected EOF in QOI pixel stream"))?;
        source += 1;

        match byte {
            0xfe => {
                let rgb = bytes
                    .get(source..source + 3)
                    .ok_or_else(|| ImageError::new("Truncated QOI RGB opcode"))?;
                source += 3;
                pixel[0] = rgb[0];
                pixel[1] = rgb[1];
                pixel[2] = rgb[2];
            }
            0xff => {
                let rgba = bytes
                    .get(source..source + 4)
                    .ok_or_else(|| ImageError::new("Truncated QOI RGBA opcode"))?;
                source += 4;
                pixel.copy_from_slice(rgba);
            }
            _ if byte & 0xc0 == 0x00 => {
                pixel = index[(byte & 0x3f) as usize];
            }
            _ if byte & 0xc0 == 0x40 => {
                pixel[0] = pixel[0].wrapping_add(((byte >> 4) & 0x03).wrapping_sub(2));
                pixel[1] = pixel[1].wrapping_add(((byte >> 2) & 0x03).wrapping_sub(2));
                pixel[2] = pixel[2].wrapping_add((byte & 0x03).wrapping_sub(2));
            }
            _ if byte & 0xc0 == 0x80 => {
                let next = *bytes
                    .get(source)
                    .ok_or_else(|| ImageError::new("Truncated QOI LUMA opcode"))?;
                source += 1;
                let dg = (byte & 0x3f).wrapping_sub(32);
                let dr_dg = (next >> 4).wrapping_sub(8);
                let db_dg = (next & 0x0f).wrapping_sub(8);
                pixel[0] = pixel[0].wrapping_add(dg).wrapping_add(dr_dg);
                pixel[1] = pixel[1].wrapping_add(dg);
                pixel[2] = pixel[2].wrapping_add(dg).wrapping_add(db_dg);
            }
            _ => {
                let run = usize::from(byte & 0x3f) + 1;
                let end = destination
                    .checked_add(run)
                    .ok_or_else(|| ImageError::new("QOI run length overflow"))?;
                if end > pixel_count {
                    return Err(ImageError::new("QOI run exceeds image dimensions"));
                }
                let packed = pack_rgba(pixel[0], pixel[1], pixel[2], pixel[3]);
                output[destination..end].fill(packed);
                destination = end;
                index[qoi_hash(pixel)] = pixel;
                continue;
            }
        }

        index[qoi_hash(pixel)] = pixel;
        output[destination] = pack_rgba(pixel[0], pixel[1], pixel[2], pixel[3]);
        destination += 1;
    }

    const END_MARKER: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 1];
    if bytes.get(source..source.saturating_add(8)) != Some(END_MARKER.as_slice()) {
        return Err(ImageError::new("QOI end marker is missing or invalid"));
    }

    Ok(DecodedQoi {
        width,
        height,
        channels,
        colorspace,
        pixels: output,
    })
}

#[inline(always)]
fn qoi_hash(pixel: [u8; 4]) -> usize {
    (usize::from(pixel[0]) * 3
        + usize::from(pixel[1]) * 5
        + usize::from(pixel[2]) * 7
        + usize::from(pixel[3]) * 11)
        & 63
}

super::impl_rgba8_decoder!(Qoi);
