//! Allocation-free image format detection and header-only information parsing.

use crate::ImageError;

/// Image container/encoding identified from an encoded byte signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    /// Portable Network Graphics.
    Png,
    /// JPEG/JFIF/Exif JPEG.
    Jpeg,
    /// WebP RIFF container.
    WebP,
    /// Graphics Interchange Format.
    Gif,
    /// Windows bitmap.
    Bmp,
    /// Flexible Image Transport System image.
    Fits,
    /// Tagged Image File Format.
    Tiff,
    /// Quite OK Image.
    Qoi,
    /// Windows icon container.
    Ico,
    /// AVIF/AV1 still-image container.
    Avif,
    /// Portable Graymap.
    Pgm,
    /// Truevision TGA.
    Tga,
    /// Wireless Bitmap Type 0.
    Wbmp,
    /// Scalable Vector Graphics.
    Svg,
}

impl ImageFormat {
    /// Detects an encoded image format without allocating.
    ///
    /// Exact/high-confidence signatures are checked before intentionally looser
    /// legacy formats such as TGA and WBMP.
    pub fn from_signature(bytes: &[u8]) -> Option<Self> {
        let len = bytes.len();

        if len >= 8 && bytes[..8] == *b"\x89PNG\r\n\x1a\n" {
            return Some(Self::Png);
        }
        if len >= 3 && bytes[..3] == [0xff, 0xd8, 0xff] {
            return Some(Self::Jpeg);
        }
        if len >= 12 && bytes[..4] == *b"RIFF" && bytes[8..12] == *b"WEBP" {
            return Some(Self::WebP);
        }
        if len >= 6 && (bytes[..6] == *b"GIF87a" || bytes[..6] == *b"GIF89a") {
            return Some(Self::Gif);
        }
        if len >= 2 && bytes[..2] == *b"BM" {
            return Some(Self::Bmp);
        }
        if len >= 6 && bytes[..6] == *b"SIMPLE" {
            return Some(Self::Fits);
        }
        if len >= 4
            && (bytes[..4] == [0x49, 0x49, 0x2a, 0x00]
                || bytes[..4] == [0x4d, 0x4d, 0x00, 0x2a])
        {
            return Some(Self::Tiff);
        }
        if len >= 4 && bytes[..4] == *b"qoif" {
            return Some(Self::Qoi);
        }
        if len >= 4 && bytes[..4] == [0x00, 0x00, 0x01, 0x00] {
            return Some(Self::Ico);
        }
        if is_avif(bytes) {
            return Some(Self::Avif);
        }
        if len >= 2 && (bytes[..2] == *b"P2" || bytes[..2] == *b"P5") {
            return Some(Self::Pgm);
        }
        if is_svg(bytes) {
            return Some(Self::Svg);
        }
        if looks_like_wbmp(bytes) {
            return Some(Self::Wbmp);
        }
        if len >= 18 && bytes[1] == 0 && matches!(bytes[2], 2 | 10) {
            return Some(Self::Tga);
        }

        None
    }
}

/// Width/height pair reported by a multi-resolution container such as ICO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageSize {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Header-only information about an encoded image.
///
/// The parser never decodes compressed pixel payloads. For animated and
/// multi-image containers it may walk container blocks/directories to count
/// frames or variants, but compressed frame data is skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInfo {
    /// Detected format.
    pub format: ImageFormat,
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Number of display frames/pages when it can be determined cheaply.
    pub frame_count: Option<u32>,
    /// Whether the encoded image can produce transparent pixels when this is
    /// cheaply knowable from headers/container metadata.
    pub has_alpha: Option<bool>,
    /// Whether the container declares more than one temporal display frame.
    pub animated: bool,
    /// Available ICO entry sizes. Empty for non-ICO formats.
    pub icon_sizes: Vec<ImageSize>,
}

impl ImageInfo {
    /// Reads image information without decoding pixel data.
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ImageError> {
        Self::read(bytes)
    }

    /// Reads image information without decoding pixel data.
    pub fn read(bytes: &[u8]) -> Result<Self, ImageError> {
        let format = ImageFormat::from_signature(bytes)
            .ok_or_else(|| ImageError::unsupported("Unsupported image format"))?;
        if !format_enabled(format) {
            return Err(ImageError::unsupported("Image format is disabled by Cargo features"));
        }
        info_for(format, bytes)
    }

