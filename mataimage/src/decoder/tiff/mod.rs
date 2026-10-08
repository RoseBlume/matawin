//! TIFF image decoding implementation.
//!
//! This module parses TIFF input, validates format-specific structures, and converts
//! decoded image content into the crate's packed pixel/frame representation.

use remata_macros::FromPrimitive;

use std::io::{Cursor, Read, Seek, SeekFrom};

use crate::ImageError;
use crate::pixels::pack_rgba;

/// Contains internal helper routines used by this decoder.
mod helpers;
use helpers::{
    read_ascii_string,
    read_rational,
    read_u16_array,
    read_u32_array,
    read_u16_value,
    read_u32_value,
};
/*
TAGS
PhotometricInterpretation (262)
Compression (259)
ResolutionUnit (296)
ImageLength (257)
ImageWidth (256)
XResolution (282)
YResolution (283)
Software (305)
DateTime (306)
ColorMap (320)
Bits Per Sample (258)
Artist (315)
RowsPerStrip (278)
StripOffsets (273)
StripByteCounts (279)
SamplesPerPixel (277)
Copyright (33432)
FillOrder (266)
FreeByteCounts (289)
FreeOffsets (288)
*/

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents a parsed TIFF container header.
pub struct Tiff {
    /// Stores the parsed TIFF file header.
    pub header: TiffHeader,
}

impl Tiff {

    /// Parses the supplied encoded input into this format-specific representation.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    pub fn parse<R: Read + Seek>(reader: &mut R) -> Result<Self, ImageError> {
        let mut byte_order_bytes = [0u8; 2];

        reader
            .read_exact(&mut byte_order_bytes)
            .map_err(|e| {
                ImageError::new(format!("Failed to read byte order: {}", e))
            })?;

        let byte_order = match &byte_order_bytes {
            b"II" => ByteOrder::LittleEndian,
            b"MM" => ByteOrder::BigEndian,
            _ => {
                return Err(ImageError::new("Invalid TIFF byte order"));
            }
        };

        let version = byte_order.read_u16(reader)?;

        if version != 42 {
            return Err(ImageError::new(
                "Invalid TIFF version, expected 42",
            ));
        }

        let ifd_offset = byte_order.read_u32(reader)?;

