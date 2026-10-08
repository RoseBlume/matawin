use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};

pub(crate) const DEFAULT_IO_CAPACITY: usize = 64 * 1024;
pub(crate) const PCM_IO_CAPACITY: usize = 32 * 1024;
pub(crate) const COMPRESSED_IO_CAPACITY: usize = 128 * 1024;

/// Tuned buffered input that tracks sequential position without repeatedly
/// calling `stream_position()` during steady-state reads.
pub(crate) struct TunedBufReader<R> {
    inner: BufReader<R>,
    position: u64,
}

impl<R: Read> TunedBufReader<R> {
    pub(crate) fn new(inner: R) -> Self { Self::with_capacity(DEFAULT_IO_CAPACITY, inner) }
    pub(crate) fn with_capacity(capacity: usize, inner: R) -> Self {
        Self { inner: BufReader::with_capacity(capacity.max(4096), inner), position: 0 }
    }
    pub(crate) fn tracked_position(&self) -> u64 { self.position }
    pub(crate) fn into_inner(self) -> R { self.inner.into_inner() }
}

impl<R: Read> Read for TunedBufReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.position = self.position.saturating_add(n as u64);
        Ok(n)
    }
}

impl<R: Read + Seek> TunedBufReader<R> {
    /// Repositions to a previously recorded byte offset while keeping the
    /// tracked logical position synchronized with the underlying buffer.
    pub(crate) fn reset_to(&mut self, byte_offset: u64) -> std::io::Result<()> {
        self.seek(SeekFrom::Start(byte_offset)).map(|_| ())
    }
}

impl<R: Read + Seek> Seek for TunedBufReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let new = self.inner.seek(pos)?;
        self.position = new;
        Ok(new)
    }

    fn stream_position(&mut self) -> std::io::Result<u64> { Ok(self.position) }
}

impl<R: Read> BufRead for TunedBufReader<R> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> { self.inner.fill_buf() }
    fn consume(&mut self, amt: usize) {
        self.inner.consume(amt);
        self.position = self.position.saturating_add(amt as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::TunedBufReader;
    use std::io::{Cursor, Read, Seek, SeekFrom};

    #[test]
    fn tracks_reads_and_resets_to_known_offset() {
        let mut reader = TunedBufReader::with_capacity(4096, Cursor::new((0u8..64).collect::<Vec<_>>()));
        let mut bytes = [0u8; 7];
        reader.read_exact(&mut bytes).unwrap();
        assert_eq!(reader.stream_position().unwrap(), 7);
        reader.reset_to(3).unwrap();
        assert_eq!(reader.tracked_position(), 3);
        reader.read_exact(&mut bytes[..2]).unwrap();
        assert_eq!(&bytes[..2], &[3, 4]);
        assert_eq!(reader.seek(SeekFrom::Current(2)).unwrap(), 7);
    }
}