    /// Returns the primary dimensions as a pair.
    #[inline]
    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

#[inline]
fn format_enabled(format: ImageFormat) -> bool {
    match format {
        ImageFormat::Png => cfg!(feature = "png"),
        ImageFormat::Jpeg => cfg!(feature = "jpeg"),
        ImageFormat::WebP => cfg!(feature = "webp"),
        ImageFormat::Gif => cfg!(feature = "gif"),
        ImageFormat::Bmp => cfg!(feature = "bmp"),
        ImageFormat::Fits => cfg!(feature = "fits"),
        ImageFormat::Tiff => cfg!(feature = "tiff"),
        ImageFormat::Qoi => cfg!(feature = "qoi"),
        ImageFormat::Ico => cfg!(feature = "ico"),
        ImageFormat::Avif => cfg!(feature = "avif"),
        ImageFormat::Pgm => cfg!(feature = "pgm"),
        ImageFormat::Tga => cfg!(feature = "tga"),
        ImageFormat::Wbmp => cfg!(feature = "wbmp"),
        ImageFormat::Svg => cfg!(feature = "svg"),
    }
}

fn info_for(format: ImageFormat, bytes: &[u8]) -> Result<ImageInfo, ImageError> {
    let mut info = ImageInfo {
        format,
        width: 0,
        height: 0,
        frame_count: Some(1),
        has_alpha: Some(false),
        animated: false,
        icon_sizes: Vec::new(),
    };

    match format {
        ImageFormat::Png => {
            #[cfg(feature = "png")]
            {
                let (w, h) = png_dimensions(bytes)?;
                info.width = w;
                info.height = h;
                let details = png_info_details(bytes)?;
                info.frame_count = details.0;
                info.has_alpha = details.1;
                info.animated = details.2;
            }
            #[cfg(not(feature = "png"))]
            return Err(ImageError::unsupported("PNG support is disabled"));
        }
        ImageFormat::Jpeg => {
            #[cfg(feature = "jpeg")]
            {
                (info.width, info.height) = jpeg_dimensions(bytes)?;
            }
            #[cfg(not(feature = "jpeg"))]
            return Err(ImageError::unsupported("JPEG support is disabled"));
        }
        ImageFormat::WebP => {
            #[cfg(feature = "webp")]
            {
                (info.width, info.height) = webp_dimensions(bytes)?;
                let details = webp_info_details(bytes)?;
                info.frame_count = details.0;
                info.has_alpha = details.1;
                info.animated = details.2;
            }
            #[cfg(not(feature = "webp"))]
            return Err(ImageError::unsupported("WebP support is disabled"));
        }
        ImageFormat::Gif => {
            #[cfg(feature = "gif")]
            {
                (info.width, info.height) = gif_dimensions(bytes)?;
                let details = gif_info_details(bytes)?;
                info.frame_count = Some(details.0);
                info.has_alpha = Some(details.1);
                info.animated = details.0 > 1;
            }
            #[cfg(not(feature = "gif"))]
            return Err(ImageError::unsupported("GIF support is disabled"));
        }
        ImageFormat::Bmp => {
            #[cfg(feature = "bmp")]
            {
                (info.width, info.height) = bmp_dimensions(bytes)?;
                info.has_alpha = bmp_has_alpha(bytes);
            }
            #[cfg(not(feature = "bmp"))]
            return Err(ImageError::unsupported("BMP support is disabled"));
        }
        ImageFormat::Fits => {
            #[cfg(feature = "fits")]
            {
                (info.width, info.height) = fits_dimensions(bytes)?;
                info.frame_count = fits_frame_count(bytes).ok();
            }
            #[cfg(not(feature = "fits"))]
            return Err(ImageError::unsupported("FITS support is disabled"));
        }
        ImageFormat::Tiff => {
            #[cfg(feature = "tiff")]
            {
                (info.width, info.height) = tiff_dimensions(bytes)?;
                info.frame_count = Some(tiff_ifd_count(bytes)?);
                info.has_alpha = None;
            }
            #[cfg(not(feature = "tiff"))]
            return Err(ImageError::unsupported("TIFF support is disabled"));
        }
        ImageFormat::Qoi => {
            #[cfg(feature = "qoi")]
            {
                (info.width, info.height) = qoi_dimensions(bytes)?;
                info.has_alpha = qoi_has_alpha(bytes);
            }
            #[cfg(not(feature = "qoi"))]
            return Err(ImageError::unsupported("QOI support is disabled"));
        }
        ImageFormat::Ico => {
            #[cfg(feature = "ico")]
            {
                let details = ico_info_details(bytes)?;
                info.width = details.0;
                info.height = details.1;
                info.frame_count = Some(details.2.len() as u32);
                info.icon_sizes = details.2;
                info.has_alpha = None;
            }
            #[cfg(not(feature = "ico"))]
            return Err(ImageError::unsupported("ICO support is disabled"));
        }
        ImageFormat::Avif => {
            #[cfg(feature = "avif")]
            {
                (info.width, info.height) = avif_dimensions(bytes)?;
                info.animated = avif_is_animated(bytes);
                info.frame_count = None;
                info.has_alpha = None;
            }
            #[cfg(not(feature = "avif"))]
            return Err(ImageError::unsupported("AVIF support is disabled"));
        }
        ImageFormat::Pgm => {
            #[cfg(feature = "pgm")]
            {
                (info.width, info.height) = pgm_dimensions(bytes)?;
            }
            #[cfg(not(feature = "pgm"))]
            return Err(ImageError::unsupported("PGM support is disabled"));
        }
        ImageFormat::Tga => {
            #[cfg(feature = "tga")]
            {
                (info.width, info.height) = tga_dimensions(bytes)?;
                info.has_alpha = tga_has_alpha(bytes);
            }
            #[cfg(not(feature = "tga"))]
            return Err(ImageError::unsupported("TGA support is disabled"));
        }
        ImageFormat::Wbmp => {
            #[cfg(feature = "wbmp")]
            {
                (info.width, info.height) = wbmp_dimensions(bytes)?;
            }
            #[cfg(not(feature = "wbmp"))]
            return Err(ImageError::unsupported("WBMP support is disabled"));
        }
        ImageFormat::Svg => {
            #[cfg(feature = "svg")]
            {
                (info.width, info.height) = svg_dimensions(bytes)?;
                info.has_alpha = Some(true);
            }
            #[cfg(not(feature = "svg"))]
            return Err(ImageError::unsupported("SVG support is disabled"));
        }
    }
    Ok(info)
}

#[inline]
fn be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let data = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([data[0], data[1], data[2], data[3]]))
}

