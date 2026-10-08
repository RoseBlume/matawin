//! ICO image decoding implementation.
//!
//! This module parses ICO input, validates format-specific structures, and converts
//! decoded image content into the crate's packed pixel/frame representation.

use crate::decoder::dib::{mask_is_contiguous, row_stride, ChannelMask};
use crate::decoder::byte_reader::{read_i32_le, read_u16_le, read_u32_le};
use crate::ImageError;

use super::png::Png;

// ============================================================================
// ICO constants
// ============================================================================

const ICO_TYPE_ICON: u16 = 1;

const BI_RGB: u32 = 0;
const BI_RLE8: u32 = 1;
const BI_RLE4: u32 = 2;
const BI_BITFIELDS: u32 = 3;
const BI_JPEG: u32 = 4;
const BI_PNG: u32 = 5;
const BI_ALPHABITFIELDS: u32 = 6;

const PNG_SIGNATURE: &[u8; 8] =
    b"\x89PNG\r\n\x1A\n";

// ============================================================================
// ICO
// ============================================================================

#[derive(Debug, Clone)]
/// Represents an ICO container and all icon frames decoded from it.
pub struct Ico {
    /// Width of the first image in the ICO.
    pub width: u32,

    /// Height of the first image in the ICO.
    pub height: u32,

    /// All images stored in the ICO.
    ///
    /// ICO files can contain several resolutions, for example:
    ///
    /// 16x16
    /// 32x32
    /// 48x48
    /// 256x256
    pub frames: Vec<IcoFrame>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Stores one decoded icon frame, including dimensions and packed RGBA pixels.
pub struct IcoFrame {
    /// Stores the image, frame, or plane width in pixels.
    pub width: u32,
    /// Stores the image, frame, or plane height in pixels.
    pub height: u32,

    /// Pixels stored as:
    ///
    /// 0xRRGGBBAA
    pub pixels: Vec<u32>,
}

/// Explicit multi-resolution image container for ICO resources.
///
/// Unlike [`crate::Image`], entries may have different dimensions and are not
/// animation frames. Use this type when all resolutions are needed; the generic
/// image loader selects one preferred entry instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconSet {
    /// Decoded icon entries in ICO directory order.
    pub frames: Vec<IcoFrame>,
}

impl IconSet {
    /// Decodes every resolution stored in an ICO file.
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        Ok(Ico::new(bytes)?.into_icon_set())
    }

    /// Returns the number of available icon resolutions.
    #[inline]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Returns true when no icon entries are present.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Borrows all decoded icon entries.
    #[inline]
    pub fn frames(&self) -> &[IcoFrame] {
        &self.frames
    }

    /// Moves all decoded icon entries out without cloning their pixel buffers.
    #[inline]
    pub fn into_frames(self) -> Vec<IcoFrame> {
        self.frames
    }

    /// Returns the best decoded entry for an optional preferred square size.
    pub fn preferred_frame(&self, preferred_size: Option<u32>) -> Option<&IcoFrame> {
        let score = |frame: &IcoFrame| frame.width.max(frame.height);
        match preferred_size {
            None => self.frames.iter().max_by_key(|frame| {
                (frame.width as u64 * frame.height as u64, score(frame))
            }),
            Some(size) => {
                if let Some(exact) = self.frames.iter().find(|frame| {
                    frame.width == size && frame.height == size
                }) {
                    return Some(exact);
                }
                self.frames
                    .iter()
                    .filter(|frame| frame.width >= size && frame.height >= size)
                    .min_by_key(|frame| (score(frame), frame.width.saturating_add(frame.height)))
                    .or_else(|| {
                        self.frames.iter().max_by_key(|frame| {
                            (score(frame), frame.width.saturating_add(frame.height))
                        })
                    })
            }
        }
    }
}

// ============================================================================
// Directory structures
// ============================================================================

#[derive(Debug, Clone, Copy)]
/// Represents one directory entry from an ICO file header.
struct IconDirEntry {
    /// Stores the image, frame, or plane width in pixels.
    width: u32,
    /// Stores the image, frame, or plane height in pixels.
    height: u32,

    #[allow(dead_code)]
    /// Stores the palette color count declared by the ICO directory entry; zero has the ICO-defined special meaning.
    color_count: u8,

    #[allow(dead_code)]
    /// Stores the color-plane count declared by the ICO directory entry.
    planes: u16,

    #[allow(dead_code)]
    /// Stores the nominal bit depth declared by the ICO directory entry.
    bits_per_pixel: u16,

    /// Stores the encoded byte length of the icon image resource.
    size: usize,
    /// Stores the byte offset of the associated encoded data.
    offset: usize,
}

#[derive(Debug, Clone, Copy)]
/// Stores parsed DIB metadata needed to decode a bitmap-backed ICO frame.
struct DibInfo {
    /// Stores the image, frame, or plane width in pixels.
    width: u32,
    /// Stores the image, frame, or plane height in pixels.
    height: u32,

    /// Records whether bitmap rows are encoded from top to bottom rather than bottom to top.
    top_down: bool,

    /// Stores the DIB pixel bit depth for the icon frame.
    bits_per_pixel: u16,
    /// Stores the compression mode declared or detected for the image.
    compression: u32,

