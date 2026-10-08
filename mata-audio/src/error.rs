use std::fmt;
use std::io;

#[derive(Debug)]
pub enum AudioError {
    Io(io::Error),
    InvalidHeader(String),
    InvalidFormat(String),
    UnsupportedFormat(u16),
    UnsupportedBitDepth(u16),
    TruncatedData,
    NoNextFrame,
    ChecksumMismatch {
        kind: &'static str,
        expected: u64,
        actual: u64,
    },
    Custom(String),
}

impl AudioError {
    #[cold]
    #[inline(never)]
    pub fn custom(message: impl Into<String>) -> Self {
        Self::Custom(message.into())
    }
}

impl fmt::Display for AudioError {
    #[cold]
    #[inline(never)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::InvalidHeader(message) => write!(f, "Invalid audio header: {message}"),
            Self::InvalidFormat(message) => write!(f, "Invalid audio format: {message}"),
            Self::UnsupportedFormat(format) => write!(f, "Unsupported audio format tag: 0x{format:04X}"),
            Self::UnsupportedBitDepth(bits) => write!(f, "Unsupported audio bit depth: {bits}"),
            Self::TruncatedData => write!(f, "Audio data is truncated"),
            Self::NoNextFrame => write!(f, "No next audio frame"),
            Self::ChecksumMismatch { kind, expected, actual } => {
                write!(f, "{kind} checksum mismatch: expected {expected:#X}, got {actual:#X}")
            }
            Self::Custom(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for AudioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for AudioError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
