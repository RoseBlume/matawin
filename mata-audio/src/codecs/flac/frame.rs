use std::io::Read;

use crate::{AudioError, codecs::AudioFrameBuffer};
use crate::codecs::support::{AlignedScratch, StageTimer};
use crate::demuxers::flac::StreamInfo;

use super::{
    bit_reader::BitReader,
    subframe::decode_subframe_into,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct DecodedFrameInfo {
    pub channels: u16,
    pub sample_rate: u32,
    pub block_size: usize,
}

pub(super) struct FlacScratch {
    channels: AlignedScratch<i64>,
    residual: AlignedScratch<i64>,
}

impl FlacScratch {
    pub fn new(stream_info: &StreamInfo) -> Result<Self, AudioError> {
        let block = stream_info.max_block_size as usize;
        let channel_samples = block.checked_mul(stream_info.channels as usize)
            .ok_or_else(|| AudioError::InvalidFormat("FLAC scratch size overflow".into()))?;
        Ok(Self {
            channels: AlignedScratch::with_len(channel_samples, 0)?,
            residual: AlignedScratch::with_len(block, 0)?,
        })
    }

    fn ensure(&mut self, block_size: usize, channels: usize) -> Result<(), AudioError> {
        let channel_samples = block_size.checked_mul(channels)
            .ok_or_else(|| AudioError::InvalidFormat("FLAC scratch size overflow".into()))?;
        if self.channels.len() < channel_samples { self.channels.try_resize(channel_samples, 0, "FLAC channel scratch")?; }
        if self.residual.len() < block_size { self.residual.try_resize(block_size, 0, "FLAC residual scratch")?; }
        Ok(())
    }

    pub(super) fn allocated_bytes(&self) -> usize { self.channels.allocated_bytes().saturating_add(self.residual.allocated_bytes()) }
    pub(super) fn peak_allocated_bytes(&self) -> usize { self.channels.peak_allocated_bytes().saturating_add(self.residual.peak_allocated_bytes()) }
    pub(super) fn trim_memory(&mut self) { self.channels.shrink_to_len(); self.residual.shrink_to_len(); }
}


#[derive(Debug, Clone, Copy)]
struct FrameHeader {
    block_size: usize,
    sample_rate: u32,
    channels: u8,
    bits_per_sample: u8,
    channel_assignment: u8,
    _blocking_strategy: bool,
    _number: u64,
}

pub(super) fn decode_frame_into<R: Read>(
    bits: &mut BitReader<R>,
    stream_info: &StreamInfo,
    scratch: &mut FlacScratch,
    output: &mut AudioFrameBuffer,
) -> Result<DecodedFrameInfo, AudioError> {
    bits.begin_frame_crc()?;
    let header = {
        let _stage = StageTimer::start("flac.header");
        decode_header(bits, stream_info)?
    };
    scratch.ensure(header.block_size, header.channels as usize)?;

    {
        let _stage = StageTimer::start("flac.subframes");
        for channel in 0..header.channels as usize {
            let channel_bps = channel_bits_per_sample(
                header.channel_assignment,
                channel as u8,
                header.bits_per_sample,
            )?;
            let start = channel * header.block_size;
            let end = start + header.block_size;
            let (channels, residual) = (&mut scratch.channels, &mut scratch.residual);
            decode_subframe_into(
                bits,
                &mut channels[start..end],
                channel_bps,
                &mut residual[..header.block_size],
            )?;
        }
    }

    bits.align_zero_padding()?;
    let expected_crc = bits.finish_frame_crc16();
    let actual_crc = bits.read_u16()?;
    if expected_crc != actual_crc {
        return Err(AudioError::ChecksumMismatch {
            kind: "FLAC frame CRC-16",
            expected: expected_crc as u64,
            actual: actual_crc as u64,
        });
    }

    {
        let _stage = StageTimer::start("flac.restore-normalize");
        restore_interleave_normalize(
            &mut scratch.channels,
            header.block_size,
            header.channels as usize,
            header.channel_assignment,
            header.bits_per_sample,
            output,
        )?;
    }

    Ok(DecodedFrameInfo {
        channels: header.channels as u16,
        sample_rate: header.sample_rate,
        block_size: header.block_size,
    })
}

fn decode_header<R: Read>(reader: &mut BitReader<R>, stream_info: &StreamInfo) -> Result<FrameHeader, AudioError> {
    let sync = reader.read_bits(15)?;
    if sync != 0x7FFC {
        return Err(AudioError::InvalidHeader(format!("Invalid FLAC frame sync: {sync:#X}")));
    }

    let blocking_strategy = reader.read_bit()? != 0;
    let block_size_code = reader.read_bits(4)? as u8;
    let sample_rate_code = reader.read_bits(4)? as u8;
    let channel_assignment_code = reader.read_bits(4)? as u8;
    let bits_per_sample_code = reader.read_bits(3)? as u8;
    if reader.read_bit()? != 0 {
        return Err(AudioError::InvalidFormat("Reserved FLAC frame-header bit is set".into()));
    }

    let number = decode_coded_number(reader)?;
    if !blocking_strategy && number > 0x7FFF_FFFF {
        return Err(AudioError::InvalidFormat("FLAC frame number exceeds 31 bits".into()));
    }

    let block_size = decode_block_size(reader, block_size_code)?;
    let sample_rate = decode_sample_rate(reader, sample_rate_code, stream_info.sample_rate)?;
    let (channels, channel_assignment) = decode_channel_assignment(channel_assignment_code)?;
    let bits_per_sample = decode_bits_per_sample(bits_per_sample_code, stream_info.bits_per_sample)?;

    if !reader.is_byte_aligned() {
        return Err(AudioError::InvalidFormat("FLAC frame header did not end on a byte boundary".into()));
    }

    let expected_crc = reader.finish_header_crc8();
    let actual_crc = reader.read_u8()?;
    if expected_crc != actual_crc {
        return Err(AudioError::ChecksumMismatch {
            kind: "FLAC frame-header CRC-8",
            expected: expected_crc as u64,
            actual: actual_crc as u64,
        });
    }

    if block_size > stream_info.max_block_size as usize {
        return Err(AudioError::InvalidFormat(format!(
            "FLAC frame block size {block_size} exceeds STREAMINFO maximum {}",
            stream_info.max_block_size
        )));
    }
    if sample_rate != stream_info.sample_rate {
        return Err(AudioError::InvalidFormat(format!(
            "FLAC sample rate changed from {} to {sample_rate}", stream_info.sample_rate
        )));
    }
    if channels != stream_info.channels {
        return Err(AudioError::InvalidFormat(format!(
            "FLAC channel count changed from {} to {channels}", stream_info.channels
        )));
    }
    if bits_per_sample != stream_info.bits_per_sample {
        return Err(AudioError::InvalidFormat(format!(
            "FLAC bit depth changed from {} to {bits_per_sample}", stream_info.bits_per_sample
        )));
    }

    Ok(FrameHeader {
        block_size,
        sample_rate,
        channels,
        bits_per_sample,
        channel_assignment,
        _blocking_strategy: blocking_strategy,
        _number: number,
    })
}

fn decode_block_size<R: Read>(reader: &mut BitReader<R>, code: u8) -> Result<usize, AudioError> {
    let size = match code {
        0 => return Err(AudioError::InvalidFormat("Reserved FLAC block-size code 0".into())),
        1 => 192,
        2..=5 => 576usize << (code - 2),
        6 => reader.read_u8()? as usize + 1,
        7 => {
            let stored = reader.read_u16()?;
            if stored == u16::MAX {
                return Err(AudioError::InvalidFormat("FLAC block size 65536 is forbidden".into()));
            }
            stored as usize + 1
        }
        8..=15 => 1usize << code,
        _ => unreachable!(),
    };
    Ok(size)
}

fn decode_sample_rate<R: Read>(reader: &mut BitReader<R>, code: u8, stream_info_rate: u32) -> Result<u32, AudioError> {
    let rate = match code {
        0 => stream_info_rate,
        1 => 88_200,
        2 => 176_400,
        3 => 192_000,
        4 => 8_000,
        5 => 16_000,
        6 => 22_050,
        7 => 24_000,
        8 => 32_000,
        9 => 44_100,
        10 => 48_000,
        11 => 96_000,
        12 => reader.read_u8()? as u32 * 1_000,
        13 => reader.read_u16()? as u32,
        14 => reader.read_u16()? as u32 * 10,
        15 => return Err(AudioError::InvalidFormat("Forbidden FLAC sample-rate code 15".into())),
        _ => unreachable!(),
    };
    if rate == 0 { return Err(AudioError::InvalidFormat("FLAC sample rate is zero".into())); }
    Ok(rate)
}

fn decode_channel_assignment(code: u8) -> Result<(u8, u8), AudioError> {
    match code {
        0..=7 => Ok((code + 1, code)),
        8..=10 => Ok((2, code)),
        _ => Err(AudioError::InvalidFormat(format!("Reserved FLAC channel assignment {code}"))),
    }
}

fn decode_bits_per_sample(code: u8, stream_info_bps: u8) -> Result<u8, AudioError> {
    match code {
        0 => Ok(stream_info_bps),
        1 => Ok(8),
        2 => Ok(12),
        3 => Err(AudioError::InvalidFormat("Reserved FLAC bit-depth code 3".into())),
        4 => Ok(16),
        5 => Ok(20),
        6 => Ok(24),
        7 => Ok(32),
        _ => unreachable!(),
    }
}

fn decode_coded_number<R: Read>(reader: &mut BitReader<R>) -> Result<u64, AudioError> {
    let first = reader.read_u8()?;
    let (bytes, first_mask) = if first & 0x80 == 0 {
        (1usize, 0x7F)
    } else if first & 0xE0 == 0xC0 {
        (2, 0x1F)
    } else if first & 0xF0 == 0xE0 {
        (3, 0x0F)
    } else if first & 0xF8 == 0xF0 {
        (4, 0x07)
    } else if first & 0xFC == 0xF8 {
        (5, 0x03)
    } else if first & 0xFE == 0xFC {
        (6, 0x01)
    } else if first == 0xFE {
        (7, 0x00)
    } else {
        return Err(AudioError::InvalidFormat(format!("Invalid FLAC coded-number prefix {first:#04X}")));
    };

    let mut value = (first & first_mask) as u64;
    for _ in 1..bytes {
        let byte = reader.read_u8()?;
        if byte & 0xC0 != 0x80 {
            return Err(AudioError::InvalidFormat("Invalid FLAC coded-number continuation byte".into()));
        }
        value = (value << 6) | (byte & 0x3F) as u64;
    }

    let minimum = match bytes {
        1 => 0,
        2 => 0x80,
        3 => 0x800,
        4 => 0x1_0000,
        5 => 0x20_0000,
        6 => 0x0400_0000,
        7 => 0x8000_0000,
        _ => unreachable!(),
    };
    if value < minimum { return Err(AudioError::InvalidFormat("Overlong FLAC coded number".into())); }
    if value > 0x0F_FFFF_FFFF { return Err(AudioError::InvalidFormat("FLAC coded number exceeds 36 bits".into())); }
    Ok(value)
}

fn channel_bits_per_sample(assignment: u8, channel: u8, bits_per_sample: u8) -> Result<u8, AudioError> {
    match assignment {
        0..=7 => Ok(bits_per_sample),
        8 => if channel == 0 { Ok(bits_per_sample) } else {
            bits_per_sample.checked_add(1).ok_or_else(|| AudioError::InvalidFormat("FLAC side-channel bit-depth overflow".into()))
        },
        9 => if channel == 0 {
            bits_per_sample.checked_add(1).ok_or_else(|| AudioError::InvalidFormat("FLAC side-channel bit-depth overflow".into()))
        } else { Ok(bits_per_sample) },
        10 => if channel == 0 { Ok(bits_per_sample) } else {
            bits_per_sample.checked_add(1).ok_or_else(|| AudioError::InvalidFormat("FLAC side-channel bit-depth overflow".into()))
        },
        _ => Err(AudioError::InvalidFormat(format!("Invalid FLAC channel assignment {assignment}"))),
    }
}

fn restore_interleave_normalize(
    channels: &mut [i64],
    block_size: usize,
    channel_count: usize,
    assignment: u8,
    bits_per_sample: u8,
    output: &mut AudioFrameBuffer,
) -> Result<(), AudioError> {
    let sample_count = block_size.checked_mul(channel_count)
        .ok_or_else(|| AudioError::InvalidFormat("FLAC output sample count overflow".into()))?;
    output.resize_for_decode(sample_count);
    let out = output.samples_mut();
    let reciprocal = (1.0 / 2.0_f64.powi(bits_per_sample as i32 - 1)) as f32;

    if channel_count == 2 && (8..=10).contains(&assignment) {
        let (first, rest) = channels.split_at_mut(block_size);
        let second = &mut rest[..block_size];
        match assignment {
            8 => crate::simd::flac_restore_left_side(first, second),
            9 => crate::simd::flac_restore_right_side(first, second),
            10 => {
                // Mid-side restoration is fused with normalization/interleave
                // below to avoid another full integer-channel pass.
                for i in 0..block_size {
                    let mid = first[i] as i128;
                    let side = second[i] as i128;
                    let combined = (mid << 1) | (side & 1);
                    let left = (combined + side) >> 1;
                    let right = (combined - side) >> 1;
                    let left = i64::try_from(left)
                        .map_err(|_| AudioError::InvalidFormat("FLAC mid-side left-channel overflow".into()))?;
                    let right = i64::try_from(right)
                        .map_err(|_| AudioError::InvalidFormat("FLAC mid-side right-channel overflow".into()))?;
                    out[i * 2] = left as f32 * reciprocal;
                    out[i * 2 + 1] = right as f32 * reciprocal;
                }
                return Ok(());
            }
            _ => unreachable!(),
        }
        for i in 0..block_size {
            out[i * 2] = first[i] as f32 * reciprocal;
            out[i * 2 + 1] = second[i] as f32 * reciprocal;
        }
        return Ok(());
    }

    // Normalization and planar-to-interleaved conversion are fused into one
    // traversal for independent-channel assignments.
    for sample_index in 0..block_size {
        for channel in 0..channel_count {
            out[sample_index * channel_count + channel] =
                channels[channel * block_size + sample_index] as f32 * reciprocal;
        }
    }
    Ok(())
}
