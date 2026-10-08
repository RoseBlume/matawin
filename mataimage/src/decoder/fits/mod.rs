//! FITS image decoding implementation.
//!
//! This module parses FITS input, validates format-specific structures, and converts
//! decoded image content into the crate's packed pixel/frame representation.

use crate::pixels::pack_rgba;
use std::convert::TryFrom;
use std::io::{Cursor, Read, Seek};

use crate::ImageError;

// ============================================================================
// FITS constants
// ============================================================================

const FITS_BLOCK_SIZE: usize = 2880;
const FITS_CARD_SIZE: usize = 80;

// ============================================================================
// Public FITS type
// ============================================================================

/// Decoder for FITS (Flexible Image Transport System) image arrays.
///
/// The FITS format stores numeric array samples rather than RGBA pixels. This
/// decoder converts the selected image HDU to grayscale 0xRRGGBBAA pixels.
/// Undefined integer samples (`BLANK`) and non-finite floating-point samples
/// are emitted as transparent black (`0x00000000`).
///
/// Supported BITPIX values:
///
/// - 8   : unsigned 8-bit integer
/// - 16  : signed 16-bit integer
/// - 32  : signed 32-bit integer
/// - 64  : signed 64-bit integer
/// - -32 : IEEE-754 f32
/// - -64 : IEEE-754 f64
///
/// Supported layouts:
///
/// - Primary image HDUs
/// - IMAGE extensions
/// - NAXIS >= 2
/// - NAXIS1 = width
/// - NAXIS2 = height
/// - NAXIS3..NAXISn are returned as additional frames
/// - BSCALE / BZERO linear scaling
/// - BLANK for integer arrays
///
/// Not supported:
///
/// - Random-groups FITS
/// - Tile-compressed images stored in BINTABLE extensions
#[derive(Debug, Clone, PartialEq)]
pub struct Fits {
    /// Stores the image, frame, or plane width in pixels.
    pub width: u32,
    /// Stores the image, frame, or plane height in pixels.
    pub height: u32,

    /// FITS BITPIX value for the decoded image HDU.
    pub bitpix: i32,

    /// FITS axis lengths. NAXIS1 is axes[0], NAXIS2 is axes[1], etc.
    pub axes: Vec<u64>,

    /// Physical value = BZERO + BSCALE * stored value.
    pub bscale: f64,
    /// Stores the FITS `BZERO` offset applied after sample scaling.
    pub bzero: f64,

    /// Integer undefined-value marker from the FITS header.
    pub blank: Option<i64>,

    /// Minimum finite, defined physical value found in the decoded image.
    pub min_value: f64,

    /// Maximum finite, defined physical value found in the decoded image.
    pub max_value: f64,

    /// Original encoded FITS file.
    pub data: Vec<u8>,

    /// Decoded frames in 0xRRGGBBAA format.
    pixels: Vec<Vec<u32>>,
}

// ============================================================================
// Internal HDU representation
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
/// Identifies the kind of FITS Header/Data Unit being processed.
enum HduKind {
    /// Identifies the primary FITS Header/Data Unit at the beginning of a FITS file.
    Primary,
    /// Identifies a FITS image-extension HDU whose payload contains image samples.
    ImageExtension,
    /// Identifies another FITS extension type and stores its extension-name string.
    OtherExtension(String),
}

#[derive(Debug, Clone)]
/// Stores parsed metadata for one FITS Header/Data Unit (HDU).
struct HduHeader {
    /// Identifies whether this FITS HDU is the primary array, an image extension, or another extension type.
    kind: HduKind,
    /// Stores the byte offset immediately after the padded FITS HDU header.
    header_end: usize,

    /// Stores the HDU `BITPIX` value describing the numeric sample representation.
    bitpix: i32,
    /// Stores the axis lengths declared for this FITS HDU.
    axes: Vec<u64>,

    /// Stores the HDU `BSCALE` sample multiplier.
    bscale: f64,
    /// Stores the HDU `BZERO` sample offset.
    bzero: f64,
    /// Stores the optional HDU `BLANK` integer sentinel.
    blank: Option<i64>,

    /// Stores the FITS `PCOUNT` parameter count associated with this HDU.
    pcount: u64,
    /// Stores the FITS `GCOUNT` group count associated with this HDU.
    gcount: u64,

    /// Records whether the FITS random-groups convention is enabled for this HDU.
    groups: bool,
    /// Records whether this HDU declares a tile-compressed image through `ZIMAGE`.
    zimage: bool,
}

#[derive(Debug)]
/// Contains the rasterized result of decoding FITS image data.
struct DecodedFits {
    /// Stores the image, frame, or plane width in pixels.
    width: u32,
    /// Stores the image, frame, or plane height in pixels.
    height: u32,
    /// Stores the `BITPIX` value used to interpret the decoded FITS samples.
    bitpix: i32,
    /// Stores the decoded FITS axis lengths.
    axes: Vec<u64>,
    /// Stores the scaling multiplier applied to decoded FITS samples.
    bscale: f64,
    /// Stores the zero-point offset applied to decoded FITS samples.
    bzero: f64,
    /// Stores the optional raw integer value representing an undefined FITS sample.
    blank: Option<i64>,
    /// Stores the minimum decoded sample used for display normalization.
    min_value: f64,
    /// Stores the maximum decoded sample used for display normalization.
    max_value: f64,
    /// Stores the decoded packed pixel values produced by the decoder.
    pixels: Vec<Vec<u32>>,
}

