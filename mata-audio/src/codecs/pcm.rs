//! Raw PCM decoding and PCM adapters for common uncompressed containers.

use std::io::{Cursor, Read, Seek};

use crate::AudioError;

#[cfg(feature = "demux-aiff")]
use crate::demuxers::aiff::{AiffDemuxer, AiffEncoding};
#[cfg(feature = "demux-wav")]
use crate::demuxers::wav::{WavDemuxer, WavEncoding};

use super::{AudioFrameBuffer, DecodedFrame, Decoder, DecoderMemoryUsage};
use super::support::{checked_allocation_len, StageTimer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmEncoding {
    Unsigned8,
    Signed8,
    Signed16Le,
    Signed16Be,
    Signed24Le,
    Signed24Be,
    Signed32Le,
    Signed32Be,
    Float32Le,
    Float32Be,
    Float64Le,
    Float64Be,
}

impl PcmEncoding {
    #[inline]
    pub const fn bytes_per_sample(self) -> usize {
        match self {
            Self::Unsigned8 | Self::Signed8 => 1,
            Self::Signed16Le | Self::Signed16Be => 2,
            Self::Signed24Le | Self::Signed24Be => 3,
            Self::Signed32Le | Self::Signed32Be | Self::Float32Le | Self::Float32Be => 4,
            Self::Float64Le | Self::Float64Be => 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcmFormat {
    pub encoding: PcmEncoding,
    pub channels: u16,
    pub sample_rate: u32,
    /// Number of meaningful bits in an integer container. For float PCM this
    /// should be 32 or 64 respectively.
    pub valid_bits_per_sample: u16,
}

impl PcmFormat {
    pub fn validate(self) -> Result<Self, AudioError> {
        if self.channels == 0 { return Err(AudioError::InvalidFormat("PCM contains zero channels".into())); }
        if self.sample_rate == 0 { return Err(AudioError::InvalidFormat("PCM sample rate is zero".into())); }
        let storage_bits = (self.encoding.bytes_per_sample() * 8) as u16;
        if self.valid_bits_per_sample == 0 || self.valid_bits_per_sample > storage_bits {
            return Err(AudioError::UnsupportedBitDepth(self.valid_bits_per_sample));
        }
        match self.encoding {
            PcmEncoding::Float32Le | PcmEncoding::Float32Be if self.valid_bits_per_sample != 32 => {
                return Err(AudioError::UnsupportedBitDepth(self.valid_bits_per_sample));
            }
            PcmEncoding::Float64Le | PcmEncoding::Float64Be if self.valid_bits_per_sample != 64 => {
                return Err(AudioError::UnsupportedBitDepth(self.valid_bits_per_sample));
            }
            _ => {}
        }
        Ok(self)
    }

    #[inline]
    pub const fn block_align(self) -> usize {
        self.encoding.bytes_per_sample() * self.channels as usize
    }
}

/// Conversion kernel selected once from the stream format. This removes the
/// encoding/endian dispatch from every steady-state PCM chunk.
type PcmDecodeFn = fn(&[u8], u16, u32, f32, &mut AudioFrameBuffer) -> Result<(), AudioError>;

/// Prevalidated PCM conversion parameters. Construct once per stream and reuse
/// for every decoded chunk.
#[derive(Debug, Clone, Copy)]
struct PcmDecodePlan {
    format: PcmFormat,
    bytes_per_sample: usize,
    valid_shift: u32,
    normalization_scale: f32,
    decode_fn: PcmDecodeFn,
}

impl PcmDecodePlan {
    fn new(format: PcmFormat) -> Result<Self, AudioError> {
        let format = format.validate()?;
        let bytes_per_sample = format.encoding.bytes_per_sample();
        let storage_bits = (bytes_per_sample * 8) as u32;
        let valid_shift = storage_bits.saturating_sub(format.valid_bits_per_sample as u32);
        let normalization_scale = match format.encoding {
            PcmEncoding::Float32Le | PcmEncoding::Float32Be |
            PcmEncoding::Float64Le | PcmEncoding::Float64Be => 1.0,
            _ => 2.0_f32.powi(format.valid_bits_per_sample as i32 - 1),
        };
        let decode_fn: PcmDecodeFn = match format.encoding {
            PcmEncoding::Unsigned8 => pcm_decode_u8,
            PcmEncoding::Signed8 => pcm_decode_i8,
            PcmEncoding::Signed16Le => pcm_decode_i16_le,
            PcmEncoding::Signed16Be => pcm_decode_i16_be,
            PcmEncoding::Signed24Le => pcm_decode_i24_le,
            PcmEncoding::Signed24Be => pcm_decode_i24_be,
            PcmEncoding::Signed32Le => pcm_decode_i32_le,
            PcmEncoding::Signed32Be => pcm_decode_i32_be,
            PcmEncoding::Float32Le => pcm_decode_f32_le,
            PcmEncoding::Float32Be => pcm_decode_f32_be,
            PcmEncoding::Float64Le => pcm_decode_f64_le,
            PcmEncoding::Float64Be => pcm_decode_f64_be,
        };
        Ok(Self { format, bytes_per_sample, valid_shift, normalization_scale, decode_fn })
    }

    #[inline]
    fn decode_into(self, data: &[u8], output: &mut AudioFrameBuffer) -> Result<(), AudioError> {
        if data.len() % self.bytes_per_sample != 0 { return Err(AudioError::TruncatedData); }
        (self.decode_fn)(data, self.format.valid_bits_per_sample, self.valid_shift, self.normalization_scale, output)
    }
}

/// Decoder for a raw, headerless PCM stream. The caller supplies the format.
pub struct Pcm<R: Read> {
    reader: R,
    plan: PcmDecodePlan,
    frames_per_chunk: usize,
    ended: bool,
    byte_scratch: Vec<u8>,
    block_align: usize,
}

impl<R: Read> Pcm<R> {
    pub fn new(reader: R, format: PcmFormat) -> Result<Self, AudioError> {
        let plan = PcmDecodePlan::new(format)?;
        let block_align = format.block_align();
        Ok(Self {
            reader,
            plan,
            frames_per_chunk: 1024,
            ended: false,
            byte_scratch: Vec::new(),
            block_align,
        })
    }

    #[inline] pub fn format(&self) -> PcmFormat { self.plan.format }

    pub fn set_frames_per_chunk(&mut self, frames: usize) -> Result<(), AudioError> {
        if frames == 0 { return Err(AudioError::custom("frames_per_chunk cannot be zero")); }
        let samples = frames.checked_mul(self.plan.format.channels as usize)
            .ok_or_else(|| AudioError::InvalidFormat("PCM chunk sample count overflow".into()))?;
        checked_allocation_len::<f32>(samples, "PCM output chunk")?;
        let bytes = frames.checked_mul(self.block_align)
            .ok_or_else(|| AudioError::InvalidFormat("PCM chunk byte count overflow".into()))?;
        checked_allocation_len::<u8>(bytes, "PCM input chunk")?;
        self.frames_per_chunk = frames;
        Ok(())
    }

    pub fn into_inner(self) -> R { self.reader }
}

impl Pcm<Cursor<Vec<u8>>> {
    pub fn from_bytes(bytes: Vec<u8>, format: PcmFormat) -> Result<Self, AudioError> {
        Self::new(Cursor::new(bytes), format)
    }
}

impl<R: Read> Decoder for Pcm<R> {
    fn sample_rate(&self) -> u32 { self.plan.format.sample_rate }
    fn channels(&self) -> u16 { self.plan.format.channels }

    fn maximum_frame_samples(&self) -> Option<usize> {
        self.frames_per_chunk.checked_mul(self.plan.format.channels as usize)
    }

    fn memory_usage(&self) -> DecoderMemoryUsage {
        let scratch = self.byte_scratch.capacity();
        DecoderMemoryUsage { persistent_state_bytes: core::mem::size_of_val(self), scratch_capacity_bytes: scratch, peak_scratch_capacity_bytes: scratch, ..DecoderMemoryUsage::default() }
    }
    fn trim_memory(&mut self) { self.byte_scratch.shrink_to_fit(); }

    fn decode_into<'a>(&mut self, output: &'a mut AudioFrameBuffer) -> Result<DecodedFrame<'a>, AudioError> {
        let _stage_timer = StageTimer::start("pcm.decode");
        if self.ended { return Err(AudioError::NoNextFrame); }
        let wanted = self.frames_per_chunk.checked_mul(self.block_align)
            .ok_or_else(|| AudioError::InvalidFormat("PCM chunk size overflow".into()))?;
        self.byte_scratch.resize(wanted, 0);
        let mut filled = 0usize;
        while filled < wanted {
            match self.reader.read(&mut self.byte_scratch[filled..wanted])? {
                0 => { self.ended = true; break; }
                n => filled += n,
            }
        }
        if filled == 0 { return Err(AudioError::NoNextFrame); }
        if filled % self.block_align != 0 { return Err(AudioError::TruncatedData); }
        self.plan.decode_into(&self.byte_scratch[..filled], output)?;
        Ok(DecodedFrame {
            samples: output.samples(),
            channels: self.plan.format.channels,
            sample_rate: self.plan.format.sample_rate,
        })
    }
}

#[cfg(feature = "demux-wav")]
/// PCM decoder backed by a RIFF/WAVE demuxer.
pub struct WavPcm<R: Read + Seek> {
    demuxer: WavDemuxer<R>,
    byte_scratch: Vec<u8>,
    plan: PcmDecodePlan,
}

#[cfg(feature = "demux-wav")]
impl<R: Read + Seek> WavPcm<R> {
    pub fn new(reader: R) -> Result<Self, AudioError> {
        let demuxer = WavDemuxer::new(reader)?;
        let f = *demuxer.format();
        let encoding = match (f.encoding, f.bits_per_sample) {
            (WavEncoding::Pcm, 8) => PcmEncoding::Unsigned8,
            (WavEncoding::Pcm, 9..=16) => PcmEncoding::Signed16Le,
            (WavEncoding::Pcm, 17..=24) => PcmEncoding::Signed24Le,
            (WavEncoding::Pcm, 25..=32) => PcmEncoding::Signed32Le,
            (WavEncoding::IeeeFloat, 32) => PcmEncoding::Float32Le,
            (WavEncoding::IeeeFloat, 64) => PcmEncoding::Float64Le,
            (_, bits) => return Err(AudioError::UnsupportedBitDepth(bits)),
        };
        let plan = PcmDecodePlan::new(PcmFormat {
            encoding,
            channels: f.channels,
            sample_rate: f.sample_rate,
            valid_bits_per_sample: f.valid_bits_per_sample,
        })?;
        let scratch_capacity = demuxer.frames_per_chunk().saturating_mul(f.block_align as usize);
        Ok(Self { demuxer, byte_scratch: Vec::with_capacity(scratch_capacity), plan })
    }

    pub fn demuxer(&self) -> &WavDemuxer<R> { &self.demuxer }
    pub fn demuxer_mut(&mut self) -> &mut WavDemuxer<R> { &mut self.demuxer }
    pub fn duration_seconds(&self) -> f64 { self.demuxer.duration_seconds() }
    pub fn seek_frame(&mut self, frame: u64) -> Result<(), AudioError> { self.demuxer.seek_frame(frame) }
    pub fn reset(&mut self) -> Result<(), AudioError> { self.demuxer.reset() }
}

#[cfg(feature = "demux-wav")]
impl WavPcm<Cursor<Vec<u8>>> {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, AudioError> { Self::new(Cursor::new(bytes)) }
}

#[cfg(feature = "demux-wav")]
impl<R: Read + Seek> Decoder for WavPcm<R> {
    fn sample_rate(&self) -> u32 { self.plan.format.sample_rate }
    fn channels(&self) -> u16 { self.plan.format.channels }
    fn maximum_frame_samples(&self) -> Option<usize> {
        self.demuxer.frames_per_chunk().checked_mul(self.channels() as usize)
    }
    fn memory_usage(&self) -> DecoderMemoryUsage {
        let scratch = self.byte_scratch.capacity();
        DecoderMemoryUsage { persistent_state_bytes: core::mem::size_of_val(self), scratch_capacity_bytes: scratch, peak_scratch_capacity_bytes: scratch, ..DecoderMemoryUsage::default() }
    }
    fn trim_memory(&mut self) { self.byte_scratch.shrink_to_fit(); }
    fn decode_into<'a>(&mut self, output: &'a mut AudioFrameBuffer) -> Result<DecodedFrame<'a>, AudioError> {
        let _stage_timer = StageTimer::start("wav-pcm.decode");
        self.demuxer.next_pcm_chunk_into(&mut self.byte_scratch)?;
        self.plan.decode_into(&self.byte_scratch, output)?;
        Ok(DecodedFrame { samples: output.samples(), channels: self.channels(), sample_rate: self.sample_rate() })
    }
}

