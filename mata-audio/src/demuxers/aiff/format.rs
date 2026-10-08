use crate::AudioError;
use super::extended::decode_extended_80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiffEncoding {
    PcmBigEndian,
    PcmLittleEndian,
}

#[derive(Debug, Clone, Copy)]
pub struct AiffFormat {
    pub channels: u16,
    pub sample_frames: u32,
    pub sample_size: u16,
    pub sample_rate: u32,
    pub encoding: AiffEncoding,
}

impl AiffFormat {
    #[inline]
    pub fn bytes_per_sample(&self) -> usize { self.sample_size.div_ceil(8) as usize }

    #[inline]
    pub fn block_align(&self) -> usize { self.channels as usize * self.bytes_per_sample() }
}

pub(super) fn parse_comm_chunk(data: &[u8], is_aifc: bool) -> Result<AiffFormat, AudioError> {
    if data.len() < 18 {
        return Err(AudioError::InvalidFormat("AIFF COMM chunk is smaller than 18 bytes".into()));
    }

    let channels = u16::from_be_bytes([data[0], data[1]]);
    let sample_frames = u32::from_be_bytes([data[2], data[3], data[4], data[5]]);
    let sample_size = u16::from_be_bytes([data[6], data[7]]);
    let sample_rate_bytes: [u8; 10] = data[8..18].try_into().unwrap();
    let sample_rate_f64 = decode_extended_80(&sample_rate_bytes)?;

    if channels == 0 { return Err(AudioError::InvalidFormat("AIFF contains zero channels".into())); }
    if sample_size == 0 || sample_size > 32 { return Err(AudioError::UnsupportedBitDepth(sample_size)); }
    if !sample_rate_f64.is_finite() || sample_rate_f64 <= 0.0 {
        return Err(AudioError::InvalidFormat(format!("Invalid AIFF sample rate: {sample_rate_f64}")));
    }
    if sample_rate_f64 > u32::MAX as f64 {
        return Err(AudioError::InvalidFormat("AIFF sample rate exceeds u32 range".into()));
    }

    let sample_rate = sample_rate_f64.round() as u32;
    if sample_rate == 0 { return Err(AudioError::InvalidFormat("AIFF sample rate is zero".into())); }

    let encoding = if is_aifc {
        if data.len() < 22 {
            return Err(AudioError::InvalidFormat("AIFC COMM chunk is missing compression type".into()));
        }
        match &data[18..22] {
            b"NONE" | b"twos" => AiffEncoding::PcmBigEndian,
            b"sowt" => AiffEncoding::PcmLittleEndian,
            compression => {
                return Err(AudioError::InvalidFormat(format!(
                    "Unsupported AIFF-C compression: {:?}", compression
                )));
            }
        }
    } else {
        AiffEncoding::PcmBigEndian
    };

    Ok(AiffFormat { channels, sample_frames, sample_size, sample_rate, encoding })
}
