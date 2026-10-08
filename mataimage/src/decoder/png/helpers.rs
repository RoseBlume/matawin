//! Small helpers shared by PNG metadata parsers.

/// Converts PNG ISO-8859-1 bytes to Unicode by preserving each byte value as
/// the corresponding Unicode code point.
#[inline]
pub(super) fn latin1_to_string(data: &[u8]) -> String {
    data.iter().map(|&byte| char::from(byte)).collect()
}

/// Constructs a consistent invalid-data error for PNG parsing/decoding.
#[inline(never)]
pub(super) fn invalid_data(message: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}
