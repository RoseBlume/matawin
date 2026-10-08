//! Borrowed metadata inspection that is separate from pixel decoding.
//!
//! The generic [`crate::Image`] never stores metadata. Callers that need
//! metadata can inspect encoded bytes explicitly through [`ImageMetadata`].
//! Payloads are borrowed from the source whenever possible; compressed PNG ICC
//! or text payloads are deliberately not decompressed by this layer.

use crate::{ImageError, ImageFormat, MetadataMode};

/// Identifies one borrowed format-specific metadata payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetadataKind {
    /// A PNG ancillary or interpretation chunk identified by its four-byte type.
    Png([u8; 4]),
    /// A JPEG marker (for example APP0, APP1, APP14, or COM).
    Jpeg(u8),
    /// A WebP RIFF chunk identified by its FourCC.
    WebP([u8; 4]),
    /// A GIF extension label.
    Gif(u8),
    /// A TIFF IFD entry identified by its tag number.
    Tiff(u16),
    /// A FITS header card keyword (the eight-byte keyword field, trimmed by callers).
    Fits,
    /// Container metadata for formats where no more specific identifier is useful.
    Container,
}

/// One borrowed metadata payload. `data` points into the caller's encoded bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetadataBlock<'a> {
    /// Format-specific block kind.
    pub kind: MetadataKind,
    /// Borrowed encoded payload. No decompression or text allocation is implied.
    pub data: &'a [u8],
}

/// Explicit metadata result for one encoded image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageMetadata<'a> {
    /// Detected encoded format.
    pub format: ImageFormat,
    /// Metadata policy used by the parser.
    pub mode: MetadataMode,
    /// Borrowed metadata blocks selected by the policy.
    pub blocks: Vec<MetadataBlock<'a>>,
}

impl<'a> ImageMetadata<'a> {
    /// Parses metadata without decoding pixels.
    ///
    /// `None` returns immediately after format detection. `Essential` retains
    /// only interpretation-affecting/header metadata. `All` returns every
    /// supported metadata block while keeping payloads borrowed and compressed.
    pub fn read(bytes: &'a [u8], mode: MetadataMode) -> Result<Self, ImageError> {
        let format = ImageFormat::from_signature(bytes)
            .ok_or_else(|| ImageError::unsupported("Unsupported image format"))?;
        let blocks = if matches!(mode, MetadataMode::None) {
            Vec::new()
        } else {
            parse_for_format(format, bytes, mode)?
        };
        Ok(Self { format, mode, blocks })
    }

    /// Iterates metadata blocks without allocating another collection.
    #[inline]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &MetadataBlock<'a>> {
        self.blocks.iter()
    }
}

fn parse_for_format<'a>(
    format: ImageFormat,
    bytes: &'a [u8],
    mode: MetadataMode,
) -> Result<Vec<MetadataBlock<'a>>, ImageError> {
    match format {
        ImageFormat::Png => {
            #[cfg(feature = "png")]
            { parse_png(bytes, mode) }
            #[cfg(not(feature = "png"))]
            { Err(ImageError::unsupported("PNG support is disabled")) }
        }
        ImageFormat::Jpeg => {
            #[cfg(feature = "jpeg")]
            { parse_jpeg(bytes, mode) }
            #[cfg(not(feature = "jpeg"))]
            { Err(ImageError::unsupported("JPEG support is disabled")) }
        }
        ImageFormat::WebP => {
            #[cfg(feature = "webp")]
            { parse_webp(bytes, mode) }
            #[cfg(not(feature = "webp"))]
            { Err(ImageError::unsupported("WebP support is disabled")) }
        }
        ImageFormat::Gif => {
            #[cfg(feature = "gif")]
            { parse_gif(bytes, mode) }
            #[cfg(not(feature = "gif"))]
            { Err(ImageError::unsupported("GIF support is disabled")) }
        }
        ImageFormat::Tiff => {
            #[cfg(feature = "tiff")]
            { parse_tiff(bytes, mode) }
            #[cfg(not(feature = "tiff"))]
            { Err(ImageError::unsupported("TIFF support is disabled")) }
        }
        ImageFormat::Fits => {
            #[cfg(feature = "fits")]
            { parse_fits(bytes, mode) }
            #[cfg(not(feature = "fits"))]
            { Err(ImageError::unsupported("FITS support is disabled")) }
        }
        // These formats either have no optional metadata currently exposed by
        // mataimage or keep it in their explicit rich decoder type.
        ImageFormat::Bmp
        | ImageFormat::Qoi
        | ImageFormat::Ico
        | ImageFormat::Avif
        | ImageFormat::Pgm
        | ImageFormat::Tga
        | ImageFormat::Wbmp
        | ImageFormat::Svg => Ok(Vec::new()),
    }
}