#[cfg(feature = "demux-aiff")]
/// PCM decoder backed by an AIFF/AIFF-C demuxer.
pub struct AiffPcm<R: Read + Seek> {
    demuxer: AiffDemuxer<R>,
    byte_scratch: Vec<u8>,
    plan: PcmDecodePlan,
}

#[cfg(feature = "demux-aiff")]
impl<R: Read + Seek> AiffPcm<R> {
    pub fn new(reader: R) -> Result<Self, AudioError> {
        let demuxer = AiffDemuxer::new(reader)?;
        let f = *demuxer.format();
        let encoding = match (f.encoding, f.sample_size) {
            (AiffEncoding::PcmBigEndian, 1..=8) => PcmEncoding::Signed8,
            (AiffEncoding::PcmBigEndian, 9..=16) => PcmEncoding::Signed16Be,
            (AiffEncoding::PcmBigEndian, 17..=24) => PcmEncoding::Signed24Be,
            (AiffEncoding::PcmBigEndian, 25..=32) => PcmEncoding::Signed32Be,
            (AiffEncoding::PcmLittleEndian, 1..=8) => PcmEncoding::Signed8,
            (AiffEncoding::PcmLittleEndian, 9..=16) => PcmEncoding::Signed16Le,
            (AiffEncoding::PcmLittleEndian, 17..=24) => PcmEncoding::Signed24Le,
            (AiffEncoding::PcmLittleEndian, 25..=32) => PcmEncoding::Signed32Le,
            (_, bits) => return Err(AudioError::UnsupportedBitDepth(bits)),
        };
        let plan = PcmDecodePlan::new(PcmFormat {
            encoding,
            channels: f.channels,
            sample_rate: f.sample_rate,
            valid_bits_per_sample: f.sample_size,
        })?;
        let scratch_capacity = demuxer.frames_per_chunk().saturating_mul(f.block_align());
        Ok(Self { demuxer, byte_scratch: Vec::with_capacity(scratch_capacity), plan })
    }

