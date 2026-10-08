use std::io::{ErrorKind, Read, Seek, SeekFrom};

use crate::AudioError;
use super::crc::{crc16_update, crc8_update};

const INPUT_BUFFER_SIZE: usize = 8192;

/// Persistent FLAC bit reader. It batches source reads, keeps a word-sized bit
/// reservoir, and computes frame/header CRCs as bytes are consumed.
pub(super) struct BitReader<R: Read> {
    reader: R,
    input: Box<[u8; INPUT_BUFFER_SIZE]>,
    input_pos: usize,
    input_len: usize,
    reservoir: u64,
    reservoir_bits: u8,
    byte_position: u64,
    eof: bool,
    crc8: Option<u8>,
    crc16: Option<u16>,
}

impl<R: Read> BitReader<R> {
    pub fn new(reader: R, byte_position: u64) -> Self {
        Self {
            reader,
            input: Box::new([0; INPUT_BUFFER_SIZE]),
            input_pos: 0,
            input_len: 0,
            reservoir: 0,
            reservoir_bits: 0,
            byte_position,
            eof: false,
            crc8: None,
            crc16: None,
        }
    }

    #[inline] pub fn byte_position(&self) -> u64 { self.byte_position }
    #[inline] pub(super) const fn buffered_capacity_bytes(&self) -> usize { INPUT_BUFFER_SIZE }
    #[inline] pub fn is_byte_aligned(&self) -> bool { self.reservoir_bits == 0 }

    pub fn into_inner(self) -> R { self.reader }

    pub fn begin_frame_crc(&mut self) -> Result<(), AudioError> {
        if !self.is_byte_aligned() {
            return Err(AudioError::InvalidFormat("FLAC frame started at a non-byte boundary".into()));
        }
        self.crc8 = Some(0);
        self.crc16 = Some(0);
        Ok(())
    }

    #[inline]
    pub fn finish_header_crc8(&mut self) -> u8 {
        self.crc8.take().unwrap_or(0)
    }

    #[inline]
    pub fn finish_frame_crc16(&mut self) -> u16 {
        self.crc16.take().unwrap_or(0)
    }