        Ok(Tiff {
            header: TiffHeader {
                byte_order,
                version,
                ifd_offset,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Stores the byte order, TIFF version, and first IFD offset from the file header.
pub struct TiffHeader {
    /// Stores the byte order used to decode multi-byte values.
    pub byte_order: ByteOrder,
    /// Stores the file-format version parsed from the header.
    pub version: u16,
    /// Stores the byte offset of the first TIFF Image File Directory.
    pub ifd_offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents a TIFF Image File Directory and its entries.
pub struct TiffIFD {
    /// Stores the entries belonging to this directory or table.
    pub entries: Vec<TiffIFDEntry>,
    /// Stores the byte offset of the next TIFF IFD, or zero if this is the final directory.
    pub next_ifd_offset: u32,
}

impl TiffIFD {
    /// Parses the supplied encoded input into this format-specific representation.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    pub fn parse<R: Read + Seek>(
        reader: &mut R,
        endian: ByteOrder,
        ifd_offset: u32,
    ) -> Result<Self, ImageError> {
        reader
            .seek(SeekFrom::Start(ifd_offset as u64))
            .map_err(|e| {
                ImageError::new(format!(
                    "Failed to seek to IFD offset: {}",
                    e
                ))
            })?;

        let entry_count = endian.read_u16(reader)? as usize;

        let mut entries = Vec::with_capacity(entry_count);

        for _ in 0..entry_count {
            let tag = endian.read_u16(reader)?;
            let field_type = endian.read_u16(reader)?;
            let count = endian.read_u32(reader)?;
            let value_offset = endian.read_u32(reader)?;

            entries.push(TiffIFDEntry {
                tag,
                field_type,
                count,
                value_offset,
            });
        }

        let next_ifd_offset = endian.read_u32(reader)?;

        Ok(TiffIFD {
            entries,
            next_ifd_offset,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents one tag entry in a TIFF Image File Directory.
pub struct TiffIFDEntry {
    /// Stores the numeric TIFF tag identifier.
    pub tag: u16,
    /// Stores the TIFF field-type identifier describing the entry's value representation.
    pub field_type: u16,
    /// Stores the number of values represented by this entry.
    pub count: u32,

    /// The fourth 4-byte field of a TIFF IFD entry.
    ///
    /// This contains the actual value when the value fits into 4 bytes,
    /// otherwise it contains an offset to the value.
    pub value_offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifies the byte order used to encode multi-byte TIFF values.
pub enum ByteOrder {
    /// Indicates TIFF values are encoded least-significant byte first (`II`).
    LittleEndian,
    /// Indicates TIFF values are encoded most-significant byte first (`MM`).
    BigEndian,
}

impl ByteOrder {
    /// Reads u16 from the supplied input while advancing the relevant cursor or bit position.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    pub fn read_u16<R: Read + Seek>(
        &self,
        reader: &mut R,
    ) -> Result<u16, ImageError> {
        let mut buf = [0u8; 2];

        reader
            .read_exact(&mut buf)
            .map_err(|e| {
                ImageError::new(format!("Failed to read u16: {}", e))
            })?;

        Ok(match self {
            ByteOrder::LittleEndian => u16::from_le_bytes(buf),
            ByteOrder::BigEndian => u16::from_be_bytes(buf),
        })
    }

    /// Reads u32 from the supplied input while advancing the relevant cursor or bit position.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    pub fn read_u32<R: Read + Seek>(
        &self,
        reader: &mut R,
    ) -> Result<u32, ImageError> {
        let mut buf = [0u8; 4];

        reader
            .read_exact(&mut buf)
            .map_err(|e| {
                ImageError::new(format!("Failed to read u32: {}", e))
            })?;

        Ok(match self {
            ByteOrder::LittleEndian => u32::from_le_bytes(buf),
            ByteOrder::BigEndian => u32::from_be_bytes(buf),
        })
    }

    /// Reads u64 from the supplied input while advancing the relevant cursor or bit position.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    pub fn read_u64<R: Read + Seek>(
        &self,
        reader: &mut R,
    ) -> Result<u64, ImageError> {
        let mut buf = [0u8; 8];

        reader
            .read_exact(&mut buf)
            .map_err(|e| {
                ImageError::new(format!("Failed to read u64: {}", e))
            })?;

        Ok(match self {
            ByteOrder::LittleEndian => u64::from_le_bytes(buf),
            ByteOrder::BigEndian => u64::from_be_bytes(buf),
        })
    }
}


/// TIFF field type.
///
/// 1 = BYTE
/// 2 = ASCII
/// 3 = SHORT
/// 4 = LONG
/// 5 = RATIONAL
/// 6 = SBYTE
/// 7 = UNDEFINED
/// 8 = SSHORT
/// 9 = SLONG
/// 10 = SRATIONAL
/// 11 = FLOAT
/// 12 = DOUBLE
// #[derive(Debug, Clone, PartialEq, Eq, FromPrimitive)]
// pub enum TiffFieldType {
//     #[value(1)]
//     Byte,

//     #[value(2)]
//     Ascii,

//     #[value(3)]
//     Short,

//     #[value(4)]
//     Long,

//     #[value(5)]
//     Rational,

//     #[value(6)]
//     SByte,

//     #[value(7)]
//     Undefined,

//     #[value(8)]
//     SShort,

//     #[value(9)]
//     SLong,

//     #[value(10)]
//     SRational,

//     #[value(11)]
//     Float,

//     #[value(12)]
//     Double,
// }


/// TIFF PhotometricInterpretation tag (262).
// #[derive(Debug, Clone, PartialEq, Eq, FromPrimitive)]
// pub enum PhotoMetricInterpretation {
//     #[value(0)]
//     WhiteIsZero,

//     #[value(1)]
//     BlackIsZero,
// }

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive)]
/// Identifies how TIFF sample values should be interpreted as image colors.
pub enum PhotoMetricInterpretation {
    #[value(0)]
    /// Interprets lower grayscale sample values as white and higher values as black.
    WhiteIsZero,

    #[value(1)]
    /// Interprets lower grayscale sample values as black and higher values as white.
    BlackIsZero,

    #[value(2)]
    /// Interprets TIFF samples as red, green, and blue color components.
    RGB,

    #[value(3)]
    /// Interprets TIFF samples as indices into a color map.
    PaletteColor,

    #[value(4)]
    /// Interprets TIFF samples as a transparency mask.
    TransparencyMask,

    #[value(5)]
    /// Interprets TIFF samples as separated color components such as CMYK.
    Separated,

    #[value(6)]
    /// Interprets TIFF samples as YCbCr luma and chroma components.
    YCbCr,

    #[value(8)]
    /// Interprets TIFF samples in the CIE L*a*b* color space.
    CIELab,
}


/// TIFF Compression tag (259).
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive)]
pub enum Compression {
    #[value(1)]
    /// Indicates that TIFF image samples are stored without compression.
    None,

    #[value(2)]
    /// Indicates CCITT fax-style compression for bilevel TIFF image data.
    CCITT,

    #[value(32773)]
    /// Indicates TIFF PackBits run-length compression.
    PackBits,
}


/// TIFF ResolutionUnit tag (296).
#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive)]
pub enum ResolutionUnit {
    #[value(1)]
    /// Indicates that TIFF X/Y resolution values have no absolute unit.
    None,

    #[value(2)]
    /// Indicates TIFF X/Y resolution values are measured in pixels per inch.
    Inch,

    #[value(3)]
    /// Indicates TIFF X/Y resolution values are measured in pixels per centimetre.
    Centimeter,
}


/// TIFF FillOrder tag (266).
#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive)]
pub enum FillOrder {
    #[value(1)]
    /// Indicates that bits within each TIFF byte are ordered least-significant bit first.
    LSB2MSB,

    #[value(2)]
    /// Indicates that bits within each TIFF byte are ordered most-significant bit first.
    MSB2LSB,
}


#[derive(Debug, Clone, PartialEq, Eq, Default)]
/// Collects TIFF image metadata and decoded pixel data for one image.
pub struct TiffImage {
    /// Stores the byte order used to decode multi-byte values.
    pub byte_order: Option<ByteOrder>,
    /// Stores how TIFF sample values should be interpreted as visible colors.
    pub photometric_interpretation: Option<PhotoMetricInterpretation>,
    /// Stores the compression mode declared or detected for the image.
    pub compression: Option<Compression>,
    /// Stores optional metadata identifying the software that created the TIFF image.
    pub software: Option<String>,
    /// Stores optional TIFF date/time metadata.
    pub date_time: Option<String>,
    /// Stores optional TIFF artist or creator metadata.
    pub artist: Option<String>,
    /// Stores optional TIFF palette entries.
    pub color_map: Option<Vec<u16>>,
    /// Stores the image, frame, or plane width in pixels.
    pub width: Option<u32>,
    /// Stores the image, frame, or plane height in pixels.
    pub height: Option<u32>,
    /// Stores the unit used to interpret TIFF resolution values.
    pub resolution_unit: Option<ResolutionUnit>,
    /// Stores the horizontal TIFF resolution as a numerator/denominator pair.
    pub x_resolution: Option<(u32, u32)>,
    /// Stores the vertical TIFF resolution as a numerator/denominator pair.
    pub y_resolution: Option<(u32, u32)>,
    /// Stores the nominal number of rows encoded in each TIFF strip.
    pub rows_per_strip: Option<u32>,
    /// Stores the file offset of each TIFF image-data strip.
    pub strip_offsets: Vec<u32>,
    /// Stores the encoded byte length of each TIFF image-data strip.
    pub strip_byte_counts: Vec<u32>,
    /// Stores the number of significant bits used for each encoded sample.
    pub bits_per_sample: Option<u16>,
    /// Stores the number of component samples that form one pixel.
    pub samples_per_pixel: Option<u16>,
    /// Stores the optional TIFF copyright metadata.
    pub copyright: Option<String>,
    /// Stores the bit order used within each encoded TIFF byte.
    pub fill_order: Option<FillOrder>,
    /// Stores optional byte counts for TIFF free-space blocks.
    pub free_byte_counts: Option<Vec<u32>>,
    /// Stores optional offsets to TIFF free-space blocks.
    pub free_offsets: Option<Vec<u32>>,
    // 338
    /// Stores declarations for TIFF extra samples such as alpha channels.
    pub extra_samples: Option<Vec<u16>>,

    /// Decoded pixels in `0xRRGGBBAA` representation.
    pub data: Vec<u32>,
}


impl TiffImage {
    /// Constructs a new decoder value from the supplied encoded byte slice.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        let mut reader = Cursor::new(bytes);
        let tiff = TiffImage::default();
        Self::parse(tiff, &mut reader)
    }

    /// Borrows decoded packed pixels without cloning them.
    #[inline]
    pub fn pixels_ref(&self) -> &[u32] {
        &self.data
    }

    /// Moves decoded packed pixels out of the rich TIFF object.
    #[inline]
    pub fn into_pixels(self) -> Vec<u32> {
        self.data
    }

    /// Parse a TIFF image and decode its pixel data.
    ///
    /// Currently supports uncompressed TIFF images with:
    ///
    /// - 8-bit grayscale
    /// - 8-bit RGB
    /// - 8-bit RGBA
    /// - 8-bit palette/color-map images
    ///
    /// The returned pixels use `0xRRGGBBAA`.
    pub fn parse<R: Read + Seek>(
        self,
        reader: &mut R,
    ) -> Result<Self, ImageError> {
        Self::parse_with_metadata(self, reader, true)
    }

    /// Internal parse policy used by the lightweight path to avoid allocating
    /// metadata that cannot affect rendered pixels.
    fn parse_with_metadata<R: Read + Seek>(
        self,
        reader: &mut R,
        retain_metadata: bool,
    ) -> Result<Self, ImageError> {
        let tiff = Tiff::parse(reader)?;
        Self::parse_ifd_with_metadata(
            self,
            reader,
            tiff.header.byte_order,
            tiff.header.ifd_offset,
            retain_metadata,
        )
    }

    /// Decodes one selected TIFF IFD. This is the core used by both page 0 and
    /// the lazy multi-page reader, so selecting a later page never requires
    /// decoding the earlier pages first.
    fn parse_ifd_with_metadata<R: Read + Seek>(
        mut self,
        reader: &mut R,
        byte_order: ByteOrder,
        ifd_offset: u32,
        retain_metadata: bool,
    ) -> Result<Self, ImageError> {
        self.byte_order = Some(byte_order);
        let ifd = TiffIFD::parse(
            reader,
            byte_order,
            ifd_offset,
        )?;

        // -------------------------------------------------------------
        // Metadata
        // -------------------------------------------------------------
        // let mut self = TiffImage::default();

        // -------------------------------------------------------------
        // Parse IFD entries
        // -------------------------------------------------------------

        for entry in &ifd.entries {
            match entry.tag {
                // PhotometricInterpretation
                262 => {
                    let value = read_u16_value(
                        reader,
                        entry,
                        &byte_order,
                    )?;

                    self.photometric_interpretation =
                        Some(
                            PhotoMetricInterpretation::try_from(value)
                                .map_err(|_| {
                                    let message = format!(
                                        "Invalid Photometric Interpretation value: {}",
                                        value
                                    );
                                    ImageError::new(message)
                                })?,
                        );
                }

                // Compression
                259 => {
                    let value = read_u16_value(
                        reader,
                        entry,
                        &byte_order,
                    )?;

                    self.compression =
                        Some(
                            Compression::try_from(value)
                                .map_err(|_| {
                                    ImageError::new(
                                        "Invalid Compression value",
                                    )
                                })?,
                        );
                }

                // Software (rich metadata only)
                305 => {
                    if retain_metadata {
                        self.software = Some(read_ascii_string(reader, entry, &byte_order)?);
                    }
                }

                // DateTime (rich metadata only)
                306 => {
                    if retain_metadata {
                        self.date_time = Some(read_ascii_string(reader, entry, &byte_order)?);
                    }
                }

                // Artist (rich metadata only)
                315 => {
                    if retain_metadata {
                        self.artist = Some(read_ascii_string(reader, entry, &byte_order)?);
                    }
                }

                // ColorMap
                320 => {
                    self.color_map = Some(read_u16_array(
                        reader,
                        entry,
                        &byte_order,
                    )?);
                }
                // ExtraSamples
                338 => {
                    self.extra_samples = Some(read_u16_array(
                        reader,
                        entry,
                        &byte_order,
                    )?);
                }

                // Copyright (rich metadata only)
                33432 => {
                    if retain_metadata {
                        self.copyright = Some(read_ascii_string(reader, entry, &byte_order)?);
                    }
                }

                // FillOrder
                266 => {
                    let value = read_u16_value(
                        reader,
                        entry,
                        &byte_order,
                    )?;

                    self.fill_order =
                        Some(
                            FillOrder::try_from(value)
                                .map_err(|_| {
                                    ImageError::new(
                                        "Invalid Fill Order value",
                                    )
                                })?,
                        );
                }

                // FreeByteCounts (rich metadata only)
                289 => {
                    if retain_metadata {
                        self.free_byte_counts = Some(read_u32_array(reader, entry, &byte_order)?);
                    }
                }

                // FreeOffsets (rich metadata only)
                288 => {
                    if retain_metadata {
                        self.free_offsets = Some(read_u32_array(reader, entry, &byte_order)?);
                    }
                }

                // ImageWidth
                256 => {
                    self.width = Some(read_u32_value(
                        reader,
                        entry,
                        &byte_order,
                    )?);
                }

                // ImageLength
                257 => {
                    self.height = Some(read_u32_value(
                        reader,
                        entry,
                        &byte_order,
                    )?);
                }

                // XResolution (rich metadata only)
                282 => {
                    if retain_metadata {
                        self.x_resolution = Some(read_rational(reader, entry, &byte_order)?);
                    }
                }

                // YResolution (rich metadata only)
                283 => {
                    if retain_metadata {
                        self.y_resolution = Some(read_rational(reader, entry, &byte_order)?);
                    }
                }

                // RowsPerStrip
                278 => {
                    self.rows_per_strip = Some(read_u32_value(
                        reader,
                        entry,
                        &byte_order,
                    )?);
                }

                // StripOffsets
                273 => {
                    self.strip_offsets = read_u32_array(
                        reader,
                        entry,
                        &byte_order,
                    )?;
                }

                // StripByteCounts
                279 => {
                    self.strip_byte_counts = read_u32_array(
                        reader,
                        entry,
                        &byte_order,
                    )?;
                }

                // BitsPerSample
                258 => {
                    let values = read_u16_array(
                        reader,
                        entry,
                        &byte_order,
                    )?;

                    if values.is_empty() {
                        return Err(ImageError::new(
                            "TIFF BitsPerSample is empty",
                        ));
                    }

                    // This implementation currently supports uniform
                    // 8-bit samples.
                    if values.iter().any(|&v| v != 8) {
                        return Err(ImageError::new(
                            "Only 8-bit TIFF samples are currently supported",
                        ));
                    }

                    self.bits_per_sample = Some(values[0]);
                }

                // SamplesPerPixel
                277 => {
                    self.samples_per_pixel = Some(read_u16_value(
                        reader,
                        entry,
                        &byte_order,
                    )?);
                }

                _ => {}
            }
        }

        // -------------------------------------------------------------
        // Validate required TIFF image information
        // -------------------------------------------------------------

        let width = self.width.ok_or_else(|| {
            ImageError::new("TIFF is missing ImageWidth")
        })?;

        let height = self.height.ok_or_else(|| {
            ImageError::new("TIFF is missing ImageLength")
        })?;

        let compression = self
            .compression
            .unwrap_or(Compression::None);

        if compression != Compression::None {
            return Err(ImageError::new(
                "Only uncompressed TIFF images are currently supported",
            ));
        }

        if self.strip_offsets.is_empty() {
            return Err(ImageError::new(
                "TIFF is missing StripOffsets",
            ));
        }

        if self.strip_byte_counts.is_empty() {
            return Err(ImageError::new(
                "TIFF is missing StripByteCounts",
            ));
        }

        if self.strip_offsets.len() != self.strip_byte_counts.len() {
            return Err(ImageError::new(
                "TIFF StripOffsets and StripByteCounts have different lengths",
            ));
        }

        let bits_per_sample = self.bits_per_sample.unwrap_or(8);

        if bits_per_sample != 8 {
            return Err(ImageError::new(
                "Only 8-bit TIFF images are currently supported",
            ));
        }

        let samples_per_pixel =
            self.samples_per_pixel.unwrap_or(1);

        // TIFF defaults to BlackIsZero for most normal grayscale
        // images when PhotometricInterpretation is omitted.
        let photometric_interpretation =
            self.photometric_interpretation
                .unwrap_or(PhotoMetricInterpretation::BlackIsZero);

        // -------------------------------------------------------------
        // Stream uncompressed strips directly into final packed pixels.
        //
        // The old implementation concatenated every StripByteCounts payload
        // into one image-sized `raw_data` allocation before converting it.
        // For the supported uncompressed 8-bit TIFF subset there is no need to
        // retain those encoded samples: each strip can be converted directly
        // into its final destination rows and the scratch allocation reused.
        // -------------------------------------------------------------

        let width_usize = usize::try_from(width)
            .map_err(|_| ImageError::new("TIFF width exceeds usize"))?;
        let height_usize = usize::try_from(height)
            .map_err(|_| ImageError::new("TIFF height exceeds usize"))?;
        let pixel_count = width_usize
            .checked_mul(height_usize)
            .ok_or_else(|| ImageError::new("TIFF image dimensions overflow"))?;
        let samples_per_pixel_usize = usize::from(samples_per_pixel);
        let row_sample_bytes = width_usize
            .checked_mul(samples_per_pixel_usize)
            .ok_or_else(|| ImageError::new("TIFF row sample count overflow"))?;
        let rows_per_strip = usize::try_from(self.rows_per_strip.unwrap_or(height).max(1))
            .map_err(|_| ImageError::new("TIFF RowsPerStrip exceeds usize"))?;
        let required_strips = height_usize.div_ceil(rows_per_strip);
        if self.strip_offsets.len() < required_strips {
            return Err(ImageError::new("TIFF does not contain enough strips for ImageLength"));
        }

        if photometric_interpretation == PhotoMetricInterpretation::WhiteIsZero
            || photometric_interpretation == PhotoMetricInterpretation::BlackIsZero
            || photometric_interpretation == PhotoMetricInterpretation::PaletteColor
            || photometric_interpretation == PhotoMetricInterpretation::TransparencyMask
        {
            if samples_per_pixel != 1 {
                return Err(ImageError::new(
                    "Single-channel TIFF photometric interpretation requires SamplesPerPixel = 1",
                ));
            }
        } else if photometric_interpretation == PhotoMetricInterpretation::RGB {
            if samples_per_pixel != 3 && samples_per_pixel != 4 {
                return Err(ImageError::new(
                    "RGB TIFF must have SamplesPerPixel = 3 or 4",
                ));
            }
            if samples_per_pixel == 4 {
                let extra_samples = self.extra_samples.as_ref().ok_or_else(|| {
                    ImageError::new(
                        "RGB TIFF has 4 samples per pixel but is missing ExtraSamples",
                    )
                })?;
                match extra_samples.first().copied() {
                    Some(0 | 1 | 2) => {}
                    Some(_) => return Err(ImageError::new("Invalid TIFF ExtraSamples value")),
                    None => return Err(ImageError::new("RGB TIFF ExtraSamples is empty")),
                }
            }
        } else {
            return Err(ImageError::new(match photometric_interpretation {
                PhotoMetricInterpretation::Separated => {
                    "TIFF Separated photometric interpretation is not yet supported"
                }
                PhotoMetricInterpretation::YCbCr => {
                    "TIFF YCbCr photometric interpretation is not yet supported"
                }
                PhotoMetricInterpretation::CIELab => {
                    "TIFF CIELab photometric interpretation is not yet supported"
                }
                _ => "Unsupported TIFF photometric interpretation",
            }));
        }

        let packed_palette = if photometric_interpretation == PhotoMetricInterpretation::PaletteColor {
            let color_map = self.color_map.as_ref().ok_or_else(|| {
                ImageError::new("PaletteColor TIFF is missing ColorMap")
            })?;
            if color_map.len() % 3 != 0 {
                return Err(ImageError::new("Invalid TIFF ColorMap length"));
            }
            let palette_size = color_map.len() / 3;
            let mut palette = Vec::with_capacity(palette_size);
            for index in 0..palette_size {
                palette.push(pack_rgba(
                    (color_map[index] >> 8) as u8,
                    (color_map[palette_size + index] >> 8) as u8,
                    (color_map[palette_size * 2 + index] >> 8) as u8,
                    255,
                ));
            }
            Some(palette)
        } else {
            None
        };

        let mut data = vec![0u32; pixel_count];
        let mut strip = Vec::<u8>::new();

        for strip_index in 0..required_strips {
            let first_row = strip_index
                .checked_mul(rows_per_strip)
                .ok_or_else(|| ImageError::new("TIFF strip row offset overflow"))?;
            let rows = rows_per_strip.min(height_usize - first_row);
            let required_bytes = rows
                .checked_mul(row_sample_bytes)
                .ok_or_else(|| ImageError::new("TIFF strip byte count overflow"))?;
            let declared_bytes = usize::try_from(self.strip_byte_counts[strip_index])
                .map_err(|_| ImageError::new("TIFF strip byte count exceeds usize"))?;
            if declared_bytes < required_bytes {
                return Err(ImageError::new(format!(
                    "TIFF strip {strip_index} is too small: need {required_bytes}, got {declared_bytes}",
                )));
            }

            reader
                .seek(SeekFrom::Start(u64::from(self.strip_offsets[strip_index])))
                .map_err(|e| ImageError::new(format!("Failed to seek to TIFF strip: {e}")))?;
            strip.resize(required_bytes, 0);
            reader
                .read_exact(&mut strip)
                .map_err(|e| ImageError::new(format!("Failed to read TIFF strip: {e}")))?;

            let first_pixel = first_row
                .checked_mul(width_usize)
                .ok_or_else(|| ImageError::new("TIFF strip pixel offset overflow"))?;
            let strip_pixels = rows
                .checked_mul(width_usize)
                .ok_or_else(|| ImageError::new("TIFF strip pixel count overflow"))?;
            let output = &mut data[first_pixel..first_pixel + strip_pixels];

            match photometric_interpretation {
                PhotoMetricInterpretation::BlackIsZero => {
                    crate::simd::gray8_to_u32(&strip[..strip_pixels], output);
                }
                PhotoMetricInterpretation::WhiteIsZero => {
                    for (destination, &gray) in output.iter_mut().zip(&strip[..strip_pixels]) {
                        let gray = 255u8.wrapping_sub(gray);
                        *destination = pack_rgba(gray, gray, gray, 255);
                    }
                }
                PhotoMetricInterpretation::RGB if samples_per_pixel == 3 => {
                    crate::simd::_rgb8_to_u32(&strip[..strip_pixels * 3], output);
                }
                PhotoMetricInterpretation::RGB => {
                    crate::simd::rgba8_to_u32(&strip[..strip_pixels * 4], output);
                }
                PhotoMetricInterpretation::PaletteColor => {
                    let palette = packed_palette.as_ref().expect("palette validated above");
                    for (destination, &index) in output.iter_mut().zip(&strip[..strip_pixels]) {
                        *destination = *palette.get(index as usize).ok_or_else(|| {
                            ImageError::new("TIFF palette index is out of range")
                        })?;
                    }
                }
                PhotoMetricInterpretation::TransparencyMask => {
                    for (destination, &alpha) in output.iter_mut().zip(&strip[..strip_pixels]) {
                        *destination = pack_rgba(255, 255, 255, alpha);
                    }
                }
                _ => unreachable!("photometric interpretation validated before strip decode"),
            }
        }

        self.data = data;
        // -------------------------------------------------------------
        // Return image
        // -------------------------------------------------------------
        Ok(self)
    }
}



/// Lazy TIFF page/IFD reader.
///
/// Construction walks only the IFD chain and stores offsets. Pixel strips for a
/// page are decoded only when [`Self::decode_page`] is called.
pub struct TiffPages<'a> {
    bytes: &'a [u8],
    byte_order: ByteOrder,
    ifd_offsets: Vec<u32>,
}

