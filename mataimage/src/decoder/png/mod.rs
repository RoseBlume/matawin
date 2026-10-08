//! PNG image decoding implementation.
//!
//! This module parses PNG input, validates format-specific structures, and converts
//! decoded image content into the crate's packed pixel/frame representation.

/// Contains chunk parsing and representation support.
mod chunk;
/// Contains color representation and conversion support.
mod color;
/// Contains checksum calculation and validation support.
mod crc;
/// Contains the decoder's core pixel reconstruction routines.
mod decode;
/// Contains scanline filter reconstruction logic.
mod filter;
/// Contains format header parsing and validation.
mod header;
/// Contains small helpers shared by PNG metadata parsers.
mod helpers;
/// Contains metadata parsing and representation types.
mod metadata;
/// Contains textual metadata parsing support.
mod text;
/// Contains compression and decompression support for this decoder.
pub mod compression;

use std::io::{Cursor, Read};

use chunk::{
    read_chunk_crc,
    read_chunk_header,
    validate_chunk_crc,
};

use text::{
    parse_itxt,
    parse_text,
};

// pub use chunk::PngChunk;

pub use color::{
    Chromaticity,
    ContentLightLevel,
    IccProfile,
    PngColorType,
    SignificantBits,
    Srgb,
    Transparency,
};

// pub use crc::crc32_chunk;

pub use decode::decode_png_pixels;

pub use header::PngHeader;

pub use metadata::{
    Physical,
    Plte,
    Time,
};

pub use text::decompress_ztxt;

use crate::ImageError;

// ============================================================================
// PNG
// ============================================================================

#[derive(Debug, Clone)]
/// Represents a decoded PNG image together with parsed metadata.
pub struct Png {
    /// Stores the parsed PNG `IHDR` header.
    pub header: PngHeader,

    /// Stores the optional RGB palette parsed from the PNG `PLTE` chunk.
    pub palette: Option<Vec<(u8, u8, u8)>>,

    /// Stores the optional standard-sRGB rendering intent metadata.
    pub srgb: Option<Srgb>,

    /// Stores the optional embedded ICC color profile.
    pub icc_profile: Option<IccProfile>,

    /// Stores decoded textual metadata from PNG text chunks.
    pub text_chunks: Vec<String>,

    /// Stores optional physical pixel-dimension metadata from `pHYs`.
    pub physical: Option<Physical>,

    /// Stores the optional PNG image gamma value.
    pub gamma: Option<u32>,

    /// Stores the optional image modification time from `tIME`.
    pub time: Option<Time>,

    /// Stores optional HDR content-light-level metadata.
    pub content_light_level: Option<ContentLightLevel>,

    /// Stores optional primary and white-point chromaticity metadata.
    pub chromaticity: Option<Chromaticity>,

    /// Stores optional transparency information parsed from `tRNS`.
    pub transparency: Option<Transparency>,

    /// Stores optional significant-bit metadata parsed from `sBIT`.
    pub significant_bits: Option<SignificantBits>,

    /// Concatenated compressed IDAT data.
    pub image_data: Vec<u8>,
}

impl Png {
    /// Parse a PNG from a borrowed byte slice.
    #[inline]
    pub fn new(
        bytes: &[u8],
    ) -> Result<Self, ImageError> {
        let mut reader = Cursor::new(bytes);

        Self::parse(&mut reader)
    }

    /// Parse a PNG from an owned byte vector.
    #[inline]
    pub fn from_bytes(
        data: Vec<u8>,
    ) -> Result<Self, ImageError> {
        let mut reader = Cursor::new(data);

        Self::parse(&mut reader)
    }