    /// Palette start relative to the beginning of the DIB resource.
    palette_offset: usize,
    /// Number of palette entries used by indexed DIBs.
    palette_entries: usize,
    /// Encoded bytes per palette entry (3 for CORE, 4 for INFO headers).
    palette_entry_size: usize,
    /// Optional bitfield masks in red, green, blue, alpha order.
    masks: Option<[u32; 4]>,

    /// Start of the XOR bitmap relative to the
    /// beginning of the DIB resource.
    xor_offset: usize,

    /// Number of bytes occupied by the XOR bitmap.
    xor_size: usize,

    /// Start of the AND mask.
    and_offset: usize,

    /// Bytes per row in the AND mask.
    and_stride: usize,
}


/// Header-only information for one ICO directory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IcoEntryInfo {
    /// Entry width in pixels.
    pub width: u32,
    /// Entry height in pixels.
    pub height: u32,
    /// Declared bits per pixel when present.
    pub bits_per_pixel: u16,
    /// Encoded payload size in bytes.
    pub encoded_size: usize,
}

/// Lazy ICO directory reader.
///
/// Directory entries are validated up front, but individual PNG/DIB payloads
/// are decoded only when the caller selects an entry.
pub struct IcoReader<'a> {
    bytes: &'a [u8],
    entries: Vec<IconDirEntry>,
}

impl<'a> IcoReader<'a> {
    /// Parses and validates the ICO directory without decoding icon pixels.
    pub fn new(bytes: &'a [u8]) -> Result<Self, ImageError> {
        let entries = Ico::read_directory(bytes)?;
        Ok(Self { bytes, entries })
    }

    /// Number of icon resolutions in the directory.
    #[inline]
    pub fn len(&self) -> usize { self.entries.len() }

    /// Returns true when the directory has no entries.
    #[inline]
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    /// Returns header information for one entry without decoding it.
    pub fn entry_info(&self, index: usize) -> Option<IcoEntryInfo> {
        self.entries.get(index).map(|entry| IcoEntryInfo {
            width: entry.width,
            height: entry.height,
            bits_per_pixel: entry.bits_per_pixel,
            encoded_size: entry.size,
        })
    }

    /// Iterates directory information without allocating a second directory.
    pub fn entries(&self) -> impl ExactSizeIterator<Item = IcoEntryInfo> + '_ {
        self.entries.iter().map(|entry| IcoEntryInfo {
            width: entry.width,
            height: entry.height,
            bits_per_pixel: entry.bits_per_pixel,
            encoded_size: entry.size,
        })
    }

    /// Decodes exactly one selected ICO entry.
    pub fn decode(&self, index: usize) -> Result<IcoFrame, ImageError> {
        let entry = *self.entries.get(index)
            .ok_or_else(|| ImageError::new("ICO entry index is out of range"))?;
        let data = Ico::entry_data(self.bytes, entry)?;
        Ico::decode_frame(data, entry)
    }

    /// Chooses an entry using the same quality policy as generic `Image` decode
    /// and decodes only that payload.
    pub fn decode_preferred(&self, preferred_size: Option<u32>) -> Result<IcoFrame, ImageError> {
        if self.entries.is_empty() {
            return Err(ImageError::new("ICO contains no images"));
        }
        self.decode(Ico::preferred_entry_index(&self.entries, preferred_size))
    }
}

// ============================================================================
// ICO implementation
// ============================================================================

