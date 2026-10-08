//! JPEG image decoding implementation.
//!
//! This module parses JPEG input, validates format-specific structures, and converts
//! decoded image content into the crate's packed pixel/frame representation.

use crate::pixels::pack_rgba;
use std::io::{Read, Seek};

use crate::ImageError;

// ============================================================================
// JPEG markers
// ============================================================================

const SOF0: u8 = 0xC0;
const DHT: u8 = 0xC4;
const SOI: u8 = 0xD8;
const EOI: u8 = 0xD9;
const SOS: u8 = 0xDA;
const DQT: u8 = 0xDB;
const DRI: u8 = 0xDD;

// ============================================================================
// Zig-zag order
// ============================================================================

/// Maps JPEG zig-zag positions to natural 8x8 coefficient positions.
///
/// For example:
///
/// ```text
/// JPEG coefficient 0 -> natural coefficient 0
/// JPEG coefficient 1 -> natural coefficient 1
/// JPEG coefficient 2 -> natural coefficient 8
/// ```
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10,
    17, 24, 32, 25, 18, 11, 4, 5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13, 6, 7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

// ============================================================================
// Public JPEG types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifies the decoded color-space interpretation used by a JPEG image.
pub enum JpegColorSpace {
    /// Represents a JPEG decoded as a single grayscale luminance component.
    Grayscale,
    /// Represents a JPEG decoded from Y, Cb, and Cr luma/chroma components.
    YCbCr,
    /// Represents a JPEG whose components are interpreted directly as red, green, and blue.
    RGB,
}

/// Native JPEG inverse-DCT scale used for thumbnail decoding.
///
/// The entropy stream is still decoded completely, but component planes and
/// packed output are reconstructed directly at the requested fraction so a
/// thumbnail does not require a full-resolution image allocation followed by a
/// generic resize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JpegScale {
    /// Full 8x8 IDCT output.
    #[default]
    Full,
    /// Four samples per source 8x8 block dimension.
    Half,
    /// Two samples per source 8x8 block dimension.
    Quarter,
    /// One sample per source 8x8 block dimension.
    Eighth,
}