#[inline]
fn le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let data = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

#[inline]
fn le_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let data = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([data[0], data[1]]))
}

#[cfg(feature = "png")]
fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    if bytes.len() < 24 || bytes.get(12..16) != Some(b"IHDR") {
        return Err(ImageError::new("PNG is missing a complete IHDR header"));
    }
    let width = be_u32(bytes, 16).ok_or_else(|| ImageError::new("Truncated PNG width"))?;
    let height = be_u32(bytes, 20).ok_or_else(|| ImageError::new("Truncated PNG height"))?;
    Ok((width, height))
}

#[cfg(feature = "jpeg")]
fn jpeg_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    if bytes.len() < 4 || bytes[..2] != [0xff, 0xd8] {
        return Err(ImageError::new("Invalid JPEG SOI signature"));
    }
    let mut pos = 2usize;
    while pos < bytes.len() {
        while pos < bytes.len() && bytes[pos] != 0xff {
            pos += 1;
        }
        while pos < bytes.len() && bytes[pos] == 0xff {
            pos += 1;
        }
        let marker = *bytes
            .get(pos)
            .ok_or_else(|| ImageError::new("Truncated JPEG marker"))?;
        pos += 1;
        if marker == 0xd9 || marker == 0xda {
            break;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let length = u16::from_be_bytes([
            *bytes.get(pos).ok_or_else(|| ImageError::new("Truncated JPEG segment"))?,
            *bytes.get(pos + 1).ok_or_else(|| ImageError::new("Truncated JPEG segment"))?,
        ]) as usize;
        if length < 2 {
            return Err(ImageError::new("Invalid JPEG segment length"));
        }
        let segment_end = pos
            .checked_add(length)
            .ok_or_else(|| ImageError::new("JPEG segment length overflow"))?;
        if segment_end > bytes.len() {
            return Err(ImageError::new("Truncated JPEG segment"));
        }
        if matches!(
            marker,
            0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc5 | 0xc6 | 0xc7 | 0xc9 | 0xca | 0xcb | 0xcd | 0xce | 0xcf
        ) {
            if length < 7 {
                return Err(ImageError::new("Truncated JPEG frame header"));
            }
            let height = u16::from_be_bytes([bytes[pos + 3], bytes[pos + 4]]) as u32;
            let width = u16::from_be_bytes([bytes[pos + 5], bytes[pos + 6]]) as u32;
            return Ok((width, height));
        }
        pos = segment_end;
    }
    Err(ImageError::new("JPEG frame dimensions were not found"))
}

#[cfg(feature = "webp")]
fn webp_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    if bytes.len() < 20 {
        return Err(ImageError::new("WebP container is truncated"));
    }
    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let kind = &bytes[pos..pos + 4];
        let size = le_u32(bytes, pos + 4)
            .ok_or_else(|| ImageError::new("Truncated WebP chunk size"))? as usize;
        let data = pos + 8;
        let end = data
            .checked_add(size)
            .ok_or_else(|| ImageError::new("WebP chunk size overflow"))?;
        if end > bytes.len() {
            return Err(ImageError::new("Truncated WebP chunk"));
        }
        match kind {
            b"VP8X" if size >= 10 => {
                let w = 1
                    + bytes[data + 4] as u32
                    + ((bytes[data + 5] as u32) << 8)
                    + ((bytes[data + 6] as u32) << 16);
                let h = 1
                    + bytes[data + 7] as u32
                    + ((bytes[data + 8] as u32) << 8)
                    + ((bytes[data + 9] as u32) << 16);
                return Ok((w, h));
            }
            b"VP8L" if size >= 5 && bytes[data] == 0x2f => {
                let bits = u32::from_le_bytes([
                    bytes[data + 1],
                    bytes[data + 2],
                    bytes[data + 3],
                    bytes[data + 4],
                ]);
                return Ok(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1));
            }
            b"VP8 " if size >= 10 && bytes[data + 3..data + 6] == [0x9d, 0x01, 0x2a] => {
                let width = u16::from_le_bytes([bytes[data + 6], bytes[data + 7]]) & 0x3fff;
                let height = u16::from_le_bytes([bytes[data + 8], bytes[data + 9]]) & 0x3fff;
                return Ok((width as u32, height as u32));
            }
            _ => {}
        }
        pos = end + (size & 1);
    }
    Err(ImageError::new("WebP dimensions were not found"))
}

#[cfg(feature = "gif")]
fn gif_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    if bytes.len() < 10 {
        return Err(ImageError::new("Truncated GIF logical screen descriptor"));
    }
    Ok((
        u16::from_le_bytes([bytes[6], bytes[7]]) as u32,
        u16::from_le_bytes([bytes[8], bytes[9]]) as u32,
    ))
}