    pub fn demuxer(&self) -> &AiffDemuxer<R> { &self.demuxer }
    pub fn demuxer_mut(&mut self) -> &mut AiffDemuxer<R> { &mut self.demuxer }
    pub fn duration_seconds(&self) -> f64 { self.demuxer.duration_seconds() }
    pub fn seek_frame(&mut self, frame: u64) -> Result<(), AudioError> { self.demuxer.seek_frame(frame) }
    pub fn reset(&mut self) -> Result<(), AudioError> { self.demuxer.reset() }
}

#[cfg(feature = "demux-aiff")]
impl AiffPcm<Cursor<Vec<u8>>> {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, AudioError> { Self::new(Cursor::new(bytes)) }
}

#[cfg(feature = "demux-aiff")]
impl<R: Read + Seek> Decoder for AiffPcm<R> {
    fn sample_rate(&self) -> u32 { self.plan.format.sample_rate }
    fn channels(&self) -> u16 { self.plan.format.channels }
    fn maximum_frame_samples(&self) -> Option<usize> {
        self.demuxer.frames_per_chunk().checked_mul(self.channels() as usize)
    }
    fn memory_usage(&self) -> DecoderMemoryUsage {
        let scratch = self.byte_scratch.capacity();
        DecoderMemoryUsage { persistent_state_bytes: core::mem::size_of_val(self), scratch_capacity_bytes: scratch, peak_scratch_capacity_bytes: scratch, ..DecoderMemoryUsage::default() }
    }
    fn trim_memory(&mut self) { self.byte_scratch.shrink_to_fit(); }
    fn decode_into<'a>(&mut self, output: &'a mut AudioFrameBuffer) -> Result<DecodedFrame<'a>, AudioError> {
        let _stage_timer = StageTimer::start("aiff-pcm.decode");
        self.demuxer.next_pcm_chunk_into(&mut self.byte_scratch)?;
        self.plan.decode_into(&self.byte_scratch, output)?;
        Ok(DecodedFrame { samples: output.samples(), channels: self.channels(), sample_rate: self.sample_rate() })
    }
}