    /// Parse a PNG from any `Read` source.
    ///
    /// Seeking is not required.
    pub fn parse<R: Read>(
        reader: &mut R,
    ) -> Result<Self, ImageError> {
        // --------------------------------------------------------------------
        // Signature
        // --------------------------------------------------------------------

        const PNG_SIGNATURE: [u8; 8] = [
            137,
            80,
            78,
            71,
            13,
            10,
            26,
            10,
        ];

        let mut signature = [0u8; 8];

        reader
            .read_exact(&mut signature)
            .map_err(|e| {
                ImageError::new(
                    format!(
                        "Failed to read PNG signature: {e}"
                    )
                )
            })?;

        if signature != PNG_SIGNATURE {
            return Err(
                ImageError::new(
                    "Invalid PNG signature"
                )
            );
        }

        // --------------------------------------------------------------------
        // Metadata
        // --------------------------------------------------------------------

        let mut header = None;

        let mut palette = None;

        let mut srgb = None;

        let mut icc_profile = None;

        let mut text_chunks =
            Vec::new();

        let mut physical = None;

        let mut gamma = None;

        let mut time = None;

        let mut content_light_level =
            None;

        let mut chromaticity = None;

        let mut transparency = None;

        let mut significant_bits = None;

        // --------------------------------------------------------------------
        // Compressed image data
        // --------------------------------------------------------------------

        let mut image_data =
            Vec::new();

        // --------------------------------------------------------------------
        // Chunks
        // --------------------------------------------------------------------

        loop {
            let (
                length,
                chunk_type,
            ) =
                read_chunk_header(
                    reader
                )?;

            // =================================================================
            // IDAT fast path
            //
            // Read directly into image_data instead of allocating a temporary
            // chunk buffer and then copying it.
            // =================================================================

            if &chunk_type == b"IDAT" {
                let start =
                    image_data.len();

                let end =
                    start
                        .checked_add(
                            length
                        )
                        .ok_or_else(|| {
                            ImageError::new(
                                "PNG IDAT size overflow"
                            )
                        })?;

                image_data
                    .try_reserve(
                        length
                    )
                    .map_err(|e| {
                        ImageError::new(
                            format!(
                                "Failed to reserve PNG IDAT memory: {e}"
                            )
                        )
                    })?;

                image_data.resize(
                    end,
                    0,
                );

                reader
                    .read_exact(
                        &mut image_data[
                            start..end
                        ]
                    )
                    .map_err(|e| {
                        ImageError::new(
                            format!(
                                "Failed to read PNG IDAT data: {e}"
                            )
                        )
                    })?;

                let expected_crc =
                    read_chunk_crc(
                        reader
                    )?;

                validate_chunk_crc(
                    &chunk_type,
                    &image_data[
                        start..end
                    ],
                    expected_crc,
                )?;

                continue;
            }

            // =================================================================
            // Normal chunk
            // =================================================================

            let mut data =
                vec![0u8; length];

            reader
                .read_exact(
                    &mut data
                )
                .map_err(|e| {
                    ImageError::new(
                        format!(
                            "Failed to read PNG chunk data: {e}"
                        )
                    )
                })?;

            let expected_crc =
                read_chunk_crc(
                    reader
                )?;

            validate_chunk_crc(
                &chunk_type,
                &data,
                expected_crc,
            )?;

            match &chunk_type {
                // ============================================================
                // IHDR
                // ============================================================

                b"IHDR" => {
                    let parsed =
                        PngHeader::parse(
                            &data
                        )
                        .ok_or_else(|| {
                            ImageError::new(
                                "Invalid PNG IHDR"
                            )
                        })?;

                    header =
                        Some(parsed);
                }

                // ============================================================
                // PLTE
                // ============================================================

                b"PLTE" => {
                    if let Some(plte) =
                        Plte::parse(
                            &data
                        )
                    {
                        palette =
                            Some(
                                plte.entries
                            );
                    }
                }

                // ============================================================
                // sRGB
                // ============================================================

                b"sRGB" => {
                    if data.len() == 1 {
                        if let Ok(value) =
                            Srgb::try_from(
                                data[0]
                            )
                        {
                            srgb =
                                Some(value);
                        }
                    }
                }

                // ============================================================
                // sBIT
                // ============================================================

                b"sBIT" => {
                    if let Some(header) =
                        &header
                    {
                        significant_bits =
                            SignificantBits::parse(
                                header.color_type,
                                &data,
                            );
                    }
                }

                // ============================================================
                // iCCP
                // ============================================================

                b"iCCP" => {
                    if let Some(value) =
                        IccProfile::parse(
                            &data
                        )
                    {
                        icc_profile =
                            Some(value);
                    }
                }

                // ============================================================
                // cICP
                //
                // cICP is not an ICC profile. Leave it unparsed until a
                // dedicated Cicp structure is added.
                // ============================================================

                b"cICP" => {}

                // ============================================================
                // cHRM
                // ============================================================

                b"cHRM" => {
                    chromaticity =
                        Chromaticity::parse(
                            &data
                        );
                }

                // ============================================================
                // cLLI
                // ============================================================

                b"cLLI" => {
                    content_light_level =
                        ContentLightLevel::parse(
                            &data
                        );
                }

                // ============================================================
                // mDCV
                // ============================================================

                b"mDCV" => {
                    // Reserved for a future mastering-display structure.
                }

                // ============================================================
                // gAMA
                // ============================================================

                b"gAMA" => {
                    if data.len() == 4 {
                        gamma =
                            Some(
                                u32::from_be_bytes([
                                    data[0],
                                    data[1],
                                    data[2],
                                    data[3],
                                ])
                            );
                    }
                }

                // ============================================================
                // tRNS
                // ============================================================

                b"tRNS" => {
                    if let Some(header) =
                        &header
                    {
                        transparency =
                            Transparency::parse(
                                header.color_type,
                                &data,
                            );
                    }
                }

                // ============================================================
                // tIME
                // ============================================================

                b"tIME" => {
                    time =
                        Time::parse(
                            &data
                        );
                }

                // ============================================================
                // tEXt
                // ============================================================

                b"tEXt" => {
                    if let Some(value) =
                        parse_text(
                            &data
                        )
                    {
                        text_chunks.push(
                            value
                        );
                    }
                }

                // ============================================================
                // zTXt
                // ============================================================

                b"zTXt" => {
                    if let Some(value) =
                        decompress_ztxt(
                            &data
                        )
                    {
                        text_chunks.push(
                            value
                        );
                    }
                }

                // ============================================================
                // iTXt
                // ============================================================

                b"iTXt" => {
                    if let Some(value) =
                        parse_itxt(
                            &data
                        )
                    {
                        text_chunks.push(
                            value
                        );
                    }
                }

                // ============================================================
                // pHYs
                // ============================================================

                b"pHYs" => {
                    physical =
                        Physical::parse(
                            &data
                        );
                }

                // ============================================================
                // IEND
                // ============================================================

                b"IEND" => {
                    break;
                }

                // ============================================================
                // Unknown
                // ============================================================

                _ => {}
            }
        }

        // --------------------------------------------------------------------
        // Required IHDR
        // --------------------------------------------------------------------

        let header =
            header.ok_or_else(|| {
                ImageError::new(
                    "Missing PNG IHDR chunk"
                )
            })?;

        Ok(Self {
            header,
            palette,
            srgb,
            icc_profile,
            text_chunks,
            physical,
            gamma,
            time,
            content_light_level,
            chromaticity,
            transparency,
            significant_bits,
            image_data,
        })
    }

