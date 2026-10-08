//! PNG chunk parsing support.
//!
//! This module contains the internal chunk parsing routines used by the PNG decoder.

use std::io::Read;

use crate::ImageError;

use super::crc::crc32_chunk;

// ============================================================================
// PNG CHUNK
// ============================================================================

// #[derive(Debug, Clone)]
// pub struct PngChunk {
//     pub chunk_type: [u8; 4],

//     pub data: Vec<u8>,
// }

// impl PngChunk {
//     pub fn read<R: Read>(
//         reader: &mut R,
//     ) -> Result<Self, ImageError> {
//         let (
//             length,
//             chunk_type,
//         ) =
//             read_chunk_header(
//                 reader
//             )?;

//         let mut data =
//             vec![0u8; length];

//         reader
//             .read_exact(
//                 &mut data
//             )
//             .map_err(|e| {
//                 ImageError::new(
//                     format!(
//                         "Failed to read PNG chunk data: {e}"
//                     )
//                 )
//             })?;

//         let expected_crc =
//             read_chunk_crc(
//                 reader
//             )?;

//         validate_chunk_crc(
//             &chunk_type,
//             &data,
//             expected_crc,
//         )?;

//         Ok(Self {
//             chunk_type,
//             data,
//         })
//     }
// }

// ============================================================================
// CHUNK HEADER
// ============================================================================

#[inline]
/// Reads chunk header from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
pub(crate) fn read_chunk_header<R: Read>(
    reader: &mut R,
) -> Result<
    (usize, [u8; 4]),
    ImageError,
> {
    let mut length_bytes =
        [0u8; 4];

    reader
        .read_exact(
            &mut length_bytes
        )
        .map_err(|e| {
            ImageError::new(
                format!(
                    "Failed to read PNG chunk length: {e}"
                )
            )
        })?;

    let length =
        u32::from_be_bytes(
            length_bytes
        ) as usize;

    let mut chunk_type =
        [0u8; 4];

    reader
        .read_exact(
            &mut chunk_type
        )
        .map_err(|e| {
            ImageError::new(
                format!(
                    "Failed to read PNG chunk type: {e}"
                )
            )
        })?;

    Ok((
        length,
        chunk_type,
    ))
}

// ============================================================================
// CHUNK CRC
// ============================================================================

#[inline]
/// Reads chunk crc from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
pub(crate) fn read_chunk_crc<R: Read>(
    reader: &mut R,
) -> Result<u32, ImageError> {
    let mut bytes =
        [0u8; 4];

    reader
        .read_exact(
            &mut bytes
        )
        .map_err(|e| {
            ImageError::new(
                format!(
                    "Failed to read PNG chunk CRC: {e}"
                )
            )
        })?;

    Ok(
        u32::from_be_bytes(
            bytes
        )
    )
}

#[inline]
/// Validates chunk crc and reports whether it satisfies the format's structural requirements.
///
/// Returns an error when the supplied data cannot be processed as required.
pub(crate) fn validate_chunk_crc(
    chunk_type: &[u8; 4],
    data: &[u8],
    expected: u32,
) -> Result<(), ImageError> {
    let actual =
        crc32_chunk(
            chunk_type,
            data,
        );

    if actual != expected {
        return Err(
            ImageError::new(
                format!(
                    "Invalid PNG CRC for chunk {}",
                    String::from_utf8_lossy(
                        chunk_type
                    )
                )
            )
        );
    }

    Ok(())
}