fn pcm_output<'a>(data: &[u8], bytes_per_sample: usize, output: &'a mut AudioFrameBuffer) -> Result<&'a mut [f32], AudioError> {
    if data.len() % bytes_per_sample != 0 { return Err(AudioError::TruncatedData); }
    let count = data.len() / bytes_per_sample;
    checked_allocation_len::<f32>(count, "PCM decoded output")?;
    Ok(output.resize_for_decode(count))
}

fn pcm_decode_u8(data:&[u8], bits:u16, shift:u32, scale:f32, output:&mut AudioFrameBuffer)->Result<(),AudioError>{
    let out=pcm_output(data,1,output)?;
    if bits==8 { crate::simd::pcm_u8_to_f32_into(data,out); }
    else { let midpoint=1i16<<(bits-1); for(dst,&value) in out.iter_mut().zip(data){*dst=(((value>>shift) as i16-midpoint) as f32)/scale;} }
    Ok(())
}
fn pcm_decode_i8(data:&[u8], _bits:u16, shift:u32, scale:f32, output:&mut AudioFrameBuffer)->Result<(),AudioError>{let out=pcm_output(data,1,output)?;for(dst,&value)in out.iter_mut().zip(data){*dst=(((value as i8)>>shift)as f32)/scale;}Ok(())}
fn pcm_decode_i16_le(data:&[u8], _bits:u16, shift:u32, scale:f32, output:&mut AudioFrameBuffer)->Result<(),AudioError>{let out=pcm_output(data,2,output)?;crate::simd::pcm_i16_le_to_f32_into(data,out,shift,scale);Ok(())}
fn pcm_decode_i16_be(data:&[u8], _bits:u16, shift:u32, scale:f32, output:&mut AudioFrameBuffer)->Result<(),AudioError>{let out=pcm_output(data,2,output)?;crate::simd::pcm_i16_be_to_f32_into(data,out,shift,scale);Ok(())}
fn pcm_decode_i24_le(data:&[u8], bits:u16, _shift:u32, _scale:f32, output:&mut AudioFrameBuffer)->Result<(),AudioError>{let out=pcm_output(data,3,output)?;crate::simd::pcm_i24_le_to_f32_into(data,bits,out)}
fn pcm_decode_i24_be(data:&[u8], bits:u16, _shift:u32, _scale:f32, output:&mut AudioFrameBuffer)->Result<(),AudioError>{let out=pcm_output(data,3,output)?;crate::simd::pcm_i24_be_to_f32_into(data,bits,out)}
fn pcm_decode_i32_le(data:&[u8], bits:u16, _shift:u32, _scale:f32, output:&mut AudioFrameBuffer)->Result<(),AudioError>{let out=pcm_output(data,4,output)?;crate::simd::pcm_i32_le_to_f32_into(data,bits,out)}
fn pcm_decode_i32_be(data:&[u8], bits:u16, _shift:u32, _scale:f32, output:&mut AudioFrameBuffer)->Result<(),AudioError>{let out=pcm_output(data,4,output)?;crate::simd::pcm_i32_be_to_f32_into(data,bits,out)}
fn pcm_decode_f32_le(data:&[u8], _bits:u16, _shift:u32, _scale:f32, output:&mut AudioFrameBuffer)->Result<(),AudioError>{let out=pcm_output(data,4,output)?;crate::simd::pcm_f32_le_to_f32_into(data,out)}
fn pcm_decode_f32_be(data:&[u8], _bits:u16, _shift:u32, _scale:f32, output:&mut AudioFrameBuffer)->Result<(),AudioError>{let out=pcm_output(data,4,output)?;crate::simd::pcm_f32_be_to_f32_into(data,out)}
fn pcm_decode_f64_le(data:&[u8], _bits:u16, _shift:u32, _scale:f32, output:&mut AudioFrameBuffer)->Result<(),AudioError>{let out=pcm_output(data,8,output)?;crate::simd::pcm_f64_le_to_f32_into(data,out)}
fn pcm_decode_f64_be(data:&[u8], _bits:u16, _shift:u32, _scale:f32, output:&mut AudioFrameBuffer)->Result<(),AudioError>{let out=pcm_output(data,8,output)?;crate::simd::pcm_f64_be_to_f32_into(data,out)}

fn decode_interleaved_prevalidated(
    data: &[u8], format: PcmFormat, _bytes_per_sample: usize, _valid_shift: u32, _normalization_scale: f32,
    output: &mut AudioFrameBuffer,
) -> Result<(), AudioError> {
    PcmDecodePlan::new(format)?.decode_into(data, output)
}

pub fn decode_interleaved(data: &[u8], format: PcmFormat) -> Result<Vec<f32>, AudioError> {
    let mut output = AudioFrameBuffer::new();
    decode_interleaved_into(data, format, &mut output)?;
    Ok(output.into_samples())
}

pub fn decode_interleaved_into(data: &[u8], format: PcmFormat, output: &mut AudioFrameBuffer) -> Result<(), AudioError> {
    let plan = PcmDecodePlan::new(format)?;
    plan.decode_into(data, output)
}