#[cfg(feature = "png")]
fn parse_png<'a>(bytes: &'a [u8], mode: MetadataMode) -> Result<Vec<MetadataBlock<'a>>, ImageError> {
    if bytes.get(..8) != Some(b"\x89PNG\r\n\x1a\n") {
        return Err(ImageError::new("Invalid PNG signature"));
    }
    let mut blocks = Vec::new();
    let mut pos = 8usize;
    while pos + 12 <= bytes.len() {
        let length = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        let kind: [u8; 4] = bytes[pos + 4..pos + 8].try_into().unwrap();
        let data_start = pos + 8;
        let data_end = data_start.checked_add(length).ok_or_else(|| ImageError::new("PNG chunk size overflow"))?;
        let end = data_end.checked_add(4).ok_or_else(|| ImageError::new("PNG chunk CRC overflow"))?;
        if end > bytes.len() { return Err(ImageError::new("Truncated PNG chunk")); }
        let ancillary = kind[0] & 0x20 != 0;
        let essential = matches!(&kind, b"PLTE" | b"tRNS" | b"sRGB" | b"gAMA" | b"cHRM" | b"iCCP" | b"acTL" | b"fcTL");
        if (matches!(mode, MetadataMode::All) && ancillary) || (matches!(mode, MetadataMode::Essential) && essential) {
            blocks.push(MetadataBlock { kind: MetadataKind::Png(kind), data: &bytes[data_start..data_end] });
        }
        pos = end;
        if &kind == b"IEND" { break; }
    }
    Ok(blocks)
}

#[cfg(feature = "jpeg")]
fn parse_jpeg<'a>(bytes: &'a [u8], mode: MetadataMode) -> Result<Vec<MetadataBlock<'a>>, ImageError> {
    if bytes.get(..2) != Some(&[0xff, 0xd8]) { return Err(ImageError::new("Invalid JPEG SOI signature")); }
    let mut blocks = Vec::new();
    let mut pos = 2usize;
    while pos < bytes.len() {
        while pos < bytes.len() && bytes[pos] != 0xff { pos += 1; }
        while pos < bytes.len() && bytes[pos] == 0xff { pos += 1; }
        let marker = *bytes.get(pos).ok_or_else(|| ImageError::new("Truncated JPEG marker"))?;
        pos += 1;
        if marker == 0xd9 || marker == 0xda { break; }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) { continue; }
        if pos + 2 > bytes.len() { return Err(ImageError::new("Truncated JPEG segment")); }
        let length = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        if length < 2 { return Err(ImageError::new("Invalid JPEG segment length")); }
        let end = pos.checked_add(length).ok_or_else(|| ImageError::new("JPEG segment overflow"))?;
        if end > bytes.len() { return Err(ImageError::new("Truncated JPEG segment")); }
        let payload = &bytes[pos + 2..end];
        let is_metadata = (0xe0..=0xef).contains(&marker) || marker == 0xfe;
        let essential = marker == 0xee; // Adobe APP14 affects color interpretation.
        if (matches!(mode, MetadataMode::All) && is_metadata) || (matches!(mode, MetadataMode::Essential) && essential) {
            blocks.push(MetadataBlock { kind: MetadataKind::Jpeg(marker), data: payload });
        }
        pos = end;
    }
    Ok(blocks)
}

