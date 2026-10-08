use std::io::{Read, Seek, SeekFrom};

use crate::AudioError;

pub(crate) const MAX_METADATA_BLOCK_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_PICTURE_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_TEXT_VALUE_BYTES: usize = 4 * 1024 * 1024;

/// Execute a parser without changing the caller-visible stream position.
pub(crate) fn preserve_position<R, T, F>(reader: &mut R, f: F) -> Result<T, AudioError>
where
    R: Read + Seek,
    F: FnOnce(&mut R) -> Result<T, AudioError>,
{
    let original = reader.stream_position()?;
    let result = f(reader);
    let restore = reader.seek(SeekFrom::Start(original));
    match (result, restore) {
        (Ok(value), Ok(_)) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error.into()),
    }
}

#[inline]
pub(crate) fn checked_len_u64(len: u64, maximum: usize, what: &str) -> Result<usize, AudioError> {
    let len = usize::try_from(len)
        .map_err(|_| AudioError::InvalidFormat(format!("{what} length does not fit usize")))?;
    if len > maximum {
        return Err(AudioError::InvalidFormat(format!(
            "{what} length {len} exceeds bounded limit {maximum}"
        )));
    }
    Ok(len)
}

#[inline]
pub(crate) fn checked_range(start: u64, len: u64, end: u64, what: &str) -> Result<u64, AudioError> {
    let next = start
        .checked_add(len)
        .ok_or_else(|| AudioError::InvalidFormat(format!("{what} offset overflow")))?;
    if next > end { return Err(AudioError::TruncatedData); }
    Ok(next)
}

pub(crate) fn read_exact_vec<R: Read>(reader: &mut R, len: usize, maximum: usize, what: &str) -> Result<Vec<u8>, AudioError> {
    if len > maximum {
        return Err(AudioError::InvalidFormat(format!(
            "{what} length {len} exceeds bounded limit {maximum}"
        )));
    }
    let mut data = vec![0u8; len];
    reader.read_exact(&mut data)?;
    Ok(data)
}

pub(crate) fn read_tail<R: Read + Seek, const N: usize>(reader: &mut R, end: u64) -> Result<([u8; N], usize), AudioError> {
    let take = (end as usize).min(N);
    let mut data = [0u8; N];
    reader.seek(SeekFrom::Start(end - take as u64))?;
    reader.read_exact(&mut data[N - take..])?;
    Ok((data, take))
}

#[inline]
pub(crate) fn seek_absolute<R: Seek>(reader: &mut R, offset: u64) -> Result<(), AudioError> {
    reader.seek(SeekFrom::Start(offset))?;
    Ok(())
}