impl JpegScale {
    #[inline(always)]
    const fn divisor(self) -> usize {
        match self {
            Self::Full => 1,
            Self::Half => 2,
            Self::Quarter => 4,
            Self::Eighth => 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Represents a decoded JPEG image and its encoded source data.
pub struct Jpeg {
    /// Stores the image, frame, or plane width in pixels.
    pub width: u32,
    /// Stores the image, frame, or plane height in pixels.
    pub height: u32,

    /// Number of JPEG components.
    ///
    /// Usually:
    ///
    /// - 1 = grayscale
    /// - 3 = YCbCr / RGB
    pub components: u8,

    /// Stores the color-space interpretation selected for the decoded image.
    pub color_space: JpegColorSpace,

    /// Original encoded JPEG file.
    pub data: Vec<u8>,

    /// Decoded pixels in 0xRRGGBBAA format.
    pixels: Vec<u32>,
}

// ============================================================================
// Quantization table
// ============================================================================

#[derive(Debug, Clone)]
/// Stores one JPEG quantization table in natural 8×8 coefficient order.
struct QuantTable {
    /// Stored in natural 8x8 order rather than zig-zag order.
    values: [i32; 64],
}

// ============================================================================
// Huffman table
// ============================================================================

/// Number of bits used by the direct Huffman lookup table.
///
/// Ten bits keeps each table at only 2 KiB while resolving the overwhelming
/// majority of JPEG Huffman symbols with one table lookup.
const HUFFMAN_FAST_BITS: u8 = 10;
const HUFFMAN_FAST_SIZE: usize = 1usize << HUFFMAN_FAST_BITS;

#[derive(Debug, Clone)]
/// Stores a canonical JPEG Huffman table plus a short-code lookup table.
struct HuffmanTable {
    /// Minimum canonical code for each code length.
    min_code: [i32; 17],
    /// Maximum canonical code for each code length.
    max_code: [i32; 17],
    /// First symbol index for each code length.
    val_ptr: [usize; 17],
    /// Symbols in canonical-code order. JPEG Huffman tables contain at most 256 symbols.
    symbols: [u8; 256],
    /// Number of initialized entries in `symbols`.
    symbol_count: usize,
    /// Direct lookup for codes up to `HUFFMAN_FAST_BITS` bits.
    ///
    /// The high byte stores the consumed bit count and the low byte stores the symbol.
    fast: [u16; HUFFMAN_FAST_SIZE],
}

impl HuffmanTable {
    /// Builds the canonical and direct-lookup representations of a JPEG Huffman table.
    fn new(
        counts: [u8; 16],
        source_symbols: &[u8],
    ) -> Result<Self, ImageError> {
        let expected: usize = counts.iter().map(|&count| count as usize).sum();

        if expected != source_symbols.len() || expected > 256 {
            return Err(ImageError::new("Invalid JPEG Huffman table"));
        }

        let mut min_code = [-1i32; 17];
        let mut max_code = [-1i32; 17];
        let mut val_ptr = [0usize; 17];
        let mut symbols = [0u8; 256];
        symbols[..expected].copy_from_slice(source_symbols);
        let mut fast = [0u16; HUFFMAN_FAST_SIZE];

        let mut code = 0u32;
        let mut symbol_index = 0usize;

        for length in 1..=16usize {
            let count = counts[length - 1] as usize;

            if code + count as u32 > (1u32 << length) {
                return Err(ImageError::new("Oversubscribed JPEG Huffman table"));
            }

            if count != 0 {
                min_code[length] = code as i32;
                max_code[length] = (code + count as u32 - 1) as i32;
                val_ptr[length] = symbol_index;

                if length <= HUFFMAN_FAST_BITS as usize {
                    let suffix_bits = HUFFMAN_FAST_BITS as usize - length;
                    let copies = 1usize << suffix_bits;

                    for offset in 0..count {
                        let canonical = code as usize + offset;
                        let first = canonical << suffix_bits;
                        let entry = ((length as u16) << 8)
                            | symbols[symbol_index + offset] as u16;
                        fast[first..first + copies].fill(entry);
                    }
                }

                symbol_index += count;
            }

            code = (code + count as u32) << 1;
        }

        Ok(Self {
            min_code,
            max_code,
            val_ptr,
            symbols,
            symbol_count: expected,
            fast,
        })
    }

    /// Decodes one Huffman symbol using a 10-bit direct lookup and a canonical fallback.
    #[inline(always)]
    fn decode(&self, reader: &mut EntropyBitReader<'_>) -> Result<u8, ImageError> {
        if let Some(prefix) = reader.try_peek_bits(HUFFMAN_FAST_BITS)? {
            let entry = self.fast[prefix as usize];
            if entry != 0 {
                let length = (entry >> 8) as u8;
                reader.drop_bits(length);
                return Ok(entry as u8);
            }
        }

        // Most codes that miss the direct table can still be decoded from one
        // 16-bit peek rather than one branch/function call per bit.
        if let Some(bits) = reader.try_peek_bits(16)? {
            for length in (HUFFMAN_FAST_BITS as usize + 1)..=16usize {
                let code = (bits >> (16 - length)) as i32;
                let min = self.min_code[length];
                let max = self.max_code[length];

                if min >= 0 && code >= min && code <= max {
                    let index = self.val_ptr[length] + (code - min) as usize;
                    if index >= self.symbol_count {
                        return Err(ImageError::new(
                            "JPEG Huffman symbol index out of range",
                        ));
                    }
                    reader.drop_bits(length as u8);
                    return Ok(self.symbols[index]);
                }
            }
        }

        self.decode_slow(reader)
    }

    /// Marker-boundary fallback used when a full lookahead cannot be buffered.
    #[inline(never)]
    fn decode_slow(&self, reader: &mut EntropyBitReader<'_>) -> Result<u8, ImageError> {
        let mut code = 0i32;

        for length in 1..=16usize {
            code = (code << 1) | reader.read_bit()? as i32;
            let min = self.min_code[length];
            let max = self.max_code[length];

            if min >= 0 && code >= min && code <= max {
                let index = self.val_ptr[length] + (code - min) as usize;
                if index < self.symbol_count {
                    return Ok(self.symbols[index]);
                }
                break;
            }
        }

        Err(ImageError::new("Invalid JPEG Huffman code"))
    }
}

// ============================================================================
// Frame structures
// ============================================================================

#[derive(Debug, Clone)]
/// Stores JPEG component sampling, table-selection, and coefficient state.
struct Component {
    /// Stores the JPEG component identifier from the frame header.
    id: u8,

    /// Horizontal sampling factor.
    h: usize,

    /// Vertical sampling factor.
    v: usize,

    /// Stores the quantization-table index selected for this JPEG component.
    quant_table: usize,

    /// Stores the number of 8×8 coefficient blocks across this component plane.
    blocks_x: usize,
    /// Stores the number of 8×8 coefficient blocks down this component plane.
    blocks_y: usize,

    /// Width of the decoded component plane in samples.
    stride: usize,

    /// Decoded component samples.
    samples: Vec<u8>,
}

#[derive(Debug, Clone)]
/// Stores the JPEG frame header and component definitions.
struct Frame {
    /// Stores the scaled output width in pixels.
    width: usize,
    /// Stores the scaled output height in pixels.
    height: usize,
    /// Native JPEG decode divisor (1, 2, 4, or 8).
    scale: usize,

    /// Stores the largest horizontal sampling factor among all JPEG frame components.
    max_h: usize,
    /// Stores the largest vertical sampling factor among all JPEG frame components.
    max_v: usize,

    /// Stores the number of minimum-coded units across the JPEG frame.
    mcu_cols: usize,
    /// Stores the number of minimum-coded units down the JPEG frame.
    mcu_rows: usize,

    /// Stores the JPEG frame's component definitions and decoded sample planes.
    components: Vec<Component>,
}

#[derive(Debug, Clone, Copy)]
/// Stores component-specific table selections for one JPEG scan.
struct ScanComponent {
    /// Stores the index of the frame component referenced by this JPEG scan component.
    frame_index: usize,
    /// Stores the DC Huffman-table index selected for this scan component.
    dc_table: usize,
    /// Stores the AC Huffman-table index selected for this scan component.
    ac_table: usize,
}

// ============================================================================
// Entropy bit reader
// ============================================================================

/// Buffered entropy reader for JPEG scan data.
///
/// Valid entropy bits are kept in the low `bit_count` bits of `bit_buffer`, with
/// the oldest unread bit at the top of that range. This turns multi-bit reads
/// into one shift/mask operation instead of repeatedly calling `read_bit`.
struct EntropyBitReader<'a> {
    /// Encoded JPEG input.
    data: &'a [u8],
    /// Next encoded byte to inspect.
    pos: usize,
    /// Buffered entropy bits.
    bit_buffer: u64,
    /// Number of valid bits currently held in `bit_buffer`.
    bit_count: u8,
}

impl<'a> EntropyBitReader<'a> {
    /// Creates a bit reader positioned at the first entropy-coded byte.
    #[inline]
    fn new(data: &'a [u8], pos: usize) -> Self {
        Self {
            data,
            pos,
            bit_buffer: 0,
            bit_count: 0,
        }
    }

    /// Returns the encoded byte position reached by the buffered reader.
    #[inline(always)]
    fn position(&self) -> usize {
        self.pos
    }

    /// Discards JPEG byte-padding bits before a restart or following marker.
    #[inline(always)]
    fn byte_align(&mut self) {
        self.bit_buffer = 0;
        self.bit_count = 0;
    }

    /// Attempts to make at least `needed` entropy bits available.
    ///
    /// Returns `Ok(false)` when a marker is reached before enough bits can be
    /// buffered. The marker is left unconsumed for restart/following-marker logic.
    #[inline(always)]
    fn try_fill(&mut self, needed: u8) -> Result<bool, ImageError> {
        while self.bit_count < needed {
            let marker_start = self.pos;
            let byte = match self.data.get(self.pos) {
                Some(&byte) => byte,
                None => return Ok(false),
            };
            self.pos += 1;

            let entropy_byte = if byte != 0xFF {
                byte
            } else {
                // JPEG escapes an entropy byte 0xFF as FF 00. Consecutive FF
                // bytes before a marker are permitted as fill bytes.
                loop {
                    let next = match self.data.get(self.pos) {
                        Some(&next) => next,
                        None => {
                            self.pos = marker_start;
                            return Ok(false);
                        }
                    };
                    self.pos += 1;

                    match next {
                        0x00 => break 0xFF,
                        0xFF => continue,
                        _marker => {
                            self.pos = marker_start;
                            return Ok(false);
                        }
                    }
                }
            };

            self.bit_buffer = (self.bit_buffer << 8) | entropy_byte as u64;
            self.bit_count += 8;
        }

        Ok(true)
    }

    /// Peeks at up to 16 entropy bits without consuming them.
    #[inline(always)]
    fn try_peek_bits(&mut self, count: u8) -> Result<Option<u32>, ImageError> {
        debug_assert!(count <= 16);
        if !self.try_fill(count)? {
            return Ok(None);
        }

        let shift = self.bit_count - count;
        let mask = (1u64 << count) - 1;
        Ok(Some(((self.bit_buffer >> shift) & mask) as u32))
    }

    /// Removes already-consumed bits from the buffer.
    #[inline(always)]
    fn drop_bits(&mut self, count: u8) {
        debug_assert!(count <= self.bit_count);
        self.bit_count -= count;

        if self.bit_count == 0 {
            self.bit_buffer = 0;
        } else {
            self.bit_buffer &= (1u64 << self.bit_count) - 1;
        }
    }

    /// Reads one entropy bit.
    #[inline(always)]
    fn read_bit(&mut self) -> Result<u8, ImageError> {
        Ok(self.read_bits(1)? as u8)
    }

    /// Reads an entropy bit field in one buffered operation.
    #[inline(always)]
    fn read_bits(&mut self, count: u8) -> Result<u32, ImageError> {
        if count == 0 {
            return Ok(0);
        }

        if !self.try_fill(count)? {
            return Err(ImageError::new(
                "Unexpected JPEG marker or end of file inside entropy data",
            ));
        }

        let shift = self.bit_count - count;
        let mask = (1u64 << count) - 1;
        let value = ((self.bit_buffer >> shift) & mask) as u32;
        self.drop_bits(count);
        Ok(value)
    }

    /// Consumes the next restart marker and resets buffered entropy state.
    fn consume_restart(&mut self, expected: u8) -> Result<(), ImageError> {
        self.byte_align();

        if self.data.get(self.pos).copied() != Some(0xFF) {
            return Err(ImageError::new("Missing JPEG restart marker"));
        }

        while self.data.get(self.pos).copied() == Some(0xFF) {
            self.pos += 1;
        }

        let marker = *self
            .data
            .get(self.pos)
            .ok_or_else(|| ImageError::new("Truncated JPEG restart marker"))?;
        self.pos += 1;

        let wanted = 0xD0 + (expected & 7);
        if marker != wanted {
            return Err(ImageError::new(format!(
                "Expected JPEG restart marker 0xFF{wanted:02X}, found 0xFF{marker:02X}",
            )));
        }

        Ok(())
    }
}

// ============================================================================
// Public API
// ============================================================================

impl Jpeg {
    /// Decodes a JPEG from a borrowed byte slice.
    ///
    /// The source bytes are copied exactly once because `Jpeg` retains the encoded file.
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        let decoded = decode_jpeg(bytes)?;
        Ok(Self::from_decoded(decoded, bytes.to_vec()))
    }

    /// Decodes an owned JPEG byte vector without copying the encoded file.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, ImageError> {
        let decoded = decode_jpeg(&data)?;
        Ok(Self::from_decoded(decoded, data))
    }

    /// Decodes an owned JPEG directly at one of the format's native thumbnail
    /// scales, avoiding a full-resolution packed image allocation.
    pub fn from_bytes_scaled(data: Vec<u8>, scale: JpegScale) -> Result<Self, ImageError> {
        let decoded = decode_jpeg_scaled(&data, scale.divisor())?;
        Ok(Self::from_decoded(decoded, data))
    }

    /// Reads and decodes a baseline sequential JPEG from a stream.
    ///
    /// Supported:
    /// - SOF0 baseline DCT
    /// - 8-bit samples
    /// - Huffman entropy coding
    /// - grayscale
    /// - YCbCr
    /// - RGB
    /// - common chroma subsampling
    /// - restart intervals
    pub fn parse<R: Read + Seek>(reader: &mut R) -> Result<Self, ImageError> {
        let mut data = Vec::new();
        reader
            .read_to_end(&mut data)
            .map_err(|e| ImageError::new(format!("Failed to read JPEG: {e}")))?;
        Self::from_bytes(data)
    }

