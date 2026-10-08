//! `mata-video` provides a small streaming video-decoding API.
//!
//! The crate has no cross-platform framework dependency of its own. Its only
//! direct platform dependencies are:
//!
//! - Windows: the `windows` crate, using Media Foundation.
//! - Linux: the `gstreamer` crate, using GStreamer.
//!
//! Frames are normalized to packed numeric `0xAARRGGBB` pixels so callers can
//! use one representation on both platforms.

#![forbid(unsafe_op_in_unsafe_fn)]

#[cfg(not(any(windows, target_os = "linux")))]
compile_error!("mata-video currently supports only Windows and Linux");

mod backend;
mod error;
mod types;

use std::path::Path;

pub use error::VideoError;
pub use types::{
    PixelFormat,
    VideoFrame,
    VideoInfo,
};

use backend::PlatformDecoder;

/// Streaming video decoder backed by the native platform media stack.
pub struct VideoDecoder {
    backend: PlatformDecoder,
}

impl VideoDecoder {
    /// Opens a video file and prepares its first video stream for decoding.
    ///
    /// The decoder does not load the whole file into memory. Frames are decoded
    /// as [`next_frame`](Self::next_frame) is called.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, VideoError> {
        Ok(Self {
            backend: PlatformDecoder::open(path.as_ref())?,
        })
    }

    /// Returns metadata describing the decoded output stream.
    #[inline]
    pub fn info(&self) -> VideoInfo {
        self.backend.info()
    }

    /// Decodes the next frame.
    ///
    /// Returns `Ok(None)` after end-of-stream has been reached.
    #[inline]
    pub fn next_frame(&mut self) -> Result<Option<VideoFrame>, VideoError> {
        self.backend.next_frame()
    }
}

impl Iterator for VideoDecoder {
    type Item = Result<VideoFrame, VideoError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_frame() {
            Ok(Some(frame)) => Some(Ok(frame)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        }
    }
}
