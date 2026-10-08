//! Animation APIs that avoid forcing all display frames into memory.
//!
//! [`AnimationDecoder`] is the low-memory playback path. It keeps only codec
//! state plus one/two compositing canvases and writes each display frame into a
//! caller-owned `&mut [u32]`.

use crate::{ImageError, ImageFormat};

/// Borrowed/consuming access to already materialized full-canvas animation frames.
pub trait AnimatedDecoder {
    /// Returns the animation canvas width in pixels.
    fn width(&self) -> u32;
    /// Returns the animation canvas height in pixels.
    fn height(&self) -> u32;
    /// Borrows all full-canvas packed `0xRRGGBBAA` frames.
    fn frames(&self) -> &[Vec<u32>];
    /// Consumes the decoder and moves all frame buffers out.
    fn into_frames(self: Box<Self>) -> Vec<Vec<u32>>;
}

/// Format-independent timing information for one streamed display frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationFrameInfo {
    /// Zero-based display-frame index.
    pub index: usize,
    /// Display duration in milliseconds.
    pub duration_ms: u32,
}

/// Sequential low-memory decoder for animated formats currently supported by
/// the streaming API.
pub enum AnimationDecoder<'a> {
    /// GIF streaming decoder.
    #[cfg(feature = "gif")]
    Gif(crate::decoder::gif::GifFrameDecoder<'a>),
    /// Animated WebP streaming decoder.
    #[cfg(feature = "webp")]
    WebP(crate::decoder::webp::WebPFrameDecoder<'a>),
}

impl<'a> AnimationDecoder<'a> {
    /// Creates a streaming animation decoder without materializing every frame.
    pub fn from_bytes(bytes: &'a [u8]) -> Result<Self, ImageError> {
        let format = ImageFormat::from_signature(bytes)
            .ok_or_else(|| ImageError::unsupported("Unsupported image format"))?;
        match format {
            ImageFormat::Gif => {
                #[cfg(feature = "gif")]
                { return Ok(Self::Gif(crate::decoder::gif::GifFrameDecoder::new(bytes)?)); }
                #[cfg(not(feature = "gif"))]
                { return Err(ImageError::unsupported("GIF support is disabled")); }
            }
            ImageFormat::WebP => {
                #[cfg(feature = "webp")]
                { return Ok(Self::WebP(crate::decoder::webp::WebPFrameDecoder::new(bytes)?)); }
                #[cfg(not(feature = "webp"))]
                { return Err(ImageError::unsupported("WebP support is disabled")); }
            }
            _ => Err(ImageError::unsupported("Format does not expose streaming animation decode")),
        }
    }

    /// Animation canvas width.
    pub fn width(&self) -> u32 {
        match self {
            #[cfg(feature = "gif")]
            Self::Gif(decoder) => decoder.width(),
            #[cfg(feature = "webp")]
            Self::WebP(decoder) => decoder.width(),
        }
    }

    /// Animation canvas height.
    pub fn height(&self) -> u32 {
        match self {
            #[cfg(feature = "gif")]
            Self::Gif(decoder) => decoder.height(),
            #[cfg(feature = "webp")]
            Self::WebP(decoder) => decoder.height(),
        }
    }

    /// Exact destination length required by [`Self::next_frame`].
    pub fn required_pixels(&self) -> usize {
        match self {
            #[cfg(feature = "gif")]
            Self::Gif(decoder) => decoder.required_pixels(),
            #[cfg(feature = "webp")]
            Self::WebP(decoder) => decoder.required_pixels(),
        }
    }

    /// Decodes the next frame into a caller-owned full-canvas buffer.
    pub fn next_frame(
        &mut self,
        output: &mut [u32],
    ) -> Result<Option<AnimationFrameInfo>, ImageError> {
        match self {
            #[cfg(feature = "gif")]
            Self::Gif(decoder) => Ok(decoder.next_frame(output)?.map(|frame| AnimationFrameInfo {
                index: frame.index,
                duration_ms: frame.delay_ms,
            })),
            #[cfg(feature = "webp")]
            Self::WebP(decoder) => Ok(decoder.next_frame(output)?.map(|frame| AnimationFrameInfo {
                index: frame.index,
                duration_ms: frame.duration_ms,
            })),
        }
    }

    /// Releases optional excess scratch capacity while preserving current decode state.
    pub fn shrink_scratch(&mut self) {
        match self {
            #[cfg(feature = "gif")]
            Self::Gif(decoder) => decoder.shrink_scratch(),
            #[cfg(feature = "webp")]
            Self::WebP(decoder) => decoder.shrink_scratch(),
        }
    }
}

#[cfg(feature = "gif")]
impl AnimatedDecoder for crate::decoder::gif::Gif {
    #[inline]
    fn width(&self) -> u32 { self.width }
    #[inline]
    fn height(&self) -> u32 { self.height }
    #[inline]
    fn frames(&self) -> &[Vec<u32>] { self.frames_ref() }
    #[inline]
    fn into_frames(self: Box<Self>) -> Vec<Vec<u32>> { (*self).into_frames() }
}

#[cfg(feature = "webp")]
impl AnimatedDecoder for crate::decoder::webp::WebP {
    #[inline]
    fn width(&self) -> u32 { self.width }
    #[inline]
    fn height(&self) -> u32 { self.height }
    #[inline]
    fn frames(&self) -> &[Vec<u32>] { self.frames_ref() }
    #[inline]
    fn into_frames(self: Box<Self>) -> Vec<Vec<u32>> { (*self).into_frames() }
}