// ============================================================================
// Public API
// ============================================================================

impl Fits {
    /// Constructs a rich FITS decoder from borrowed input.
    ///
    /// The rich object retains an owned copy of the encoded FITS source. Use
    /// [`crate::Image::from_bytes`] when that source/metadata is unnecessary.
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        let mut reader = Cursor::new(bytes);
        Self::parse(&mut reader)
    }

    /// Parses a FITS file from owned bytes without copying the encoded source.
    ///
    /// The rich object intentionally retains the source in [`Self::data`]. Use
    /// [`crate::Image::from_bytes`] when source retention is unnecessary.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, ImageError> {
        let decoded = decode_fits(&data)?;
        Ok(Self::from_decoded(decoded, data))
    }

    /// Explicit owned-source constructor for callers that want rich FITS metadata
    /// and source retention.
    #[inline]
    pub fn parse_owned(data: Vec<u8>) -> Result<Self, ImageError> {
        Self::from_bytes(data)
    }

    /// Parse and decode the first decodable FITS image HDU.
    ///
    /// If the primary HDU contains no image, IMAGE extensions are searched.
    pub fn parse<R: Read + Seek>(reader: &mut R) -> Result<Self, ImageError> {
        let mut data = Vec::new();

        reader
            .read_to_end(&mut data)
            .map_err(|e| ImageError::new(format!("Failed to read FITS: {e}")))?;

        let decoded = decode_fits(&data)?;
        Ok(Self::from_decoded(decoded, data))
    }

    #[inline]
    fn from_decoded(decoded: DecodedFits, data: Vec<u8>) -> Self {
        Self {
            width: decoded.width,
            height: decoded.height,
            bitpix: decoded.bitpix,
            axes: decoded.axes,
            bscale: decoded.bscale,
            bzero: decoded.bzero,
            blank: decoded.blank,
            min_value: decoded.min_value,
            max_value: decoded.max_value,
            pixels: decoded.pixels,
            data,
        }
    }

    /// Borrows all decoded FITS image planes without cloning them.
    #[inline]
    pub fn frames_ref(&self) -> &[Vec<u32>] {
        &self.pixels
    }

    /// Borrows the first decoded FITS image plane.
    #[inline]
    pub fn pixels_ref(&self) -> &[u32] {
        self.pixels.first().map(Vec::as_slice).unwrap_or(&[])
    }

    /// Moves all decoded FITS planes out without cloning pixel buffers.
    #[inline]
    pub fn into_frames(self) -> Vec<Vec<u32>> {
        self.pixels
    }

    /// Moves the first decoded FITS plane out.
    #[inline]
    pub fn into_pixels(self) -> Vec<u32> {
        self.pixels.into_iter().next().unwrap_or_default()
    }

    /// Borrows the retained encoded FITS source.
    #[inline]
    pub fn encoded_bytes(&self) -> &[u8] {
        &self.data
    }
}

/// Decodes FITS directly from borrowed input without retaining the encoded file.
pub(crate) fn decode_image(
    bytes: &[u8],
    options: &crate::DecodeOptions,
) -> Result<crate::Image, ImageError> {
    let decoded = decode_fits(bytes)?;
    options.checked_rgba_bytes(decoded.width, decoded.height)?;
    let pixels = decoded
        .pixels
        .into_iter()
        .next()
        .ok_or_else(|| ImageError::new("FITS decoder produced no image plane"))?;
    crate::Image::from_packed_pixels(decoded.width, decoded.height, pixels, options)
}


/// Header-only information for one FITS Header/Data Unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FitsHduInfo {
    /// Zero-based HDU index in the file.
    pub index: usize,
    /// Whether this HDU is a primary/image-extension array that can be decoded.
    pub is_image: bool,
    /// Width for image HDUs with at least two axes.
    pub width: Option<u32>,
    /// Height for image HDUs with at least two axes.
    pub height: Option<u32>,
    /// Number of 2D planes represented by axes 3..N.
    pub plane_count: Option<usize>,
    /// FITS BITPIX value.
    pub bitpix: i32,
}

#[derive(Debug, Clone)]
struct FitsHduEntry {
    index: usize,
    header: HduHeader,
    data_start: usize,
    data_end: usize,
}

/// Lazy FITS HDU/plane reader.
///
/// Construction parses headers and byte ranges only. Numeric image samples are
/// converted to pixels only for the HDU/plane explicitly requested by a caller.
pub struct FitsReader<'a> {
    data: &'a [u8],
    hdus: Vec<FitsHduEntry>,
}