impl<'a> TiffPages<'a> {
    /// Parses the TIFF header and IFD chain without decoding page pixels.
    pub fn new(bytes: &'a [u8]) -> Result<Self, ImageError> {
        let mut reader = Cursor::new(bytes);
        let tiff = Tiff::parse(&mut reader)?;
        let byte_order = tiff.header.byte_order;
        let mut offset = tiff.header.ifd_offset;
        let mut offsets = Vec::new();
        let mut guard = 0usize;
        while offset != 0 {
            guard += 1;
            if guard > 1_000_000 {
                return Err(ImageError::new("TIFF IFD chain is cyclic or excessive"));
            }
            offsets.push(offset);
            let ifd = TiffIFD::parse(&mut reader, byte_order, offset)?;
            offset = ifd.next_ifd_offset;
        }
        if offsets.is_empty() {
            return Err(ImageError::new("TIFF contains no image file directories"));
        }
        Ok(Self { bytes, byte_order, ifd_offsets: offsets })
    }

    /// Number of TIFF IFD/pages.
    #[inline]
    pub fn len(&self) -> usize { self.ifd_offsets.len() }

    /// Returns true when no pages were found.
    #[inline]
    pub fn is_empty(&self) -> bool { self.ifd_offsets.is_empty() }

    /// Returns the raw IFD offset for a page without decoding it.
    #[inline]
    pub fn ifd_offset(&self, index: usize) -> Option<u32> {
        self.ifd_offsets.get(index).copied()
    }

