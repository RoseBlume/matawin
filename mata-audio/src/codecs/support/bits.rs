use crate::AudioError;

/// MSB-first 64-bit reservoir over an in-memory byte slice.
#[derive(Debug, Clone)]
pub(crate) struct BitReservoir64<'a> {
    input: &'a [u8],
    byte_pos: usize,
    reservoir: u64,
    bits: u8,
}

impl<'a> BitReservoir64<'a> {
    pub(crate) fn new(input: &'a [u8]) -> Self {
        Self { input, byte_pos: 0, reservoir: 0, bits: 0 }
    }

    #[inline]
    pub(crate) fn byte_position(&self) -> usize {
        self.byte_pos.saturating_sub((self.bits as usize + 7) / 8)
    }

    #[inline]
    pub(crate) fn bits_remaining(&self) -> usize {
        self.bits as usize + (self.input.len().saturating_sub(self.byte_pos)) * 8
    }

    fn refill(&mut self, needed: u8) -> Result<(), AudioError> {
        if needed > 64 {
            return Err(AudioError::InvalidFormat("bit request exceeds u64 reservoir".into()));
        }
        while self.bits < needed && self.byte_pos < self.input.len() {
            // Keep enough room before shifting another byte into the reservoir.
            if self.bits > 56 { break; }
            // The loop condition is the coarse bounds check; the unchecked
            // load keeps the refill loop free of a second per-byte check.
            let byte = unsafe { *self.input.get_unchecked(self.byte_pos) };
            self.reservoir = (self.reservoir << 8) | byte as u64;
            self.byte_pos += 1;
            self.bits += 8;
        }
        if self.bits < needed { return Err(AudioError::TruncatedData); }
        Ok(())
    }

    #[inline]
    pub(crate) fn read_bits(&mut self, count: u8) -> Result<u64, AudioError> {
        if count == 0 { return Ok(0); }
        if count > 64 { return Err(AudioError::InvalidFormat("bit request exceeds u64 reservoir".into())); }
        // An unaligned 57..64-bit read may not leave room to append a full
        // byte. Consume the already-resident prefix first, then read the
        // remaining suffix from a fresh reservoir.
        if count > self.bits && self.bits > 56 {
            let prefix_bits = self.bits;
            let suffix_bits = count - prefix_bits;
            let prefix = self.read_bits(prefix_bits)?;
            let suffix = self.read_bits(suffix_bits)?;
            return Ok((prefix << suffix_bits) | suffix);
        }
        self.refill(count)?;
        let shift = self.bits - count;
        let value = if count == 64 { self.reservoir } else {
            (self.reservoir >> shift) & ((1u64 << count) - 1)
        };
        self.bits = shift;
        if self.bits == 0 { self.reservoir = 0; }
        else if self.bits < 64 { self.reservoir &= (1u64 << self.bits) - 1; }
        Ok(value)
    }

    #[inline]
    pub(crate) fn read_u8(&mut self) -> Result<u8, AudioError> {
        if self.bits == 0 && self.byte_pos < self.input.len() {
            let value = unsafe { *self.input.get_unchecked(self.byte_pos) };
            self.byte_pos += 1;
            return Ok(value);
        }
        Ok(self.read_bits(8)? as u8)
    }

    #[inline]
    pub(crate) fn read_u16(&mut self) -> Result<u16, AudioError> {
        if self.bits == 0 && self.byte_pos + 2 <= self.input.len() {
            let p = unsafe { self.input.as_ptr().add(self.byte_pos) };
            let value = u16::from_be_bytes(unsafe { [*p, *p.add(1)] });
            self.byte_pos += 2;
            return Ok(value);
        }
        Ok(self.read_bits(16)? as u16)
    }

    #[inline]
    pub(crate) fn read_u24(&mut self) -> Result<u32, AudioError> {
        if self.bits == 0 && self.byte_pos + 3 <= self.input.len() {
            let p = unsafe { self.input.as_ptr().add(self.byte_pos) };
            let value = unsafe { ((*p as u32) << 16) | ((*p.add(1) as u32) << 8) | *p.add(2) as u32 };
            self.byte_pos += 3;
            return Ok(value);
        }
        Ok(self.read_bits(24)? as u32)
    }

    #[inline]
    pub(crate) fn read_u32(&mut self) -> Result<u32, AudioError> {
        if self.bits == 0 && self.byte_pos + 4 <= self.input.len() {
            let p = unsafe { self.input.as_ptr().add(self.byte_pos) };
            let value = u32::from_be_bytes(unsafe { [*p, *p.add(1), *p.add(2), *p.add(3)] });
            self.byte_pos += 4;
            return Ok(value);
        }
        Ok(self.read_bits(32)? as u32)
    }

    pub(crate) fn align_to_byte(&mut self) {
        let drop = self.bits & 7;
        if drop != 0 {
            self.bits -= drop;
            if self.bits == 0 { self.reservoir = 0; }
            else { self.reservoir &= (1u64 << self.bits) - 1; }
        }
    }