impl<'a> FitsReader<'a> {
    /// Indexes all FITS HDUs without decoding any image plane.
    pub fn new(data: &'a [u8]) -> Result<Self, ImageError> {
        if data.len() < FITS_BLOCK_SIZE {
            return Err(ImageError::new("FITS file is smaller than one 2880-byte block"));
        }
        let mut hdus = Vec::new();
        let mut offset = 0usize;
        let mut index = 0usize;
        while offset < data.len() {
            let header = parse_hdu_header(data, offset, index)?;
            let data_start = header.header_end;
            let data_len = hdu_data_len(&header)?;
            let data_end = data_start.checked_add(data_len)
                .ok_or_else(|| ImageError::new("FITS HDU data size overflow"))?;
            if data_end > data.len() {
                return Err(ImageError::new("FITS HDU data extends past end of file"));
            }
            hdus.push(FitsHduEntry { index, header, data_start, data_end });
            let padded = round_up_block(data_len)?;
            offset = data_start.checked_add(padded)
                .ok_or_else(|| ImageError::new("FITS HDU offset overflow"))?;
            if offset == data.len() { break; }
            if offset > data.len() { return Err(ImageError::new("FITS HDU padding extends past end of file")); }
            index += 1;
        }
        Ok(Self { data, hdus })
    }

    /// Number of HDUs in the indexed file.
    #[inline]
    pub fn len(&self) -> usize { self.hdus.len() }

    /// Returns true when no HDU is indexed.
    #[inline]
    pub fn is_empty(&self) -> bool { self.hdus.is_empty() }

    /// Returns cheap information for one HDU without decoding it.
    pub fn hdu_info(&self, index: usize) -> Option<FitsHduInfo> {
        let entry = self.hdus.get(index)?;
        let is_image = matches!(entry.header.kind, HduKind::Primary | HduKind::ImageExtension)
            && entry.header.axes.len() >= 2
            && entry.header.axes[0] != 0
            && entry.header.axes[1] != 0;
        let width = is_image.then(|| u32::try_from(entry.header.axes[0]).ok()).flatten();
        let height = is_image.then(|| u32::try_from(entry.header.axes[1]).ok()).flatten();
        let plane_count = if is_image {
            let plane = entry.header.axes[0].checked_mul(entry.header.axes[1]);
            plane.and_then(|p| axis_product(&entry.header.axes).ok().and_then(|all| usize::try_from(all / p).ok()))
        } else { None };
        Some(FitsHduInfo {
            index: entry.index,
            is_image,
            width,
            height,
            plane_count,
            bitpix: entry.header.bitpix,
        })
    }

    /// Iterates HDU information without decoding pixels.
    pub fn hdus(&self) -> impl Iterator<Item = FitsHduInfo> + '_ {
        (0..self.hdus.len()).filter_map(|index| self.hdu_info(index))
    }

    /// Decodes one selected 2D plane without materializing sibling planes.
    pub fn decode_plane(&self, hdu_index: usize, plane_index: usize) -> Result<crate::Image, ImageError> {
        let entry = self.hdus.get(hdu_index)
            .ok_or_else(|| ImageError::new("FITS HDU index is out of range"))?;
        decode_image_hdu_plane(
            self.data,
            entry.data_start,
            entry.data_end,
            entry.header.clone(),
            plane_index,
        )
    }

    /// Decodes all planes in one selected image HDU. Other HDUs remain untouched.
    pub fn decode_hdu(&self, hdu_index: usize) -> Result<crate::Image, ImageError> {
        let entry = self.hdus.get(hdu_index)
            .ok_or_else(|| ImageError::new("FITS HDU index is out of range"))?;
        let decoded = decode_image_hdu(self.data, entry.data_start, entry.data_end, entry.header.clone())?;
        let mut options = crate::DecodeOptions::default();
        options.animation = crate::AnimationMode::AllFrames;
        crate::Image::from_frame_pixels(decoded.width, decoded.height, decoded.pixels, &options)
    }
}

// ============================================================================
// Decoder implementation
// ============================================================================

impl super::Decoder for Fits {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn into_image(self: Box<Self>) -> Result<crate::Image, ImageError> {
        let this = *self;
        let pixels = this
            .pixels
            .into_iter()
            .next()
            .ok_or_else(|| ImageError::new("FITS decoder produced no image plane"))?;
        crate::Image::from_packed_pixels(
            this.width,
            this.height,
            pixels,
            &crate::DecodeOptions::default(),
        )
    }
}

// ============================================================================
// FITS decoding
// ============================================================================