    /// Decodes this rich PNG's pixels without retaining an additional copy.
    pub fn decode_pixels(&self) -> Result<Vec<u32>, ImageError> {
        decode_png_pixels(self)
            .map_err(|error| ImageError::new(format!("PNG pixel decode failed: {error}")))
    }

    /// Consumes this rich PNG and returns its decoded packed pixels.
    ///
    /// PNG stores compressed IDAT data in the rich object, so this performs the
    /// decode once and releases all metadata/compressed storage afterward.
    pub fn into_pixels(self) -> Result<Vec<u32>, ImageError> {
        self.decode_pixels()
    }
}


/// Parses only PNG structures that can affect rendered pixels and decodes
/// directly into the lightweight generic image. Ancillary text/color/physical
/// metadata is skipped without allocating payload buffers.
pub(crate) fn decode_image(
    bytes: &[u8],
    options: &crate::DecodeOptions,
) -> Result<crate::Image, ImageError> {
    decode_image_impl(bytes, options, None)
}

/// Lightweight PNG decode with caller-owned reusable work buffers.
pub(crate) fn decode_image_with_scratch(
    bytes: &[u8],
    options: &crate::DecodeOptions,
    scratch: &mut crate::DecodeScratch,
) -> Result<crate::Image, ImageError> {
    decode_image_impl(bytes, options, Some(scratch))
}

