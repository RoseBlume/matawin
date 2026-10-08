//! TIFF supporting parser helpers support.
//!
//! This module contains the internal supporting parser helpers routines used by the TIFF decoder.

use crate::error::ImageError;
use std::io::{Read, Seek, SeekFrom};
use super::{
    ByteOrder,
    TiffIFDEntry,
};

/// Return the size in bytes of a TIFF field type.
pub fn field_type_size(field_type: u16) -> Result<usize, ImageError> {
    match field_type {
        1 | 2 | 6 | 7 => Ok(1), // BYTE, ASCII, SBYTE, UNDEFINED
        3 | 8 => Ok(2),          // SHORT, SSHORT
        4 | 9 => Ok(4),          // LONG, SLONG
        5 | 10 => Ok(8),         // RATIONAL, SRATIONAL
        11 => Ok(4),             // FLOAT
        12 => Ok(8),             // DOUBLE
        _ => Err(ImageError::new(format!(
            "Unsupported TIFF field type: {}",
            field_type
        ))),
    }
}


/// Read the raw bytes represented by an IFD entry.
///
/// TIFF stores values of four bytes or less directly inside the
/// `value_offset` field. Larger values are stored elsewhere and
/// `value_offset` contains an offset to those bytes.
pub fn read_entry_bytes<R: Read + Seek>(
    reader: &mut R,
    entry: &TiffIFDEntry,
) -> Result<Vec<u8>, ImageError> {
    let type_size = field_type_size(entry.field_type)?;

    let total_size = type_size
        .checked_mul(entry.count as usize)
        .ok_or_else(|| {
            ImageError::new("TIFF field size overflow")
        })?;

    if total_size <= 4 {
        let bytes = entry.value_offset.to_ne_bytes();

        Ok(bytes[..total_size].to_vec())
    } else {
        reader
            .seek(SeekFrom::Start(
                entry.value_offset as u64
            ))
            .map_err(|e| {
                ImageError::new(format!(
                    "Failed to seek TIFF field value: {}",
                    e
                ))
            })?;

        let mut bytes = vec![0u8; total_size];

        reader
            .read_exact(&mut bytes)
            .map_err(|e| {
                ImageError::new(format!(
                    "Failed to read TIFF field value: {}",
                    e
                ))
            })?;

        Ok(bytes)
    }
}


/// Read a single u16 TIFF value.
pub fn read_u16_value<R: Read + Seek>(
    reader: &mut R,
    entry: &TiffIFDEntry,
    byte_order: &ByteOrder,
) -> Result<u16, ImageError> {
    if entry.count != 1 {
        return Err(ImageError::new(
            "Expected exactly one TIFF value",
        ));
    }

    if entry.field_type != 3 {
        return Err(ImageError::new(
            "Expected TIFF SHORT field",
        ));
    }

    let bytes = read_entry_bytes(reader, entry)?;

    if bytes.len() < 2 {
        return Err(ImageError::new(
            "Invalid TIFF SHORT value",
        ));
    }

    let bytes = [bytes[0], bytes[1]];

    Ok(match byte_order {
        ByteOrder::LittleEndian => u16::from_le_bytes(bytes),
        ByteOrder::BigEndian => u16::from_be_bytes(bytes),
    })
}


/// Read a single u32 TIFF value.
pub fn read_u32_value<R: Read + Seek>(
    reader: &mut R,
    entry: &TiffIFDEntry,
    byte_order: &ByteOrder,
) -> Result<u32, ImageError> {
    if entry.count != 1 {
        return Err(ImageError::new(
            "Expected exactly one TIFF value",
        ));
    }

    if (entry.field_type != 4) && (entry.field_type != 3) {
        let message = format!(
            "Expected TIFF LONG or SHORT field, got type {}",
            entry.field_type
        );
        return Err(ImageError::new(message));
    }

    let bytes = read_entry_bytes(reader, entry)?;

    if entry.field_type == 4 && bytes.len() < 4 {
        return Err(ImageError::new(
            "Invalid TIFF LONG value",
        ));
    }
    if entry.field_type == 3 && bytes.len() < 2 {
        return Err(ImageError::new(
            "Invalid TIFF SHORT value",
        ));
    }

    if entry.field_type == 3 {
        Ok(match byte_order {
            ByteOrder::LittleEndian => u16::from_le_bytes([bytes[0], bytes[1]]) as u32,
            ByteOrder::BigEndian => u16::from_be_bytes([bytes[0], bytes[1]]) as u32,
        })
    }
    else if entry.field_type == 4 {
        Ok(match byte_order {
            ByteOrder::LittleEndian => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            ByteOrder::BigEndian => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        })
    }
    else {
        Err(ImageError::new(
            "Invalid TIFF field type for u32 value",
        ))
    }
}