/// Decodes fits data into the representation required by the surrounding decoder.
///
/// Returns an error when the supplied data cannot be processed as required.
fn decode_fits(data: &[u8]) -> Result<DecodedFits, ImageError> {
    if data.len() < FITS_BLOCK_SIZE {
        return Err(ImageError::new(
            "FITS file is smaller than one 2880-byte block",
        ));
    }

    let mut offset = 0usize;
    let mut hdu_index = 0usize;

    while offset < data.len() {
        let header = parse_hdu_header(data, offset, hdu_index)?;
        let data_start = header.header_end;

        if header.groups {
            return Err(ImageError::new(
                "Random-groups FITS images are not supported",
            ));
        }

        if header.zimage {
            return Err(ImageError::new(
                "Tile-compressed FITS images in BINTABLE extensions are not supported",
            ));
        }

        let data_len = hdu_data_len(&header)?;

        let data_end = data_start
            .checked_add(data_len)
            .ok_or_else(|| ImageError::new("FITS HDU data size overflow"))?;

        if data_end > data.len() {
            return Err(ImageError::new(
                "FITS HDU data extends past end of file",
            ));
        }

        let is_image_hdu = matches!(
            header.kind,
            HduKind::Primary | HduKind::ImageExtension
        );

        if is_image_hdu
            && header.axes.len() >= 2
            && header.axes[0] != 0
            && header.axes[1] != 0
        {
            return decode_image_hdu(data, data_start, data_end, header);
        }

        let padded_data_len = round_up_block(data_len)?;
        offset = data_start
            .checked_add(padded_data_len)
            .ok_or_else(|| ImageError::new("FITS HDU offset overflow"))?;

        if offset == data.len() {
            break;
        }

        if offset > data.len() {
            return Err(ImageError::new(
                "FITS HDU padding extends past end of file",
            ));
        }

        hdu_index += 1;
    }

    Err(ImageError::new(
        "FITS file contains no supported image HDU",
    ))
}

/// Decodes image hdu data into the representation required by the surrounding decoder.
///
/// Returns an error when the supplied data cannot be processed as required.
fn decode_image_hdu(
    data: &[u8],
    data_start: usize,
    data_end: usize,
    header: HduHeader,
) -> Result<DecodedFits, ImageError> {
    if header.kind == HduKind::ImageExtension
        && (header.pcount != 0 || header.gcount != 1)
    {
        return Err(ImageError::new(
            "Unsupported FITS IMAGE extension PCOUNT/GCOUNT values",
        ));
    }

    let width_u64 = header.axes[0];
    let height_u64 = header.axes[1];

    let width = u32::try_from(width_u64)
        .map_err(|_| ImageError::new("FITS width exceeds u32::MAX"))?;

    let height = u32::try_from(height_u64)
        .map_err(|_| ImageError::new("FITS height exceeds u32::MAX"))?;

    let plane_pixels = width_u64
        .checked_mul(height_u64)
        .ok_or_else(|| ImageError::new("FITS image dimensions overflow"))?;

    let sample_count = axis_product(&header.axes)?;

    if sample_count == 0 {
        return Err(ImageError::new("FITS image contains no samples"));
    }

    if sample_count % plane_pixels != 0 {
        return Err(ImageError::new(
            "FITS image axis dimensions do not form complete frames",
        ));
    }

    let frame_count_u64 = sample_count / plane_pixels;
    let frame_count = usize::try_from(frame_count_u64)
        .map_err(|_| ImageError::new("FITS frame count exceeds usize::MAX"))?;

    let plane_pixels_usize = usize::try_from(plane_pixels)
        .map_err(|_| ImageError::new("FITS frame size exceeds usize::MAX"))?;

    let bytes_per_sample = bitpix_bytes(header.bitpix)?;
    let expected_data_len_u64 = sample_count
        .checked_mul(bytes_per_sample as u64)
        .ok_or_else(|| ImageError::new("FITS image byte count overflow"))?;

    let expected_data_len = usize::try_from(expected_data_len_u64)
        .map_err(|_| ImageError::new("FITS image byte count exceeds usize::MAX"))?;

    if data_end - data_start < expected_data_len {
        return Err(ImageError::new("Truncated FITS image data"));
    }

    let image_data = &data[data_start..data_start + expected_data_len];

    // First pass: determine a displayable global physical-value range.
    // FITS itself does not prescribe an 8-bit display transfer function.
    let mut min_value = f64::INFINITY;
    let mut max_value = f64::NEG_INFINITY;

    for index in 0..sample_count {
        if let Some(value) = read_physical_sample(
            image_data,
            index,
            header.bitpix,
            header.bscale,
            header.bzero,
            header.blank,
        )? {
            if value.is_finite() {
                min_value = min_value.min(value);
                max_value = max_value.max(value);
            }
        }
    }

    let has_valid_samples = min_value.is_finite() && max_value.is_finite();

    if !has_valid_samples {
        min_value = 0.0;
        max_value = 0.0;
    }

    let mut pixels = Vec::with_capacity(frame_count);

    for frame_index in 0..frame_count {
        let mut frame = Vec::with_capacity(plane_pixels_usize);
        let frame_sample_start = (frame_index as u64)
            .checked_mul(plane_pixels)
            .ok_or_else(|| ImageError::new("FITS frame sample offset overflow"))?;

        for pixel_index in 0..plane_pixels {
            let sample_index = frame_sample_start
                .checked_add(pixel_index)
                .ok_or_else(|| ImageError::new("FITS sample index overflow"))?;

            let value = read_physical_sample(
                image_data,
                sample_index,
                header.bitpix,
                header.bscale,
                header.bzero,
                header.blank,
            )?;

            match value {
                Some(value) if value.is_finite() => {
                    let gray = normalize_to_u8(value, min_value, max_value);
                    frame.push(pack_rgba(gray, gray, gray, 255));
                }

                _ => {
                    frame.push(0x00000000);
                }
            }
        }

        pixels.push(frame);
    }

    Ok(DecodedFits {
        width,
        height,
        bitpix: header.bitpix,
        axes: header.axes,
        bscale: header.bscale,
        bzero: header.bzero,
        blank: header.blank,
        min_value,
        max_value,
        pixels,
    })
}


