//! Shared bitstream readers used by multiple image formats.
//!
//! The [`LsbBitReader`] implements the little-endian/least-significant-bit-first
//! packing used by DEFLATE, GIF LZW, and VP8L. JPEG keeps a format-specific entropy reader
//! because JPEG additionally performs byte stuffing and marker detection.

use std::{fmt, io};

use crate::ImageError;

/// Errors produced by the shared least-significant-bit-first bit reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BitReaderError {
    /// A caller requested a field wider than the reader supports in one operation.
    ReadTooLarge {
        /// Number of requested bits.
        requested: u32,
        /// Maximum number of supported bits.
        maximum: u32,
    },
    /// The encoded input ended before the requested field was available.
    UnexpectedEnd,
    /// A bit/byte position calculation overflowed `usize`.
    PositionOverflow,
}

impl fmt::Display for BitReaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::ReadTooLarge { requested, maximum } => write!(
                formatter,
                "bit read of {requested} bits exceeds the {maximum}-bit limit"
            ),
            Self::UnexpectedEnd => formatter.write_str("unexpected end of bitstream"),
            Self::PositionOverflow => formatter.write_str("bitstream position overflow"),
        }
    }
}

impl std::error::Error for BitReaderError {}

impl From<BitReaderError> for io::Error {
    #[inline]
    fn from(error: BitReaderError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, error)
    }
}

impl From<BitReaderError> for ImageError {
    #[inline]
    fn from(error: BitReaderError) -> Self {
        ImageError::new(error.to_string())
    }
}

/// Buffered reader for least-significant-bit-first bitstreams.
///
/// DEFLATE, GIF LZW, and VP8L pack their relevant bit fields from the low bit upward. The
/// reader keeps a small `u64` accumulator so common Huffman-heavy paths can peek
/// and consume fields without re-reading one bit at a time.
pub(crate) struct LsbBitReader<'a> {
    /// Complete encoded input.
    data: &'a [u8],
    /// Next byte that has not yet been copied into `buffer`.
    byte_position: usize,
    /// Prefetched unread bits, with the next bit in bit zero.
    buffer: u64,
    /// Number of valid unread bits held in `buffer`.
    buffered_bits: u32,
}

impl<'a> LsbBitReader<'a> {
    /// Largest field returned by one read operation.
    pub(crate) const MAX_READ_BITS: u32 = 24;