    /// Returns the packed decoded pixels without cloning them.
    #[inline]
    pub fn packed_pixels(&self) -> &[u32] {
        &self.pixels
    }

    /// Alias for [`Self::packed_pixels`] used by the common rich-decoder naming.
    #[inline]
    pub fn pixels_ref(&self) -> &[u32] {
        &self.pixels
    }

    /// Consumes the rich JPEG object and moves its decoded pixels out.
    #[inline]
    pub fn into_pixels(self) -> Vec<u32> {
        self.pixels
    }

    /// Borrows the encoded JPEG source retained by the rich API.
    #[inline]
    pub fn encoded_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Constructs a rich JPEG from owned encoded bytes. The encoded source is
    /// retained intentionally because the rich API exposes it through `data`.
    #[inline]
    pub fn parse_owned(data: Vec<u8>) -> Result<Self, ImageError> {
        Self::from_bytes(data)
    }

    /// Constructs the public decoder object from an already-decoded frame.
    #[inline]
    fn from_decoded(decoded: DecodedJpeg, data: Vec<u8>) -> Self {
        Self {
            width: decoded.width as u32,
            height: decoded.height as u32,
            components: decoded.components,
            color_space: decoded.color_space,
            pixels: decoded.pixels,
            data,
        }
    }
}

/// Decodes directly into the lightweight generic image representation without
/// retaining a second copy of the encoded JPEG file or cloning the output pixels.
pub(crate) fn decode_image(
    bytes: &[u8],
    options: &crate::DecodeOptions,
) -> Result<crate::Image, ImageError> {
    let scale = jpeg_scale_for_output(bytes, options.output_dimensions)?;
    let decoded = decode_jpeg_scaled(bytes, scale)?;
    let width = u32::try_from(decoded.width)
        .map_err(|_| ImageError::new("JPEG width exceeds u32"))?;
    let height = u32::try_from(decoded.height)
        .map_err(|_| ImageError::new("JPEG height exceeds u32"))?;
    crate::Image::from_packed_pixels(width, height, decoded.pixels, options)
}

fn jpeg_scale_for_output(
    bytes: &[u8],
    requested: Option<(u32, u32)>,
) -> Result<usize, ImageError> {
    let Some((target_w, target_h)) = requested else { return Ok(1); };
    if target_w == 0 || target_h == 0 { return Ok(1); }
    let info = crate::ImageInfo::read(bytes)?;
    let source_w = info.width as usize;
    let source_h = info.height as usize;
    let target_w = target_w as usize;
    let target_h = target_h as usize;
    for scale in [8usize, 4, 2] {
        if source_w.div_ceil(scale) >= target_w && source_h.div_ceil(scale) >= target_h {
            return Ok(scale);
        }
    }
    Ok(1)
}

// ============================================================================
// Decoder implementation
// ============================================================================

impl super::Decoder for Jpeg {
    /// Returns the decoded image or frame width in pixels.
    fn width(&self) -> u32 {
        self.width
    }

    /// Returns the decoded image or frame height in pixels.
    fn height(&self) -> u32 {
        self.height
    }

    fn into_image(self: Box<Self>) -> Result<crate::Image, ImageError> {
        let this = *self;
        crate::Image::from_packed_pixels(
            this.width,
            this.height,
            this.pixels,
            &crate::DecodeOptions::default(),
        )
    }
}

// ============================================================================
// JPEG decoding
// ============================================================================

/// Contains the fully reconstructed JPEG image produced by the internal decoder.
struct DecodedJpeg {
    /// Stores the image, frame, or plane width in pixels.
    width: usize,
    /// Stores the image, frame, or plane height in pixels.
    height: usize,

    /// Stores the number or collection of color components represented by this image.
    components: u8,

    /// Stores the color-space interpretation selected for the decoded image.
    color_space: JpegColorSpace,

    /// Stores the decoded packed pixel values produced by the decoder.
    pixels: Vec<u32>,
}

/// Decodes jpeg data into the representation required by the surrounding decoder.
///
/// Returns an error when the supplied data cannot be processed as required.
fn decode_jpeg(
    data: &[u8],
) -> Result<DecodedJpeg, ImageError> {
    decode_jpeg_scaled(data, 1)
}

fn decode_jpeg_scaled(
    data: &[u8],
    scale: usize,
) -> Result<DecodedJpeg, ImageError> {
    if !matches!(scale, 1 | 2 | 4 | 8) {
        return Err(ImageError::new("JPEG native scale must be 1, 2, 4, or 8"));
    }
    // ------------------------------------------------------------------------
    // SOI
    // ------------------------------------------------------------------------

    if data.len() < 2
        || data[0] != 0xFF
        || data[1] != SOI
    {
        return Err(ImageError::new(
            "Invalid JPEG SOI signature",
        ));
    }

    let mut pos = 2usize;

    // ------------------------------------------------------------------------
    // Tables
    // ------------------------------------------------------------------------

    let mut quant_tables:
        [Option<QuantTable>; 4] =
        std::array::from_fn(|_| None);

    let mut dc_tables:
        [Option<HuffmanTable>; 4] =
        std::array::from_fn(|_| None);

    let mut ac_tables:
        [Option<HuffmanTable>; 4] =
        std::array::from_fn(|_| None);

    let mut frame: Option<Frame> = None;

    let mut restart_interval = 0usize;

    let mut saw_jfif = false;
    let mut adobe_transform: Option<u8> = None;

    let mut decoded_scan = false;
    let mut decoded_pixels: Option<Vec<u32>> = None;
    let mut decoded_color_space: Option<JpegColorSpace> = None;

    // ------------------------------------------------------------------------
    // Marker loop
    // ------------------------------------------------------------------------

    while pos < data.len() {
        let marker =
            next_marker(data, &mut pos)?;

        match marker {
            EOI => {
                break;
            }

            SOI => {
                continue;
            }

            0xD0..=0xD7 => {
                return Err(ImageError::new(
                    "Unexpected JPEG restart marker outside scan",
                ));
            }

            // TEM marker has no segment length.
            0x01 => {
                continue;
            }

            _ => {}
        }

        let segment =
            read_segment(data, &mut pos)?;

        match marker {
            // ---------------------------------------------------------------
            // APP0 / JFIF
            // ---------------------------------------------------------------

            0xE0 => {
                if segment.starts_with(
                    b"JFIF\0",
                ) {
                    saw_jfif = true;
                }
            }

            // ---------------------------------------------------------------
            // APP14 / Adobe
            // ---------------------------------------------------------------

            0xEE => {
                if segment.len() >= 12
                    && segment.starts_with(
                        b"Adobe",
                    )
                {
                    adobe_transform =
                        Some(segment[11]);
                }
            }

            // ---------------------------------------------------------------
            // Quantization tables
            // ---------------------------------------------------------------

            DQT => {
                parse_dqt(
                    segment,
                    &mut quant_tables,
                )?;
            }

            // ---------------------------------------------------------------
            // Huffman tables
            // ---------------------------------------------------------------

            DHT => {
                parse_dht(
                    segment,
                    &mut dc_tables,
                    &mut ac_tables,
                )?;
            }

            // ---------------------------------------------------------------
            // Restart interval
            // ---------------------------------------------------------------

            DRI => {
                if segment.len() != 2 {
                    return Err(
                        ImageError::new(
                            "Invalid JPEG DRI segment",
                        ),
                    );
                }

                restart_interval =
                    u16::from_be_bytes([
                        segment[0],
                        segment[1],
                    ]) as usize;
            }

            // ---------------------------------------------------------------
            // Baseline frame
            // ---------------------------------------------------------------

            SOF0 => {
                if frame.is_some() {
                    return Err(
                        ImageError::new(
                            "JPEG contains multiple SOF frames",
                        ),
                    );
                }

                frame =
                    Some(parse_sof0(segment, scale)?);
            }

            // ---------------------------------------------------------------
            // Start of scan
            // ---------------------------------------------------------------

            SOS => {
                if decoded_scan {
                    return Err(
                        ImageError::new(
                            "Multi-scan baseline JPEGs are not supported",
                        ),
                    );
                }

                let frame =
                    frame
                        .as_mut()
                        .ok_or_else(|| {
                            ImageError::new(
                                "JPEG SOS encountered before SOF0",
                            )
                        })?;

                let scan_components =
                    parse_sos(
                        segment,
                        frame,
                    )?;

                // Common baseline JPEGs contain
                // all frame components in one scan.
                if scan_components.len()
                    != frame.components.len()
                {
                    return Err(
                        ImageError::new(
                            "Multi-scan color JPEGs are not supported",
                        ),
                    );
                }

                let color_space = determine_color_space(frame, saw_jfif, adobe_transform)?;
                let (next_pos, pixels) = decode_scan(
                    data,
                    pos,
                    frame,
                    &scan_components,
                    &quant_tables,
                    &dc_tables,
                    &ac_tables,
                    restart_interval,
                    color_space,
                )?;
                pos = next_pos;
                decoded_pixels = Some(pixels);
                decoded_color_space = Some(color_space);
                decoded_scan = true;
            }

            // ---------------------------------------------------------------
            // Unsupported SOF modes
            // ---------------------------------------------------------------

            0xC1
            | 0xC2
            | 0xC3
            | 0xC5
            | 0xC6
            | 0xC7
            | 0xC9
            | 0xCA
            | 0xCB
            | 0xCD
            | 0xCE
            | 0xCF => {
                return Err(
                    ImageError::new(
                        format!(
                            "Unsupported JPEG coding process \
                             0xFF{marker:02X}; \
                             only baseline SOF0 is supported"
                        ),
                    ),
                );
            }

            // ---------------------------------------------------------------
            // Other APPn / COM / metadata
            // ---------------------------------------------------------------

            _ => {
                // Segment intentionally ignored.
            }
        }
    }

    let frame =
        frame.ok_or_else(|| {
            ImageError::new(
                "JPEG is missing SOF0",
            )
        })?;

    if !decoded_scan {
        return Err(ImageError::new(
            "JPEG contains no decodable scan",
        ));
    }

    let color_space = determine_color_space(&frame, saw_jfif, adobe_transform)?;
    if decoded_color_space != Some(color_space) {
        return Err(ImageError::new(
            "JPEG color metadata changed after the decoded scan",
        ));
    }
    let pixels = decoded_pixels
        .ok_or_else(|| ImageError::new("JPEG contains no decoded pixel output"))?;

    Ok(DecodedJpeg {
        width: frame.width,
        height: frame.height,
        components:
            frame.components.len() as u8,
        color_space,
        pixels,
    })
}

