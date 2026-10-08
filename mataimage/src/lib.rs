//! Fast image decoding with lightweight generic output and opt-in rich format APIs.
//!
//! The generic [`Image`] type intentionally contains only renderable data: one
//! canvas size, a primary pixel buffer, and additional full-canvas animation
//! frames. Format-specific metadata remains on types such as [`Png`] and [`Jpeg`].

// #![deny(missing_docs)]
// #![cfg_attr(all(feature = "simd", nightly, any(target_arch = "riscv32", target_arch = "riscv64")), feature(riscv_ext_intrinsics))]

/// Low-memory sequential animation decoding.
pub mod animated;
/// Contains the shared decoder trait and format-specific decoder implementations.
mod decoder;
/// Allocation-free format detection and header-only information parsing.
mod format;
/// Explicit borrowed metadata inspection, separate from pixel decoding.
pub mod metadata;
/// Decode policy and resource guards.
mod options;
/// Shared process-wide performance policy.
mod performance;
/// Reusable caller-owned decode working storage.
mod scratch;
/// Contains scalar primitives plus optional architecture-specific SIMD acceleration.
mod simd;
/// Defines the crate's image decoding error type.
pub mod error;
/// Provides packed pixel-format conversion helpers.
pub mod pixels;

pub use animated::{AnimationDecoder, AnimationFrameInfo};
pub use decoder::Decoder;
pub use error::{ImageError, ImageErrorKind};
pub use format::{ImageFormat, ImageInfo, ImageSize};
pub use options::{AnimationMode, DecodeLimits, DecodeOptions, MetadataMode, OutputPixelFormat};
pub use scratch::DecodeScratch;

#[cfg(feature = "bmp")]
pub use decoder::bmp::Bmp;
#[cfg(feature = "pgm")]
pub use decoder::pgm::Pgm;
#[cfg(feature = "tga")]
pub use decoder::tga::Tga;
#[cfg(feature = "png")]
pub use decoder::png::Png;
#[cfg(feature = "wbmp")]
pub use decoder::wbmp::Wbmp;
#[cfg(feature = "tiff")]
pub use decoder::tiff::{TiffImage, TiffPages};
#[cfg(feature = "jpeg")]
pub use decoder::jpeg::{Jpeg, JpegScale};
#[cfg(feature = "qoi")]
pub use decoder::qoi::Qoi;
#[cfg(feature = "webp")]
pub use decoder::webp::WebP;
#[cfg(feature = "avif")]
pub use decoder::avif::Av1 as Avif;
#[cfg(feature = "gif")]
pub use decoder::gif::Gif;
#[cfg(feature = "fits")]
pub use decoder::fits::{Fits, FitsHduInfo, FitsReader};
#[cfg(feature = "ico")]
pub use decoder::ico::{IconSet, IcoEntryInfo, IcoFrame, IcoReader, Ico};
#[cfg(feature = "svg")]
pub use decoder::svg::Svg;

use std::io::{Read, Seek};

/// Lightweight decoded image data.
///
/// Pixels use the logical packed representation `0xRRGGBBAA` on every host.
/// The numeric value is endianness independent; callers should not assume that
/// a `&[u32]` can be reinterpreted as an in-memory RGBA byte slice.
///
/// `pixels` stores the primary/still/first animation frame. `frames` contains
/// only additional full-canvas frames, so still images do not allocate a
/// one-element outer frame vector and animation frames do not repeat dimensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Primary frame pixels in logical `0xRRGGBBAA` order.
    pub pixels: Vec<u32>,
    /// Additional full-canvas frames in display order.
    pub frames: Vec<ImageFrame>,
}

impl Image {
    /// Loads and decodes an image from a filesystem path with default options.
    pub fn load(path: &str) -> Result<Self, ImageError> {
        Self::load_with_options(path, DecodeOptions::default())
    }

    /// Loads an image and explicitly materializes every animation frame.
    pub fn load_animated(path: &str) -> Result<Self, ImageError> {
        let mut options = DecodeOptions::default();
        options.animation = AnimationMode::AllFrames;
        Self::load_with_options(path, options)
    }

    /// Loads and decodes an image from a filesystem path using explicit options.
    pub fn load_with_options(path: &str, options: DecodeOptions) -> Result<Self, ImageError> {
        let mut file = std::fs::File::open(path)
            .map_err(|error| ImageError::io(format!("Failed to open image file: {error}")))?;
        // `from_reader_with_options` recognizes PNG from the prefix and keeps
        // IDAT streaming all the way into the inflater. Other codecs retain the
        // compatibility adapter until they grow native streaming parsers.
        Self::from_reader_with_options(&mut file, options)
    }

