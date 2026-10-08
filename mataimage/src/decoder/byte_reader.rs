//! Shared fixed-width byte-slice readers.
//!
//! These helpers cover the identical random-access little-endian reads used by
//! BMP and ICO parsing. Sequential readers with format-specific cursor behavior
//! remain local to their decoders.

use crate::ImageError;

/// Returns an input range or a common truncated-data error.
#[inline]
fn bytes_at(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8], ImageError> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| ImageError::new("Byte offset overflow"))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| ImageError::new("Unexpected end of image data"))
}

/// Reads a random-access little-endian `u16`.
#[inline]
pub(crate) fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16, ImageError> {
    let data = bytes_at(bytes, offset, 2)?;
    Ok(u16::from_le_bytes([data[0], data[1]]))
}

/// Reads a random-access little-endian `u32`.
#[inline]
pub(crate) fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, ImageError> {
    let data = bytes_at(bytes, offset, 4)?;
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// Reads a random-access little-endian `i32`.
#[inline]
pub(crate) fn read_i32_le(bytes: &[u8], offset: usize) -> Result<i32, ImageError> {
    let data = bytes_at(bytes, offset, 4)?;
    Ok(i32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// Reads one byte and advances a caller-owned cursor.
#[inline]
pub(crate) fn read_u8_advance(bytes: &[u8], offset: &mut usize) -> Result<u8, ImageError> {
    let value = bytes
        .get(*offset)
        .copied()
        .ok_or_else(|| ImageError::new("Unexpected end of image data"))?;
    *offset = (*offset)
        .checked_add(1)
        .ok_or_else(|| ImageError::new("Byte offset overflow"))?;
    Ok(value)
}

/// Reads one little-endian `u16` and advances a caller-owned cursor.
#[inline]
pub(crate) fn read_u16_le_advance(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<u16, ImageError> {
    let value = read_u16_le(bytes, *offset)?;
    *offset = (*offset)
        .checked_add(2)
        .ok_or_else(|| ImageError::new("Byte offset overflow"))?;
    Ok(value)
}