// ============================================================================
// Marker parsing
// ============================================================================

/// Scans forward to the next JPEG marker and returns its marker byte.
///
/// Returns an error when the supplied data cannot be processed as required.
fn next_marker(
    data: &[u8],
    pos: &mut usize,
) -> Result<u8, ImageError> {
    // Search for marker prefix.
    while *pos < data.len()
        && data[*pos] != 0xFF
    {
        *pos += 1;
    }

    if *pos >= data.len() {
        return Err(
            ImageError::new(
                "Expected JPEG marker",
            ),
        );
    }

    // Skip FF fill bytes.
    while *pos < data.len()
        && data[*pos] == 0xFF
    {
        *pos += 1;
    }

    if *pos >= data.len() {
        return Err(
            ImageError::new(
                "Truncated JPEG marker",
            ),
        );
    }

    let marker = data[*pos];

    *pos += 1;

    if marker == 0x00 {
        return Err(
            ImageError::new(
                "Unexpected stuffed FF byte outside JPEG scan",
            ),
        );
    }

    Ok(marker)
}

/// Reads segment from the supplied input while advancing the relevant cursor or bit position.
///
/// Returns an error when the supplied data cannot be processed as required.
fn read_segment<'a>(
    data: &'a [u8],
    pos: &mut usize,
) -> Result<&'a [u8], ImageError> {
    if *pos + 2 > data.len() {
        return Err(
            ImageError::new(
                "Truncated JPEG segment length",
            ),
        );
    }

    let length =
        u16::from_be_bytes([
            data[*pos],
            data[*pos + 1],
        ]) as usize;

    *pos += 2;

    if length < 2 {
        return Err(
            ImageError::new(
                "Invalid JPEG segment length",
            ),
        );
    }

    let payload_len =
        length - 2;

    let end =
        (*pos)
            .checked_add(payload_len)
            .ok_or_else(|| {
                ImageError::new(
                    "JPEG segment length overflow",
                )
            })?;

    if end > data.len() {
        return Err(
            ImageError::new(
                "JPEG segment extends past end of file",
            ),
        );
    }

    let segment =
        &data[*pos..end];

    *pos = end;

    Ok(segment)
}

// ============================================================================
// DQT
// ============================================================================

/// Parses dqt from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_dqt(
    mut segment: &[u8],
    tables: &mut [Option<QuantTable>; 4],
) -> Result<(), ImageError> {
    while !segment.is_empty() {
        let info = segment[0];

        segment =
            &segment[1..];

        let precision =
            info >> 4;

        let table_id =
            (info & 0x0F) as usize;

        if table_id >= 4 {
            return Err(
                ImageError::new(
                    "JPEG quantization table ID is out of range",
                ),
            );
        }

        let mut values =
            [0i32; 64];

        match precision {
            // ---------------------------------------------------------------
            // 8-bit quantization table
            // ---------------------------------------------------------------

            0 => {
                if segment.len() < 64 {
                    return Err(
                        ImageError::new(
                            "Truncated JPEG quantization table",
                        ),
                    );
                }

                for i in 0..64 {
                    values[ZIGZAG[i]] =
                        segment[i] as i32;
                }

                segment =
                    &segment[64..];
            }

            // ---------------------------------------------------------------
            // 16-bit quantization table
            // ---------------------------------------------------------------

            1 => {
                if segment.len() < 128 {
                    return Err(
                        ImageError::new(
                            "Truncated 16-bit JPEG quantization table",
                        ),
                    );
                }

                for i in 0..64 {
                    let value =
                        u16::from_be_bytes([
                            segment[i * 2],
                            segment[
                                i * 2 + 1
                            ],
                        ]);

                    values[ZIGZAG[i]] =
                        value as i32;
                }

                segment =
                    &segment[128..];
            }

            _ => {
                return Err(
                    ImageError::new(
                        "Invalid JPEG quantization table precision",
                    ),
                );
            }
        }

        tables[table_id] =
            Some(QuantTable {
                values,
            });
    }

    Ok(())
}

// ============================================================================
// DHT
// ============================================================================

/// Parses dht from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_dht(
    mut segment: &[u8],
    dc_tables: &mut [Option<HuffmanTable>; 4],
    ac_tables: &mut [Option<HuffmanTable>; 4],
) -> Result<(), ImageError> {
    while !segment.is_empty() {
        if segment.len() < 17 {
            return Err(
                ImageError::new(
                    "Truncated JPEG Huffman table",
                ),
            );
        }

        let info =
            segment[0];

        segment =
            &segment[1..];

        let class =
            info >> 4;

        let table_id =
            (info & 0x0F) as usize;

        if table_id >= 4
            || class > 1
        {
            return Err(
                ImageError::new(
                    "Invalid JPEG Huffman table selector",
                ),
            );
        }

        let mut counts =
            [0u8; 16];

        counts.copy_from_slice(
            &segment[..16],
        );

        segment =
            &segment[16..];

        let symbol_count: usize =
            counts
                .iter()
                .map(|&count| {
                    count as usize
                })
                .sum();

        if segment.len()
            < symbol_count
        {
            return Err(
                ImageError::new(
                    "Truncated JPEG Huffman symbols",
                ),
            );
        }

        let symbols = &segment[..symbol_count];
        segment = &segment[symbol_count..];

        let table = HuffmanTable::new(counts, symbols)?;

        match class {
            0 => {
                dc_tables[table_id] =
                    Some(table);
            }

            1 => {
                ac_tables[table_id] =
                    Some(table);
            }

            _ => unreachable!(),
        }
    }

    Ok(())
}

// ============================================================================
// SOF0
// ============================================================================