/// Decodes exactly one plane from an image HDU while preserving the same global
/// normalization range used by the full-HDU decoder.
fn decode_image_hdu_plane(
    data: &[u8],
    data_start: usize,
    data_end: usize,
    header: HduHeader,
    plane_index: usize,
) -> Result<crate::Image, ImageError> {
    if header.groups || header.zimage {
        return Err(ImageError::unsupported("Unsupported FITS grouped/tile-compressed image"));
    }
    if header.kind == HduKind::ImageExtension && (header.pcount != 0 || header.gcount != 1) {
        return Err(ImageError::unsupported("Unsupported FITS IMAGE extension PCOUNT/GCOUNT values"));
    }
    if header.axes.len() < 2 || header.axes[0] == 0 || header.axes[1] == 0 {
        return Err(ImageError::new("Selected FITS HDU is not a 2D image array"));
    }
    let width_u64 = header.axes[0];
    let height_u64 = header.axes[1];
    let width = u32::try_from(width_u64).map_err(|_| ImageError::new("FITS width exceeds u32::MAX"))?;
    let height = u32::try_from(height_u64).map_err(|_| ImageError::new("FITS height exceeds u32::MAX"))?;
    let plane_pixels = width_u64.checked_mul(height_u64).ok_or_else(|| ImageError::new("FITS image dimensions overflow"))?;
    let sample_count = axis_product(&header.axes)?;
    if plane_pixels == 0 || sample_count % plane_pixels != 0 {
        return Err(ImageError::new("FITS image axis dimensions do not form complete frames"));
    }
    let plane_count = usize::try_from(sample_count / plane_pixels)
        .map_err(|_| ImageError::new("FITS frame count exceeds usize::MAX"))?;
    if plane_index >= plane_count { return Err(ImageError::new("FITS plane index is out of range")); }
    let bytes_per_sample = bitpix_bytes(header.bitpix)?;
    let expected_data_len = usize::try_from(sample_count.checked_mul(bytes_per_sample as u64)
        .ok_or_else(|| ImageError::new("FITS image byte count overflow"))?)
        .map_err(|_| ImageError::new("FITS image byte count exceeds usize::MAX"))?;
    if data_end - data_start < expected_data_len { return Err(ImageError::new("Truncated FITS image data")); }
    let image_data = &data[data_start..data_start + expected_data_len];

    let mut min_value = f64::INFINITY;
    let mut max_value = f64::NEG_INFINITY;
    for index in 0..sample_count {
        if let Some(value) = read_physical_sample(image_data, index, header.bitpix, header.bscale, header.bzero, header.blank)? {
            if value.is_finite() { min_value = min_value.min(value); max_value = max_value.max(value); }
        }
    }
    if !min_value.is_finite() || !max_value.is_finite() { min_value = 0.0; max_value = 0.0; }

    let count = usize::try_from(plane_pixels).map_err(|_| ImageError::new("FITS frame size exceeds usize::MAX"))?;
    let mut pixels = Vec::with_capacity(count);
    let start = (plane_index as u64).checked_mul(plane_pixels).ok_or_else(|| ImageError::new("FITS frame sample offset overflow"))?;
    for pixel_index in 0..plane_pixels {
        let sample = read_physical_sample(
            image_data,
            start + pixel_index,
            header.bitpix,
            header.bscale,
            header.bzero,
            header.blank,
        )?;
        match sample {
            Some(value) if value.is_finite() => {
                let gray = normalize_to_u8(value, min_value, max_value);
                pixels.push(pack_rgba(gray, gray, gray, 255));
            }
            _ => pixels.push(0),
        }
    }
    crate::Image::from_packed_pixels(width, height, pixels, &crate::DecodeOptions::default())
}

// ============================================================================
// HDU/header parsing
// ============================================================================

