//! Shared video data types.

use std::time::Duration;

/// Pixel layout returned by every decoder backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// One packed `u32` per pixel in numeric `0xAARRGGBB` form.
    ///
    /// In memory, each pixel therefore occupies BGRA bytes on little-endian
    /// machines. Both supported platform backends normalize their output to
    /// this representation.
    Argb32,
}

/// Basic information about an opened video stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoInfo {
    /// Width of decoded frames in pixels.
    pub width: u32,
    /// Height of decoded frames in pixels.
    pub height: u32,
    /// Frame-rate numerator. Zero means unknown.
    pub frame_rate_numerator: u32,
    /// Frame-rate denominator. Zero means unknown.
    pub frame_rate_denominator: u32,
    /// Declared stream duration when the backend can determine it.
    pub duration: Option<Duration>,
    /// Pixel layout returned in [`VideoFrame::pixels`].
    pub pixel_format: PixelFormat,
}

impl VideoInfo {
    /// Returns the frame rate as a floating-point value when known.
    #[inline]
    pub fn frames_per_second(&self) -> Option<f64> {
        if self.frame_rate_numerator == 0 || self.frame_rate_denominator == 0 {
            None
        } else {
            Some(
                self.frame_rate_numerator as f64
                    / self.frame_rate_denominator as f64,
            )
        }
    }
}

/// One decoded video frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrame {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Packed pixels in numeric `0xAARRGGBB` form.
    pub pixels: Vec<u32>,
    /// Presentation timestamp relative to the start of the stream.
    pub timestamp: Duration,
    /// Duration for which this frame should normally be displayed, when known.
    pub duration: Option<Duration>,
}

impl VideoFrame {
    /// Returns the number of decoded pixels in this frame.
    #[inline]
    pub fn pixel_count(&self) -> usize {
        self.pixels.len()
    }

    /// Returns whether this frame contains no pixel data.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.pixels.is_empty()
    }
}