fn decode_image_impl(
    bytes: &[u8],
    options: &crate::DecodeOptions,
    scratch: Option<&mut crate::DecodeScratch>,
) -> Result<crate::Image, ImageError> {
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.get(..8) != Some(SIGNATURE.as_slice()) {
        return Err(ImageError::new("Invalid PNG signature"));
    }

    let mut position = 8usize;
    let mut header: Option<PngHeader> = None;
    let mut palette: Option<Vec<(u8, u8, u8)>> = None;
    let mut transparency: Option<Transparency> = None;
    let mut idat_ranges: Vec<(usize, usize)> = Vec::new();
    let mut saw_iend = false;

    while position < bytes.len() {
        let chunk_header_end = position
            .checked_add(8)
            .ok_or_else(|| ImageError::new("PNG chunk header overflow"))?;
        let chunk_header = bytes
            .get(position..chunk_header_end)
            .ok_or_else(|| ImageError::new("Truncated PNG chunk header"))?;
        let length = u32::from_be_bytes([
            chunk_header[0],
            chunk_header[1],
            chunk_header[2],
            chunk_header[3],
        ]) as usize;
        let chunk_type = [
            chunk_header[4],
            chunk_header[5],
            chunk_header[6],
            chunk_header[7],
        ];
        let data_start = chunk_header_end;
        let data_end = data_start
            .checked_add(length)
            .ok_or_else(|| ImageError::new("PNG chunk size overflow"))?;
        let crc_end = data_end
            .checked_add(4)
            .ok_or_else(|| ImageError::new("PNG chunk CRC offset overflow"))?;
        let payload = bytes
            .get(data_start..data_end)
            .ok_or_else(|| ImageError::new("Truncated PNG chunk payload"))?;
        let crc_bytes = bytes
            .get(data_end..crc_end)
            .ok_or_else(|| ImageError::new("Truncated PNG chunk CRC"))?;

        if options.verify_checksums {
            let expected = u32::from_be_bytes([
                crc_bytes[0],
                crc_bytes[1],
                crc_bytes[2],
                crc_bytes[3],
            ]);
            let actual = crc::crc32_chunk(&chunk_type, payload);
            if expected != actual {
                return Err(ImageError::new(format!(
                    "PNG chunk {:?} has invalid CRC",
                    core::str::from_utf8(&chunk_type).unwrap_or("????")
                )));
            }
        }

        match &chunk_type {
            b"IHDR" => {
                if header.is_some() {
                    return Err(ImageError::new("PNG contains multiple IHDR chunks"));
                }
                let parsed = PngHeader::parse(payload)
                    .ok_or_else(|| ImageError::new("Invalid PNG IHDR"))?;
                parsed
                    .validate()
                    .map_err(|error| ImageError::new(error.to_string()))?;
                options.checked_rgba_bytes(parsed.width, parsed.height)?;
                let row_bytes = parsed
                    .row_bytes()
                    .ok_or_else(|| ImageError::new("PNG row size overflow"))?;
                let stride = row_bytes
                    .checked_add(1)
                    .ok_or_else(|| ImageError::new("PNG stride overflow"))?;
                let decompressed = (parsed.height as usize)
                    .checked_mul(stride)
                    .ok_or_else(|| ImageError::new("PNG decompressed size overflow"))?;
                options.check_decompressed_bytes(decompressed)?;
                header = Some(parsed);
            }
            b"PLTE" => {
                let png_header = header
                    .as_ref()
                    .ok_or_else(|| ImageError::new("PNG PLTE appears before IHDR"))?;
                // Only indexed-color lightweight decoding consumes palette data.
                // Truecolor PNGs may legally carry a suggested palette, but it has
                // no effect on rendered pixels and is therefore skipped here.
                if matches!(png_header.color_type, PngColorType::IndexedColor) {
                    if let Some(parsed) = Plte::parse(payload) {
                        palette = Some(parsed.entries);
                    }
                }
            }
            b"tRNS" => {
                let png_header = header
                    .as_ref()
                    .ok_or_else(|| ImageError::new("PNG tRNS appears before IHDR"))?;
                // Color types with an explicit alpha channel ignore tRNS for
                // pixel reconstruction, so avoid parsing it in the lightweight path.
                if !matches!(png_header.color_type, PngColorType::GreyscaleWithAlpha | PngColorType::TruecolorWithAlpha) {
                    transparency = Transparency::parse(png_header.color_type, payload);
                }
            }
            b"IDAT" => {
                if header.is_none() {
                    return Err(ImageError::new("PNG IDAT appears before IHDR"));
                }
                // Keep only borrowed ranges into the caller's encoded byte slice.
                // The segmented reader below presents these ranges as one zlib
                // stream, eliminating the old concatenated compressed-IDAT copy.
                idat_ranges.push((data_start, data_end));
            }
            b"IEND" => {
                saw_iend = true;
                break;
            }
            _ => {
                // Unknown critical chunks cannot safely be ignored. Ancillary
                // chunks have a lowercase first type byte and are skipped by
                // borrowing their payload directly from the source slice.
                if chunk_type[0].is_ascii_uppercase() {
                    return Err(ImageError::new(format!(
                        "Unsupported critical PNG chunk {:?}",
                        core::str::from_utf8(&chunk_type).unwrap_or("????")
                    )));
                }
            }
        }

        position = crc_end;
    }

    if !saw_iend {
        return Err(ImageError::new("PNG is missing IEND"));
    }
    let header = header.ok_or_else(|| ImageError::new("Missing PNG IHDR chunk"))?;
    if idat_ranges.is_empty() {
        return Err(ImageError::new("PNG contains no IDAT data"));
    }

    let width = header.width;
    let height = header.height;
    let lightweight = Png {
        header,
        palette,
        srgb: None,
        icc_profile: None,
        text_chunks: Vec::new(),
        physical: None,
        gamma: None,
        time: None,
        content_light_level: None,
        chromaticity: None,
        transparency,
        significant_bits: None,
        image_data: Vec::new(),
    };
    let idat = SliceIdatReader::new(bytes, idat_ranges);
    let pixels = decode::decode_png_reader_pixels(
        &lightweight,
        idat,
        options.verify_checksums,
        scratch,
    )
    .map_err(|error| ImageError::new(format!("PNG pixel decode failed: {error}")))?;
    crate::Image::from_packed_pixels(width, height, pixels, options)
}


