//! Error types used by `mata-video`.

use core::fmt;

/// Error returned while opening, decoding, or reading a video stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoError {
    message: String,
}

impl VideoError {
    /// Creates a new video error from a human-readable message.
    #[inline]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the error message.
    #[inline]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for VideoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for VideoError {}

impl From<std::io::Error> for VideoError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}