    /// Decodes an image from an already-borrowed encoded byte slice.
    ///
    /// This is the baseline zero-copy input path: the dispatcher itself never
    /// duplicates the encoded buffer.
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ImageError> {
        Self::from_bytes_with_options(bytes, DecodeOptions::default())
    }

    /// Decodes an image and explicitly materializes every animation frame.
    pub fn from_bytes_animated(bytes: &[u8]) -> Result<Self, ImageError> {
        let mut options = DecodeOptions::default();
        options.animation = AnimationMode::AllFrames;
        Self::from_bytes_with_options(bytes, options)
    }

    /// Decodes pixels and explicitly parses borrowed metadata in one call.
    /// The lightweight `Image` itself remains metadata-free; callers pay for the
    /// separate metadata block index only when they request it.
    pub fn from_bytes_and_metadata<'a>(
        bytes: &'a [u8],
        mode: MetadataMode,
    ) -> Result<(Self, metadata::ImageMetadata<'a>), ImageError> {
        let image = Self::from_bytes(bytes)?;
        let metadata = metadata::ImageMetadata::read(bytes, mode)?;
        Ok((image, metadata))
    }

    /// Decodes an image from a borrowed encoded byte slice using explicit options.
    pub fn from_bytes_with_options(
        bytes: &[u8],
        options: DecodeOptions,
    ) -> Result<Self, ImageError> {
        let format = Self::format(bytes)?;
        let mut image = match format {
            ImageFormat::Png => {
                #[cfg(feature = "png")]
                {
                    decoder::png::decode_image(bytes, &options)?
                }
                #[cfg(not(feature = "png"))]
                {
                    return Err(ImageError::unsupported("png is feature gated out"));
                }
            }
            ImageFormat::Jpeg => {
                #[cfg(feature = "jpeg")]
                {
                    decoder::jpeg::decode_image(bytes, &options)?
                }
                #[cfg(not(feature = "jpeg"))]
                {
                    return Err(ImageError::unsupported("jpeg is feature gated out"));
                }
            }
            ImageFormat::WebP => {
                #[cfg(feature = "webp")]
                {
                    decoder::webp::decode_image(bytes, &options)?
                }
                #[cfg(not(feature = "webp"))]
                {
                    return Err(ImageError::unsupported("webp is feature gated out"));
                }
            }
            ImageFormat::Gif => {
                #[cfg(feature = "gif")]
                {
                    decoder::gif::decode_image(bytes, &options)?
                }
                #[cfg(not(feature = "gif"))]
                {
                    return Err(ImageError::unsupported("gif is feature gated out"));
                }
            }
            ImageFormat::Bmp => {
                #[cfg(feature = "bmp")]
                {
                    decoder::bmp::decode_image(bytes, &options)?
                }
                #[cfg(not(feature = "bmp"))]
                {
                    return Err(ImageError::unsupported("bmp is feature gated out"));
                }
            }
            ImageFormat::Fits => {
                #[cfg(feature = "fits")]
                {
                    decoder::fits::decode_image(bytes, &options)?
                }
                #[cfg(not(feature = "fits"))]
                {
                    return Err(ImageError::unsupported("fits is feature gated out"));
                }
            }
            ImageFormat::Tiff => {
                #[cfg(feature = "tiff")]
                {
                    decoder::tiff::decode_image(bytes, &options)?
                }
                #[cfg(not(feature = "tiff"))]
                {
                    return Err(ImageError::unsupported("tiff is feature gated out"));
                }
            }
            ImageFormat::Qoi => {
                #[cfg(feature = "qoi")]
                {
                    decoder::qoi::decode_image(bytes, &options)?
                }
                #[cfg(not(feature = "qoi"))]
                {
                    return Err(ImageError::unsupported("qoi is feature gated out"));
                }
            }
            ImageFormat::Ico => {
                #[cfg(feature = "ico")]
                {
                    decoder::ico::decode_image(bytes, &options)?
                }
                #[cfg(not(feature = "ico"))]
                {
                    return Err(ImageError::unsupported("ico is feature gated out"));
                }
            }
            ImageFormat::Avif => {
                #[cfg(feature = "avif")]
                {
                    decoder::avif::decode_image(bytes, &options)?
                }
                #[cfg(not(feature = "avif"))]
                {
                    return Err(ImageError::unsupported("avif is feature gated out"));
                }
            }
            ImageFormat::Pgm => {
                #[cfg(feature = "pgm")]
                {
                    decoder::pgm::decode_image(bytes, &options)?
                }
                #[cfg(not(feature = "pgm"))]
                {
                    return Err(ImageError::unsupported("pgm is feature gated out"));
                }
            }
            ImageFormat::Tga => {
                #[cfg(feature = "tga")]
                {
                    decoder::tga::decode_image(bytes, &options)?
                }
                #[cfg(not(feature = "tga"))]
                {
                    return Err(ImageError::unsupported("tga is feature gated out"));
                }
            }
            ImageFormat::Wbmp => {
                #[cfg(feature = "wbmp")]
                {
                    decoder::wbmp::decode_image(bytes, &options)?
                }
                #[cfg(not(feature = "wbmp"))]
                {
                    return Err(ImageError::unsupported("wbmp is feature gated out"));
                }
            }
            ImageFormat::Svg => {
                #[cfg(feature = "svg")]
                {
                    decoder::svg::decode_image(bytes, &options)?
                }
                #[cfg(not(feature = "svg"))]
                {
                    return Err(ImageError::unsupported("svg is feature gated out"));
                }
            }
        };

        if let Some((width, height)) = options.output_dimensions {
            options.checked_rgba_bytes(width, height)?;
            if image.width != width || image.height != height {
                image.rescale(width, height);
            }
        }
        Ok(image)
    }

    /// Decodes an image while reusing caller-owned codec scratch storage.
    ///
    /// PNG currently reuses its DEFLATE history, streaming chunk, and previous
    /// scanline across calls. Other formats transparently use their normal
    /// lightweight decode path until they have profitable cross-image scratch.
    pub fn from_bytes_with_scratch(
        bytes: &[u8],
        options: DecodeOptions,
        scratch: &mut DecodeScratch,
    ) -> Result<Self, ImageError> {
        let format = Self::format(bytes)?;
        if format == ImageFormat::Png {
            #[cfg(feature = "png")]
            {
                let mut image = decoder::png::decode_image_with_scratch(bytes, &options, scratch)?;
                if let Some((width, height)) = options.output_dimensions {
                    options.checked_rgba_bytes(width, height)?;
                    if image.width != width || image.height != height {
                        image.rescale(width, height);
                    }
                }
                return Ok(image);
            }
            #[cfg(not(feature = "png"))]
            {
                return Err(ImageError::unsupported("png is feature gated out"));
            }
        }
        Self::from_bytes_with_options(bytes, options)
    }

    /// Decodes from a stream. PNG uses a true bounded-memory IDAT path; formats
    /// without a native streaming parser fall back to one owned encoded buffer.
    pub fn from_reader<R: Read>(reader: &mut R) -> Result<Self, ImageError> {
        Self::from_reader_with_options(reader, DecodeOptions::default())
    }

    /// Stream adapter with explicit decode options.
    pub fn from_reader_with_options<R: Read>(
        reader: &mut R,
        options: DecodeOptions,
    ) -> Result<Self, ImageError> {
        let mut prefix = [0u8; 8];
        let mut prefix_len = 0usize;
        while prefix_len < prefix.len() {
            let count = reader.read(&mut prefix[prefix_len..])
                .map_err(|error| ImageError::io(format!("Failed to read image stream: {error}")))?;
            if count == 0 {
                break;
            }
            prefix_len += count;
        }

        #[cfg(feature = "png")]
        if prefix_len == 8 && &prefix == b"\x89PNG\r\n\x1a\n" {
            let mut chained = std::io::Cursor::new(prefix).chain(reader);
            let mut image = decoder::png::decode_reader(&mut chained, &options)?;
            if let Some((width, height)) = options.output_dimensions {
                options.checked_rgba_bytes(width, height)?;
                if image.width != width || image.height != height {
                    image.rescale(width, height);
                }
            }
            return Ok(image);
        }

        let mut bytes = Vec::with_capacity(prefix_len.saturating_add(8192));
        bytes.extend_from_slice(&prefix[..prefix_len]);
        reader.read_to_end(&mut bytes)
            .map_err(|error| ImageError::io(format!("Failed to read image stream: {error}")))?;
        Self::from_bytes_with_options(&bytes, options)
    }

    /// Seek-capable stream adapter retained for formats/callers that already use
    /// `Read + Seek`. The generic decoder does not seek unnecessarily.
    #[inline]
    pub fn from_reader_seek<R: Read + Seek>(reader: &mut R) -> Result<Self, ImageError> {
        Self::from_reader(reader)
    }

    /// Detects an encoded image's format without decoding or allocating.
    #[inline]
    pub fn format(bytes: &[u8]) -> Result<ImageFormat, ImageError> {
        ImageFormat::from_signature(bytes)
            .ok_or_else(|| ImageError::unsupported("Unsupported image format"))
    }

    /// Reads image dimensions from headers without decoding pixel data.
    #[inline]
    pub fn dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
        let info = ImageInfo::read(bytes)?;
        Ok((info.width, info.height))
    }

    /// Parses header/container information without decoding compressed pixels.
    #[inline]
    pub fn info(bytes: &[u8]) -> Result<ImageInfo, ImageError> {
        ImageInfo::read(bytes)
    }

    /// Decodes the primary display frame into a caller-owned packed-pixel buffer.
    ///
    /// The destination length is validated from header information before codec
    /// work begins. Individual codecs may still use internal temporary buffers,
    /// but callers can reuse one persistent texture/staging allocation.
    pub fn decode_into(bytes: &[u8], destination: &mut [u32]) -> Result<ImageInfo, ImageError> {
        Self::decode_into_with_options(bytes, destination, DecodeOptions::default())
    }

    /// Decodes the primary display frame into caller-owned storage using explicit
    /// options. Animation is forced to first-frame because one destination slice
    /// represents one canvas.
    pub fn decode_into_with_options(
        bytes: &[u8],
        destination: &mut [u32],
        mut options: DecodeOptions,
    ) -> Result<ImageInfo, ImageError> {
        let info = ImageInfo::read(bytes)?;
        let expected = (info.width as usize)
            .checked_mul(info.height as usize)
            .ok_or_else(|| ImageError::new("Image dimensions overflow destination size"))?;
        if destination.len() != expected {
            return Err(ImageError::buffer_size(format!(
                "Destination has {} pixels but {}x{} requires {expected}",
                destination.len(), info.width, info.height
            )));
        }
        options.animation = AnimationMode::FirstFrame;
        options.output_dimensions = None;
        let image = Self::from_bytes_with_options(bytes, options)?;
        if image.pixels.len() != destination.len() {
            return Err(ImageError::new("Decoder output does not match header dimensions"));
        }
        destination.copy_from_slice(&image.pixels);
        Ok(info)
    }

    /// Decodes the primary frame into caller-owned pixels while also reusing
    /// caller-owned codec scratch across repeated decodes.
    pub fn decode_into_with_scratch(
        bytes: &[u8],
        destination: &mut [u32],
        mut options: DecodeOptions,
        scratch: &mut DecodeScratch,
    ) -> Result<ImageInfo, ImageError> {
        let info = ImageInfo::read(bytes)?;
        let expected = (info.width as usize)
            .checked_mul(info.height as usize)
            .ok_or_else(|| ImageError::new("Image dimensions overflow destination size"))?;
        if destination.len() != expected {
            return Err(ImageError::buffer_size(format!(
                "Destination has {} pixels but {}x{} requires {expected}",
                destination.len(), info.width, info.height
            )));
        }
        options.animation = AnimationMode::FirstFrame;
        options.output_dimensions = None;
        let image = Self::from_bytes_with_scratch(bytes, options, scratch)?;
        if image.pixels.len() != destination.len() {
            return Err(ImageError::new("Decoder output does not match header dimensions"));
        }
        destination.copy_from_slice(&image.pixels);
        Ok(info)
    }

    /// Borrows the primary frame's packed pixels.
    #[inline]
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    /// Mutably borrows the primary frame's packed pixels.
    #[inline]
    pub fn pixels_mut(&mut self) -> &mut [u32] {
        &mut self.pixels
    }

    /// Moves the primary frame's packed pixels out of the image.
    #[inline]
    pub fn into_pixels(self) -> Vec<u32> {
        self.pixels
    }

    /// Borrows the additional full-canvas animation frames after the primary frame.
    #[inline]
    pub fn frames(&self) -> &[ImageFrame] {
        &self.frames
    }

    /// Iterates the primary frame followed by additional full-canvas frames as
    /// borrowed pixel slices, without allocating a temporary frame collection.
    pub fn frame_iter(&self) -> impl ExactSizeIterator<Item = &[u32]> + '_ {
        FrameIter {
            primary: Some(self.pixels.as_slice()),
            frames: self.frames.iter(),
        }
    }

    /// Mutably borrows the additional full-canvas animation frames.
    #[inline]
    pub fn frames_mut(&mut self) -> &mut [ImageFrame] {
        &mut self.frames
    }

    /// Number of decoded display frames. A successfully decoded image always has
    /// a primary frame, so this is one plus the number of additional frames.
    #[inline]
    pub fn frame_count(&self) -> usize {
        1 + self.frames.len()
    }

    /// Moves every decoded frame into a flat vector without cloning pixel buffers.
    pub fn into_frames(self) -> Vec<Vec<u32>> {
        self.into_frame_iter().collect()
    }

    /// Consumes the image and yields owned frame buffers one at a time.
    /// This avoids a second outer `Vec` when a consumer can process frames as a
    /// stream of owned allocations.
    pub fn into_frame_iter(self) -> impl ExactSizeIterator<Item = Vec<u32>> {
        IntoFrameIter {
            primary: Some(self.pixels),
            frames: self.frames.into_iter(),
        }
    }

    /// Rescales the primary and every additional full-canvas frame using a
    /// nearest-neighbour path that performs no division in the per-pixel loop.
    pub fn rescale(&mut self, new_width: u32, new_height: u32) {
        if self.width == new_width && self.height == new_height {
            return;
        }

        let source_width = self.width;
        let source_height = self.height;

        // A pure shrink can safely overwrite the beginning of each source
        // buffer because nearest-neighbour source indices never precede their
        // destination indices. This removes a full image-sized allocation for
        // the common thumbnail/downscale case.
        if new_width <= source_width && new_height <= source_height {
            rescale_pixels_in_place(
                &mut self.pixels,
                source_width,
                source_height,
                new_width,
                new_height,
            );
            for frame in &mut self.frames {
                rescale_pixels_in_place(
                    &mut frame.pixels,
                    source_width,
                    source_height,
                    new_width,
                    new_height,
                );
            }
            self.width = new_width;
            self.height = new_height;
            self.debug_assert_invariants();
            return;
        }

        let x_map = if rescale_needs_x_map(source_width, source_height, new_width, new_height) {
            build_nearest_x_map(source_width, new_width)
        } else {
            Vec::new()
        };
        self.pixels = rescale_pixels(
            &self.pixels,
            source_width,
            source_height,
            new_width,
            new_height,
            &x_map,
        );
        for frame in &mut self.frames {
            frame.pixels = rescale_pixels(
                &frame.pixels,
                source_width,
                source_height,
                new_width,
                new_height,
                &x_map,
            );
        }
        self.width = new_width;
        self.height = new_height;
        self.debug_assert_invariants();
    }

    /// Rescales into caller-owned storage, reusing destination vector capacity
    /// across repeated operations when possible.
    pub fn rescale_into(&self, destination: &mut Image, new_width: u32, new_height: u32) {
        let x_map = if rescale_needs_x_map(self.width, self.height, new_width, new_height) {
            build_nearest_x_map(self.width, new_width)
        } else {
            Vec::new()
        };
        rescale_pixels_into(
            &self.pixels,
            self.width,
            self.height,
            new_width,
            new_height,
            &x_map,
            &mut destination.pixels,
        );

        if destination.frames.len() > self.frames.len() {
            destination.frames.truncate(self.frames.len());
        }
        while destination.frames.len() < self.frames.len() {
            destination.frames.push(ImageFrame { pixels: Vec::new() });
        }
        for (source, target) in self.frames.iter().zip(&mut destination.frames) {
            rescale_pixels_into(
                &source.pixels,
                self.width,
                self.height,
                new_width,
                new_height,
                &x_map,
                &mut target.pixels,
            );
        }
        destination.width = new_width;
        destination.height = new_height;
        destination.debug_assert_invariants();
    }

    /// Constructs a checked lightweight image from one primary pixel buffer.
    pub(crate) fn from_packed_pixels(
        width: u32,
        height: u32,
        pixels: Vec<u32>,
        options: &DecodeOptions,
    ) -> Result<Self, ImageError> {
        let expected = options.checked_pixel_count(width, height)?;
        options.checked_rgba_bytes(width, height)?;
        if pixels.len() != expected {
            return Err(ImageError::new(format!(
                "Decoded pixel count {} does not match {width}x{height} ({expected})",
                pixels.len()
            )));
        }
        let image = Self {
            width,
            height,
            pixels,
            frames: Vec::new(),
        };
        image.debug_assert_invariants();
        Ok(image)
    }

    /// Constructs a checked image from full-canvas frame buffers, moving every
    /// buffer and keeping the first frame outside the additional-frame vector.
    pub(crate) fn from_frame_pixels(
        width: u32,
        height: u32,
        frame_pixels: Vec<Vec<u32>>,
        options: &DecodeOptions,
    ) -> Result<Self, ImageError> {
        let expected = options.checked_pixel_count(width, height)?;
        options.checked_rgba_bytes(width, height)?;
        if frame_pixels.is_empty() {
            return Err(ImageError::new("Decoder produced no image frames"));
        }
        options.check_frame_count(frame_pixels.len())?;

        let keep_all = matches!(options.animation, AnimationMode::AllFrames);
        let mut iter = frame_pixels.into_iter();
        let pixels = iter.next().expect("non-empty frame vector checked above");
        if pixels.len() != expected {
            return Err(ImageError::new("Primary frame pixel count does not match canvas dimensions"));
        }

        let mut frames = if keep_all {
            Vec::with_capacity(iter.size_hint().0)
        } else {
            Vec::new()
        };
        if keep_all {
            for pixels in iter {
                if pixels.len() != expected {
                    return Err(ImageError::new(
                        "Animation frame pixel count does not match canvas dimensions",
                    ));
                }
                frames.push(ImageFrame { pixels });
            }
        }

        let image = Self {
            width,
            height,
            pixels,
            frames,
        };
        image.debug_assert_invariants();
        Ok(image)
    }

    /// Debug/test-only invariant validation for the compact full-canvas frame
    /// representation. This compiles to nothing in optimized non-debug builds.
    #[inline]
    fn debug_assert_invariants(&self) {
        if cfg!(debug_assertions) {
            let expected = (self.width as usize)
                .checked_mul(self.height as usize)
                .expect("Image dimensions overflow usize");
            debug_assert_eq!(self.pixels.len(), expected, "primary frame size mismatch");
            for (index, frame) in self.frames.iter().enumerate() {
                debug_assert_eq!(
                    frame.pixels.len(),
                    expected,
                    "animation frame {index} size mismatch",
                );
            }
        }
    }
}