/// Read an array of u16 TIFF values.
pub fn read_u16_array<R: Read + Seek>(
    reader: &mut R,
    entry: &TiffIFDEntry,
    byte_order: &ByteOrder,
) -> Result<Vec<u16>, ImageError> {
    if entry.field_type != 3 {
        return Err(ImageError::new(
            "Expected TIFF SHORT array",
        ));
    }

    let bytes = read_entry_bytes(reader, entry)?;

    if bytes.len() != entry.count as usize * 2 {
        return Err(ImageError::new(
            "Invalid TIFF SHORT array size",
        ));
    }

    let mut values =
        Vec::with_capacity(entry.count as usize);

    for i in 0..entry.count as usize {
        let start = i * 2;

        let bytes = [
            bytes[start],
            bytes[start + 1],
        ];

        let value = match byte_order {
            ByteOrder::LittleEndian => {
                u16::from_le_bytes(bytes)
            }

            ByteOrder::BigEndian => {
                u16::from_be_bytes(bytes)
            }
        };

        values.push(value);
    }

    Ok(values)
}


/// Read an array of u32 TIFF values.
pub fn read_u32_array<R: Read + Seek>(
    reader: &mut R,
    entry: &TiffIFDEntry,
    byte_order: &ByteOrder,
) -> Result<Vec<u32>, ImageError> {
    if entry.field_type != 4 {
        return Err(ImageError::new(
            "Expected TIFF LONG array",
        ));
    }

    let bytes = read_entry_bytes(reader, entry)?;

    if bytes.len() != entry.count as usize * 4 {
        return Err(ImageError::new(
            "Invalid TIFF LONG array size",
        ));
    }

    let mut values =
        Vec::with_capacity(entry.count as usize);

    for i in 0..entry.count as usize {
        let start = i * 4;

        let bytes = [
            bytes[start],
            bytes[start + 1],
            bytes[start + 2],
            bytes[start + 3],
        ];

        let value = match byte_order {
            ByteOrder::LittleEndian => {
                u32::from_le_bytes(bytes)
            }

            ByteOrder::BigEndian => {
                u32::from_be_bytes(bytes)
            }
        };

        values.push(value);
    }

    Ok(values)
}


/// Read a TIFF RATIONAL value.
pub fn read_rational<R: Read + Seek>(
    reader: &mut R,
    entry: &TiffIFDEntry,
    byte_order: &ByteOrder,
) -> Result<(u32, u32), ImageError> {
    if entry.field_type != 5 {
        return Err(ImageError::new(
            "Expected TIFF RATIONAL field",
        ));
    }

    if entry.count != 1 {
        return Err(ImageError::new(
            "Expected exactly one TIFF RATIONAL",
        ));
    }

    let bytes = read_entry_bytes(reader, entry)?;

    if bytes.len() != 8 {
        return Err(ImageError::new(
            "Invalid TIFF RATIONAL size",
        ));
    }

    let numerator_bytes = [
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
    ];

    let denominator_bytes = [
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
    ];

    let numerator = match byte_order {
        ByteOrder::LittleEndian => {
            u32::from_le_bytes(numerator_bytes)
        }

        ByteOrder::BigEndian => {
            u32::from_be_bytes(numerator_bytes)
        }
    };

    let denominator = match byte_order {
        ByteOrder::LittleEndian => {
            u32::from_le_bytes(denominator_bytes)
        }

        ByteOrder::BigEndian => {
            u32::from_be_bytes(denominator_bytes)
        }
    };

    Ok((numerator, denominator))
}


/// Read an ASCII TIFF field.
pub fn read_ascii_string<R: Read + Seek>(
    reader: &mut R,
    entry: &TiffIFDEntry,
    _byte_order: &ByteOrder,
) -> Result<String, ImageError> {
    if entry.field_type != 2 {
        return Err(ImageError::new(
            "Expected TIFF ASCII field",
        ));
    }

    let buf = read_entry_bytes(reader, entry)?;

    let nul_index = buf
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(buf.len());

    let slice = &buf[..nul_index];

    std::str::from_utf8(slice)
        .map(|s| s.to_string())
        .map_err(|_| {
            ImageError::new(
                "Invalid ASCII string value",
            )
        })
}