/// Parses hdu header from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_hdu_header(
    data: &[u8],
    start: usize,
    hdu_index: usize,
) -> Result<HduHeader, ImageError> {
    if start % FITS_BLOCK_SIZE != 0 {
        return Err(ImageError::new(
            "FITS HDU does not begin on a 2880-byte boundary",
        ));
    }

    if start
        .checked_add(FITS_CARD_SIZE)
        .map_or(true, |end| end > data.len())
    {
        return Err(ImageError::new("Truncated FITS header"));
    }

    let mut pos = start;
    let mut first_keyword: Option<String> = None;

    let mut simple: Option<bool> = None;
    let mut xtension: Option<String> = None;

    let mut bitpix: Option<i32> = None;
    let mut naxis: Option<usize> = None;
    let mut axes: Vec<Option<u64>> = Vec::new();

    let mut bscale = 1.0f64;
    let mut bzero = 0.0f64;
    let mut blank: Option<i64> = None;

    let mut pcount = 0u64;
    let mut gcount = 1u64;
    let mut groups = false;
    let mut zimage = false;

    let header_end = loop {
        let card_end = pos
            .checked_add(FITS_CARD_SIZE)
            .ok_or_else(|| ImageError::new("FITS header offset overflow"))?;

        if card_end > data.len() {
            return Err(ImageError::new(
                "FITS header is missing END or is truncated",
            ));
        }

        let card = &data[pos..card_end];
        let keyword = parse_keyword(card)?;

        if first_keyword.is_none() {
            first_keyword = Some(keyword.to_owned());
        }

        if keyword == "END" {
            break round_up_block(card_end)?;
        }

        if !keyword.is_empty() {
            match keyword {
                "SIMPLE" => {
                    simple = Some(parse_card_bool(card, "SIMPLE")?);
                }

                "XTENSION" => {
                    xtension = Some(parse_card_string(card, "XTENSION")?);
                }

                "BITPIX" => {
                    bitpix = Some(parse_card_i32(card, "BITPIX")?);
                }

                "NAXIS" => {
                    let value = parse_card_i64(card, "NAXIS")?;
                    if value < 0 {
                        return Err(ImageError::new("FITS NAXIS cannot be negative"));
                    }

                    let value = usize::try_from(value)
                        .map_err(|_| ImageError::new("FITS NAXIS is too large"))?;

                    naxis = Some(value);
                    axes.resize(value, None);
                }

                "BSCALE" => {
                    bscale = parse_card_f64(card, "BSCALE")?;
                }

                "BZERO" => {
                    bzero = parse_card_f64(card, "BZERO")?;
                }

                "BLANK" => {
                    blank = Some(parse_card_i64(card, "BLANK")?);
                }

                "PCOUNT" => {
                    pcount = parse_nonnegative_u64(card, "PCOUNT")?;
                }

                "GCOUNT" => {
                    gcount = parse_nonnegative_u64(card, "GCOUNT")?;
                }

                "GROUPS" => {
                    groups = parse_card_bool(card, "GROUPS")?;
                }

                "ZIMAGE" => {
                    zimage = parse_card_bool(card, "ZIMAGE")?;
                }

                _ => {
                    if let Some(axis_number) = parse_axis_keyword(keyword) {
                        let value = parse_nonnegative_u64(card, keyword)?;

                        if axis_number == 0 {
                            return Err(ImageError::new("Invalid FITS NAXISn keyword"));
                        }

                        if let Some(naxis) = naxis {
                            if axis_number > naxis {
                                return Err(ImageError::new(format!(
                                    "FITS {keyword} exceeds NAXIS={naxis}",
                                )));
                            }

                            axes[axis_number - 1] = Some(value);
                        }
                    }
                }
            }
        }

        pos = card_end;
    };

    if header_end > data.len() {
        return Err(ImageError::new("Truncated FITS header block"));
    }

    let kind = if hdu_index == 0 {
        if first_keyword.as_deref() != Some("SIMPLE") {
            return Err(ImageError::new(
                "FITS primary HDU must begin with SIMPLE",
            ));
        }

        if simple != Some(true) {
            return Err(ImageError::new(
                "FITS SIMPLE keyword is missing or is not T",
            ));
        }

        HduKind::Primary
    } else {
        if first_keyword.as_deref() != Some("XTENSION") {
            return Err(ImageError::new(
                "FITS extension HDU must begin with XTENSION",
            ));
        }

        let extension = xtension
            .ok_or_else(|| ImageError::new("Missing FITS XTENSION value"))?;

        if extension.trim_end() == "IMAGE" {
            HduKind::ImageExtension
        } else {
            HduKind::OtherExtension(extension)
        }
    };

    let bitpix = bitpix.ok_or_else(|| ImageError::new("FITS header is missing BITPIX"))?;
    bitpix_bytes(bitpix)?;

    let naxis = naxis.ok_or_else(|| ImageError::new("FITS header is missing NAXIS"))?;

    if axes.len() != naxis {
        return Err(ImageError::new("Invalid FITS axis list"));
    }

    let mut resolved_axes = Vec::with_capacity(naxis);

    for (index, axis) in axes.into_iter().enumerate() {
        resolved_axes.push(axis.ok_or_else(|| {
            ImageError::new(format!("FITS header is missing NAXIS{}", index + 1))
        })?);
    }

    if !bscale.is_finite() || !bzero.is_finite() {
        return Err(ImageError::new(
            "FITS BSCALE/BZERO must be finite",
        ));
    }

    if gcount == 0 {
        return Err(ImageError::new("FITS GCOUNT cannot be zero"));
    }

    Ok(HduHeader {
        kind,
        header_end,
        bitpix,
        axes: resolved_axes,
        bscale,
        bzero,
        blank,
        pcount,
        gcount,
        groups,
        zimage,
    })
}