/// Parses sof0 from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_sof0(
    segment: &[u8],
    scale: usize,
) -> Result<Frame, ImageError> {
    if segment.len() < 6 {
        return Err(
            ImageError::new(
                "Truncated JPEG SOF0 segment",
            ),
        );
    }

    let precision =
        segment[0];

    if precision != 8 {
        return Err(
            ImageError::new(
                format!(
                    "Unsupported JPEG precision {precision}; \
                     baseline decoder requires 8-bit samples"
                ),
            ),
        );
    }

    let height =
        u16::from_be_bytes([
            segment[1],
            segment[2],
        ]) as usize;

    let width =
        u16::from_be_bytes([
            segment[3],
            segment[4],
        ]) as usize;

    let component_count =
        segment[5] as usize;

    if width == 0
        || height == 0
    {
        return Err(
            ImageError::new(
                "JPEG width or height is zero",
            ),
        );
    }

    if component_count != 1
        && component_count != 3
    {
        return Err(
            ImageError::new(
                format!(
                    "Unsupported JPEG component count {component_count}; \
                     expected 1 or 3"
                ),
            ),
        );
    }

    let expected_length =
        6 + component_count * 3;

    if segment.len()
        != expected_length
    {
        return Err(
            ImageError::new(
                "Invalid JPEG SOF0 component data",
            ),
        );
    }

    let mut raw_components =
        Vec::with_capacity(
            component_count,
        );

    let mut max_h = 0usize;
    let mut max_v = 0usize;

    for i in 0..component_count {
        let base =
            6 + i * 3;

        let id =
            segment[base];

        let sampling =
            segment[base + 1];

        let h =
            (sampling >> 4)
                as usize;

        let v =
            (sampling & 0x0F)
                as usize;

        let quant_table =
            segment[base + 2]
                as usize;

        if h == 0
            || v == 0
            || h > 4
            || v > 4
        {
            return Err(
                ImageError::new(
                    "Invalid JPEG sampling factor",
                ),
            );
        }

        if quant_table >= 4 {
            return Err(
                ImageError::new(
                    "JPEG quantization table selector is out of range",
                ),
            );
        }

        if raw_components
            .iter()
            .any(
                |component:
                    &(u8, usize, usize, usize)| {
                    component.0 == id
                },
            )
        {
            return Err(
                ImageError::new(
                    "Duplicate JPEG component ID",
                ),
            );
        }

        max_h =
            max_h.max(h);

        max_v =
            max_v.max(v);

        raw_components.push((
            id,
            h,
            v,
            quant_table,
        ));
    }

    // Grayscale images should normally
    // use 1x1 sampling.
    if component_count == 1
        && (max_h != 1
            || max_v != 1)
    {
        return Err(
            ImageError::new(
                "Unsupported grayscale JPEG sampling factor",
            ),
        );
    }

    let mcu_width =
        max_h * 8;

    let mcu_height =
        max_v * 8;

    let mcu_cols =
        width.div_ceil(
            mcu_width,
        );

    let mcu_rows =
        height.div_ceil(
            mcu_height,
        );

    let mut components =
        Vec::with_capacity(
            component_count,
        );

    for (
        id,
        h,
        v,
        quant_table,
    ) in raw_components
    {
        let blocks_x =
            mcu_cols
                .checked_mul(h)
                .ok_or_else(|| {
                    ImageError::new(
                        "JPEG component width overflow",
                    )
                })?;

        let blocks_y =
            mcu_rows
                .checked_mul(v)
                .ok_or_else(|| {
                    ImageError::new(
                        "JPEG component height overflow",
                    )
                })?;

        let block_output = 8 / scale;
        let stride =
            blocks_x
                .checked_mul(block_output)
                .ok_or_else(|| {
                    ImageError::new(
                        "JPEG component stride overflow",
                    )
                })?;

        // Only one MCU row is retained. Baseline JPEG prediction has no
        // dependency on previously reconstructed spatial rows, so each completed
        // MCU row can be color-converted directly into final packed output.
        let plane_height = v
            .checked_mul(block_output)
            .ok_or_else(|| {
                ImageError::new(
                    "JPEG component row buffer overflow",
                )
            })?;

        let sample_count =
            stride
                .checked_mul(
                    plane_height,
                )
                .ok_or_else(|| {
                    ImageError::new(
                        "JPEG component sample count overflow",
                    )
                })?;

        components.push(
            Component {
                id,
                h,
                v,
                quant_table,
                blocks_x,
                blocks_y,
                stride,
                samples:
                    vec![
                        0;
                        sample_count
                    ],
            },
        );
    }

    Ok(Frame {
        width: width.div_ceil(scale),
        height: height.div_ceil(scale),
        scale,
        max_h,
        max_v,
        mcu_cols,
        mcu_rows,
        components,
    })
}

// ============================================================================
// SOS
// ============================================================================

/// Parses sos from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_sos(
    segment: &[u8],
    frame: &Frame,
) -> Result<
    Vec<ScanComponent>,
    ImageError,
> {
    if segment.is_empty() {
        return Err(
            ImageError::new(
                "Truncated JPEG SOS segment",
            ),
        );
    }

    let component_count =
        segment[0] as usize;

    let expected_length =
        1
            + component_count * 2
            + 3;

    if component_count == 0
        || segment.len()
            != expected_length
    {
        return Err(
            ImageError::new(
                "Invalid JPEG SOS segment",
            ),
        );
    }

    let mut components =
        Vec::with_capacity(
            component_count,
        );

    for i in 0..component_count {
        let base =
            1 + i * 2;

        let component_id =
            segment[base];

        let selectors =
            segment[base + 1];

        let dc_table =
            (selectors >> 4)
                as usize;

        let ac_table =
            (selectors & 0x0F)
                as usize;

        if dc_table >= 4
            || ac_table >= 4
        {
            return Err(
                ImageError::new(
                    "JPEG Huffman table selector is out of range",
                ),
            );
        }

        let frame_index =
            frame
                .components
                .iter()
                .position(
                    |component| {
                        component.id
                            == component_id
                    },
                )
                .ok_or_else(|| {
                    ImageError::new(
                        "JPEG scan references unknown component",
                    )
                })?;

        if components
            .iter()
            .any(
                |component:
                    &ScanComponent| {
                    component.frame_index
                        == frame_index
                },
            )
        {
            return Err(
                ImageError::new(
                    "Duplicate JPEG scan component",
                ),
            );
        }

        components.push(
            ScanComponent {
                frame_index,
                dc_table,
                ac_table,
            },
        );
    }

    // Baseline sequential scan parameters:
    //
    // Ss = 0
    // Se = 63
    // Ah = 0
    // Al = 0

    let spectral_start =
        segment[
            1 + component_count * 2
        ];

    let spectral_end =
        segment[
            2 + component_count * 2
        ];

    let approximation =
        segment[
            3 + component_count * 2
        ];

    if spectral_start != 0
        || spectral_end != 63
        || approximation != 0
    {
        return Err(
            ImageError::new(
                "Unsupported JPEG scan parameters; \
                 expected baseline Ss=0, Se=63, Ah/Al=0",
            ),
        );
    }

    Ok(components)
}

// ============================================================================
// Entropy decode
// ============================================================================

/// Scan component state with table selections resolved before entering the MCU loop.
struct ResolvedScanComponent<'a> {
    component_index: usize,
    horizontal_sampling: usize,
    vertical_sampling: usize,
    quant_table: &'a QuantTable,
    dc_table: &'a HuffmanTable,
    ac_table: &'a HuffmanTable,
}