/// One additional full-canvas animation frame.
///
/// Width and height live once on [`Image`], avoiding repeated dimensions for
/// animations whose composited display frames all share one canvas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFrame {
    /// Packed `0xRRGGBBAA` pixels.
    pub pixels: Vec<u32>,
}

impl ImageFrame {
    /// Borrows this full-canvas frame's pixels.
    #[inline]
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    /// Moves this frame's pixels out without cloning.
    #[inline]
    pub fn into_pixels(self) -> Vec<u32> {
        self.pixels
    }
}

/// Zero-allocation iterator over the primary frame followed by additional frames.
struct FrameIter<'a> {
    primary: Option<&'a [u32]>,
    frames: std::slice::Iter<'a, ImageFrame>,
}

impl<'a> Iterator for FrameIter<'a> {
    type Item = &'a [u32];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.primary
            .take()
            .or_else(|| self.frames.next().map(|frame| frame.pixels.as_slice()))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl DoubleEndedIterator for FrameIter<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.frames
            .next_back()
            .map(|frame| frame.pixels.as_slice())
            .or_else(|| self.primary.take())
    }
}

impl ExactSizeIterator for FrameIter<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.frames.len() + self.primary.is_some() as usize
    }
}

impl std::iter::FusedIterator for FrameIter<'_> {}