    fn refill(&mut self) -> Result<(), AudioError> {
        if self.input_pos < self.input_len { return Ok(()); }
        if self.eof { return Err(AudioError::TruncatedData); }
        loop {
            match self.reader.read(&mut self.input[..]) {
                Ok(0) => {
                    self.eof = true;
                    return Err(AudioError::TruncatedData);
                }
                Ok(n) => {
                    self.input_pos = 0;
                    self.input_len = n;
                    return Ok(());
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => return Err(AudioError::Io(error)),
            }
        }
    }

    #[inline]
    fn pull_byte(&mut self) -> Result<u8, AudioError> {
        if self.input_pos == self.input_len { self.refill()?; }
        let byte = self.input[self.input_pos];
        self.input_pos += 1;
        self.byte_position = self.byte_position.saturating_add(1);
        if let Some(crc) = self.crc8.as_mut() { *crc = crc8_update(*crc, byte); }
        if let Some(crc) = self.crc16.as_mut() { *crc = crc16_update(*crc, byte); }
        Ok(byte)
    }

    #[inline]
    fn ensure_bits(&mut self, count: u8) -> Result<(), AudioError> {
        debug_assert!(count <= 32);
        while self.reservoir_bits < count {
            let byte = self.pull_byte()?;
            self.reservoir = (self.reservoir << 8) | byte as u64;
            self.reservoir_bits += 8;
        }
        Ok(())
    }

    #[inline]
    fn consume(&mut self, count: u8) {
        debug_assert!(count <= self.reservoir_bits);
        self.reservoir_bits -= count;
        self.reservoir = match self.reservoir_bits {
            0 => 0,
            64 => self.reservoir,
            bits => self.reservoir & ((1u64 << bits) - 1),
        };
    }

    #[inline]
    fn read_bits_small(&mut self, count: u8) -> Result<u64, AudioError> {
        if count == 0 { return Ok(0); }
        debug_assert!(count <= 32);
        self.ensure_bits(count)?;
        let shift = self.reservoir_bits - count;
        let mask = if count == 64 { u64::MAX } else { (1u64 << count) - 1 };
        let value = (self.reservoir >> shift) & mask;
        self.consume(count);
        Ok(value)
    }

    #[inline]
    pub fn read_bit(&mut self) -> Result<u8, AudioError> {
        Ok(self.read_bits_small(1)? as u8)
    }

    pub fn read_bits(&mut self, count: u8) -> Result<u64, AudioError> {
        if count > 64 {
            return Err(AudioError::InvalidFormat(format!("Cannot read {count} bits into a u64")));
        }
        if count <= 32 { return self.read_bits_small(count); }
        let high_count = count - 32;
        let high = self.read_bits_small(high_count)?;
        let low = self.read_bits_small(32)?;
        Ok((high << 32) | low)
    }

    #[inline] pub fn read_u8(&mut self) -> Result<u8, AudioError> { Ok(self.read_bits_small(8)? as u8) }
    #[inline] pub fn read_u16(&mut self) -> Result<u16, AudioError> { Ok(self.read_bits_small(16)? as u16) }

    pub fn read_signed(&mut self, bits: u8) -> Result<i64, AudioError> {
        if bits == 0 { return Ok(0); }
        if bits > 64 { return Err(AudioError::InvalidFormat(format!("Cannot decode a {bits}-bit signed integer"))); }
        let value = self.read_bits(bits)?;
        if bits == 64 { return Ok(value as i64); }
        let sign_bit = 1u64 << (bits - 1);
        if value & sign_bit != 0 {
            Ok((value | (!0u64 << bits)) as i64)
        } else {
            Ok(value as i64)
        }
    }

    /// Decode a unary zero prefix using word scans rather than one bit at a time.
    pub fn read_unary_zeros(&mut self) -> Result<u64, AudioError> {
        let mut zeros = 0u64;
        loop {
            if self.reservoir_bits == 0 { self.ensure_bits(1)?; }
            let bits = self.reservoir_bits;
            let aligned = self.reservoir << (64 - bits);
            let leading = aligned.leading_zeros().min(bits as u32) as u8;
            if leading < bits {
                self.consume(leading + 1);
                zeros = zeros.checked_add(leading as u64)
                    .ok_or_else(|| AudioError::InvalidFormat("Unary value overflow".into()))?;
                return Ok(zeros);
            }
            self.consume(bits);
            zeros = zeros.checked_add(bits as u64)
                .ok_or_else(|| AudioError::InvalidFormat("Unary value overflow".into()))?;
            if zeros > u32::MAX as u64 {
                return Err(AudioError::InvalidFormat("FLAC unary value exceeds supported range".into()));
            }
        }
    }

    pub fn align_zero_padding(&mut self) -> Result<(), AudioError> {
        if self.reservoir_bits == 0 { return Ok(()); }
        let bits = self.reservoir_bits;
        if self.read_bits_small(bits)? != 0 {
            return Err(AudioError::InvalidFormat("Non-zero FLAC frame padding".into()));
        }
        Ok(())
    }

    /// True when no buffered or source bytes remain. This can refill the input
    /// buffer, but never consumes a logical byte from the stream.
    pub fn is_eof(&mut self) -> Result<bool, AudioError> {
        if self.reservoir_bits != 0 || self.input_pos < self.input_len { return Ok(false); }
        if self.eof { return Ok(true); }
        loop {
            match self.reader.read(&mut self.input[..]) {
                Ok(0) => { self.eof = true; return Ok(true); }
                Ok(n) => { self.input_pos = 0; self.input_len = n; return Ok(false); }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => return Err(AudioError::Io(error)),
            }
        }
    }
}

impl<R: Read + Seek> BitReader<R> {
    pub fn seek_to(&mut self, position: u64) -> Result<(), AudioError> {
        self.reader.seek(SeekFrom::Start(position))?;
        self.input_pos = 0;
        self.input_len = 0;
        self.reservoir = 0;
        self.reservoir_bits = 0;
        self.byte_position = position;
        self.eof = false;
        self.crc8 = None;
        self.crc16 = None;
        Ok(())
    }
}

#[cfg(test)]
mod hardening_fuzz {
    use super::BitReader;
    use std::io::Cursor;
    fn next(s:&mut u64)->u64{*s^=*s<<13;*s^=*s>>7;*s^=*s<<17;*s}
    #[test]
    fn variable_width_reads_equal_msb_first_reference_and_boundaries() {
        let mut seed=0x77ba_da55_1234_5678u64;
        for _ in 0..300 {
            let len=(next(&mut seed)%129) as usize;
            let bytes:Vec<u8>=(0..len).map(|_|next(&mut seed) as u8).collect();
            let mut reader=BitReader::new(Cursor::new(bytes.clone()),0);
            let mut consumed=0usize;
            while consumed<len*8 {
                let requested=(next(&mut seed)%65) as u8;
                let take=(len*8-consumed).min(requested as usize) as u8;
                let got=reader.read_bits(take).unwrap();
                let mut expected=0u64;
                for i in 0..take as usize { let index=consumed+i;expected=(expected<<1)|((bytes[index/8]>>(7-index%8))&1)as u64; }
                assert_eq!(got,expected,"read at {consumed}, width {take}");
                consumed+=take as usize;
                if take==0 {assert_eq!(reader.read_bits(0).unwrap(),0);}
            }
            assert!(reader.read_bit().is_err(),"EOF must fail");
        }
    }
    #[test]
    fn unary_prefixes_and_seeks_keep_reader_consistent() {
        for zeros in 0..1000usize {
            let mut bytes=vec![0u8;zeros/8+1];bytes[zeros/8]|=0x80>>(zeros%8);
            let mut reader=BitReader::new(Cursor::new(bytes),0);
            assert_eq!(reader.read_unary_zeros().unwrap(),zeros as u64);
            reader.seek_to(0).unwrap();
            assert_eq!(reader.read_unary_zeros().unwrap(),zeros as u64);
        }
    }
}