#[allow(clippy::too_many_arguments)]
/// Decodes one baseline sequential scan.
fn decode_scan(
    data: &[u8],
    start: usize,
    frame: &mut Frame,
    scan_components: &[ScanComponent],
    quant_tables: &[Option<QuantTable>; 4],
    dc_tables: &[Option<HuffmanTable>; 4],
    ac_tables: &[Option<HuffmanTable>; 4],
    restart_interval: usize,
    color_space: JpegColorSpace,
) -> Result<(usize, Vec<u32>), ImageError> {
    let total_mcus = frame
        .mcu_cols
        .checked_mul(frame.mcu_rows)
        .ok_or_else(|| ImageError::new("JPEG MCU count overflow"))?;

    // Resolve all table/component lookups once, not once per MCU/block.
    let mut resolved = Vec::with_capacity(scan_components.len());
    for scan in scan_components {
        let component = &frame.components[scan.frame_index];
        let quant_table = quant_tables[component.quant_table]
            .as_ref()
            .ok_or_else(|| ImageError::new(format!(
                "Missing JPEG quantization table {}",
                component.quant_table
            )))?;
        let dc_table = dc_tables[scan.dc_table]
            .as_ref()
            .ok_or_else(|| ImageError::new(format!(
                "Missing JPEG DC Huffman table {}",
                scan.dc_table
            )))?;
        let ac_table = ac_tables[scan.ac_table]
            .as_ref()
            .ok_or_else(|| ImageError::new(format!(
                "Missing JPEG AC Huffman table {}",
                scan.ac_table
            )))?;

        resolved.push(ResolvedScanComponent {
            component_index: scan.frame_index,
            horizontal_sampling: component.h,
            vertical_sampling: component.v,
            quant_table,
            dc_table,
            ac_table,
        });
    }

    let mut reader = EntropyBitReader::new(data, start);
    // SOF0 is limited to 1 or 3 components; fixed storage avoids a heap allocation.
    let mut dc_predictors = [0i32; 4];
    let mut restart_index = 0u8;
    let mut processed_mcus = 0usize;
    let mut mcus_until_restart = restart_interval;
    let pixel_count = frame.width
        .checked_mul(frame.height)
        .ok_or_else(|| ImageError::new("JPEG pixel count overflow"))?;
    let mut pixels = vec![0u32; pixel_count];

    // Nested coordinates avoid a division and remainder for every MCU.
    for mcu_y in 0..frame.mcu_rows {
        for mcu_x in 0..frame.mcu_cols {
            for scan in &resolved {
                let component_index = scan.component_index;
                let predictor = &mut dc_predictors[component_index];

                for block_y in 0..scan.vertical_sampling {
                    for block_x in 0..scan.horizontal_sampling {
                        let (coefficients, has_ac) = decode_block(
                            &mut reader,
                            scan.dc_table,
                            scan.ac_table,
                            scan.quant_table,
                            predictor,
                        )?;

                        let destination_x = mcu_x * scan.horizontal_sampling + block_x;
                        let destination_y = block_y;

                        idct_block_into(
                            coefficients,
                            has_ac,
                            &mut frame.components[component_index],
                            destination_x,
                            destination_y,
                            frame.scale,
                        )?;
                    }
                }
            }

            processed_mcus += 1;
            if restart_interval != 0 {
                mcus_until_restart -= 1;
                if mcus_until_restart == 0 && processed_mcus < total_mcus {
                    reader.consume_restart(restart_index)?;
                    restart_index = (restart_index + 1) & 7;
                    dc_predictors.fill(0);
                    mcus_until_restart = restart_interval;
                }
            }
        }

        convert_completed_mcu_row(frame, color_space, mcu_y, &mut pixels)?;
    }

    reader.byte_align();
    Ok((reader.position(), pixels))
}


/// Converts one completed JPEG MCU row to final packed pixels and immediately
/// makes the component row buffers reusable for the next entropy row.
fn convert_completed_mcu_row(
    frame: &Frame,
    color_space: JpegColorSpace,
    mcu_y: usize,
    pixels: &mut [u32],
) -> Result<(), ImageError> {
    let block_output = 8 / frame.scale;
    let row_capacity = frame.max_v * block_output;
    let output_y = mcu_y * row_capacity;
    if output_y >= frame.height {
        return Ok(());
    }
    let rows = row_capacity.min(frame.height - output_y);
    let start = output_y * frame.width;
    let end = start + rows * frame.width;
    let output = &mut pixels[start..end];

    match color_space {
        JpegColorSpace::YCbCr => convert_ycbcr_rows(frame, 0, output),
        JpegColorSpace::Grayscale => {
            let component = &frame.components[0];
            for local_y in 0..rows {
                let source_row = local_y * component.v / frame.max_v * component.stride;
                let dst = &mut output[local_y * frame.width..(local_y + 1) * frame.width];
                if component.h == frame.max_h {
                    crate::simd::gray8_to_u32(
                        &component.samples[source_row..source_row + frame.width],
                        dst,
                    );
                } else {
                    let mut sx = 0usize;
                    let mut phase = 0usize;
                    for pixel in dst {
                        let value = component.samples[source_row + sx];
                        *pixel = pack_rgba(value, value, value, 255);
                        phase += component.h;
                        if phase >= frame.max_h {
                            phase -= frame.max_h;
                            sx += 1;
                        }
                    }
                }
            }
        }
        JpegColorSpace::RGB => {
            let r = &frame.components[0];
            let g = &frame.components[1];
            let b = &frame.components[2];
            for local_y in 0..rows {
                let rr = local_y * r.v / frame.max_v * r.stride;
                let gr = local_y * g.v / frame.max_v * g.stride;
                let br = local_y * b.v / frame.max_v * b.stride;
                let dst = &mut output[local_y * frame.width..(local_y + 1) * frame.width];
                let (mut rx, mut gx, mut bx) = (0usize, 0usize, 0usize);
                let (mut rp, mut gp, mut bp) = (0usize, 0usize, 0usize);
                for pixel in dst {
                    *pixel = pack_rgba(
                        r.samples[rr + rx],
                        g.samples[gr + gx],
                        b.samples[br + bx],
                        255,
                    );
                    rp += r.h;
                    if rp >= frame.max_h { rp -= frame.max_h; rx += 1; }
                    gp += g.h;
                    if gp >= frame.max_h { gp -= frame.max_h; gx += 1; }
                    bp += b.h;
                    if bp >= frame.max_h { bp -= frame.max_h; bx += 1; }
                }
            }
        }
    }
    Ok(())
}

// ============================================================================
// Decode one 8x8 block
// ============================================================================

/// Decodes and dequantizes one JPEG block.
///
/// The boolean result records whether any AC coefficient was present so the IDCT
/// can take the constant-block path without scanning 63 coefficients again.
#[inline]
fn decode_block(
    reader: &mut EntropyBitReader<'_>,
    dc_table: &HuffmanTable,
    ac_table: &HuffmanTable,
    quant_table: &QuantTable,
    dc_predictor: &mut i32,
) -> Result<([i32; 64], bool), ImageError> {
    let mut coefficients = [0i32; 64];

    let dc_size = dc_table.decode(reader)?;
    if dc_size > 11 {
        return Err(ImageError::new(
            "Invalid baseline JPEG DC coefficient size",
        ));
    }

    let dc_difference = receive_extend(reader, dc_size)?;
    *dc_predictor = dc_predictor
        .checked_add(dc_difference)
        .ok_or_else(|| ImageError::new("JPEG DC predictor overflow"))?;

    coefficients[0] = (*dc_predictor)
        .checked_mul(quant_table.values[0])
        .ok_or_else(|| ImageError::new("JPEG DC dequantization overflow"))?;

    let mut k = 1usize;
    let mut has_ac = false;

    while k < 64 {
        let symbol = ac_table.decode(reader)?;
        if symbol == 0x00 {
            break;
        }
        if symbol == 0xF0 {
            k += 16;
            if k > 64 {
                return Err(ImageError::new("JPEG AC zero run exceeds block"));
            }
            continue;
        }

        let zero_run = (symbol >> 4) as usize;
        let size = symbol & 0x0F;
        if size == 0 || size > 10 {
            return Err(ImageError::new(
                "Invalid baseline JPEG AC coefficient size",
            ));
        }

        k += zero_run;
        if k >= 64 {
            return Err(ImageError::new("JPEG AC coefficient exceeds block"));
        }

        let value = receive_extend(reader, size)?;
        let natural_index = ZIGZAG[k];
        coefficients[natural_index] = value
            .checked_mul(quant_table.values[natural_index])
            .ok_or_else(|| ImageError::new("JPEG AC dequantization overflow"))?;
        has_ac = true;
        k += 1;
    }

    Ok((coefficients, has_ac))
}

// ============================================================================
// JPEG signed coefficient extension
// ============================================================================

/// Reads a JPEG magnitude and performs branch-light sign extension.
#[inline(always)]
fn receive_extend(reader: &mut EntropyBitReader<'_>, size: u8) -> Result<i32, ImageError> {
    if size == 0 {
        return Ok(0);
    }

    let value = reader.read_bits(size)? as i32;
    let threshold = 1i32 << (size - 1);
    // For negative JPEG magnitudes add -(2^size - 1); otherwise add zero.
    let correction = ((value - threshold) >> 31) & ((-1i32 << size) + 1);
    Ok(value + correction)
}