#[cfg(feature = "webp")]
fn parse_webp<'a>(bytes: &'a [u8], mode: MetadataMode) -> Result<Vec<MetadataBlock<'a>>, ImageError> {
    if bytes.len() < 12 || bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WEBP") {
        return Err(ImageError::new("Invalid WebP RIFF header"));
    }
    let mut blocks = Vec::new();
    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let kind: [u8; 4] = bytes[pos..pos + 4].try_into().unwrap();
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let data_start = pos + 8;
        let data_end = data_start.checked_add(size).ok_or_else(|| ImageError::new("WebP chunk size overflow"))?;
        if data_end > bytes.len() { return Err(ImageError::new("Truncated WebP chunk")); }
        let is_metadata = matches!(&kind, b"EXIF" | b"XMP " | b"ICCP" | b"VP8X" | b"ANIM" | b"ANMF" | b"ALPH");
        let essential = matches!(&kind, b"VP8X" | b"ANIM" | b"ANMF" | b"ALPH" | b"ICCP");
        if (matches!(mode, MetadataMode::All) && is_metadata) || (matches!(mode, MetadataMode::Essential) && essential) {
            blocks.push(MetadataBlock { kind: MetadataKind::WebP(kind), data: &bytes[data_start..data_end] });
        }
        pos = data_end.checked_add(size & 1).ok_or_else(|| ImageError::new("WebP padding overflow"))?;
    }
    Ok(blocks)
}

#[cfg(feature = "gif")]
fn parse_gif<'a>(bytes: &'a [u8], mode: MetadataMode) -> Result<Vec<MetadataBlock<'a>>, ImageError> {
    if bytes.len() < 13 { return Err(ImageError::new("Truncated GIF header")); }
    let mut blocks = Vec::new();
    let packed = bytes[10];
    let mut pos = 13usize;
    if packed & 0x80 != 0 {
        pos = pos.checked_add((1usize << ((packed & 7) + 1)) * 3).ok_or_else(|| ImageError::new("GIF color table overflow"))?;
    }
    while pos < bytes.len() {
        let marker = bytes[pos]; pos += 1;
        match marker {
            0x3b => break,
            0x21 => {
                let label = *bytes.get(pos).ok_or_else(|| ImageError::new("Truncated GIF extension"))?; pos += 1;
                let start = pos;
                loop {
                    let n = *bytes.get(pos).ok_or_else(|| ImageError::new("Truncated GIF extension sub-block"))? as usize; pos += 1;
                    if n == 0 { break; }
                    pos = pos.checked_add(n).ok_or_else(|| ImageError::new("GIF extension overflow"))?;
                    if pos > bytes.len() { return Err(ImageError::new("Truncated GIF extension payload")); }
                }
                let essential = label == 0xf9 || label == 0xff;
                if matches!(mode, MetadataMode::All) || (matches!(mode, MetadataMode::Essential) && essential) {
                    blocks.push(MetadataBlock { kind: MetadataKind::Gif(label), data: &bytes[start..pos] });
                }
            }
            0x2c => {
                if pos + 9 > bytes.len() { return Err(ImageError::new("Truncated GIF image descriptor")); }
                let p = bytes[pos + 8]; pos += 9;
                if p & 0x80 != 0 {
                    let table_bytes = (1usize << ((p & 7) + 1)) * 3;
                    pos = pos.checked_add(table_bytes)
                        .ok_or_else(|| ImageError::new("GIF local color table overflow"))?;
                    if pos > bytes.len() {
                        return Err(ImageError::new("Truncated GIF local color table"));
                    }
                }
                pos = pos.checked_add(1)
                    .ok_or_else(|| ImageError::new("GIF LZW offset overflow"))?; // minimum code size
                if pos > bytes.len() {
                    return Err(ImageError::new("Truncated GIF LZW code size"));
                }
                loop {
                    let n = *bytes.get(pos).ok_or_else(|| ImageError::new("Truncated GIF image sub-block"))? as usize; pos += 1;
                    if n == 0 { break; }
                    pos = pos.checked_add(n).ok_or_else(|| ImageError::new("GIF image data overflow"))?;
                    if pos > bytes.len() { return Err(ImageError::new("Truncated GIF image data")); }
                }
            }
            _ => return Err(ImageError::new("Unknown GIF block marker while parsing metadata")),
        }
    }
    Ok(blocks)
}