impl Ico {
    /// Constructs a new decoder value from the supplied encoded byte slice.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    pub fn new(
        bytes: &[u8],
    ) -> Result<Self, ImageError> {
        let entries = Self::read_directory(bytes)?;
        let mut frames = Vec::with_capacity(entries.len());
        for entry in entries {
            let image_data = Self::entry_data(bytes, entry)?;
            frames.push(Self::decode_frame(image_data, entry)?);
        }
        let first = frames
            .first()
            .ok_or_else(|| ImageError::new("ICO contains no decoded frames"))?;
        Ok(Self {
            width: first.width,
            height: first.height,
            frames,
        })
    }

    /// Borrows the decoded multi-resolution entries without cloning them.
    #[inline]
    pub fn frames_ref(&self) -> &[IcoFrame] {
        &self.frames
    }

    /// Converts this legacy rich ICO object into the dedicated multi-resolution
    /// container without copying any pixel buffers.
    #[inline]
    pub fn into_icon_set(self) -> IconSet {
        IconSet { frames: self.frames }
    }

    /// Moves all decoded ICO entries out without cloning pixel buffers.
    #[inline]
    pub fn into_frames(self) -> Vec<IcoFrame> {
        self.frames
    }

    /// Parses and validates only the ICO directory. Pixel payloads remain
    /// borrowed and are decoded only after a caller selects an entry.
    fn read_directory(bytes: &[u8]) -> Result<Vec<IconDirEntry>, ImageError> {
        if bytes.len() < 6 {
            return Err(ImageError::new("File too small for ICO header"));
        }
        let reserved = read_u16_le(bytes, 0)?;
        let image_type = read_u16_le(bytes, 2)?;
        let image_count = read_u16_le(bytes, 4)? as usize;
        if reserved != 0 {
            return Err(ImageError::new("Invalid ICO reserved field"));
        }
        if image_type != ICO_TYPE_ICON {
            return Err(ImageError::new("File is not an ICO icon"));
        }
        if image_count == 0 {
            return Err(ImageError::new("ICO contains no images"));
        }
        let directory_size = image_count
            .checked_mul(16)
            .and_then(|size| size.checked_add(6))
            .ok_or_else(|| ImageError::new("ICO directory size overflow"))?;
        if directory_size > bytes.len() {
            return Err(ImageError::new("ICO directory is truncated"));
        }

        let mut entries = Vec::with_capacity(image_count);
        for index in 0..image_count {
            let offset = 6 + index * 16;
            let width = if bytes[offset] == 0 { 256 } else { bytes[offset] as u32 };
            let height = if bytes[offset + 1] == 0 { 256 } else { bytes[offset + 1] as u32 };
            let color_count = bytes[offset + 2];
            if bytes[offset + 3] != 0 {
                return Err(ImageError::new(format!(
                    "ICO directory entry {index} has invalid reserved field"
                )));
            }
            let planes = read_u16_le(bytes, offset + 4)?;
            let bits_per_pixel = read_u16_le(bytes, offset + 6)?;
            let size = read_u32_le(bytes, offset + 8)? as usize;
            let image_offset = read_u32_le(bytes, offset + 12)? as usize;
            if size == 0 {
                return Err(ImageError::new(format!("ICO image {index} has zero size")));
            }
            let end = image_offset
                .checked_add(size)
                .ok_or_else(|| ImageError::new("ICO image offset overflow"))?;
            if end > bytes.len() {
                return Err(ImageError::new(format!(
                    "ICO image {index} extends beyond the file"
                )));
            }
            entries.push(IconDirEntry {
                width,
                height,
                color_count,
                planes,
                bits_per_pixel,
                size,
                offset: image_offset,
            });
        }
        Ok(entries)
    }

    #[inline]
    fn entry_data(bytes: &[u8], entry: IconDirEntry) -> Result<&[u8], ImageError> {
        let end = entry
            .offset
            .checked_add(entry.size)
            .ok_or_else(|| ImageError::new("ICO image offset overflow"))?;
        bytes
            .get(entry.offset..end)
            .ok_or_else(|| ImageError::new("ICO image extends beyond the file"))
    }

    fn preferred_entry_index(entries: &[IconDirEntry], preferred: Option<u32>) -> usize {
        #[inline]
        fn area(entry: &IconDirEntry) -> u64 {
            entry.width as u64 * entry.height as u64
        }

        #[inline]
        fn longest_side(entry: &IconDirEntry) -> u32 {
            entry.width.max(entry.height)
        }

        let Some(size) = preferred else {
            return entries
                .iter()
                .enumerate()
                .max_by_key(|(_, entry)| (area(entry), entry.bits_per_pixel))
                .map(|(index, _)| index)
                .unwrap_or(0);
        };

        // ICO selection deliberately favors quality over minimum work:
        // 1. exact dimensions,
        // 2. smallest icon that is at least as large as requested,
        // 3. largest icon below the request,
        // with the highest declared bit depth breaking dimension ties.
        if let Some((index, _)) = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.width == size && entry.height == size)
            .max_by_key(|(_, entry)| entry.bits_per_pixel)
        {
            return index;
        }

        if let Some((index, _)) = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.width >= size && entry.height >= size)
            .min_by_key(|(_, entry)| {
                (
                    longest_side(entry),
                    entry.width.saturating_add(entry.height),
                    u16::MAX - entry.bits_per_pixel,
                )
            })
        {
            return index;
        }

        entries
            .iter()
            .enumerate()
            .max_by_key(|(_, entry)| {
                (
                    longest_side(entry),
                    entry.width.saturating_add(entry.height),
                    entry.bits_per_pixel,
                )
            })
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    // ========================================================================
    // Frame decoding
    // ========================================================================

    /// Decodes frame data into the representation required by the surrounding decoder.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn decode_frame(
        data: &[u8],
        entry: IconDirEntry,
    ) -> Result<IcoFrame, ImageError> {
        if data.len() >= 8
            && &data[..8]
                == PNG_SIGNATURE
        {
            return Self::decode_png_frame(
                data,
                entry,
            );
        }

        Self::decode_dib_frame(
            data,
            entry,
        )
    }

    // ========================================================================
    // PNG-backed ICO images
    // ========================================================================

    /// Decodes png frame data into the representation required by the surrounding decoder.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn decode_png_frame(
        data: &[u8],
        entry: IconDirEntry,
    ) -> Result<IcoFrame, ImageError> {
        /*
         * This assumes your PNG module follows the same interface as
         * the other image decoders:
         *
         *     Png::new(&[u8])
         *
         * If yours is currently named `from_bytes`, the only line
         * that needs changing is this constructor call.
         */

        let png = Png::new(data)?;
        let width = png.header.width;
        let height = png.header.height;
        let pixels = png.decode_pixels()?;

        // Directory dimensions are useful for detecting
        // malformed/corrupt ICO files.
        if width != entry.width
            || height != entry.height
        {
            return Err(ImageError::new(
                format!(
                    "ICO PNG dimensions disagree with directory: \
                     directory={}x{}, PNG={}x{}",
                    entry.width,
                    entry.height,
                    width,
                    height,
                )
            ));
        }

        Ok(IcoFrame {
            width,
            height,
            pixels,
        })
    }

    // ========================================================================
    // DIB-backed ICO images
    // ========================================================================

    /// Decodes dib frame data into the representation required by the surrounding decoder.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn decode_dib_frame(
        data: &[u8],
        entry: IconDirEntry,
    ) -> Result<IcoFrame, ImageError> {
        let info =
            Self::parse_dib(
                data
            )?;

        if info.width != entry.width
            || info.height != entry.height
        {
            return Err(ImageError::new(
                format!(
                    "ICO DIB dimensions disagree with directory: \
                     directory={}x{}, DIB={}x{}",
                    entry.width,
                    entry.height,
                    info.width,
                    info.height,
                )
            ));
        }

        // Traditional ICO 32-bit BI_RGB data is BGRA.
        //
        // This cannot go through the normal BMP BI_RGB 32-bit
        // decoder because ordinary BMP's fourth byte is considered
        // reserved and your BMP decoder intentionally forces alpha
        // to 255.
        let mut pixels =
            if info.bits_per_pixel == 32
                && info.compression == BI_RGB
            {
                Self::decode_32bit_icon(
                    data,
                    &info,
                )?
            } else {
                Self::decode_dib_pixels(
                    data,
                    &info,
                )?
            };

        // Apply the ICO AND mask.
        //
        // For 32-bit icons, it is legal for the mask to be omitted
        // in some real-world files because alpha carries transparency.
        Self::apply_and_mask(
            data,
            &info,
            &mut pixels,
        )?;

        Ok(
            IcoFrame {
                width: info.width,
                height: info.height,
                pixels,
            }
        )
    }

    // ========================================================================
    // Parse icon DIB
    // ========================================================================

    /// Parses dib from the supplied encoded or textual input.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn parse_dib(
        data: &[u8],
    ) -> Result<DibInfo, ImageError> {
        if data.len() < 12 {
            return Err(ImageError::new(
                "ICO DIB header is truncated"
            ));
        }

        let dib_size =
            read_u32_le(
                data,
                0,
            )? as usize;

        // --------------------------------------------------------------------
        // BITMAPCOREHEADER
        // --------------------------------------------------------------------

        if dib_size == 12 {
            return Self::parse_core_dib(
                data
            );
        }

        // --------------------------------------------------------------------
        // BITMAPINFOHEADER or newer
        // --------------------------------------------------------------------

        if dib_size < 40 {
            return Err(ImageError::new(
                format!(
                    "Unsupported ICO DIB header size: {}",
                    dib_size
                )
            ));
        }

        if dib_size > data.len() {
            return Err(ImageError::new(
                "ICO DIB header is truncated"
            ));
        }

        let width_signed =
            read_i32_le(
                data,
                4,
            )?;

        let stored_height_signed =
            read_i32_le(
                data,
                8,
            )?;

        if width_signed <= 0 {
            return Err(ImageError::new(
                "ICO DIB width must be positive"
            ));
        }

        if stored_height_signed == 0 {
            return Err(ImageError::new(
                "ICO DIB height cannot be zero"
            ));
        }

        let planes =
            read_u16_le(
                data,
                12,
            )?;

        if planes != 1 {
            return Err(ImageError::new(
                "ICO DIB plane count must be 1"
            ));
        }

        let bits_per_pixel =
            read_u16_le(
                data,
                14,
            )?;

        match bits_per_pixel {
            1
            | 4
            | 8
            | 16
            | 24
            | 32 => {}

            _ => {
                return Err(ImageError::new(
                    format!(
                        "Unsupported ICO bit depth: {}",
                        bits_per_pixel
                    )
                ));
            }
        }

        let compression =
            read_u32_le(
                data,
                16,
            )?;

        match compression {
            BI_RGB
            | BI_BITFIELDS
            | BI_ALPHABITFIELDS => {}

            BI_RLE8 => {
                return Err(ImageError::new(
                    "RLE8-compressed ICO DIBs are not currently supported"
                ));
            }

            BI_RLE4 => {
                return Err(ImageError::new(
                    "RLE4-compressed ICO DIBs are not currently supported"
                ));
            }

            BI_JPEG => {
                return Err(ImageError::new(
                    "JPEG-compressed ICO DIBs are not supported"
                ));
            }

            BI_PNG => {
                return Err(ImageError::new(
                    "BI_PNG inside an ICO DIB is not supported"
                ));
            }

            _ => {
                return Err(ImageError::new(
                    format!(
                        "Unsupported ICO DIB compression: {}",
                        compression
                    )
                ));
            }
        }

        if compression == BI_BITFIELDS
            || compression
                == BI_ALPHABITFIELDS
        {
            if bits_per_pixel != 16
                && bits_per_pixel != 32
            {
                return Err(ImageError::new(
                    "ICO bitfields require 16-bit or 32-bit pixels"
                ));
            }
        }

        let top_down =
            stored_height_signed < 0;

        let stored_height =
            stored_height_signed
                .checked_abs()
                .ok_or_else(|| {
                    ImageError::new(
                        "ICO DIB height overflow"
                    )
                })? as u32;

        // ICO DIB height includes:
        //
        //     XOR bitmap height
        //   + AND bitmap height
        //
        // Therefore it must be divided by two.
        if stored_height < 2
            || stored_height & 1 != 0
        {
            return Err(ImageError::new(
                format!(
                    "ICO DIB stored height {} is not divisible by two",
                    stored_height
                )
            ));
        }

        let width =
            width_signed as u32;

        let height =
            stored_height / 2;

        let colors_used =
            read_u32_le(
                data,
                32,
            )?;

        // --------------------------------------------------------------------
        // Locate masks / palette / XOR bitmap
        // --------------------------------------------------------------------

        let mut xor_offset = dib_size;
        let mut masks = None;

        if compression == BI_BITFIELDS || compression == BI_ALPHABITFIELDS {
            let (red, green, blue, alpha) = if dib_size >= 52 {
                (
                    read_u32_le(data, 40)?,
                    read_u32_le(data, 44)?,
                    read_u32_le(data, 48)?,
                    if dib_size >= 56 { read_u32_le(data, 52)? } else { 0 },
                )
            } else {
                // BITMAPINFOHEADER stores masks immediately after the 40-byte header.
                let red = read_u32_le(data, 40)?;
                let green = read_u32_le(data, 44)?;
                let blue = read_u32_le(data, 48)?;
                let alpha = if compression == BI_ALPHABITFIELDS { read_u32_le(data, 52)? } else { 0 };
                let mask_count = if compression == BI_ALPHABITFIELDS { 4 } else { 3 };
                xor_offset = xor_offset.checked_add(mask_count * 4).ok_or_else(|| {
                    ImageError::new("ICO bitfield offset overflow")
                })?;
                (red, green, blue, alpha)
            };

            for mask in [red, green, blue, alpha] {
                if mask != 0 && !mask_is_contiguous(mask) {
                    return Err(ImageError::new("ICO color masks must contain contiguous bits"));
                }
            }
            if red == 0 || green == 0 || blue == 0
                || red & green != 0 || red & blue != 0 || green & blue != 0
                || (alpha != 0 && alpha & (red | green | blue) != 0)
            {
                return Err(ImageError::new("Invalid or overlapping ICO bitfield masks"));
            }
            masks = Some([red, green, blue, alpha]);
        }

        let palette_offset = xor_offset;
        let mut palette_entries = 0usize;
        if bits_per_pixel <= 8 {
            let maximum_colors = 1usize << bits_per_pixel;
            palette_entries = if colors_used == 0 {
                maximum_colors
            } else {
                let count = colors_used as usize;
                if count > maximum_colors {
                    return Err(ImageError::new("ICO palette contains too many entries"));
                }
                count
            };

            let palette_bytes = palette_entries.checked_mul(4).ok_or_else(|| {
                ImageError::new("ICO palette size overflow")
            })?;
            xor_offset = xor_offset.checked_add(palette_bytes).ok_or_else(|| {
                ImageError::new("ICO pixel offset overflow")
            })?;
        }

        if xor_offset > data.len() {
            return Err(ImageError::new(
                "ICO DIB palette or masks are truncated"
            ));
        }

        let xor_stride =
            row_stride(
                width,
                bits_per_pixel,
            )?;

        let xor_size =
            xor_stride
                .checked_mul(
                    height as usize
                )
                .ok_or_else(|| {
                    ImageError::new(
                        "ICO XOR bitmap size overflow"
                    )
                })?;

        let and_offset =
            xor_offset
                .checked_add(
                    xor_size
                )
                .ok_or_else(|| {
                    ImageError::new(
                        "ICO AND mask offset overflow"
                    )
                })?;

        if and_offset > data.len() {
            return Err(ImageError::new(
                "ICO XOR bitmap is truncated"
            ));
        }

        let and_stride =
            row_stride(
                width,
                1,
            )?;

        Ok(
            DibInfo {
                width,
                height,
                top_down,
                bits_per_pixel,
                compression,
                palette_offset,
                palette_entries,
                palette_entry_size: 4,
                masks,
                xor_offset,
                xor_size,
                and_offset,
                and_stride,
            }
        )
    }

    // ========================================================================
    // BITMAPCOREHEADER
    // ========================================================================

    /// Parses core dib from the supplied encoded or textual input.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn parse_core_dib(
        data: &[u8],
    ) -> Result<DibInfo, ImageError> {
        let width =
            read_u16_le(
                data,
                4,
            )? as u32;

        let stored_height =
            read_u16_le(
                data,
                6,
            )? as u32;

        let planes =
            read_u16_le(
                data,
                8,
            )?;

        let bits_per_pixel =
            read_u16_le(
                data,
                10,
            )?;

        if width == 0 {
            return Err(ImageError::new(
                "ICO CORE DIB width cannot be zero"
            ));
        }

        if stored_height < 2
            || stored_height & 1 != 0
        {
            return Err(ImageError::new(
                "ICO CORE DIB stored height must be divisible by two"
            ));
        }

        if planes != 1 {
            return Err(ImageError::new(
                "ICO CORE DIB plane count must be 1"
            ));
        }

        match bits_per_pixel {
            1
            | 4
            | 8
            | 16
            | 24
            | 32 => {}

            _ => {
                return Err(ImageError::new(
                    format!(
                        "Unsupported ICO CORE bit depth: {}",
                        bits_per_pixel
                    )
                ));
            }
        }

        let height =
            stored_height / 2;

        let palette_offset = 12usize;
        let palette_entries = if bits_per_pixel <= 8 { 1usize << bits_per_pixel } else { 0 };
        let palette_bytes = palette_entries.checked_mul(3).ok_or_else(|| {
            ImageError::new("ICO CORE palette size overflow")
        })?;
        let xor_offset = palette_offset.checked_add(palette_bytes).ok_or_else(|| {
            ImageError::new("ICO CORE pixel offset overflow")
        })?;

        if xor_offset > data.len() {
            return Err(ImageError::new(
                "ICO CORE palette is truncated"
            ));
        }

        let xor_stride =
            row_stride(
                width,
                bits_per_pixel,
            )?;

        let xor_size =
            xor_stride
                .checked_mul(
                    height as usize
                )
                .ok_or_else(|| {
                    ImageError::new(
                        "ICO CORE XOR bitmap size overflow"
                    )
                })?;

        let and_offset =
            xor_offset
                .checked_add(
                    xor_size
                )
                .ok_or_else(|| {
                    ImageError::new(
                        "ICO CORE AND mask offset overflow"
                    )
                })?;

        if and_offset > data.len() {
            return Err(ImageError::new(
                "ICO CORE XOR bitmap is truncated"
            ));
        }

        let and_stride =
            row_stride(
                width,
                1,
            )?;

        Ok(
            DibInfo {
                width,
                height,
                top_down: false,
                bits_per_pixel,
                compression: BI_RGB,
                palette_offset,
                palette_entries,
                palette_entry_size: 3,
                masks: None,
                xor_offset,
                xor_size,
                and_offset,
                and_stride,
            }
        )
    }

    // ========================================================================
    // Direct DIB decode
    // ========================================================================

    /// Decodes an ICO DIB resource directly into packed pixels.
    ///
    /// The old path prepended a synthetic 14-byte BMP header and copied the
    /// complete DIB/XOR payload before invoking the BMP decoder. ICO already has
    /// all layout information after directory/DIB parsing, so decoding in place
    /// removes that image-sized temporary allocation and copy.
    fn decode_dib_pixels(
        data: &[u8],
        info: &DibInfo,
    ) -> Result<Vec<u32>, ImageError> {
        let width = info.width as usize;
        let height = info.height as usize;
        let pixel_count = width.checked_mul(height)
            .ok_or_else(|| ImageError::new("ICO pixel count overflow"))?;
        let stride = row_stride(info.width, info.bits_per_pixel)?;
        let xor_end = info.xor_offset.checked_add(info.xor_size)
            .ok_or_else(|| ImageError::new("ICO XOR bitmap size overflow"))?;
        if xor_end > data.len() {
            return Err(ImageError::new("ICO XOR bitmap is truncated"));
        }

        let mut palette = [0u32; 256];
        if info.palette_entries != 0 {
            if info.palette_entries > palette.len() {
                return Err(ImageError::new("ICO palette contains too many entries"));
            }
            for (index, slot) in palette[..info.palette_entries].iter_mut().enumerate() {
                let entry_offset = info.palette_offset
                    .checked_add(index.checked_mul(info.palette_entry_size)
                        .ok_or_else(|| ImageError::new("ICO palette offset overflow"))?)
                    .ok_or_else(|| ImageError::new("ICO palette offset overflow"))?;
                let entry_end = entry_offset.checked_add(info.palette_entry_size)
                    .ok_or_else(|| ImageError::new("ICO palette offset overflow"))?;
                let entry = data.get(entry_offset..entry_end)
                    .ok_or_else(|| ImageError::new("ICO palette is truncated"))?;
                *slot = u32::from_be_bytes([entry[2], entry[1], entry[0], 255]);
            }
        }

        let masks = if info.compression == BI_BITFIELDS || info.compression == BI_ALPHABITFIELDS {
            info.masks.ok_or_else(|| ImageError::new("ICO bitfields are missing channel masks"))?
        } else if info.bits_per_pixel == 16 {
            // BI_RGB 16-bit DIBs use the historical RGB555 layout.
            [0x7c00, 0x03e0, 0x001f, 0]
        } else {
            [0, 0, 0, 0]
        };
        let red = ChannelMask::new(masks[0]);
        let green = ChannelMask::new(masks[1]);
        let blue = ChannelMask::new(masks[2]);
        let alpha = ChannelMask::new(masks[3]);

        let mut output = vec![0u32; pixel_count];
        for y in 0..height {
            let source_y = if info.top_down { y } else { height - 1 - y };
            let row_start = info.xor_offset
                .checked_add(source_y.checked_mul(stride)
                    .ok_or_else(|| ImageError::new("ICO row offset overflow"))?)
                .ok_or_else(|| ImageError::new("ICO row offset overflow"))?;
            let row_end = row_start.checked_add(stride)
                .ok_or_else(|| ImageError::new("ICO row offset overflow"))?;
            let row = data.get(row_start..row_end)
                .ok_or_else(|| ImageError::new("ICO pixel row is truncated"))?;
            let dst = &mut output[y * width..(y + 1) * width];

            match info.bits_per_pixel {
                1 => {
                    for (x, pixel) in dst.iter_mut().enumerate() {
                        let index = ((row[x >> 3] >> (7 - (x & 7))) & 1) as usize;
                        if index >= info.palette_entries {
                            return Err(ImageError::new("ICO palette index is out of range"));
                        }
                        *pixel = palette[index];
                    }
                }
                4 => {
                    for (x, pixel) in dst.iter_mut().enumerate() {
                        let byte = row[x >> 1];
                        let index = (if x & 1 == 0 { byte >> 4 } else { byte & 0x0f }) as usize;
                        if index >= info.palette_entries {
                            return Err(ImageError::new("ICO palette index is out of range"));
                        }
                        *pixel = palette[index];
                    }
                }
                8 => {
                    for (pixel, &encoded) in dst.iter_mut().zip(&row[..width]) {
                        let index = encoded as usize;
                        if index >= info.palette_entries {
                            return Err(ImageError::new("ICO palette index is out of range"));
                        }
                        *pixel = palette[index];
                    }
                }
                16 => {
                    for (x, pixel) in dst.iter_mut().enumerate() {
                        let base = x * 2;
                        let value = u16::from_le_bytes([row[base], row[base + 1]]) as u32;
                        let a = if masks[3] == 0 { 255 } else { alpha.extract(value) };
                        *pixel = u32::from_be_bytes([
                            red.extract(value), green.extract(value), blue.extract(value), a,
                        ]);
                    }
                }
                24 => {
                    for (x, pixel) in dst.iter_mut().enumerate() {
                        let base = x * 3;
                        *pixel = u32::from_be_bytes([
                            row[base + 2], row[base + 1], row[base], 255,
                        ]);
                    }
                }
                32 => {
                    // BI_RGB 32-bit icons use the alpha-aware specialized path;
                    // this branch therefore handles bitfield-compressed 32-bit DIBs.
                    for (x, pixel) in dst.iter_mut().enumerate() {
                        let base = x * 4;
                        let value = u32::from_le_bytes([
                            row[base], row[base + 1], row[base + 2], row[base + 3],
                        ]);
                        let a = if masks[3] == 0 { 255 } else { alpha.extract(value) };
                        *pixel = u32::from_be_bytes([
                            red.extract(value), green.extract(value), blue.extract(value), a,
                        ]);
                    }
                }
                _ => return Err(ImageError::new("Unsupported ICO DIB bit depth")),
            }
        }

        Ok(output)
    }

    // ========================================================================
    // 32-bit BGRA ICO
    // ========================================================================

    /// Decodes 32bit icon data into the representation required by the surrounding decoder.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn decode_32bit_icon(
        data: &[u8],
        info: &DibInfo,
    ) -> Result<Vec<u32>, ImageError> {
        let width =
            info.width as usize;

        let height =
            info.height as usize;

        let stride =
            row_stride(
                info.width,
                32,
            )?;

        let pixel_count =
            width
                .checked_mul(
                    height
                )
                .ok_or_else(|| {
                    ImageError::new(
                        "ICO pixel count overflow"
                    )
                })?;

        let mut pixels =
            vec![
                0u32;
                pixel_count
            ];

        let mut has_real_alpha =
            false;

        // --------------------------------------------------------------------
        // First pass: determine whether alpha is actually being used.
        //
        // Old 32-bit ICOs often contain zero in every alpha byte.
        // In those files, zero means "unused", not fully transparent.
        // --------------------------------------------------------------------

        for y in 0..height {
            let source_y =
                if info.top_down {
                    y
                } else {
                    height - 1 - y
                };

            let row =
                info.xor_offset
                    .checked_add(
                        source_y * stride
                    )
                    .ok_or_else(|| {
                        ImageError::new(
                            "ICO row offset overflow"
                        )
                    })?;

            let required =
                width
                    .checked_mul(4)
                    .ok_or_else(|| {
                        ImageError::new(
                            "ICO row size overflow"
                        )
                    })?;

            if row
                .checked_add(required)
                .is_none_or(
                    |end| {
                        end > data.len()
                    }
                )
            {
                return Err(ImageError::new(
                    "Unexpected EOF in 32-bit ICO pixel data"
                ));
            }

            for x in 0..width {
                let src =
                    row + x * 4;

                if data[src + 3] != 0 {
                    has_real_alpha =
                        true;

                    break;
                }
            }

            if has_real_alpha {
                break;
            }
        }

        // --------------------------------------------------------------------
        // Decode BGRA -> RRGGBBAA
        // --------------------------------------------------------------------

        for y in 0..height {
            let source_y = if info.top_down {
                y
            } else {
                height - 1 - y
            };
            let row = info.xor_offset + source_y * stride;
            let dst = &mut pixels[y * width..(y + 1) * width];

            if has_real_alpha {
                crate::simd::bgra8_to_u32(
                    &data[row..row + width * 4],
                    dst,
                );
            } else {
                for (x, pixel) in dst.iter_mut().enumerate() {
                    let src = row + x * 4;
                    *pixel = u32::from_be_bytes([
                        data[src + 2],
                        data[src + 1],
                        data[src],
                        255,
                    ]);
                }
            }
        }

        Ok(pixels)
    }

    // ========================================================================
    // ICO AND mask
    // ========================================================================

    /// Applies and mask to the supplied decoding or rendering state.
    ///
    /// Returns an error when the supplied data cannot be processed as required.
    fn apply_and_mask(
        data: &[u8],
        info: &DibInfo,
        pixels: &mut [u32],
    ) -> Result<(), ImageError> {
        let width =
            info.width as usize;

        let height =
            info.height as usize;

        let expected_pixels =
            width
                .checked_mul(height)
                .ok_or_else(|| {
                    ImageError::new(
                        "ICO pixel count overflow"
                    )
                })?;

        if pixels.len()
            != expected_pixels
        {
            return Err(ImageError::new(
                "ICO pixel buffer has invalid size"
            ));
        }

        let mask_size =
            info.and_stride
                .checked_mul(height)
                .ok_or_else(|| {
                    ImageError::new(
                        "ICO AND mask size overflow"
                    )
                })?;

        let mask_end =
            info.and_offset
                .checked_add(
                    mask_size
                )
                .ok_or_else(|| {
                    ImageError::new(
                        "ICO AND mask size overflow"
                    )
                })?;

        // Some 32-bit icons omit the legacy AND mask
        // because alpha already supplies transparency.
        if mask_end > data.len() {
            if info.bits_per_pixel == 32 {
                return Ok(());
            }

            return Err(ImageError::new(
                "ICO AND transparency mask is truncated"
            ));
        }

        // AND mask rows are stored bottom-up.
        for y in 0..height {
            let source_y =
                height - 1 - y;

            let row =
                info.and_offset
                    + source_y
                        * info.and_stride;

            for x in 0..width {
                let byte =
                    data[
                        row + x / 8
                    ];

                let bit =
                    7 - (x & 7);

                let transparent =
                    (
                        byte >> bit
                    ) & 1
                        != 0;

                if transparent {
                    let index =
                        y * width + x;

                    // Keep RGB and set A = 0.
                    pixels[index] &=
                        0xFFFFFF00;
                }
            }
        }

        Ok(())
    }

    // ========================================================================
    // Additional ICO-specific methods
    // ========================================================================

    /// Number of images stored in the ICO.
    #[inline]
    pub fn frame_count(
        &self,
    ) -> usize {
        self.frames.len()
    }

    /// Width of every ICO image in directory order.
    pub fn widths(
        &self,
    ) -> Vec<u32> {
        self.frames
            .iter()
            .map(
                |frame| {
                    frame.width
                }
            )
            .collect()
    }

    /// Height of every ICO image in directory order.
    pub fn heights(
        &self,
    ) -> Vec<u32> {
        self.frames
            .iter()
            .map(
                |frame| {
                    frame.height
                }
            )
            .collect()
    }

    /// Return information about an individual ICO frame.
    #[inline]
    pub fn frame(
        &self,
        index: usize,
    ) -> Option<&IcoFrame> {
        self.frames.get(index)
    }

    /// Returns the index of the frame containing
    /// the greatest number of pixels.
    pub fn largest_frame_index(
        &self,
    ) -> Option<usize> {
        self.frames
            .iter()
            .enumerate()
            .max_by_key(
                |(_, frame)| {
                    (
                        frame.width as u64
                    ) * (
                        frame.height as u64
                    )
                }
            )
            .map(
                |(index, _)| {
                    index
                }
            )
    }
}