// ============================================================================
// Integer IDCT + component output
// ============================================================================

const IDCT_W1: i64 = 2841;
const IDCT_W2: i64 = 2676;
const IDCT_W3: i64 = 2408;
const IDCT_W5: i64 = 1609;
const IDCT_W6: i64 = 1108;
const IDCT_W7: i64 = 565;

/// Runs a fast integer IDCT and writes the 8x8 result directly into the component plane.
///
/// Writing directly avoids constructing a second 64-byte spatial block and then copying it.
#[inline]
fn idct_block_into(
    mut block: [i32; 64],
    has_ac: bool,
    component: &mut Component,
    block_x: usize,
    block_y: usize,
    scale: usize,
) -> Result<(), ImageError> {
    if block_x >= component.blocks_x || block_y >= component.blocks_y {
        return Err(ImageError::new("JPEG block destination is out of range"));
    }

    let block_output = 8 / scale;
    let start_x = block_x * block_output;
    let start_y = block_y * block_output;
    let first = start_y * component.stride + start_x;
    let last = first + (block_output - 1) * component.stride + block_output;
    if last > component.samples.len() {
        return Err(ImageError::new("JPEG block destination exceeds component plane"));
    }

    if !has_ac {
        let value = clamp_i64_u8(((block[0] as i64 + 4) >> 3) + 128);
        for row in 0..block_output {
            let destination = first + row * component.stride;
            component.samples[destination..destination + block_output].fill(value);
        }
        return Ok(());
    }

    for row in 0..8 {
        idct_row(&mut block, row * 8);
    }

    if scale == 1 {
        // Bounds were validated above. Raw output writes remove 64 repeated
        // bounds checks from every non-constant full-resolution block.
        let output = component.samples.as_mut_ptr();
        for column in 0..8 {
            // SAFETY: `last <= samples.len()` proves every address written by
            // the column pass is inside the component buffer.
            unsafe {
                idct_column_into(&block, column, output.add(first + column), component.stride);
            }
        }
        return Ok(());
    }

    // Reduced-size JPEG decode: run the integer IDCT into one stack block and
    // keep only the native 1/2, 1/4, or 1/8 grid. This avoids allocating full
    // component planes and a full packed image before thumbnail rescaling.
    let mut spatial = [0u8; 64];
    for column in 0..8 {
        // SAFETY: `spatial` contains exactly eight rows with a stride of eight.
        unsafe { idct_column_into(&block, column, spatial.as_mut_ptr().add(column), 8); }
    }
    for out_y in 0..block_output {
        let source_y = out_y * scale;
        let destination = first + out_y * component.stride;
        for out_x in 0..block_output {
            component.samples[destination + out_x] = spatial[source_y * 8 + out_x * scale];
        }
    }
    Ok(())
}

/// Performs the horizontal pass of the integer IDCT.
#[inline(always)]
fn idct_row(block: &mut [i32; 64], base: usize) {
    let mut x1 = (block[base + 4] as i64) << 11;
    let mut x2 = block[base + 6] as i64;
    let mut x3 = block[base + 2] as i64;
    let mut x4 = block[base + 1] as i64;
    let mut x5 = block[base + 7] as i64;
    let mut x6 = block[base + 5] as i64;
    let mut x7 = block[base + 3] as i64;

    if (x1 | x2 | x3 | x4 | x5 | x6 | x7) == 0 {
        let dc = block[base] << 3;
        block[base..base + 8].fill(dc);
        return;
    }

    let mut x0 = ((block[base] as i64) << 11) + 128;
    let mut x8 = IDCT_W7 * (x4 + x5);
    x4 = x8 + (IDCT_W1 - IDCT_W7) * x4;
    x5 = x8 - (IDCT_W1 + IDCT_W7) * x5;
    x8 = IDCT_W3 * (x6 + x7);
    x6 = x8 - (IDCT_W3 - IDCT_W5) * x6;
    x7 = x8 - (IDCT_W3 + IDCT_W5) * x7;

    x8 = x0 + x1;
    x0 -= x1;
    x1 = IDCT_W6 * (x3 + x2);
    x2 = x1 - (IDCT_W2 + IDCT_W6) * x2;
    x3 = x1 + (IDCT_W2 - IDCT_W6) * x3;
    x1 = x4 + x6;
    x4 -= x6;
    x6 = x5 + x7;
    x5 -= x7;
    x7 = x8 + x3;
    x8 -= x3;
    x3 = x0 + x2;
    x0 -= x2;
    x2 = (181 * (x4 + x5) + 128) >> 8;
    x4 = (181 * (x4 - x5) + 128) >> 8;

    block[base] = ((x7 + x1) >> 8) as i32;
    block[base + 1] = ((x3 + x2) >> 8) as i32;
    block[base + 2] = ((x0 + x4) >> 8) as i32;
    block[base + 3] = ((x8 + x6) >> 8) as i32;
    block[base + 4] = ((x8 - x6) >> 8) as i32;
    block[base + 5] = ((x0 - x4) >> 8) as i32;
    block[base + 6] = ((x3 - x2) >> 8) as i32;
    block[base + 7] = ((x7 - x1) >> 8) as i32;
}

/// Performs the vertical pass and writes one column directly to component memory.
///
/// # Safety
/// `output` must point to the first pixel of a valid 8-row destination column,
/// with `stride` bytes available between successive rows.
#[inline(always)]
unsafe fn idct_column_into(
    block: &[i32; 64],
    column: usize,
    output: *mut u8,
    stride: usize,
) {
    let mut x1 = (block[column + 8 * 4] as i64) << 8;
    let mut x2 = block[column + 8 * 6] as i64;
    let mut x3 = block[column + 8 * 2] as i64;
    let mut x4 = block[column + 8] as i64;
    let mut x5 = block[column + 8 * 7] as i64;
    let mut x6 = block[column + 8 * 5] as i64;
    let mut x7 = block[column + 8 * 3] as i64;

    if (x1 | x2 | x3 | x4 | x5 | x6 | x7) == 0 {
        let value = clamp_i64_u8(((block[column] as i64 + 32) >> 6) + 128);
        for row in 0..8 {
            // SAFETY: upheld by the function contract.
            unsafe { *output.add(row * stride) = value; }
        }
        return;
    }

    let mut x0 = ((block[column] as i64) << 8) + 8192;
    let mut x8 = IDCT_W7 * (x4 + x5) + 4;
    x4 = (x8 + (IDCT_W1 - IDCT_W7) * x4) >> 3;
    x5 = (x8 - (IDCT_W1 + IDCT_W7) * x5) >> 3;
    x8 = IDCT_W3 * (x6 + x7) + 4;
    x6 = (x8 - (IDCT_W3 - IDCT_W5) * x6) >> 3;
    x7 = (x8 - (IDCT_W3 + IDCT_W5) * x7) >> 3;

    x8 = x0 + x1;
    x0 -= x1;
    x1 = IDCT_W6 * (x3 + x2) + 4;
    x2 = (x1 - (IDCT_W2 + IDCT_W6) * x2) >> 3;
    x3 = (x1 + (IDCT_W2 - IDCT_W6) * x3) >> 3;
    x1 = x4 + x6;
    x4 -= x6;
    x6 = x5 + x7;
    x5 -= x7;
    x7 = x8 + x3;
    x8 -= x3;
    x3 = x0 + x2;
    x0 -= x2;
    x2 = (181 * (x4 + x5) + 128) >> 8;
    x4 = (181 * (x4 - x5) + 128) >> 8;

    let values = [
        ((x7 + x1) >> 14) + 128,
        ((x3 + x2) >> 14) + 128,
        ((x0 + x4) >> 14) + 128,
        ((x8 + x6) >> 14) + 128,
        ((x8 - x6) >> 14) + 128,
        ((x0 - x4) >> 14) + 128,
        ((x3 - x2) >> 14) + 128,
        ((x7 - x1) >> 14) + 128,
    ];

    for row in 0..8 {
        // SAFETY: upheld by the function contract.
        unsafe { *output.add(row * stride) = clamp_i64_u8(values[row]); }
    }
}

// ============================================================================
// Color-space detection
// ============================================================================