// ============================================================================
// Header card helpers
// ============================================================================

/// Parses keyword from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_keyword(card: &[u8]) -> Result<&str, ImageError> {
    let keyword = std::str::from_utf8(&card[..8])
        .map_err(|_| ImageError::new("FITS header contains non-ASCII keyword bytes"))?;

    Ok(keyword.trim_end())
}

/// Extracts the value text portion of a FITS header card.
///
/// Returns an error when the supplied data cannot be processed as required.
fn card_value_text<'a>(card: &'a [u8], keyword: &str) -> Result<&'a str, ImageError> {
    if card.len() != FITS_CARD_SIZE {
        return Err(ImageError::new("Invalid FITS header card length"));
    }

    if card[8] != b'=' {
        return Err(ImageError::new(format!(
            "FITS {keyword} card is missing '='",
        )));
    }

    let field = std::str::from_utf8(&card[10..])
        .map_err(|_| ImageError::new("FITS header contains invalid ASCII text"))?;

    let bytes = field.as_bytes();
    let mut in_string = false;
    let mut end = bytes.len();
    let mut i = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                if in_string && i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                    i += 2;
                    continue;
                }

                in_string = !in_string;
            }

            b'/' if !in_string => {
                end = i;
                break;
            }

            _ => {}
        }

        i += 1;
    }

    Ok(field[..end].trim())
}

/// Parses card bool from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_card_bool(card: &[u8], keyword: &str) -> Result<bool, ImageError> {
    match card_value_text(card, keyword)? {
        "T" => Ok(true),
        "F" => Ok(false),
        _ => Err(ImageError::new(format!(
            "Invalid FITS logical value for {keyword}",
        ))),
    }
}

/// Parses card string from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_card_string(card: &[u8], keyword: &str) -> Result<String, ImageError> {
    let text = card_value_text(card, keyword)?;

    if !text.starts_with('\'') {
        return Err(ImageError::new(format!(
            "Invalid FITS string value for {keyword}",
        )));
    }

    let mut chars = text[1..].chars().peekable();
    let mut output = String::new();

    while let Some(ch) = chars.next() {
        if ch == '\'' {
            if chars.peek().copied() == Some('\'') {
                chars.next();
                output.push('\'');
                continue;
            }

            return Ok(output.trim_end().to_owned());
        }

        output.push(ch);
    }

    Err(ImageError::new(format!(
        "Unterminated FITS string value for {keyword}",
    )))
}

/// Parses card i32 from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_card_i32(card: &[u8], keyword: &str) -> Result<i32, ImageError> {
    let value = parse_card_i64(card, keyword)?;
    i32::try_from(value)
        .map_err(|_| ImageError::new(format!("FITS {keyword} is outside i32 range")))
}

/// Parses card i64 from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_card_i64(card: &[u8], keyword: &str) -> Result<i64, ImageError> {
    card_value_text(card, keyword)?
        .parse::<i64>()
        .map_err(|_| ImageError::new(format!("Invalid FITS integer value for {keyword}")))
}

/// Parses nonnegative u64 from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_nonnegative_u64(card: &[u8], keyword: &str) -> Result<u64, ImageError> {
    let value = parse_card_i64(card, keyword)?;

    if value < 0 {
        return Err(ImageError::new(format!(
            "FITS {keyword} cannot be negative",
        )));
    }

    Ok(value as u64)
}

/// Parses card f64 from the supplied encoded or textual input.
///
/// Returns an error when the supplied data cannot be processed as required.
fn parse_card_f64(card: &[u8], keyword: &str) -> Result<f64, ImageError> {
    let text = card_value_text(card, keyword)?;

    // FITS permits D exponents in floating-point header values.
    let normalized = text.replace('D', "E").replace('d', "E");

    normalized
        .parse::<f64>()
        .map_err(|_| ImageError::new(format!("Invalid FITS floating value for {keyword}")))
}

/// Parses axis keyword from the supplied encoded or textual input.
fn parse_axis_keyword(keyword: &str) -> Option<usize> {
    let suffix = keyword.strip_prefix("NAXIS")?;

    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    suffix.parse::<usize>().ok()
}

// ============================================================================
// HDU size helpers
// ============================================================================

/// Computes the encoded data length of a FITS Header/Data Unit from its parsed header values.
///
/// Returns an error when the supplied data cannot be processed as required.
fn hdu_data_len(header: &HduHeader) -> Result<usize, ImageError> {
    let bytes_per_sample = bitpix_bytes(header.bitpix)? as u64;

    let element_count = if header.axes.is_empty() {
        0u64
    } else {
        axis_product(&header.axes)?
    };

    let values_per_group = header
        .pcount
        .checked_add(element_count)
        .ok_or_else(|| ImageError::new("FITS PCOUNT/data size overflow"))?;

    let total_values = header
        .gcount
        .checked_mul(values_per_group)
        .ok_or_else(|| ImageError::new("FITS GCOUNT/data size overflow"))?;

    let bytes = total_values
        .checked_mul(bytes_per_sample)
        .ok_or_else(|| ImageError::new("FITS HDU byte count overflow"))?;

    usize::try_from(bytes)
        .map_err(|_| ImageError::new("FITS HDU byte count exceeds usize::MAX"))
}