    /// Creates a reader positioned at the first bit of `data`.
    #[inline]
    pub(crate) const fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_position: 0,
            buffer: 0,
            buffered_bits: 0,
        }
    }

    /// Returns the number of logical unread bits remaining in the input.
    #[inline(always)]
    pub(crate) fn available_bits(&self) -> usize {
        self.data
            .len()
            .saturating_sub(self.byte_position)
            .saturating_mul(8)
            .saturating_add(self.buffered_bits as usize)
    }

    /// Returns the logical bit offset represented by the current prefetch state.
    #[inline(always)]
    fn logical_bit_position(&self) -> usize {
        self.byte_position
            .saturating_mul(8)
            .saturating_sub(self.buffered_bits as usize)
    }

    /// Ensures at least `count` low-order bits are available in the accumulator.
    #[inline]
    fn ensure_bits(&mut self, count: u32) -> Result<(), BitReaderError> {
        if count > Self::MAX_READ_BITS {
            return Err(BitReaderError::ReadTooLarge {
                requested: count,
                maximum: Self::MAX_READ_BITS,
            });
        }
        if self.buffered_bits >= count {
            return Ok(());
        }
        if self.available_bits() < count as usize {
            return Err(BitReaderError::UnexpectedEnd);
        }

        while self.buffered_bits < count {
            let remaining = self.data.len() - self.byte_position;

            // Start an empty accumulator with an eight-byte refill when possible.
            // Subsequent refills stay at four bytes because appending 64 bits to a
            // non-empty accumulator would exceed its capacity.
            if self.buffered_bits == 0 && remaining >= 8 {
                // SAFETY: the length check guarantees eight readable bytes and
                // read_unaligned imposes no alignment requirement.
                let word = unsafe {
                    std::ptr::read_unaligned(
                        self.data.as_ptr().add(self.byte_position) as *const u64,
                    )
                };
                self.buffer = u64::from_le(word);
                self.byte_position += 8;
                self.buffered_bits = 64;
            } else if remaining >= 4 {
                // SAFETY: the length check guarantees four readable bytes.
                let word = unsafe {
                    std::ptr::read_unaligned(
                        self.data.as_ptr().add(self.byte_position) as *const u32,
                    )
                };
                self.buffer |= (u32::from_le(word) as u64) << self.buffered_bits;
                self.byte_position += 4;
                self.buffered_bits += 32;
            } else if remaining != 0 {
                self.buffer |= (self.data[self.byte_position] as u64) << self.buffered_bits;
                self.byte_position += 1;
                self.buffered_bits += 8;
            } else {
                return Err(BitReaderError::UnexpectedEnd);
            }
        }
        Ok(())
    }

    /// Ensures a compile-time constant number of bits and returns them without
    /// consuming. Huffman fast tables use this to make the common width visible
    /// to the optimizer while retaining the same checked refill path.
    #[inline(always)]
    pub(crate) fn peek_bits_fast<const COUNT: u32>(&mut self) -> Result<u32, BitReaderError> {
        debug_assert!(COUNT <= Self::MAX_READ_BITS);
        if COUNT == 0 {
            return Ok(0);
        }
        self.ensure_bits(COUNT)?;
        Ok(self.peek_bits_unchecked(COUNT))
    }

    /// Returns the next `count` bits without consuming them.
    #[inline(always)]
    pub(crate) fn peek_bits(&mut self, count: u32) -> Result<u32, BitReaderError> {
        if count == 0 {
            return Ok(0);
        }
        self.ensure_bits(count)?;
        Ok(self.peek_bits_unchecked(count))
    }

    /// Returns buffered bits after the caller has ensured availability.
    #[inline(always)]
    pub(crate) fn peek_bits_unchecked(&self, count: u32) -> u32 {
        debug_assert!(count <= self.buffered_bits);
        debug_assert!(count <= Self::MAX_READ_BITS);
        if count == 0 {
            0
        } else {
            (self.buffer & ((1u64 << count) - 1)) as u32
        }
    }

    /// Discards `count` bits that are already known to be buffered.
    #[inline(always)]
    pub(crate) fn drop_bits(&mut self, count: u32) {
        debug_assert!(count <= self.buffered_bits);
        self.buffer >>= count;
        self.buffered_bits -= count;
    }

    /// Reads one bit.
    #[inline(always)]
    pub(crate) fn read_bit(&mut self) -> Result<u32, BitReaderError> {
        self.ensure_bits(1)?;
        let value = self.buffer as u32 & 1;
        self.drop_bits(1);
        Ok(value)
    }

    /// Reads up to 24 bits in least-significant-bit-first order.
    #[inline(always)]
    pub(crate) fn read_bits(&mut self, count: u32) -> Result<u32, BitReaderError> {
        if count == 0 {
            return Ok(0);
        }
        self.ensure_bits(count)?;
        let value = self.peek_bits_unchecked(count);
        self.drop_bits(count);
        Ok(value)
    }

    /// Advances to the next whole-byte boundary.
    #[inline]
    pub(crate) fn align_to_byte(&mut self) {
        // Prefetch always advances byte_position by whole bytes, therefore the
        // logical bit position modulo eight is the inverse of buffered_bits.
        // Dropping buffered_bits % 8 reaches the next byte boundary directly.
        let skip = self.buffered_bits & 7;
        if skip != 0 {
            self.drop_bits(skip);
        }
    }

    /// Returns the next logical byte position after byte alignment.
    #[inline(always)]
    fn logical_byte_position(&self) -> usize {
        debug_assert_eq!(self.buffered_bits & 7, 0);
        self.byte_position
            .saturating_sub((self.buffered_bits >> 3) as usize)
    }

    /// Reads a byte-aligned little-endian `u16`.
    #[inline]
    pub(crate) fn read_u16_le(&mut self) -> Result<u16, BitReaderError> {
        self.align_to_byte();
        let start = self.logical_byte_position();
        let end = start.checked_add(2).ok_or(BitReaderError::PositionOverflow)?;
        let bytes = self.data.get(start..end).ok_or(BitReaderError::UnexpectedEnd)?;
        let value = u16::from_le_bytes([bytes[0], bytes[1]]);
        self.reset_to_byte_position(end);
        Ok(value)
    }

    /// Reads `length` byte-aligned bytes without copying them.
    #[inline]
    pub(crate) fn read_bytes(&mut self, length: usize) -> Result<&'a [u8], BitReaderError> {
        self.align_to_byte();
        let start = self.logical_byte_position();
        let end = start.checked_add(length).ok_or(BitReaderError::PositionOverflow)?;
        let data = self.data;
        let bytes = data.get(start..end).ok_or(BitReaderError::UnexpectedEnd)?;
        self.reset_to_byte_position(end);
        Ok(bytes)
    }

    /// Returns whether all complete bytes through the final meaningful bit were consumed.
    #[inline(always)]
    pub(crate) fn finished_at_end(&self) -> bool {
        self.logical_bit_position()
            .checked_add(7)
            .map(|position| (position >> 3) == self.data.len())
            .unwrap_or(false)
    }

    /// Resets prefetch state after a byte-oriented operation.
    #[inline(always)]
    fn reset_to_byte_position(&mut self, byte_position: usize) {
        self.byte_position = byte_position;
        self.buffer = 0;
        self.buffered_bits = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::LsbBitReader;

    fn reference_bits(data: &[u8], start: usize, count: usize) -> u32 {
        let mut value = 0u32;
        for bit in 0..count {
            let position = start + bit;
            let source = (data[position >> 3] >> (position & 7)) & 1;
            value |= (source as u32) << bit;
        }
        value
    }

    #[test]
    fn reads_match_reference_across_refill_boundaries() {
        let data: Vec<u8> = (0..97).map(|i| (i * 37 + 11) as u8).collect();
        for width in 1..=LsbBitReader::MAX_READ_BITS as usize {
            let mut reader = LsbBitReader::new(&data);
            let mut position = 0usize;
            while position + width <= data.len() * 8 {
                let got = reader.read_bits(width as u32).unwrap();
                assert_eq!(got, reference_bits(&data, position, width));
                position += width;
            }
        }
    }

    #[test]
    fn byte_alignment_preserves_prefetched_data() {
        let data = [0b1011_0110, 0x34, 0x12, 0x99, 0x42];
        let mut reader = LsbBitReader::new(&data);
        assert_eq!(reader.read_bits(3).unwrap(), 0b110);
        assert_eq!(reader.read_u16_le().unwrap(), 0x1234);
        assert_eq!(reader.read_bytes(2).unwrap(), &[0x99, 0x42]);
        assert!(reader.finished_at_end());
    }

    #[test]
    fn deterministic_fuzz_matches_reference_reader() {
        // A dependency-free property-style fuzz pass. It deliberately crosses
        // 8/32/64-bit refill boundaries and varies read widths on every step.
        let mut state = 0x4d41_5441_d15c_a11eu64;
        for length in 1usize..=257 {
            let mut data = vec![0u8; length];
            for byte in &mut data {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state as u8;
            }

            let mut reader = LsbBitReader::new(&data);
            let mut position = 0usize;
            while position < data.len() * 8 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let remaining = data.len() * 8 - position;
                let width = (1 + (state as usize % LsbBitReader::MAX_READ_BITS as usize))
                    .min(remaining);
                let expected = reference_bits(&data, position, width);
                assert_eq!(reader.read_bits(width as u32).unwrap(), expected);
                position += width;
            }
            assert_eq!(reader.available_bits(), 0);
        }
    }

    #[test]
    fn every_start_bit_offset_matches_reference() {
        let data: Vec<u8> = (0..80).map(|i| (i * 73 + 19) as u8).collect();
        for start in 0usize..64 {
            for width in 1usize..=24 {
                if start + width > data.len() * 8 { continue; }
                let mut reader = LsbBitReader::new(&data);
                for _ in 0..start { reader.read_bit().unwrap(); }
                assert_eq!(reader.read_bits(width as u32).unwrap(), reference_bits(&data, start, width),
                    "start={start} width={width}");
            }
        }
    }

    #[test]
    fn const_generic_fast_peek_matches_dynamic_peek() {
        let data: Vec<u8> = (0..64).map(|value| (value * 29 + 7) as u8).collect();
        let mut fast = LsbBitReader::new(&data);
        let mut dynamic = LsbBitReader::new(&data);
        for _ in 0..40 {
            assert_eq!(fast.peek_bits_fast::<9>().unwrap(), dynamic.peek_bits(9).unwrap());
            fast.drop_bits(7);
            dynamic.drop_bits(7);
        }
    }
}