/// Zero-allocation consuming iterator over the primary frame followed by additional frames.
struct IntoFrameIter {
    primary: Option<Vec<u32>>,
    frames: std::vec::IntoIter<ImageFrame>,
}

impl Iterator for IntoFrameIter {
    type Item = Vec<u32>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.primary
            .take()
            .or_else(|| self.frames.next().map(|frame| frame.pixels))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl DoubleEndedIterator for IntoFrameIter {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.frames
            .next_back()
            .map(|frame| frame.pixels)
            .or_else(|| self.primary.take())
    }
}

impl ExactSizeIterator for IntoFrameIter {
    #[inline]
    fn len(&self) -> usize {
        self.frames.len() + self.primary.is_some() as usize
    }
}

impl std::iter::FusedIterator for IntoFrameIter {}



#[inline]
fn rescale_needs_x_map(
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
) -> bool {
    if source_width == 0 || source_height == 0 || destination_width == 0 || destination_height == 0 {
        return false;
    }
    if source_width == destination_width && source_height == destination_height {
        return false;
    }
    let exact_up = destination_width >= source_width
        && destination_height >= source_height
        && destination_width % source_width == 0
        && destination_height % source_height == 0;
    let exact_down = source_width >= destination_width
        && source_height >= destination_height
        && source_width % destination_width == 0
        && source_height % destination_height == 0;
    !(exact_up || exact_down)
}