#[cfg(feature = "bmp")]
fn bmp_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    let dib_size = le_u32(bytes, 14).ok_or_else(|| ImageError::new("Truncated BMP DIB header"))?;
    if dib_size == 12 {
        let width = le_u16(bytes, 18).ok_or_else(|| ImageError::new("Truncated BMP width"))? as u32;
        let height = le_u16(bytes, 20).ok_or_else(|| ImageError::new("Truncated BMP height"))? as u32;
        return Ok((width, height));
    }
    let width = le_u32(bytes, 18).ok_or_else(|| ImageError::new("Truncated BMP width"))? as i32;
    let height = le_u32(bytes, 22).ok_or_else(|| ImageError::new("Truncated BMP height"))? as i32;
    if width <= 0 || height == 0 || height == i32::MIN {
        return Err(ImageError::new("Invalid BMP dimensions"));
    }
    Ok((width as u32, height.unsigned_abs()))
}

#[cfg(feature = "qoi")]
fn qoi_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    if bytes.len() < 14 {
        return Err(ImageError::new("Truncated QOI header"));
    }
    Ok((
        be_u32(bytes, 4).ok_or_else(|| ImageError::new("Truncated QOI width"))?,
        be_u32(bytes, 8).ok_or_else(|| ImageError::new("Truncated QOI height"))?,
    ))
}

#[cfg(feature = "ico")]
fn ico_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    if bytes.len() < 22 {
        return Err(ImageError::new("Truncated ICO directory"));
    }
    let count = le_u16(bytes, 4).ok_or_else(|| ImageError::new("Truncated ICO count"))?;
    if count == 0 {
        return Err(ImageError::new("ICO contains no images"));
    }
    let width = if bytes[6] == 0 { 256 } else { bytes[6] as u32 };
    let height = if bytes[7] == 0 { 256 } else { bytes[7] as u32 };
    Ok((width, height))
}

#[cfg(feature = "pgm")]
fn pgm_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    let mut pos = 0usize;
    let magic = next_pnm_token(bytes, &mut pos).ok_or_else(|| ImageError::new("Missing PGM magic"))?;
    if magic != b"P2" && magic != b"P5" {
        return Err(ImageError::new("Unsupported PGM magic"));
    }
    let width = parse_ascii_u32(next_pnm_token(bytes, &mut pos).ok_or_else(|| ImageError::new("Missing PGM width"))?)?;
    let height = parse_ascii_u32(next_pnm_token(bytes, &mut pos).ok_or_else(|| ImageError::new("Missing PGM height"))?)?;
    Ok((width, height))
}

#[cfg(feature = "tga")]
fn tga_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    if bytes.len() < 18 {
        return Err(ImageError::new("Truncated TGA header"));
    }
    Ok((
        u16::from_le_bytes([bytes[12], bytes[13]]) as u32,
        u16::from_le_bytes([bytes[14], bytes[15]]) as u32,
    ))
}

#[cfg(feature = "wbmp")]
fn wbmp_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    let (width, height, _) = parse_wbmp_header(bytes)?;
    Ok((width, height))
}

#[cfg(feature = "fits")]
fn fits_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    let mut pos = 0usize;
    let mut naxis = None::<u32>;
    let mut axis1 = None::<u32>;
    let mut axis2 = None::<u32>;
    while pos + 80 <= bytes.len() {
        let card = &bytes[pos..pos + 80];
        pos += 80;
        let key = trim_ascii(&card[..8]);
        if key == b"END" {
            break;
        }
        if card.get(8) != Some(&b'=') {
            continue;
        }
        let value = trim_ascii(card.get(10..30).unwrap_or(&[]));
        match key {
            b"NAXIS" => naxis = parse_ascii_u32(value).ok(),
            b"NAXIS1" => axis1 = parse_ascii_u32(value).ok(),
            b"NAXIS2" => axis2 = parse_ascii_u32(value).ok(),
            _ => {}
        }
    }
    if naxis.unwrap_or(0) < 2 {
        return Err(ImageError::new("FITS primary image has fewer than two axes"));
    }
    Ok((
        axis1.ok_or_else(|| ImageError::new("FITS NAXIS1 is missing"))?,
        axis2.ok_or_else(|| ImageError::new("FITS NAXIS2 is missing"))?,
    ))
}

#[cfg(feature = "tiff")]
fn tiff_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    if bytes.len() < 8 {
        return Err(ImageError::new("Truncated TIFF header"));
    }
    let little = match &bytes[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return Err(ImageError::new("Invalid TIFF byte order")),
    };
    let read16 = |offset: usize| -> Option<u16> {
        let s = bytes.get(offset..offset + 2)?;
        Some(if little {
            u16::from_le_bytes([s[0], s[1]])
        } else {
            u16::from_be_bytes([s[0], s[1]])
        })
    };
    let read32 = |offset: usize| -> Option<u32> {
        let s = bytes.get(offset..offset + 4)?;
        Some(if little {
            u32::from_le_bytes([s[0], s[1], s[2], s[3]])
        } else {
            u32::from_be_bytes([s[0], s[1], s[2], s[3]])
        })
    };
    let ifd = read32(4).ok_or_else(|| ImageError::new("Truncated TIFF IFD offset"))? as usize;
    let count = read16(ifd).ok_or_else(|| ImageError::new("Truncated TIFF IFD"))? as usize;
    let mut width = None;
    let mut height = None;
    for index in 0..count {
        let entry = ifd + 2 + index * 12;
        if entry + 12 > bytes.len() {
            return Err(ImageError::new("Truncated TIFF IFD entry"));
        }
        let tag = read16(entry).unwrap_or(0);
        let field_type = read16(entry + 2).unwrap_or(0);
        let count = read32(entry + 4).unwrap_or(0);
        if count != 1 || !matches!(tag, 256 | 257) {
            continue;
        }
        let value = match field_type {
            3 => read16(entry + 8).map(u32::from),
            4 => read32(entry + 8),
            _ => None,
        };
        if tag == 256 {
            width = value;
        } else {
            height = value;
        }
        if width.is_some() && height.is_some() {
            break;
        }
    }
    Ok((
        width.ok_or_else(|| ImageError::new("TIFF ImageWidth tag is missing"))?,
        height.ok_or_else(|| ImageError::new("TIFF ImageLength tag is missing"))?,
    ))
}

