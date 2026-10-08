use crate::AudioError;

const WAVE_FORMAT_PCM: u16 = 0x0001;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

const PCM_GUID: [u8; 16] = [
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00,
    0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
];

const IEEE_FLOAT_GUID: [u8; 16] = [
    0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00,
    0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavEncoding {
    Pcm,
    IeeeFloat,
}

#[derive(Debug, Clone, Copy)]
pub struct WavFormat {
    pub encoding: WavEncoding,
    pub channels: u16,
    pub sample_rate: u32,
    pub byte_rate: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    pub valid_bits_per_sample: u16,
    pub channel_mask: Option<u32>,
}

impl WavFormat {
    pub fn bytes_per_sample(&self) -> usize {
        self.bits_per_sample.div_ceil(8) as usize
    }
}

pub fn parse_fmt_chunk(data: &[u8]) -> Result<WavFormat, AudioError> {
    if data.len() < 16 {
        return Err(AudioError::InvalidFormat("fmt chunk is smaller than 16 bytes".into()));
    }

    let format_tag = u16::from_le_bytes([data[0], data[1]]);
    let channels = u16::from_le_bytes([data[2], data[3]]);
    let sample_rate = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let byte_rate = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let block_align = u16::from_le_bytes([data[12], data[13]]);
    let bits_per_sample = u16::from_le_bytes([data[14], data[15]]);

    if channels == 0 {
        return Err(AudioError::InvalidFormat("WAV contains zero channels".into()));
    }
    if sample_rate == 0 {
        return Err(AudioError::InvalidFormat("WAV sample rate is zero".into()));
    }
    if block_align == 0 {
        return Err(AudioError::InvalidFormat("WAV block alignment is zero".into()));
    }

    let (encoding, valid_bits_per_sample, channel_mask) = match format_tag {
        WAVE_FORMAT_PCM => (WavEncoding::Pcm, bits_per_sample, None),
        WAVE_FORMAT_IEEE_FLOAT => (WavEncoding::IeeeFloat, bits_per_sample, None),
        WAVE_FORMAT_EXTENSIBLE => parse_extensible(data, bits_per_sample)?,
        other => return Err(AudioError::UnsupportedFormat(other)),
    };

    let bytes_per_sample = bits_per_sample.div_ceil(8) as u32;
    let expected_block_align = channels as u32 * bytes_per_sample;

    if block_align as u32 != expected_block_align {
        return Err(AudioError::InvalidFormat(format!(
            "Invalid block alignment: expected {expected_block_align}, found {block_align}"
        )));
    }

    Ok(WavFormat {
        encoding,
        channels,
        sample_rate,
        byte_rate,
        block_align,
        bits_per_sample,
        valid_bits_per_sample,
        channel_mask,
    })
}

fn parse_extensible(
    data: &[u8],
    bits_per_sample: u16,
) -> Result<(WavEncoding, u16, Option<u32>), AudioError> {
    if data.len() < 40 {
        return Err(AudioError::InvalidFormat(
            "WAVE_FORMAT_EXTENSIBLE fmt chunk is smaller than 40 bytes".into(),
        ));
    }

    let cb_size = u16::from_le_bytes([data[16], data[17]]);
    if cb_size < 22 {
        return Err(AudioError::InvalidFormat(format!(
            "Invalid WAVE_FORMAT_EXTENSIBLE cbSize: {cb_size}"
        )));
    }

    let mut valid_bits = u16::from_le_bytes([data[18], data[19]]);
    if valid_bits == 0 {
        valid_bits = bits_per_sample;
    }
    if valid_bits > bits_per_sample {
        return Err(AudioError::InvalidFormat(format!(
            "Valid bits ({valid_bits}) exceeds container bits ({bits_per_sample})"
        )));
    }

    let channel_mask = u32::from_le_bytes([data[20], data[21], data[22], data[23]]);
    let mut guid = [0u8; 16];
    guid.copy_from_slice(&data[24..40]);

    let encoding = if guid == PCM_GUID {
        WavEncoding::Pcm
    } else if guid == IEEE_FLOAT_GUID {
        WavEncoding::IeeeFloat
    } else {
        let tag = u16::from_le_bytes([guid[0], guid[1]]);
        return Err(AudioError::UnsupportedFormat(tag));
    };

    Ok((encoding, valid_bits, Some(channel_mask)))
}
