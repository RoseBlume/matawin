//! Error types used throughout image parsing and decoding.
//!
//! Static decoder failures borrow their message text instead of allocating a
//! `String`. Errors that need dynamic values still own a formatted string.

use std::borrow::Cow;

/// Broad machine-readable category for an image error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageErrorKind {
    /// The input is malformed, truncated, internally inconsistent, or otherwise invalid.
    InvalidData,
    /// The input format or a feature used by it is not supported/enabled.
    Unsupported,
    /// An I/O operation failed while loading or reading source data.
    Io,
    /// A configured resource limit would be exceeded.
    LimitExceeded,
    /// A requested caller buffer is the wrong size.
    BufferSize,
}

/// Represents an error produced while reading, parsing, or decoding image data.
#[derive(Clone, PartialEq, Eq)]
pub struct ImageError {
    kind: ImageErrorKind,
    message: Cow<'static, str>,
}

impl ImageError {
    /// Constructs an invalid-data error. String literals are borrowed without
    /// allocating; dynamically formatted `String`s remain owned.
    #[cold]
    #[inline(never)]
    pub fn new(message: impl Into<Cow<'static, str>>) -> Self {
        Self::with_kind(ImageErrorKind::InvalidData, message)
    }

    /// Constructs an error with an explicit machine-readable category.
    #[cold]
    #[inline(never)]
    pub fn with_kind(
        kind: ImageErrorKind,
        message: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Constructs an unsupported-format/feature error.
    #[cold]
    #[inline(never)]
    pub fn unsupported(message: impl Into<Cow<'static, str>>) -> Self {
        Self::with_kind(ImageErrorKind::Unsupported, message)
    }

    /// Constructs an I/O error while preserving a useful dynamic diagnostic.
    #[cold]
    #[inline(never)]
    pub fn io(message: impl Into<Cow<'static, str>>) -> Self {
        Self::with_kind(ImageErrorKind::Io, message)
    }

    /// Constructs a resource-limit error.
    #[cold]
    #[inline(never)]
    pub fn limit(message: impl Into<Cow<'static, str>>) -> Self {
        Self::with_kind(ImageErrorKind::LimitExceeded, message)
    }

    /// Constructs a destination-buffer-size error.
    #[cold]
    #[inline(never)]
    pub fn buffer_size(message: impl Into<Cow<'static, str>>) -> Self {
        Self::with_kind(ImageErrorKind::BufferSize, message)
    }

    /// Returns the machine-readable error category.
    #[inline]
    pub const fn kind(&self) -> ImageErrorKind {
        self.kind
    }

    /// Returns the human-readable error message without allocation.
    #[inline]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns true when this error's message is borrowed static storage.
    #[inline]
    pub fn has_borrowed_message(&self) -> bool {
        matches!(self.message, Cow::Borrowed(_))
    }
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ImageError: {}", self.message)
    }
}

impl std::fmt::Debug for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageError")
            .field("kind", &self.kind)
            .field("message", &self.message)
            .finish()
    }
}

impl std::error::Error for ImageError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_messages_do_not_allocate_owned_strings() {
        let error = ImageError::new("static decoder error");
        assert!(error.has_borrowed_message());
        assert_eq!(error.kind(), ImageErrorKind::InvalidData);
    }

    #[test]
    fn dynamic_messages_are_preserved() {
        let error = ImageError::new(format!("bad value {}", 7));
        assert!(!error.has_borrowed_message());
        assert_eq!(error.message(), "bad value 7");
    }
}