/// Presents borrowed IDAT payload ranges as one contiguous `Read` stream.
///
/// The input itself remains borrowed and no compressed image bytes are copied.
struct SliceIdatReader<'a> {
    bytes: &'a [u8],
    ranges: Vec<(usize, usize)>,
    range_index: usize,
    position: usize,
}

impl<'a> SliceIdatReader<'a> {
    #[inline]
    fn new(bytes: &'a [u8], ranges: Vec<(usize, usize)>) -> Self {
        let position = ranges.first().map(|range| range.0).unwrap_or(0);
        Self { bytes, ranges, range_index: 0, position }
    }
}

impl Read for SliceIdatReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let mut written = 0usize;
        while written < output.len() && self.range_index < self.ranges.len() {
            let (start, end) = self.ranges[self.range_index];
            if self.position < start {
                self.position = start;
            }
            if self.position >= end {
                self.range_index += 1;
                if let Some((next, _)) = self.ranges.get(self.range_index) {
                    self.position = *next;
                }
                continue;
            }
            let available = end - self.position;
            let count = available.min(output.len() - written);
            output[written..written + count]
                .copy_from_slice(&self.bytes[self.position..self.position + count]);
            self.position += count;
            written += count;
        }
        Ok(written)
    }
}

// ============================================================================
// STREAMING LIGHTWEIGHT DECODER
// ============================================================================