    /// Decodes exactly one page and retains its rich TIFF metadata.
    pub fn decode_page(&self, index: usize) -> Result<TiffImage, ImageError> {
        let offset = self.ifd_offsets.get(index).copied()
            .ok_or_else(|| ImageError::new("TIFF page index is out of range"))?;
        let mut reader = Cursor::new(self.bytes);
        TiffImage::parse_ifd_with_metadata(
            TiffImage::default(),
            &mut reader,
            self.byte_order,
            offset,
            true,
        )
    }

    /// Decodes one page while skipping optional authoring/text metadata.
    pub fn decode_page_pixels(&self, index: usize) -> Result<TiffImage, ImageError> {
        let offset = self.ifd_offsets.get(index).copied()
            .ok_or_else(|| ImageError::new("TIFF page index is out of range"))?;
        let mut reader = Cursor::new(self.bytes);
        TiffImage::parse_ifd_with_metadata(
            TiffImage::default(),
            &mut reader,
            self.byte_order,
            offset,
            false,
        )
    }
}

pub(crate) fn decode_image(
    bytes: &[u8],
    options: &crate::DecodeOptions,
) -> Result<crate::Image, ImageError> {
    let info = crate::ImageInfo::read(bytes)?;
    options.checked_pixel_count(info.width, info.height)?;

    let mut reader = Cursor::new(bytes);
    let decoded = TiffImage::parse_with_metadata(TiffImage::default(), &mut reader, false)?;
    let width = decoded.width.unwrap_or(info.width);
    let height = decoded.height.unwrap_or(info.height);
    options.checked_rgba_bytes(width, height)?;

    crate::Image::from_packed_pixels(width, height, decoded.data, options)
}



impl super::Decoder for TiffImage {
    fn width(&self) -> u32 {
        self.width.unwrap_or(0)
    }

    fn height(&self) -> u32 {
        self.height.unwrap_or(0)
    }

    fn into_image(self: Box<Self>) -> Result<crate::Image, ImageError> {
        let this = *self;
        let width = this.width.ok_or_else(|| ImageError::new("TIFF width is unavailable"))?;
        let height = this.height.ok_or_else(|| ImageError::new("TIFF height is unavailable"))?;
        crate::Image::from_packed_pixels(
            width,
            height,
            this.data,
            &crate::DecodeOptions::default(),
        )
    }
}