#[cfg(feature = "avif")]
fn avif_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    // Image Spatial Extents property. This intentionally avoids constructing a
    // complete ISO-BMFF tree for a header-only query.
    let mut i = 4usize;
    while i + 16 <= bytes.len() {
        if bytes.get(i..i + 4) == Some(b"ispe") {
            let box_start = i - 4;
            let size = be_u32(bytes, box_start).unwrap_or(0) as usize;
            if size >= 20 && box_start.saturating_add(size) <= bytes.len() {
                let width = be_u32(bytes, i + 8).unwrap_or(0);
                let height = be_u32(bytes, i + 12).unwrap_or(0);
                if width != 0 && height != 0 {
                    return Ok((width, height));
                }
            }
        }
        i += 1;
    }
    Err(ImageError::new("AVIF ispe dimensions were not found"))
}

#[cfg(feature = "svg")]
fn svg_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    let text = core::str::from_utf8(bytes).map_err(|_| ImageError::new("SVG is not valid UTF-8"))?;
    let start = text.find("<svg").ok_or_else(|| ImageError::new("SVG root element was not found"))?;
    let end = text[start..]
        .find('>')
        .map(|v| start + v)
        .ok_or_else(|| ImageError::new("SVG root element is truncated"))?;
    let tag = &text[start..=end];
    let width = parse_svg_dimension(tag, "width");
    let height = parse_svg_dimension(tag, "height");
    if let (Some(width), Some(height)) = (width, height) {
        return Ok((width, height));
    }
    if let Some(view_box) = attribute_value(tag, "viewBox") {
        let mut values = view_box.split_ascii_whitespace().filter_map(|v| v.parse::<f64>().ok());
        let _x = values.next();
        let _y = values.next();
        if let (Some(w), Some(h)) = (values.next(), values.next()) {
            if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 {
                return Ok((w.ceil() as u32, h.ceil() as u32));
            }
        }
    }
    Err(ImageError::new("SVG width/height could not be determined without rasterizing"))
}

#[cfg(feature = "svg")]
fn parse_svg_dimension(tag: &str, name: &str) -> Option<u32> {
    let value = attribute_value(tag, name)?;
    let numeric = value.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%');
    let parsed = numeric.trim().parse::<f64>().ok()?;
    (parsed.is_finite() && parsed > 0.0).then(|| parsed.ceil() as u32)
}

#[cfg(feature = "svg")]
fn attribute_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut cursor = 0usize;
    while let Some(found) = tag[cursor..].find(name) {
        let at = cursor + found;
        let before_ok = at == 0 || !tag.as_bytes()[at - 1].is_ascii_alphanumeric();
        let after = at + name.len();
        if before_ok && tag.get(after..)?.trim_start().starts_with('=') {
            let eq = after + tag[after..].find('=')? + 1;
            let rest = tag.get(eq..)?.trim_start();
            let quote = rest.as_bytes().first().copied()?;
            if quote == b'\'' || quote == b'"' {
                let inner = &rest[1..];
                let end = inner.find(quote as char)?;
                return Some(&inner[..end]);
            }
        }
        cursor = after;
    }
    None
}


#[cfg(feature = "png")]
fn png_info_details(bytes: &[u8]) -> Result<(Option<u32>, Option<bool>, bool), ImageError> {
    if bytes.len() < 29 || bytes.get(12..16) != Some(b"IHDR") {
        return Err(ImageError::new("PNG is missing a complete IHDR header"));
    }
    let color_type = bytes[25];
    let mut has_alpha = matches!(color_type, 4 | 6);
    let mut frame_count = Some(1u32);
    let mut animated = false;
    let mut pos = 8usize;
    while pos + 12 <= bytes.len() {
        let length = be_u32(bytes, pos).ok_or_else(|| ImageError::new("Truncated PNG chunk length"))? as usize;
        let data = pos.checked_add(8).ok_or_else(|| ImageError::new("PNG chunk offset overflow"))?;
        let end = data.checked_add(length).ok_or_else(|| ImageError::new("PNG chunk size overflow"))?;
        let crc_end = end.checked_add(4).ok_or_else(|| ImageError::new("PNG chunk CRC overflow"))?;
        if crc_end > bytes.len() {
            return Err(ImageError::new("Truncated PNG chunk"));
        }
        let kind = &bytes[pos + 4..pos + 8];
        if kind == b"tRNS" {
            has_alpha = true;
        } else if kind == b"acTL" {
            if length < 8 {
                return Err(ImageError::new("Truncated PNG acTL chunk"));
            }
            let count = be_u32(bytes, data).ok_or_else(|| ImageError::new("Truncated PNG animation frame count"))?;
            frame_count = Some(count);
            animated = count > 1;
        } else if kind == b"IEND" {
            break;
        }
        pos = crc_end;
    }
    Ok((frame_count, Some(has_alpha), animated))
}