/// Lightweight ICO decoding selects exactly one resolution instead of treating
/// a multi-resolution icon set as an animation.
pub(crate) fn decode_image(
    bytes: &[u8],
    options: &crate::DecodeOptions,
) -> Result<crate::Image, ImageError> {
    let entries = Ico::read_directory(bytes)?;
    let index = Ico::preferred_entry_index(&entries, options.preferred_icon_size);
    let entry = entries[index];
    options.checked_rgba_bytes(entry.width, entry.height)?;
    let data = Ico::entry_data(bytes, entry)?;

    if data.get(..PNG_SIGNATURE.len()) == Some(PNG_SIGNATURE.as_slice()) {
        let image = crate::decoder::png::decode_image(data, options)?;
        if image.width != entry.width || image.height != entry.height {
            return Err(ImageError::new("ICO PNG dimensions disagree with directory"));
        }
        return Ok(image);
    }

    let frame = Ico::decode_frame(data, entry)?;
    crate::Image::from_packed_pixels(frame.width, frame.height, frame.pixels, options)
}

// ============================================================================
// Decoder
// ============================================================================

impl super::Decoder for Ico {
    #[inline]
    fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    fn height(&self) -> u32 {
        self.height
    }

    /// Converting a rich ICO through the polymorphic decoder selects one
    /// preferred (largest) icon; it never treats resolutions as animation frames.
    fn into_image(self: Box<Self>) -> Result<crate::Image, ImageError> {
        let index = self
            .largest_frame_index()
            .ok_or_else(|| ImageError::new("ICO contains no decoded frames"))?;
        let this = *self;
        let frame = this
            .frames
            .into_iter()
            .nth(index)
            .ok_or_else(|| ImageError::new("ICO preferred frame is unavailable"))?;
        crate::Image::from_packed_pixels(
            frame.width,
            frame.height,
            frame.pixels,
            &crate::DecodeOptions::default(),
        )
    }
}