/// Determines the decoded JPEG color-space interpretation from component identifiers and frame metadata.
///
/// Returns an error when the supplied data cannot be processed as required.
fn determine_color_space(
    frame: &Frame,
    saw_jfif: bool,
    adobe_transform: Option<u8>,
) -> Result<JpegColorSpace, ImageError> {
    match frame.components.len() {
        // --------------------------------------------------------------------
        // Grayscale
        // --------------------------------------------------------------------

        1 => {
            Ok(
                JpegColorSpace::Grayscale,
            )
        }

        // --------------------------------------------------------------------
        // RGB / YCbCr
        // --------------------------------------------------------------------

        3 => {
            let component_ids_are_rgb =
                frame
                    .components
                    .iter()
                    .map(
                        |component| {
                            component.id
                        },
                    )
                    .eq([
                        b'R',
                        b'G',
                        b'B',
                    ]);

            // Adobe transform 0 normally
            // means no YCbCr transform.
            if adobe_transform
                == Some(0)
                || component_ids_are_rgb
            {
                return Ok(
                    JpegColorSpace::RGB,
                );
            }

            if saw_jfif
                || adobe_transform
                    == Some(1)
                || adobe_transform
                    .is_none()
            {
                return Ok(
                    JpegColorSpace::YCbCr,
                );
            }

            Err(
                ImageError::new(
                    format!(
                        "Unsupported Adobe JPEG color transform {:?}",
                        adobe_transform
                    ),
                ),
            )
        }

        count => {
            Err(
                ImageError::new(
                    format!(
                        "Unsupported JPEG component count {count}"
                    ),
                ),
            )
        }
    }
}

// ============================================================================
// Component planes -> 0xRRGGBBAA
// ============================================================================
/// Converts a contiguous set of output rows from YCbCr to packed RGBA.
fn convert_ycbcr_rows(frame: &Frame, start_y: usize, output: &mut [u32]) {
    let width = frame.width;
    if width == 0 || output.is_empty() {
        return;
    }

    let y_plane = &frame.components[0];
    let cb_plane = &frame.components[1];
    let cr_plane = &frame.components[2];
    let row_count = output.len() / width;

    let y_full = y_plane.h == frame.max_h && y_plane.v == frame.max_v;
    let chroma_same = cb_plane.h == cr_plane.h && cb_plane.v == cr_plane.v;

    if y_full && chroma_same {
        let h_ratio = frame.max_h / cb_plane.h;
        let v_ratio = frame.max_v / cb_plane.v;
        if cb_plane.h * h_ratio == frame.max_h
            && cb_plane.v * v_ratio == frame.max_v
            && matches!(h_ratio, 1 | 2 | 4)
            && matches!(v_ratio, 1 | 2 | 4)
        {
            for local_y in 0..row_count {
                let y = start_y + local_y;
                let y_base = y * y_plane.stride;
                let c_base = (y / v_ratio) * cb_plane.stride;
                let cr_base = (y / v_ratio) * cr_plane.stride;
                let dst = &mut output[local_y * width..(local_y + 1) * width];

                match h_ratio {
                    1 => {
                        for x in 0..width {
                            let terms = chroma_terms(
                                cb_plane.samples[c_base + x],
                                cr_plane.samples[cr_base + x],
                            );
                            dst[x] = y_with_chroma_terms(
                                y_plane.samples[y_base + x],
                                terms,
                            );
                        }
                    }
                    2 => {
                        let pairs = width / 2;
                        for c in 0..pairs {
                            let terms = chroma_terms(
                                cb_plane.samples[c_base + c],
                                cr_plane.samples[cr_base + c],
                            );
                            let x = c * 2;
                            dst[x] = y_with_chroma_terms(
                                y_plane.samples[y_base + x],
                                terms,
                            );
                            dst[x + 1] = y_with_chroma_terms(
                                y_plane.samples[y_base + x + 1],
                                terms,
                            );
                        }
                        if width & 1 != 0 {
                            let x = width - 1;
                            let c = x >> 1;
                            let terms = chroma_terms(
                                cb_plane.samples[c_base + c],
                                cr_plane.samples[cr_base + c],
                            );
                            dst[x] = y_with_chroma_terms(
                                y_plane.samples[y_base + x],
                                terms,
                            );
                        }
                    }
                    4 => {
                        let groups = width / 4;
                        for c in 0..groups {
                            let terms = chroma_terms(
                                cb_plane.samples[c_base + c],
                                cr_plane.samples[cr_base + c],
                            );
                            let x = c * 4;
                            dst[x] = y_with_chroma_terms(y_plane.samples[y_base + x], terms);
                            dst[x + 1] = y_with_chroma_terms(y_plane.samples[y_base + x + 1], terms);
                            dst[x + 2] = y_with_chroma_terms(y_plane.samples[y_base + x + 2], terms);
                            dst[x + 3] = y_with_chroma_terms(y_plane.samples[y_base + x + 3], terms);
                        }
                        for x in (groups * 4)..width {
                            let c = x >> 2;
                            let terms = chroma_terms(
                                cb_plane.samples[c_base + c],
                                cr_plane.samples[cr_base + c],
                            );
                            dst[x] = y_with_chroma_terms(y_plane.samples[y_base + x], terms);
                        }
                    }
                    _ => unreachable!(),
                }
            }
            return;
        }
    }

    // Rare sampling layouts use accumulator stepping instead of integer division per pixel.
    for local_y in 0..row_count {
        let y = start_y + local_y;
        let yy_row = y * y_plane.v / frame.max_v * y_plane.stride;
        let cb_row = y * cb_plane.v / frame.max_v * cb_plane.stride;
        let cr_row = y * cr_plane.v / frame.max_v * cr_plane.stride;
        let dst = &mut output[local_y * width..(local_y + 1) * width];

        let mut yy_x = 0usize;
        let mut cb_x = 0usize;
        let mut cr_x = 0usize;
        let mut yy_phase = 0usize;
        let mut cb_phase = 0usize;
        let mut cr_phase = 0usize;

        for pixel in dst {
            *pixel = ycbcr_to_rgba(
                y_plane.samples[yy_row + yy_x],
                cb_plane.samples[cb_row + cb_x],
                cr_plane.samples[cr_row + cr_x],
            );

            yy_phase += y_plane.h;
            if yy_phase >= frame.max_h {
                yy_phase -= frame.max_h;
                yy_x += 1;
            }
            cb_phase += cb_plane.h;
            if cb_phase >= frame.max_h {
                cb_phase -= frame.max_h;
                cb_x += 1;
            }
            cr_phase += cr_plane.h;
            if cr_phase >= frame.max_h {
                cr_phase -= frame.max_h;
                cr_x += 1;
            }
        }
    }
}
// ============================================================================
// Pixel packing / color conversion
// ============================================================================

/// Calculates the RGB additions contributed by one Cb/Cr sample pair.
///
/// Separating this from luminance conversion lets 4:2:2 and 4:2:0 paths reuse
/// the relatively expensive fixed-point multiplications for adjacent pixels.
#[inline(always)]
fn chroma_terms(cb: u8, cr: u8) -> (i32, i32, i32) {
    let cb = cb as i32 - 128;
    let cr = cr as i32 - 128;
    (
        (91881 * cr + 32768) >> 16,
        -((22554 * cb + 46802 * cr + 32768) >> 16),
        (116130 * cb + 32768) >> 16,
    )
}

/// Applies precomputed chroma contributions to one luminance sample.
#[inline(always)]
fn y_with_chroma_terms(y: u8, terms: (i32, i32, i32)) -> u32 {
    let yy = y as i32;
    pack_rgba(
        clamp_i32_u8(yy + terms.0),
        clamp_i32_u8(yy + terms.1),
        clamp_i32_u8(yy + terms.2),
        255,
    )
}

/// Converts one JPEG YCbCr sample triplet using 16.16 fixed-point coefficients.
#[inline(always)]
fn ycbcr_to_rgba(y: u8, cb: u8, cr: u8) -> u32 {
    y_with_chroma_terms(y, chroma_terms(cb, cr))
}

/// Packs channels as 0xRRGGBBAA.

/// Saturates an i32 to u8.
#[inline(always)]
fn clamp_i32_u8(value: i32) -> u8 {
    if (value as u32) <= 255 {
        value as u8
    } else if value < 0 {
        0
    } else {
        255
    }
}

/// Saturates an i64 to u8.
#[inline(always)]
fn clamp_i64_u8(value: i64) -> u8 {
    if (value as u64) <= 255 {
        value as u8
    } else if value < 0 {
        0
    } else {
        255
    }
}