fn build_nearest_x_map(source_width: u32, destination_width: u32) -> Vec<usize> {
    if source_width == 0 || destination_width == 0 {
        return Vec::new();
    }

    let sw = source_width as usize;
    let dw = destination_width as usize;
    let quotient = sw / dw;
    let remainder_step = sw % dw;
    let mut source_x = 0usize;
    let mut remainder = 0usize;
    let mut map = Vec::with_capacity(dw);

    // Exact Bresenham-style evaluation of floor(x * sw / dw). Unlike a
    // truncated fixed-point step, this cannot drift one pixel low when the
    // rational mapping lands exactly on an integer boundary.
    for _ in 0..dw {
        map.push(source_x.min(sw - 1));
        source_x += quotient;
        remainder += remainder_step;
        if remainder >= dw {
            source_x += 1;
            remainder -= dw;
        }
    }
    map
}

fn rescale_pixels_in_place(
    pixels: &mut Vec<u32>,
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
) {
    if destination_width == 0 || destination_height == 0 {
        pixels.clear();
        return;
    }
    if source_width == 0 || source_height == 0 {
        pixels.clear();
        return;
    }
    let Some(source_len) = (source_width as usize).checked_mul(source_height as usize) else {
        pixels.clear();
        return;
    };
    let Some(destination_len) = (destination_width as usize).checked_mul(destination_height as usize) else {
        pixels.clear();
        return;
    };
    if pixels.len() < source_len {
        pixels.clear();
        return;
    }

    debug_assert!(destination_width <= source_width);
    debug_assert!(destination_height <= source_height);

    let sw = source_width as usize;
    let dw = destination_width as usize;

    // Exact integer downscales avoid both coordinate division and mapping
    // tables: nearest-neighbour simply selects every Nth source pixel/row.
    if source_width % destination_width == 0 && source_height % destination_height == 0 {
        let x_step = (source_width / destination_width) as usize;
        let y_step = (source_height / destination_height) as usize;
        for destination_y in 0..destination_height as usize {
            let source_y = destination_y * y_step;
            for destination_x in 0..dw {
                let source_x = destination_x * x_step;
                let value = pixels[source_y * sw + source_x];
                pixels[destination_y * dw + destination_x] = value;
            }
        }
        pixels.truncate(destination_len);
        return;
    }

    let sh = source_height as usize;
    let dh = destination_height as usize;
    let x_quotient = sw / dw;
    let x_remainder_step = sw % dw;
    let y_quotient = sh / dh;
    let y_remainder_step = sh % dh;
    let mut source_y = 0usize;
    let mut y_remainder = 0usize;

    for destination_y in 0..dh {
        let mut source_x = 0usize;
        let mut x_remainder = 0usize;
        for destination_x in 0..dw {
            let value = pixels[source_y * sw + source_x];
            pixels[destination_y * dw + destination_x] = value;
            source_x += x_quotient;
            x_remainder += x_remainder_step;
            if x_remainder >= dw {
                source_x += 1;
                x_remainder -= dw;
            }
        }
        source_y += y_quotient;
        y_remainder += y_remainder_step;
        if y_remainder >= dh {
            source_y += 1;
            y_remainder -= dh;
        }
    }
    pixels.truncate(destination_len);
}