    pub(crate) fn read_aligned_bytes(&mut self, output: &mut [u8]) -> Result<(), AudioError> {
        self.align_to_byte();
        // Drain any complete bytes already resident in the reservoir first.
        let mut written = 0usize;
        while self.bits >= 8 && written < output.len() {
            output[written] = self.read_bits(8)? as u8;
            written += 1;
        }
        let remaining = output.len() - written;
        if remaining != 0 {
            if self.byte_pos + remaining > self.input.len() { return Err(AudioError::TruncatedData); }
            output[written..].copy_from_slice(&self.input[self.byte_pos..self.byte_pos + remaining]);
            self.byte_pos += remaining;
        }
        Ok(())
    }

    /// Counts zero bits until the first one. Whole zero bytes are skipped in
    /// batches and non-zero prefixes use `leading_zeros`.
    pub(crate) fn read_unary_zeros(&mut self) -> Result<u32, AudioError> {
        let mut zeros = 0u32;
        loop {
            if self.bits == 0 {
                if self.byte_pos >= self.input.len() { return Err(AudioError::TruncatedData); }
                let byte = self.input[self.byte_pos];
                self.byte_pos += 1;
                if byte == 0 {
                    zeros = zeros.checked_add(8).ok_or_else(|| AudioError::InvalidFormat("unary value overflow".into()))?;
                    continue;
                }
                let prefix = byte.leading_zeros();
                zeros = zeros.checked_add(prefix).ok_or_else(|| AudioError::InvalidFormat("unary value overflow".into()))?;
                let consumed = prefix + 1;
                let remaining = 8 - consumed;
                self.bits = remaining as u8;
                self.reservoir = if remaining == 0 { 0 } else { (byte as u64) & ((1u64 << remaining) - 1) };
                return Ok(zeros);
            }

            let bit = self.read_bits(1)?;
            if bit != 0 { return Ok(zeros); }
            zeros = zeros.checked_add(1).ok_or_else(|| AudioError::InvalidFormat("unary value overflow".into()))?;
        }
    }
}

/// Experimental u128 reservoir used by microbenchmarks and codecs that can
/// profit from wider refill batches. Kept separate so benchmarking can decide
/// whether the larger integer is worthwhile on each target.
#[derive(Debug, Clone)]
pub(crate) struct BitReservoir128<'a> {
    input: &'a [u8],
    byte_pos: usize,
    reservoir: u128,
    bits: u8,
}

impl<'a> BitReservoir128<'a> {
    pub(crate) fn new(input: &'a [u8]) -> Self {
        Self { input, byte_pos: 0, reservoir: 0, bits: 0 }
    }

    fn refill(&mut self, needed: u8) -> Result<(), AudioError> {
        if needed > 128 { return Err(AudioError::InvalidFormat("bit request exceeds u128 reservoir".into())); }
        while self.bits < needed && self.byte_pos < self.input.len() && self.bits <= 120 {
            let byte = unsafe { *self.input.get_unchecked(self.byte_pos) };
            self.reservoir = (self.reservoir << 8) | byte as u128;
            self.byte_pos += 1;
            self.bits += 8;
        }
        if self.bits < needed { return Err(AudioError::TruncatedData); }
        Ok(())
    }

    pub(crate) fn read_bits(&mut self, count: u8) -> Result<u128, AudioError> {
        if count == 0 { return Ok(0); }
        if count > 128 { return Err(AudioError::InvalidFormat("bit request exceeds u128 reservoir".into())); }
        if count > self.bits && self.bits > 120 {
            let prefix_bits = self.bits;
            let suffix_bits = count - prefix_bits;
            let prefix = self.read_bits(prefix_bits)?;
            let suffix = self.read_bits(suffix_bits)?;
            return Ok((prefix << suffix_bits) | suffix);
        }
        self.refill(count)?;
        let shift = self.bits - count;
        let value = if count == 128 { self.reservoir } else {
            (self.reservoir >> shift) & ((1u128 << count) - 1)
        };
        self.bits = shift;
        if self.bits == 0 { self.reservoir = 0; }
        else if self.bits < 128 { self.reservoir &= (1u128 << self.bits) - 1; }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{BitReservoir64, BitReservoir128};

    #[test]
    fn common_widths_and_unaligned_reads() {
        let bytes = [0x12, 0x34, 0x56, 0x78, 0x80];
        let mut bits = BitReservoir64::new(&bytes);
        assert_eq!(bits.read_bits(4).unwrap(), 1);
        assert_eq!(bits.read_u8().unwrap(), 0x23);
        assert_eq!(bits.read_bits(12).unwrap(), 0x456);
        assert_eq!(bits.read_u8().unwrap(), 0x78);
        assert_eq!(bits.read_bits(1).unwrap(), 1);
    }

    #[test]
    fn unary_uses_zero_byte_batches() {
        let mut bits = BitReservoir64::new(&[0, 0, 0b0001_0000]);
        assert_eq!(bits.read_unary_zeros().unwrap(), 19);
        assert_eq!(bits.read_bits(4).unwrap(), 0);
    }

    #[test]
    fn u128_matches_expected_value() {
        let mut bits = BitReservoir128::new(&[0xAB, 0xCD, 0xEF]);
        assert_eq!(bits.read_bits(20).unwrap(), 0xABCDE);
    }

    #[test]
    fn supports_unaligned_full_width_reads() {
        let bytes = [0xF1, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x55];
        let mut bits = BitReservoir64::new(&bytes);
        assert_eq!(bits.read_bits(4).unwrap(), 0xF);
        assert_eq!(bits.read_bits(64).unwrap(), 0x1234_5678_9ABC_DEF5);
    }
}