#[cfg(feature = "tiff")]
fn parse_tiff<'a>(bytes: &'a [u8], mode: MetadataMode) -> Result<Vec<MetadataBlock<'a>>, ImageError> {
    if bytes.len() < 8 { return Err(ImageError::new("Truncated TIFF header")); }
    let little = match &bytes[..2] { b"II" => true, b"MM" => false, _ => return Err(ImageError::new("Invalid TIFF byte order")) };
    let read16 = |at: usize| -> Option<u16> { let s=bytes.get(at..at+2)?; Some(if little{u16::from_le_bytes([s[0],s[1]])}else{u16::from_be_bytes([s[0],s[1]])}) };
    let read32 = |at: usize| -> Option<u32> { let s=bytes.get(at..at+4)?; Some(if little{u32::from_le_bytes([s[0],s[1],s[2],s[3]])}else{u32::from_be_bytes([s[0],s[1],s[2],s[3]])}) };
    let ifd = read32(4).ok_or_else(|| ImageError::new("Truncated TIFF IFD offset"))? as usize;
    let count = read16(ifd).ok_or_else(|| ImageError::new("Truncated TIFF IFD"))? as usize;
    let mut blocks = Vec::new();
    for index in 0..count {
        let at = ifd + 2 + index * 12;
        let entry = bytes.get(at..at + 12).ok_or_else(|| ImageError::new("Truncated TIFF IFD entry"))?;
        let tag = read16(at).unwrap();
        let essential = matches!(tag, 256 | 257 | 258 | 259 | 262 | 273 | 277 | 278 | 279 | 284 | 317 | 320 | 322 | 323 | 324 | 325 | 338);
        if matches!(mode, MetadataMode::All) || (matches!(mode, MetadataMode::Essential) && essential) {
            blocks.push(MetadataBlock { kind: MetadataKind::Tiff(tag), data: entry });
        }
    }
    Ok(blocks)
}

#[cfg(feature = "fits")]
fn parse_fits<'a>(bytes: &'a [u8], mode: MetadataMode) -> Result<Vec<MetadataBlock<'a>>, ImageError> {
    let mut blocks = Vec::new();
    let mut pos = 0usize;
    while pos + 80 <= bytes.len() {
        let card = &bytes[pos..pos + 80];
        pos += 80;
        let key_field = &card[..8];
        let key_end = key_field.iter().position(|b| b.is_ascii_whitespace()).unwrap_or(key_field.len());
        let key = &key_field[..key_end];
        let essential = key == b"SIMPLE" || key == b"BITPIX" || key == b"NAXIS"
            || key == b"NAXIS1" || key == b"NAXIS2" || key == b"BSCALE"
            || key == b"BZERO" || key == b"BLANK" || key == b"XTENSION"
            || key == b"PCOUNT" || key == b"GCOUNT";
        if matches!(mode, MetadataMode::All) || (matches!(mode, MetadataMode::Essential) && essential) {
            blocks.push(MetadataBlock { kind: MetadataKind::Fits, data: card });
        }
        if key == b"END" { break; }
    }
    Ok(blocks)
}