/// Decodes a PNG directly from `Read` without concatenating IDAT chunks or
/// retaining the encoded file in memory.
pub(crate) fn decode_reader<R: Read>(
    reader: &mut R,
    options: &crate::DecodeOptions,
) -> Result<crate::Image, ImageError> {
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    let mut signature = [0u8; 8];
    reader.read_exact(&mut signature)
        .map_err(|error| ImageError::io(format!("Failed to read PNG signature: {error}")))?;
    if &signature != SIGNATURE {
        return Err(ImageError::new("Invalid PNG signature"));
    }

    let mut header: Option<PngHeader> = None;
    let mut palette: Option<Vec<(u8, u8, u8)>> = None;
    let mut transparency: Option<Transparency> = None;

    loop {
        let (length, chunk_type) = read_stream_chunk_header(reader)?;
        match &chunk_type {
            b"IHDR" => {
                if header.is_some() {
                    return Err(ImageError::new("PNG contains multiple IHDR chunks"));
                }
                if length != 13 {
                    return Err(ImageError::new("Invalid PNG IHDR length"));
                }
                let payload = read_stream_chunk_payload(reader, length, chunk_type, options.verify_checksums)?;
                let parsed = PngHeader::parse(&payload)
                    .ok_or_else(|| ImageError::new("Invalid PNG IHDR"))?;
                parsed.validate().map_err(|error| ImageError::new(error.to_string()))?;
                options.checked_rgba_bytes(parsed.width, parsed.height)?;
                let row_bytes = parsed.row_bytes().ok_or_else(|| ImageError::new("PNG row size overflow"))?;
                let stride = row_bytes.checked_add(1).ok_or_else(|| ImageError::new("PNG stride overflow"))?;
                let decompressed = (parsed.height as usize).checked_mul(stride)
                    .ok_or_else(|| ImageError::new("PNG decompressed size overflow"))?;
                options.check_decompressed_bytes(decompressed)?;
                header = Some(parsed);
            }
            b"PLTE" => {
                let png_header = header.as_ref()
                    .ok_or_else(|| ImageError::new("PNG PLTE appears before IHDR"))?;
                if length > 768 || length % 3 != 0 {
                    return Err(ImageError::new("Invalid PNG PLTE length"));
                }
                if matches!(png_header.color_type, PngColorType::IndexedColor) {
                    let payload = read_stream_chunk_payload(reader, length, chunk_type, options.verify_checksums)?;
                    if let Some(parsed) = Plte::parse(&payload) {
                        palette = Some(parsed.entries);
                    }
                } else {
                    // A PLTE chunk on a truecolor image is only a suggested palette.
                    // Stream it past without allocating because it cannot affect pixels.
                    skip_stream_chunk(reader, length, chunk_type, options.verify_checksums)?;
                }
            }
            b"tRNS" => {
                let png_header = header.as_ref()
                    .ok_or_else(|| ImageError::new("PNG tRNS appears before IHDR"))?;
                // The largest valid tRNS payload is one byte per palette entry.
                if length > 256 {
                    return Err(ImageError::new("Invalid PNG tRNS length"));
                }
                if !matches!(png_header.color_type, PngColorType::GreyscaleWithAlpha | PngColorType::TruecolorWithAlpha) {
                    let payload = read_stream_chunk_payload(reader, length, chunk_type, options.verify_checksums)?;
                    transparency = Transparency::parse(png_header.color_type, &payload);
                } else {
                    skip_stream_chunk(reader, length, chunk_type, options.verify_checksums)?;
                }
            }
            b"IDAT" => {
                let header = header.take()
                    .ok_or_else(|| ImageError::new("PNG IDAT appears before IHDR"))?;
                let width = header.width;
                let height = header.height;
                let lightweight = Png {
                    header,
                    palette,
                    srgb: None,
                    icc_profile: None,
                    text_chunks: Vec::new(),
                    physical: None,
                    gamma: None,
                    time: None,
                    content_light_level: None,
                    chromaticity: None,
                    transparency,
                    significant_bits: None,
                    image_data: Vec::new(),
                };

                let mut idat = IdatStream::new(reader, length, options.verify_checksums);
                let pixels = decode::decode_png_reader_pixels(
                    &lightweight,
                    &mut idat,
                    options.verify_checksums,
                    None,
                ).map_err(|error| ImageError::new(format!("PNG streaming pixel decode failed: {error}")))?;
                idat.finish_after_idat()?;
                return crate::Image::from_packed_pixels(width, height, pixels, options);
            }
            b"IEND" => {
                // Consume/validate the chunk so truncated streams do not look
                // like a valid empty PNG.
                skip_stream_chunk(reader, length, chunk_type, options.verify_checksums)?;
                return Err(ImageError::new("PNG contains no IDAT data"));
            }
            _ => {
                if chunk_type[0].is_ascii_uppercase() {
                    return Err(ImageError::new(format!(
                        "Unsupported critical PNG chunk {:?}",
                        core::str::from_utf8(&chunk_type).unwrap_or("????")
                    )));
                }
                // Ancillary metadata is intentionally streamed past for the
                // lightweight path. CRC is updated while bytes are discarded.
                skip_stream_chunk(reader, length, chunk_type, options.verify_checksums)?;
            }
        }
    }
}