/// Computes the product of FITS axis lengths while checking for integer overflow.
///
/// Returns an error when the supplied data cannot be processed as required.
fn axis_product(axes: &[u64]) -> Result<u64, ImageError> {
    let mut product = 1u64;

    for &axis in axes {
        product = product
            .checked_mul(axis)
            .ok_or_else(|| ImageError::new("FITS axis product overflow"))?;
    }

    Ok(product)
}

/// Returns the number of bytes occupied by one FITS sample for the supplied `BITPIX` value.
///
/// Returns an error when the supplied data cannot be processed as required.
fn bitpix_bytes(bitpix: i32) -> Result<usize, ImageError> {
    match bitpix {
        8 => Ok(1),
        16 => Ok(2),
        32 | -32 => Ok(4),
        64 | -64 => Ok(8),
        _ => Err(ImageError::new(format!(
            "Unsupported FITS BITPIX value {bitpix}",
        ))),
    }
}

/// Rounds a byte length up to the next FITS block boundary.
///
/// Returns an error when the supplied data cannot be processed as required.
fn round_up_block(value: usize) -> Result<usize, ImageError> {
    if value == 0 {
        return Ok(0);
    }

    let remainder = value % FITS_BLOCK_SIZE;

    if remainder == 0 {
        return Ok(value);
    }

    value
        .checked_add(FITS_BLOCK_SIZE - remainder)
        .ok_or_else(|| ImageError::new("FITS block alignment overflow"))
}

// ============================================================================
// Sample decoding
// ============================================================================

/// Returns None for undefined samples.
fn read_physical_sample(
    data: &[u8],
    sample_index: u64,
    bitpix: i32,
    bscale: f64,
    bzero: f64,
    blank: Option<i64>,
) -> Result<Option<f64>, ImageError> {
    let bytes_per_sample = bitpix_bytes(bitpix)?;

    let byte_index_u64 = sample_index
        .checked_mul(bytes_per_sample as u64)
        .ok_or_else(|| ImageError::new("FITS sample byte offset overflow"))?;

    let byte_index = usize::try_from(byte_index_u64)
        .map_err(|_| ImageError::new("FITS sample byte offset exceeds usize::MAX"))?;

    let end = byte_index
        .checked_add(bytes_per_sample)
        .ok_or_else(|| ImageError::new("FITS sample byte range overflow"))?;

    let sample = data
        .get(byte_index..end)
        .ok_or_else(|| ImageError::new("Truncated FITS sample data"))?;

    let raw_value = match bitpix {
        8 => {
            let raw = sample[0] as i64;

            if blank == Some(raw) {
                return Ok(None);
            }

            raw as f64
        }

        16 => {
            let raw = i16::from_be_bytes([sample[0], sample[1]]) as i64;

            if blank == Some(raw) {
                return Ok(None);
            }

            raw as f64
        }

        32 => {
            let raw = i32::from_be_bytes([
                sample[0], sample[1], sample[2], sample[3],
            ]) as i64;

            if blank == Some(raw) {
                return Ok(None);
            }

            raw as f64
        }

        64 => {
            let raw = i64::from_be_bytes([
                sample[0], sample[1], sample[2], sample[3],
                sample[4], sample[5], sample[6], sample[7],
            ]);

            if blank == Some(raw) {
                return Ok(None);
            }

            raw as f64
        }

        -32 => {
            let raw = f32::from_bits(u32::from_be_bytes([
                sample[0], sample[1], sample[2], sample[3],
            ])) as f64;

            if !raw.is_finite() {
                return Ok(None);
            }

            raw
        }

        -64 => {
            let raw = f64::from_bits(u64::from_be_bytes([
                sample[0], sample[1], sample[2], sample[3],
                sample[4], sample[5], sample[6], sample[7],
            ]));

            if !raw.is_finite() {
                return Ok(None);
            }

            raw
        }

        _ => {
            return Err(ImageError::new(format!(
                "Unsupported FITS BITPIX value {bitpix}",
            )));
        }
    };

    let physical = bzero + bscale * raw_value;

    if physical.is_finite() {
        Ok(Some(physical))
    } else {
        Ok(None)
    }
}

// ============================================================================
// Display conversion
// ============================================================================

#[inline]
/// Normalizes a numeric FITS sample into an 8-bit display intensity.
fn normalize_to_u8(value: f64, min_value: f64, max_value: f64) -> u8 {
    if max_value <= min_value {
        // Constant images are made visible instead of becoming solid black.
        return 128;
    }

    let normalized = (value - min_value) / (max_value - min_value);

    (normalized * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8
}

