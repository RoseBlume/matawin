use std::io::{Read, Seek, SeekFrom};

use crate::AudioError;

/// Small reusable demuxer header buffer. Keeping this fixed-size avoids tiny
/// heap allocations while scanning RIFF/AIFF/EBML/Ogg structure headers.
#[derive(Debug, Clone)]
pub(crate) struct HeaderScratch {
    bytes: [u8; 64],
}

impl Default for HeaderScratch {
    fn default() -> Self { Self { bytes: [0; 64] } }
}

impl HeaderScratch {
    #[inline]
    pub(crate) fn read_exact<'a, R: Read>(&'a mut self, reader: &mut R, len: usize) -> Result<&'a [u8], AudioError> {
        if len > self.bytes.len() {
            return Err(AudioError::InvalidFormat(format!("demux header read {len} exceeds scratch capacity")));
        }
        reader.read_exact(&mut self.bytes[..len]).map_err(AudioError::from)?;
        Ok(&self.bytes[..len])
    }
}

/// Skip a payload without allocating it. Bounds are checked by callers before
/// invoking this helper.
#[inline]
pub(crate) fn seek_skip<R: Seek>(reader: &mut R, absolute: u64) -> Result<(), AudioError> {
    reader.seek(SeekFrom::Start(absolute))?;
    Ok(())
}

/// Convert an element/chunk length to usize only after applying a hard bound.
pub(crate) fn bounded_len(value: u64, maximum: usize, what: &str) -> Result<usize, AudioError> {
    let value = usize::try_from(value)
        .map_err(|_| AudioError::InvalidFormat(format!("{what} does not fit this platform")))?;
    if value > maximum {
        return Err(AudioError::InvalidFormat(format!("{what} length {value} exceeds {maximum}")));
    }
    Ok(value)
}