fn read_stream_chunk_header<R: Read>(reader: &mut R) -> Result<(usize, [u8; 4]), ImageError> {
    let mut header = [0u8; 8];
    reader.read_exact(&mut header)
        .map_err(|error| ImageError::io(format!("Failed to read PNG chunk header: {error}")))?;
    let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
    Ok((length, [header[4], header[5], header[6], header[7]]))
}

fn read_stream_chunk_payload<R: Read>(
    reader: &mut R,
    length: usize,
    chunk_type: [u8; 4],
    verify: bool,
) -> Result<Vec<u8>, ImageError> {
    let mut payload = Vec::new();
    payload.try_reserve_exact(length)
        .map_err(|_| ImageError::new("Failed to reserve PNG chunk payload"))?;
    payload.resize(length, 0);
    reader.read_exact(&mut payload)
        .map_err(|error| ImageError::io(format!("Failed to read PNG chunk payload: {error}")))?;
    let mut crc_bytes = [0u8; 4];
    reader.read_exact(&mut crc_bytes)
        .map_err(|error| ImageError::io(format!("Failed to read PNG chunk CRC: {error}")))?;
    if verify {
        let expected = u32::from_be_bytes(crc_bytes);
        let actual = crc::crc32_chunk(&chunk_type, &payload);
        if expected != actual {
            return Err(ImageError::new("Invalid PNG chunk CRC"));
        }
    }
    Ok(payload)
}

fn skip_stream_chunk<R: Read>(
    reader: &mut R,
    mut length: usize,
    chunk_type: [u8; 4],
    verify: bool,
) -> Result<(), ImageError> {
    let mut crc_state = crc::crc32_begin_chunk(&chunk_type);
    let mut buffer = [0u8; 8192];
    while length != 0 {
        let take = length.min(buffer.len());
        reader.read_exact(&mut buffer[..take])
            .map_err(|error| ImageError::io(format!("Failed to skip PNG chunk: {error}")))?;
        if verify {
            crc_state = crc::crc32_update_state(crc_state, &buffer[..take]);
        }
        length -= take;
    }
    let mut crc_bytes = [0u8; 4];
    reader.read_exact(&mut crc_bytes)
        .map_err(|error| ImageError::io(format!("Failed to read PNG chunk CRC: {error}")))?;
    if verify && u32::from_be_bytes(crc_bytes) != crc::crc32_finish(crc_state) {
        return Err(ImageError::new("Invalid PNG chunk CRC"));
    }
    Ok(())
}

/// Presents consecutive PNG IDAT payloads as one `Read` stream while validating
/// each chunk CRC during the same refill that feeds the zlib decoder.
struct IdatStream<'a, R: Read> {
    reader: &'a mut R,
    remaining: usize,
    crc_state: u32,
    verify: bool,
    pending_header: Option<(usize, [u8; 4])>,
    done: bool,
}