#[cfg(feature = "webp")]
fn webp_info_details(bytes: &[u8]) -> Result<(Option<u32>, Option<bool>, bool), ImageError> {
    if bytes.len() < 12 || bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WEBP") {
        return Err(ImageError::new("Invalid WebP RIFF header"));
    }
    let mut pos = 12usize;
    let mut frames = 0u32;
    let mut animated = false;
    let mut has_alpha = None;
    while pos + 8 <= bytes.len() {
        let kind = &bytes[pos..pos + 4];
        let size = le_u32(bytes, pos + 4).ok_or_else(|| ImageError::new("Truncated WebP chunk size"))? as usize;
        let data = pos + 8;
        let end = data.checked_add(size).ok_or_else(|| ImageError::new("WebP chunk size overflow"))?;
        if end > bytes.len() {
            return Err(ImageError::new("Truncated WebP chunk"));
        }
        match kind {
            b"VP8X" if size >= 10 => {
                let flags = bytes[data];
                animated = flags & 0x02 != 0;
                has_alpha = Some(flags & 0x10 != 0);
            }
            b"ANMF" => {
                frames = frames.checked_add(1).ok_or_else(|| ImageError::new("WebP frame count overflow"))?;
            }
            b"ALPH" => has_alpha = Some(true),
            b"VP8 " if has_alpha.is_none() => has_alpha = Some(false),
            b"VP8L" if size >= 5 && bytes[data] == 0x2f => {
                let bits = u32::from_le_bytes([bytes[data + 1], bytes[data + 2], bytes[data + 3], bytes[data + 4]]);
                has_alpha = Some(((bits >> 28) & 1) != 0);
            }
            _ => {}
        }
        pos = end.checked_add(size & 1).ok_or_else(|| ImageError::new("WebP padding overflow"))?;
    }
    let frame_count = if animated { Some(frames) } else { Some(1) };
    Ok((frame_count, has_alpha, animated || frames > 1))
}

#[cfg(feature = "gif")]
fn gif_info_details(bytes: &[u8]) -> Result<(u32, bool), ImageError> {
    if bytes.len() < 13 {
        return Err(ImageError::new("Truncated GIF header"));
    }
    let packed = bytes[10];
    let mut pos = 13usize;
    if packed & 0x80 != 0 {
        let entries = 1usize << ((packed & 0x07) + 1);
        pos = pos.checked_add(entries * 3).ok_or_else(|| ImageError::new("GIF color table overflow"))?;
        if pos > bytes.len() {
            return Err(ImageError::new("Truncated GIF global color table"));
        }
    }
    let mut frames = 0u32;
    let mut has_alpha = false;
    loop {
        let marker = *bytes.get(pos).ok_or_else(|| ImageError::new("Truncated GIF block stream"))?;
        pos += 1;
        match marker {
            0x3b => break,
            0x21 => {
                let label = *bytes.get(pos).ok_or_else(|| ImageError::new("Truncated GIF extension"))?;
                pos += 1;
                if label == 0xf9 {
                    let size = *bytes.get(pos).ok_or_else(|| ImageError::new("Truncated GIF graphic control extension"))? as usize;
                    pos += 1;
                    if size != 4 || pos + 4 > bytes.len() {
                        return Err(ImageError::new("Invalid GIF graphic control extension"));
                    }
                    if bytes[pos] & 0x01 != 0 {
                        has_alpha = true;
                    }
                    pos += 4;
                    if bytes.get(pos) != Some(&0) {
                        return Err(ImageError::new("GIF graphic control extension is missing terminator"));
                    }
                    pos += 1;
                } else {
                    skip_gif_sub_blocks(bytes, &mut pos, true)?;
                }
            }
            0x2c => {
                frames = frames.checked_add(1).ok_or_else(|| ImageError::new("GIF frame count overflow"))?;
                if pos + 9 > bytes.len() {
                    return Err(ImageError::new("Truncated GIF image descriptor"));
                }
                let image_packed = bytes[pos + 8];
                pos += 9;
                if image_packed & 0x80 != 0 {
                    let entries = 1usize << ((image_packed & 0x07) + 1);
                    pos = pos.checked_add(entries * 3).ok_or_else(|| ImageError::new("GIF local color table overflow"))?;
                    if pos > bytes.len() {
                        return Err(ImageError::new("Truncated GIF local color table"));
                    }
                }
                pos = pos.checked_add(1).ok_or_else(|| ImageError::new("GIF LZW header overflow"))?;
                if pos > bytes.len() {
                    return Err(ImageError::new("Truncated GIF LZW code size"));
                }
                skip_gif_sub_blocks(bytes, &mut pos, false)?;
            }
            _ => return Err(ImageError::new("Unknown GIF block marker while reading image info")),
        }
    }
    Ok((frames, has_alpha))
}