fn rescale_pixels(
    source: &[u32],
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
    x_map: &[usize],
) -> Vec<u32> {
    let mut destination = Vec::new();
    rescale_pixels_into(
        source,
        source_width,
        source_height,
        destination_width,
        destination_height,
        x_map,
        &mut destination,
    );
    destination
}

fn rescale_pixels_into(
    source: &[u32],
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
    x_map: &[usize],
    destination: &mut Vec<u32>,
) {
    if destination_width == 0 || destination_height == 0 {
        destination.clear();
        return;
    }
    let Some(source_len) = (source_width as usize).checked_mul(source_height as usize) else {
        destination.clear();
        return;
    };
    if source_width == 0 || source_height == 0 || source.len() < source_len {
        destination.clear();
        return;
    }
    let Some(destination_len) = (destination_width as usize).checked_mul(destination_height as usize) else {
        destination.clear();
        return;
    };

    destination.clear();
    destination.resize(destination_len, 0);

    if source_width == destination_width && source_height == destination_height {
        destination.copy_from_slice(&source[..source_len]);
        return;
    }

    // Exact integer upscales can replace coordinate mapping with repeated runs,
    // then duplicate completed rows with copy_from_slice.
    if destination_width >= source_width
        && destination_height >= source_height
        && destination_width % source_width == 0
        && destination_height % source_height == 0
    {
        let x_scale = (destination_width / source_width) as usize;
        let y_scale = (destination_height / source_height) as usize;
        let sw = source_width as usize;
        let dw = destination_width as usize;
        for source_y in 0..source_height as usize {
            let first_dst_y = source_y * y_scale;
            let row = &mut destination[first_dst_y * dw..(first_dst_y + 1) * dw];
            for (x, &pixel) in source[source_y * sw..source_y * sw + sw].iter().enumerate() {
                row[x * x_scale..(x + 1) * x_scale].fill(pixel);
            }
            for repeat in 1..y_scale {
                let (head, tail) = destination.split_at_mut((first_dst_y + repeat) * dw);
                let source_row = &head[first_dst_y * dw..(first_dst_y + 1) * dw];
                tail[..dw].copy_from_slice(source_row);
            }
        }
        return;
    }

    // Exact integer downscales select every Nth source row/pixel. This avoids
    // the coordinate map and all fixed-point arithmetic.
    if source_width >= destination_width
        && source_height >= destination_height
        && source_width % destination_width == 0
        && source_height % destination_height == 0
    {
        let x_step = (source_width / destination_width) as usize;
        let y_step = (source_height / destination_height) as usize;
        let sw = source_width as usize;
        let dw = destination_width as usize;
        for destination_y in 0..destination_height as usize {
            let source_row = &source[(destination_y * y_step) * sw..][..sw];
            let destination_row = &mut destination[destination_y * dw..][..dw];
            for (destination_x, pixel) in destination_row.iter_mut().enumerate() {
                *pixel = source_row[destination_x * x_step];
            }
        }
        return;
    }

    let workers = crate::performance::worker_count_for(
        crate::performance::WorkKind::Rescale,
        None,
        destination_len,
        destination_height as usize,
    );
    if workers > 1 {
        let dw = destination_width as usize;
        let rows_per_worker = (destination_height as usize).div_ceil(workers);
        let pixels_per_worker = rows_per_worker.saturating_mul(dw).max(dw);
        std::thread::scope(|scope| {
            for (worker, output) in destination.chunks_mut(pixels_per_worker).enumerate() {
                let start_y = worker * rows_per_worker;
                scope.spawn(move || {
                    fill_nearest_rows(
                        source,
                        source_width,
                        source_height,
                        destination_width,
                        destination_height,
                        x_map,
                        start_y,
                        output,
                    );
                });
            }
        });
        return;
    }

    fill_nearest_rows(
        source,
        source_width,
        source_height,
        destination_width,
        destination_height,
        x_map,
        0,
        destination,
    );
}