impl<'a, R: Read> IdatStream<'a, R> {
    fn new(reader: &'a mut R, first_length: usize, verify: bool) -> Self {
        Self {
            reader,
            remaining: first_length,
            crc_state: crc::crc32_begin_chunk(b"IDAT"),
            verify,
            pending_header: None,
            done: false,
        }
    }

    fn finish_current_crc(&mut self) -> std::io::Result<()> {
        let mut crc_bytes = [0u8; 4];
        self.reader.read_exact(&mut crc_bytes)?;
        if self.verify && u32::from_be_bytes(crc_bytes) != crc::crc32_finish(self.crc_state) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid PNG IDAT CRC"));
        }
        Ok(())
    }

    fn read_next_header_io(&mut self) -> std::io::Result<(usize, [u8; 4])> {
        let mut header = [0u8; 8];
        self.reader.read_exact(&mut header)?;
        Ok((
            u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize,
            [header[4], header[5], header[6], header[7]],
        ))
    }

    fn finish_after_idat(mut self) -> Result<(), ImageError> {
        let mut next = self.pending_header.take()
            .ok_or_else(|| ImageError::new("PNG stream ended before IEND"))?;
        loop {
            let (length, chunk_type) = next;
            if &chunk_type == b"IDAT" {
                return Err(ImageError::new("PNG IDAT chunks must be consecutive"));
            }
            if &chunk_type == b"IEND" {
                if length != 0 {
                    return Err(ImageError::new("Invalid PNG IEND length"));
                }
                skip_stream_chunk(self.reader, length, chunk_type, self.verify)?;
                return Ok(());
            }
            if chunk_type[0].is_ascii_uppercase() {
                return Err(ImageError::new(format!(
                    "Unsupported critical PNG chunk {:?}",
                    core::str::from_utf8(&chunk_type).unwrap_or("????")
                )));
            }
            skip_stream_chunk(self.reader, length, chunk_type, self.verify)?;
            next = read_stream_chunk_header(self.reader)?;
        }
    }
}

impl<R: Read> Read for IdatStream<'_, R> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() || self.done {
            return Ok(0);
        }

        let mut written = 0usize;
        while written < output.len() {
            if self.remaining == 0 {
                self.finish_current_crc()?;
                let (length, chunk_type) = self.read_next_header_io()?;
                if &chunk_type != b"IDAT" {
                    self.pending_header = Some((length, chunk_type));
                    self.done = true;
                    break;
                }
                self.remaining = length;
                self.crc_state = crc::crc32_begin_chunk(b"IDAT");
                if self.remaining == 0 {
                    continue;
                }
            }

            let take = self.remaining.min(output.len() - written);
            let count = self.reader.read(&mut output[written..written + take])?;
            if count == 0 {
                return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Truncated PNG IDAT payload"));
            }
            if self.verify {
                self.crc_state = crc::crc32_update_state(
                    self.crc_state,
                    &output[written..written + count],
                );
            }
            self.remaining -= count;
            written += count;
        }
        Ok(written)
    }
}

// ============================================================================
// DECODER IMPLEMENTATION
// ============================================================================

impl super::Decoder for Png {
    #[inline]
    fn width(&self) -> u32 {
        self.header.width
    }

    #[inline]
    fn height(&self) -> u32 {
        self.header.height
    }

    fn into_image(self: Box<Self>) -> Result<crate::Image, ImageError> {
        let width = self.header.width;
        let height = self.header.height;
        let pixels = self.decode_pixels()?;
        crate::Image::from_packed_pixels(
            width,
            height,
            pixels,
            &crate::DecodeOptions::default(),
        )
    }
}

// /// Convert a raw pixel value into 0xRRGGBB
// #[inline(always)]
// fn pixel_to_rgb(pixel: u32) -> u32 {
//     // If your internal Image stores RGBA as 0xRRGGBBAA, strip alpha:
//     (pixel >> 8) & 0x00FFFFFF
// }