#[cfg(feature = "gif")]
fn skip_gif_sub_blocks(bytes: &[u8], pos: &mut usize, extension_has_header: bool) -> Result<(), ImageError> {
    if extension_has_header {
        let first = *bytes.get(*pos).ok_or_else(|| ImageError::new("Truncated GIF extension header"))? as usize;
        *pos += 1;
        *pos = (*pos).checked_add(first).ok_or_else(|| ImageError::new("GIF extension overflow"))?;
        if *pos > bytes.len() {
            return Err(ImageError::new("Truncated GIF extension header payload"));
        }
    }
    loop {
        let size = *bytes.get(*pos).ok_or_else(|| ImageError::new("Truncated GIF sub-block"))? as usize;
        *pos += 1;
        if size == 0 {
            return Ok(());
        }
        *pos = (*pos).checked_add(size).ok_or_else(|| ImageError::new("GIF sub-block overflow"))?;
        if *pos > bytes.len() {
            return Err(ImageError::new("Truncated GIF sub-block payload"));
        }
    }
}

#[cfg(feature = "bmp")]
fn bmp_has_alpha(bytes: &[u8]) -> Option<bool> {
    let dib = le_u32(bytes, 14)?;
    if dib < 40 {
        return Some(false);
    }
    let bpp = le_u16(bytes, 28)?;
    if bpp < 32 { Some(false) } else { None }
}

#[cfg(feature = "qoi")]
fn qoi_has_alpha(bytes: &[u8]) -> Option<bool> {
    bytes.get(12).map(|channels| *channels == 4)
}

#[cfg(feature = "ico")]
fn ico_info_details(bytes: &[u8]) -> Result<(u32, u32, Vec<ImageSize>), ImageError> {
    if bytes.len() < 6 {
        return Err(ImageError::new("Truncated ICO directory"));
    }
    let count = le_u16(bytes, 4).ok_or_else(|| ImageError::new("Truncated ICO count"))? as usize;
    if count == 0 {
        return Err(ImageError::new("ICO contains no images"));
    }
    let directory_end = 6usize.checked_add(count.checked_mul(16).ok_or_else(|| ImageError::new("ICO directory size overflow"))?)
        .ok_or_else(|| ImageError::new("ICO directory size overflow"))?;
    if directory_end > bytes.len() {
        return Err(ImageError::new("Truncated ICO directory"));
    }
    let mut sizes = Vec::with_capacity(count);
    let mut best = ImageSize { width: 0, height: 0 };
    for index in 0..count {
        let at = 6 + index * 16;
        let size = ImageSize {
            width: if bytes[at] == 0 { 256 } else { bytes[at] as u32 },
            height: if bytes[at + 1] == 0 { 256 } else { bytes[at + 1] as u32 },
        };
        if (size.width as u64 * size.height as u64) > (best.width as u64 * best.height as u64) {
            best = size;
        }
        sizes.push(size);
    }
    Ok((best.width, best.height, sizes))
}

#[cfg(feature = "fits")]
fn fits_frame_count(bytes: &[u8]) -> Result<u32, ImageError> {
    let mut pos = 0usize;
    let mut naxis = None::<usize>;
    let mut axes = [1u64; 16];
    while pos + 80 <= bytes.len() {
        let card = &bytes[pos..pos + 80];
        pos += 80;
        let key = trim_ascii(&card[..8]);
        if key == b"END" { break; }
        if card.get(8) != Some(&b'=') { continue; }
        let value = trim_ascii(card.get(10..30).unwrap_or(&[]));
        if key == b"NAXIS" {
            naxis = Some(parse_ascii_u32(value)? as usize);
        } else if key.starts_with(b"NAXIS") && key.len() > 5 {
            if let Ok(index) = parse_ascii_u32(&key[5..]) {
                if index > 0 && (index as usize) <= axes.len() {
                    axes[index as usize - 1] = parse_ascii_u32(value)? as u64;
                }
            }
        }
    }
    let n = naxis.unwrap_or(0);
    if n < 2 { return Ok(0); }
    let mut frames = 1u64;
    for &axis in axes.iter().take(n).skip(2) {
        frames = frames.checked_mul(axis).ok_or_else(|| ImageError::new("FITS frame count overflow"))?;
    }
    u32::try_from(frames).map_err(|_| ImageError::new("FITS frame count exceeds u32"))
}

#[cfg(feature = "tiff")]
fn tiff_ifd_count(bytes: &[u8]) -> Result<u32, ImageError> {
    if bytes.len() < 8 { return Err(ImageError::new("Truncated TIFF header")); }
    let little = match &bytes[..2] { b"II" => true, b"MM" => false, _ => return Err(ImageError::new("Invalid TIFF byte order")) };
    let read16 = |offset: usize| -> Option<u16> {
        let s = bytes.get(offset..offset + 2)?;
        Some(if little { u16::from_le_bytes([s[0],s[1]]) } else { u16::from_be_bytes([s[0],s[1]]) })
    };
    let read32 = |offset: usize| -> Option<u32> {
        let s = bytes.get(offset..offset + 4)?;
        Some(if little { u32::from_le_bytes([s[0],s[1],s[2],s[3]]) } else { u32::from_be_bytes([s[0],s[1],s[2],s[3]]) })
    };
    let mut offset = read32(4).ok_or_else(|| ImageError::new("Truncated TIFF IFD offset"))? as usize;
    let mut count = 0u32;
    let mut guard = 0usize;
    while offset != 0 {
        guard += 1;
        if guard > 1_000_000 { return Err(ImageError::new("TIFF IFD chain is cyclic or excessive")); }
        let entries = read16(offset).ok_or_else(|| ImageError::new("Truncated TIFF IFD"))? as usize;
        let next_at = offset.checked_add(2).and_then(|v| v.checked_add(entries.checked_mul(12)?))
            .ok_or_else(|| ImageError::new("TIFF IFD size overflow"))?;
        offset = read32(next_at).ok_or_else(|| ImageError::new("Truncated TIFF next-IFD offset"))? as usize;
        count = count.checked_add(1).ok_or_else(|| ImageError::new("TIFF IFD count overflow"))?;
    }
    Ok(count)
}

