use crate::AudioError;

/// Vorbis packets are packed least-significant bit first within each byte.
/// Reads use a word-sized little-endian reservoir assembled from at most nine
/// source bytes, rather than walking individual bits/bytes in the hot path.
#[derive(Debug, Clone)]
pub(super) struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    #[inline] pub fn new(data: &'a [u8]) -> Self { Self { data, bit_pos: 0 } }
    #[inline] pub fn remaining_bits(&self) -> usize { self.data.len().saturating_mul(8).saturating_sub(self.bit_pos) }
    #[inline] pub fn position(&self) -> usize { self.bit_pos }

    #[inline]
    pub fn read_bool(&mut self) -> Result<bool, AudioError> { Ok(self.read_bits(1)? != 0) }

    #[inline]
    pub fn peek_bits(&self, count: u8) -> Result<u64, AudioError> {
        if count > 64 { return Err(AudioError::InvalidFormat(format!("Vorbis bit read of {count} bits exceeds u64"))); }
        if count == 0 { return Ok(0); }
        if self.remaining_bits() < count as usize { return Err(AudioError::TruncatedData); }

        let byte_index = self.bit_pos >> 3;
        let bit_shift = self.bit_pos & 7;
        let bytes_needed = (bit_shift + count as usize + 7) >> 3;
        let mut reservoir = 0u128;
        // At most 9 bytes are needed for an unaligned 64-bit read.
        for i in 0..bytes_needed {
            reservoir |= (self.data[byte_index + i] as u128) << (i * 8);
        }
        let value = reservoir >> bit_shift;
        let mask = if count == 64 { u64::MAX as u128 } else { (1u128 << count) - 1 };
        Ok((value & mask) as u64)
    }

    #[inline]
    pub fn skip_bits(&mut self, count: usize) -> Result<(), AudioError> {
        if self.remaining_bits() < count { return Err(AudioError::TruncatedData); }
        self.bit_pos += count;
        Ok(())
    }

    #[inline]
    pub fn read_bits(&mut self, count: u8) -> Result<u64, AudioError> {
        let value = self.peek_bits(count)?;
        self.bit_pos += count as usize;
        Ok(value)
    }

    #[inline] pub fn read_u8(&mut self, bits: u8) -> Result<u8, AudioError> { if bits>8{return Err(AudioError::InvalidFormat("Vorbis u8 bit width exceeds 8".into()));} Ok(self.read_bits(bits)? as u8) }
    #[inline] pub fn read_u16(&mut self, bits: u8) -> Result<u16, AudioError> { if bits>16{return Err(AudioError::InvalidFormat("Vorbis u16 bit width exceeds 16".into()));} Ok(self.read_bits(bits)? as u16) }
    #[inline] pub fn read_u32(&mut self, bits: u8) -> Result<u32, AudioError> { if bits>32{return Err(AudioError::InvalidFormat("Vorbis u32 bit width exceeds 32".into()));} Ok(self.read_bits(bits)? as u32) }
    #[inline] pub fn read_u64(&mut self, bits: u8) -> Result<u64, AudioError> { self.read_bits(bits) }

    pub fn read_vorbis_f32(&mut self) -> Result<f32, AudioError> {
        let packed=self.read_u32(32)?;let negative=packed&0x8000_0000!=0;let exponent=((packed&0x7fe0_0000)>>21)as i32;let mantissa=(packed&0x001f_ffff)as f64;let signed=if negative{-mantissa}else{mantissa};Ok((signed*2.0f64.powi(exponent-788))as f32)
    }
}

#[inline]
pub(super) fn ilog(value:u64)->u8{if value==0{0}else{(64-value.leading_zeros())as u8}}

#[cfg(test)]
mod tests {
    use super::BitReader;

    #[test]
    fn reservoir_matches_lsb_first_reference() {
        let data=[0b1011_0110,0b0101_1100,0xA5,0x7E,0x11,0x22,0x33,0x44,0x55];
        for start in 0..8usize { for width in 1..=56u8 {
            if start+width as usize>data.len()*8{break;}
            let mut reader=BitReader::new(&data);reader.skip_bits(start).unwrap();let got=reader.read_bits(width).unwrap();
            let mut expected=0u64;for bit in 0..width as usize{let pos=start+bit;expected|=(((data[pos/8]>>(pos%8))&1)as u64)<<bit;}
            assert_eq!(got,expected,"start={start} width={width}");
        }}
    }
}

#[cfg(test)]
mod hardening_fuzz {
    use super::BitReader;
    fn next(s:&mut u64)->u64{*s^=*s<<13;*s^=*s>>7;*s^=*s<<17;*s}
    #[test]
    fn random_offsets_64bit_and_truncation_are_lsb_exact() {
        let mut seed=0xC0FF_EE12_D15C_A11Eu64;
        for _ in 0..1500 {
            let len=(next(&mut seed)%65) as usize;
            let bytes:Vec<u8>=(0..len).map(|_|next(&mut seed) as u8).collect();
            let start=(next(&mut seed)%((len*8+1) as u64)) as usize;
            let width=(next(&mut seed)%65) as u8;
            let mut reader=BitReader::new(&bytes);
            reader.skip_bits(start).unwrap();
            if start+width as usize > len*8 {assert!(reader.read_bits(width).is_err());continue;}
            let got=reader.read_bits(width).unwrap();
            let mut expected=0u64;
            for k in 0..width as usize {let p=start+k;expected|=(((bytes[p/8]>>(p%8))&1) as u64)<<k;}
            assert_eq!(got,expected,"len={len}, offset={start}, width={width}");
        }
    }
}