fn fill_nearest_rows(
    source: &[u32],
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
    x_map: &[usize],
    start_y: usize,
    destination: &mut [u32],
) {
    if destination.is_empty() || destination_width == 0 || destination_height == 0 {
        return;
    }

    let sw = source_width as usize;
    let sh = source_height as usize;
    let dw = destination_width as usize;
    let dh = destination_height as usize;
    let y_quotient = sh / dh;
    let y_remainder_step = sh % dh;

    // Initialize the exact rational state for this worker's first row. The
    // multiplication is bounded by u32 dimensions, so it fits in u64.
    let numerator = (start_y as u64) * (source_height as u64);
    let mut source_y = (numerator / destination_height as u64) as usize;
    let mut y_remainder = (numerator % destination_height as u64) as usize;

    for destination_row in destination.chunks_mut(dw) {
        let source_row = &source[source_y.min(sh - 1) * sw..][..sw];

        if source_width == destination_width {
            destination_row.copy_from_slice(source_row);
        } else {
            debug_assert_eq!(x_map.len(), dw);
            for (pixel, &source_x) in destination_row.iter_mut().zip(x_map) {
                *pixel = source_row[source_x];
            }
        }

        source_y += y_quotient;
        y_remainder += y_remainder_step;
        if y_remainder >= dh {
            source_y += 1;
            y_remainder -= dh;
        }
    }
}

#[cfg(test)]
mod p0_invariant_tests {
    use super::*;

    #[test]
    fn rejects_full_canvas_frame_length_mismatch() {
        let options = DecodeOptions::default();
        let frames = vec![vec![0u32; 4], vec![0u32; 3]];
        assert!(Image::from_frame_pixels(2, 2, frames, &options).is_err());
    }

    #[test]
    fn rejects_pathological_animation_canvas_before_allocation() {
        let mut options = DecodeOptions::default();
        options.limits.max_pixels = usize::MAX;
        options.limits.max_decompressed_bytes = usize::MAX;
        assert!(Image::from_frame_pixels(u32::MAX, u32::MAX, vec![Vec::new()], &options).is_err());
    }
}