#[cfg(feature = "tga")]
fn tga_has_alpha(bytes: &[u8]) -> Option<bool> {
    if bytes.len() < 18 { return None; }
    Some(bytes[16] == 32 || (bytes[17] & 0x0f) != 0)
}

#[cfg(feature = "avif")]
fn avif_is_animated(bytes: &[u8]) -> bool {
    if bytes.len() < 16 || bytes.get(4..8) != Some(b"ftyp") { return false; }
    if bytes.get(8..12) == Some(b"avis") { return true; }
    let size = be_u32(bytes, 0).unwrap_or(0) as usize;
    let end = if size == 0 { bytes.len() } else { size.min(bytes.len()) };
    end >= 16 && bytes[16..end].chunks_exact(4).any(|brand| brand == b"avis")
}

fn is_avif(bytes: &[u8]) -> bool {
    if bytes.len() < 16 || bytes.get(4..8) != Some(b"ftyp") {
        return false;
    }
    let size = be_u32(bytes, 0).unwrap_or(0) as usize;
    let end = if size == 0 { bytes.len() } else { size.min(bytes.len()) };
    if end < 16 {
        return false;
    }
    if matches!(bytes.get(8..12), Some(b"avif") | Some(b"avis")) {
        return true;
    }
    bytes[16..end]
        .chunks_exact(4)
        .any(|brand| brand == b"avif" || brand == b"avis")
}

fn is_svg(bytes: &[u8]) -> bool {
    let mut slice = bytes;
    if slice.starts_with(&[0xef, 0xbb, 0xbf]) {
        slice = &slice[3..];
    }
    while let Some((&first, rest)) = slice.split_first() {
        if first.is_ascii_whitespace() {
            slice = rest;
        } else {
            break;
        }
    }
    let probe = &slice[..slice.len().min(512)];
    if probe.starts_with(b"<svg") {
        return true;
    }
    probe
        .windows(4)
        .any(|window| window.eq_ignore_ascii_case(b"<svg"))
}

fn parse_wbmp_header(bytes: &[u8]) -> Result<(u32, u32, usize), ImageError> {
    if bytes.len() < 4 || bytes[0] != 0 || bytes[1] != 0 {
        return Err(ImageError::new("Invalid WBMP Type 0 header"));
    }
    let mut pos = 2usize;
    let width = read_mbint(bytes, &mut pos)?;
    let height = read_mbint(bytes, &mut pos)?;
    if width == 0 || height == 0 {
        return Err(ImageError::new("Invalid WBMP dimensions"));
    }
    Ok((width, height, pos))
}

fn looks_like_wbmp(bytes: &[u8]) -> bool {
    let Ok((width, height, pos)) = parse_wbmp_header(bytes) else {
        return false;
    };
    let row_bytes = (width as usize).div_ceil(8);
    row_bytes
        .checked_mul(height as usize)
        .and_then(|size| pos.checked_add(size))
        .is_some_and(|end| end == bytes.len())
}

fn read_mbint(bytes: &[u8], pos: &mut usize) -> Result<u32, ImageError> {
    let mut result = 0u32;
    for _ in 0..5 {
        let byte = *bytes.get(*pos).ok_or_else(|| ImageError::new("Truncated WBMP mbint"))?;
        *pos += 1;
        if result > (u32::MAX >> 7) {
            return Err(ImageError::new("WBMP mbint overflow"));
        }
        result = (result << 7) | u32::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err(ImageError::new("WBMP mbint is too long"))
}

fn next_pnm_token<'a>(bytes: &'a [u8], pos: &mut usize) -> Option<&'a [u8]> {
    loop {
        while bytes.get(*pos).is_some_and(u8::is_ascii_whitespace) {
            *pos += 1;
        }
        if bytes.get(*pos) == Some(&b'#') {
            while let Some(&value) = bytes.get(*pos) {
                *pos += 1;
                if value == b'\n' {
                    break;
                }
            }
            continue;
        }
        break;
    }
    let start = *pos;
    while bytes
        .get(*pos)
        .is_some_and(|value| !value.is_ascii_whitespace() && *value != b'#')
    {
        *pos += 1;
    }
    (start != *pos).then_some(&bytes[start..*pos])
}

fn parse_ascii_u32(bytes: &[u8]) -> Result<u32, ImageError> {
    if bytes.is_empty() {
        return Err(ImageError::new("Expected an unsigned integer"));
    }
    let mut value = 0u32;
    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return Err(ImageError::new("Invalid unsigned integer"));
        }
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u32::from(byte - b'0')))
            .ok_or_else(|| ImageError::new("Unsigned integer overflow"))?;
    }
    Ok(value)
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}
