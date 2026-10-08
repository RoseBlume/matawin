//! Audio codec implementations.
//!
//! This module contains decoding only. Container parsing belongs in
//! [`crate::demuxers`], while tags and descriptive data belong in
//! [`crate::metadata`]. Every codec implementation is independently feature
//! gated; use `all-codecs` (enabled by default) to compile the complete set.

use crate::AudioError;

pub(crate) mod support;

#[cfg(feature = "offline")]
pub mod offline;


#[cfg(feature = "benchmarking")]
pub mod benchmark_support {
    //! Stable wrappers around private buffering primitives for the benchmark binary.
    use std::{hint::black_box, io::{Cursor, Read}};
    use super::support::{BitReservoir64, BitReservoir128, ByteWindow, TunedBufReader};

    pub fn sequential_read(bytes: usize, rounds: usize) -> u64 {
        let source = vec![0xA5u8; bytes.max(1)];
        let mut checksum = 0u64;
        for _ in 0..rounds.max(1) {
            let cursor = Cursor::new(source.as_slice());
            let mut reader = TunedBufReader::with_capacity(64 * 1024, cursor);
            let mut scratch = [0u8; 16 * 1024];
            loop {
                let n = reader.read(&mut scratch).unwrap();
                if n == 0 { break; }
                checksum = checksum.wrapping_add(scratch[..n].iter().map(|&b| b as u64).sum::<u64>());
            }
        }
        black_box(checksum)
    }

    pub fn prefix_consume(bytes: usize, chunk: usize, rounds: usize) -> u64 {
        let data = vec![0x5Au8; bytes.max(chunk.max(1))];
        let mut checksum = 0u64;
        for _ in 0..rounds.max(1) {
            let mut window = ByteWindow::with_capacity(data.len());
            window.extend_from_slice(&data);
            while window.len() > chunk.max(1) {
                checksum ^= window.as_slice()[0] as u64;
                window.consume(chunk.max(1).min(window.len())).unwrap();
            }
        }
        black_box(checksum)
    }

    pub fn compact_refill(bytes: usize, rounds: usize) -> u64 {
        let source = vec![0x3Cu8; bytes.max(64 * 1024)];
        let mut checksum = 0u64;
        for _ in 0..rounds.max(1) {
            let mut reader = Cursor::new(source.as_slice());
            let mut window = ByteWindow::with_capacity(32 * 1024);
            while window.read_from(&mut reader, 32 * 1024, 8 * 1024).unwrap() != 0 || !window.is_empty() {
                if window.is_empty() { break; }
                checksum = checksum.wrapping_add(window.as_slice()[0] as u64);
                let consume = (window.len() / 2).max(1);
                window.consume(consume).unwrap();
            }
        }
        black_box(checksum)
    }

    pub fn bit_reservoir64(bytes: usize, rounds: usize) -> u64 {
        let data = vec![0x69u8; bytes.max(8)];
        let mut sum = 0u64;
        for _ in 0..rounds.max(1) {
            let mut bits = BitReservoir64::new(&data);
            while bits.bits_remaining() >= 24 {
                sum = sum.wrapping_add(bits.read_u24().unwrap() as u64);
            }
        }
        black_box(sum)
    }

    pub fn bit_reservoir128(bytes: usize, rounds: usize) -> u128 {
        let data = vec![0x96u8; bytes.max(16)];
        let mut sum = 0u128;
        for _ in 0..rounds.max(1) {
            let mut bits = BitReservoir128::new(&data);
            let mut remaining = data.len() * 8;
            while remaining >= 48 {
                sum = sum.wrapping_add(bits.read_bits(48).unwrap());
                remaining -= 48;
            }
        }
        black_box(sum)
    }

    #[cfg(feature = "perf-instrumentation")]
    pub fn stage_snapshot() -> Vec<(&'static str, u64, u128, u128)> {
        super::support::instrumentation::snapshot()
            .into_iter()
            .map(|(name, stats)| (name, stats.calls, stats.total.as_nanos(), stats.max.as_nanos()))
            .collect()
    }

    #[cfg(feature = "perf-instrumentation")]
    pub fn clear_stage_timings() { super::support::instrumentation::clear(); }
}

#[cfg(feature = "codec-flac")]
pub mod flac;
#[cfg(any(feature = "codec-mp1", feature = "codec-mp2", feature = "codec-mp3"))]
pub mod mpeg;
#[cfg(feature = "codec-pcm")]
pub mod pcm;
#[cfg(feature = "codec-vorbis")]
pub mod vorbis;

#[cfg(feature = "codec-flac")]
pub use flac::Flac;
#[cfg(any(feature = "codec-mp1", feature = "codec-mp2", feature = "codec-mp3"))]
pub use mpeg::MpegFrameInfo;
#[cfg(feature = "codec-mp1")]
pub use mpeg::Mp1;
#[cfg(feature = "codec-mp2")]
pub use mpeg::Mp2;
#[cfg(feature = "codec-mp3")]
pub use mpeg::Mp3;
#[cfg(feature = "codec-pcm")]
pub use pcm::{Pcm, PcmEncoding, PcmFormat};
#[cfg(all(feature = "codec-pcm", feature = "demux-aiff"))]
pub use pcm::AiffPcm;
#[cfg(all(feature = "codec-pcm", feature = "demux-wav"))]
pub use pcm::WavPcm;
#[cfg(feature = "codec-vorbis")]
pub use vorbis::Vorbis;

/// Decoder-owned memory accounting used by performance tooling and long-run
/// memory-flatness checks. Capacities, rather than current logical lengths,
/// are reported so the values represent retained/high-water working sets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DecoderMemoryUsage {
    pub persistent_state_bytes: usize,
    pub scratch_capacity_bytes: usize,
    pub compressed_capacity_bytes: usize,
    pub index_capacity_bytes: usize,
    pub metadata_capacity_bytes: usize,
    pub peak_scratch_capacity_bytes: usize,
    pub peak_compressed_capacity_bytes: usize,
}

impl DecoderMemoryUsage {
    #[inline]
    pub fn retained_bytes(self) -> usize {
        self.persistent_state_bytes
            .saturating_add(self.scratch_capacity_bytes)
            .saturating_add(self.compressed_capacity_bytes)
            .saturating_add(self.index_capacity_bytes)
            .saturating_add(self.metadata_capacity_bytes)
    }

    #[inline]
    pub fn high_water_bytes(self) -> usize {
        self.persistent_state_bytes
            .saturating_add(self.peak_scratch_capacity_bytes.max(self.scratch_capacity_bytes))
            .saturating_add(self.peak_compressed_capacity_bytes.max(self.compressed_capacity_bytes))
            .saturating_add(self.index_capacity_bytes)
            .saturating_add(self.metadata_capacity_bytes)
    }
}

#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
}

impl AudioFrame {
    pub fn new(samples: Vec<f32>, channels: u16, sample_rate: u32) -> Self {
        Self { samples, channels, sample_rate }
    }

    #[inline]
    pub fn sample_count(&self) -> usize { self.samples.len() }

    #[inline]
    pub fn frame_count(&self) -> usize {
        if self.channels == 0 { 0 } else { self.samples.len() / self.channels as usize }
    }

    #[inline]
    pub fn is_empty(&self) -> bool { self.samples.is_empty() }

    pub fn samples_as<T>(&self) -> Vec<T>
    where
        T: FromF32Sample,
    {
        let mut output = Vec::with_capacity(self.samples.len());
        output.extend(self.samples.iter().copied().map(T::from_f32_sample));
        output
    }

    pub fn into_samples_as<T>(self) -> Vec<T>
    where
        T: FromF32Sample,
    {
        let mut output = Vec::with_capacity(self.samples.len());
        output.extend(self.samples.into_iter().map(T::from_f32_sample));
        output
    }

    pub fn write_samples_as<T>(&self, output: &mut [T]) -> Result<usize, AudioError>
    where
        T: FromF32Sample,
    {
        if output.len() < self.samples.len() {
            return Err(AudioError::custom(format!(
                "Output buffer has {} samples but {} are required",
                output.len(), self.samples.len(),
            )));
        }
        for (output, sample) in output.iter_mut().zip(self.samples.iter().copied()) {
            *output = T::from_f32_sample(sample);
        }
        Ok(self.samples.len())
    }
}

/// Reusable owned output storage for [`Decoder::decode_into`].
///
/// The allocation is intentionally retained when a frame is cleared or a
/// decoder is reset. Code that decodes in a loop should create one buffer and
/// reuse it for the lifetime of the stream.
#[derive(Debug, Clone)]
pub struct AudioFrameBuffer {
    samples: Vec<f32>,
    peak_capacity: usize,
}

impl Default for AudioFrameBuffer {
    fn default() -> Self { Self { samples: Vec::new(), peak_capacity: 0 } }
}

impl AudioFrameBuffer {
    pub fn new() -> Self { Self::default() }
    pub fn with_capacity(samples: usize) -> Self { Self { samples: Vec::with_capacity(samples), peak_capacity: samples } }
    #[inline] pub fn samples(&self) -> &[f32] { &self.samples }
    #[inline] pub fn samples_mut(&mut self) -> &mut [f32] { &mut self.samples }
    #[inline] pub fn len(&self) -> usize { self.samples.len() }
    #[inline] pub fn is_empty(&self) -> bool { self.samples.is_empty() }
    #[inline] pub fn capacity(&self) -> usize { self.samples.capacity() }
    #[inline] pub fn peak_capacity(&self) -> usize { self.peak_capacity }
    #[inline] pub fn retained_bytes(&self) -> usize { self.samples.capacity().saturating_mul(core::mem::size_of::<f32>()) }
    #[inline] pub fn peak_bytes(&self) -> usize { self.peak_capacity.saturating_mul(core::mem::size_of::<f32>()) }
    #[inline] pub fn clear(&mut self) { self.samples.clear(); }
    pub fn reserve(&mut self, additional: usize) { self.samples.reserve(additional); self.peak_capacity = self.peak_capacity.max(self.samples.capacity()); }

    /// Explicit non-real-time compaction. Normal decode/reset paths never call
    /// this so steady-state buffers remain reusable.
    pub fn shrink_to_fit_non_realtime(&mut self) { self.samples.shrink_to_fit(); self.peak_capacity = self.peak_capacity.max(self.samples.capacity()); }

    /// Resizes without shrinking capacity. Decoders can then fill the returned
    /// slice directly, avoiding a fresh PCM allocation for each frame.
    pub fn resize_for_decode(&mut self, samples: usize) -> &mut [f32] {
        if samples > self.samples.capacity() {
            let geometric = self.samples.capacity().max(64).saturating_mul(2);
            let target = samples.max(geometric);
            self.samples.reserve_exact(target.saturating_sub(self.samples.len()));
        }
        self.samples.resize(samples, 0.0);
        self.peak_capacity = self.peak_capacity.max(self.samples.capacity());
        &mut self.samples
    }

    pub fn truncate(&mut self, samples: usize) { self.samples.truncate(samples); }

    /// Swaps the retained PCM allocation with another vector. This is useful
    /// when ownership can move directly into the next pipeline stage: the
    /// stage's old allocation comes back here for reuse on the next decode.
    pub fn swap_samples(&mut self, other: &mut Vec<f32>) {
        std::mem::swap(&mut self.samples, other);
        self.peak_capacity = self.peak_capacity.max(self.samples.capacity());
    }

    /// Moves an already-decoded vector into the reusable frame container. This
    /// is used by legacy codec internals while they are migrated to direct
    /// output; no PCM copy occurs.
    pub(crate) fn replace_samples(&mut self, mut samples: Vec<f32>) {
        std::mem::swap(&mut self.samples, &mut samples);
        self.peak_capacity = self.peak_capacity.max(self.samples.capacity());
        // `samples` now owns the previous allocation and is dropped. Codecs
        // with direct decode paths never take this branch.
    }

    pub fn into_samples(self) -> Vec<f32> { self.samples }
}

/// Metadata describing one decoded frame without owning its PCM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameInfo {
    pub channels: u16,
    pub sample_rate: u32,
    pub sample_count: usize,
}

impl FrameInfo {
    #[inline]
    pub fn frame_count(self) -> usize {
        if self.channels == 0 { 0 } else { self.sample_count / self.channels as usize }
    }
}

/// Borrowed view returned by [`Decoder::decode_into`].
#[derive(Debug, Clone, Copy)]
pub struct DecodedFrame<'a> {
    pub samples: &'a [f32],
    pub channels: u16,
    pub sample_rate: u32,
}

impl<'a> DecodedFrame<'a> {
    #[inline] pub fn sample_count(self) -> usize { self.samples.len() }
    #[inline] pub fn frame_count(self) -> usize {
        if self.channels == 0 { 0 } else { self.samples.len() / self.channels as usize }
    }
    #[inline] pub fn info(self) -> FrameInfo {
        FrameInfo { channels: self.channels, sample_rate: self.sample_rate, sample_count: self.samples.len() }
    }
}

/// Streaming decoding interface.
///
/// `decode_into` is the allocation-conscious primary path. `next_frame`
/// remains as the compatibility API and is implemented as a wrapper around a
/// temporary [`AudioFrameBuffer`].
pub trait Decoder {
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> u16;

    fn decode_into<'a>(
        &mut self,
        output: &'a mut AudioFrameBuffer,
    ) -> Result<DecodedFrame<'a>, AudioError>;

    fn next_frame(&mut self) -> Result<AudioFrame, AudioError> {
        let initial = self.preferred_frame_samples().unwrap_or(0);
        let mut output = AudioFrameBuffer::with_capacity(initial);
        let (channels, sample_rate) = {
            let decoded = self.decode_into(&mut output)?;
            (decoded.channels, decoded.sample_rate)
        };
        Ok(AudioFrame::new(output.into_samples(), channels, sample_rate))
    }

    /// Maximum interleaved sample count produced by one decode call when it is
    /// known from stream headers.
    fn maximum_frame_samples(&self) -> Option<usize> { None }

    /// Preferred interleaved output capacity for steady-state decoding.
    fn preferred_frame_samples(&self) -> Option<usize> { self.maximum_frame_samples() }

    /// Fast reciprocal cached by callers that repeatedly convert sample counts
    /// to seconds.
    #[inline]
    fn sample_rate_reciprocal(&self) -> f64 { 1.0 / self.sample_rate().max(1) as f64 }

    #[inline]
    fn channel_count_usize(&self) -> usize { self.channels() as usize }

    /// Optional native sample-index seek. `Ok(None)` means the decoder has no
    /// format-native seek path and a caller may choose a decode-forward fallback.
    fn seek_to_sample(&mut self, _sample: u64) -> Result<Option<u64>, AudioError> { Ok(None) }

    /// Optional native timestamp seek layered on the sample-index capability.
    fn seek_to_seconds(&mut self, seconds: f64) -> Result<Option<f64>, AudioError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(AudioError::InvalidFormat("seek time must be finite and non-negative".into()));
        }
        let sample = (seconds * self.sample_rate().max(1) as f64).round() as u64;
        Ok(self.seek_to_sample(sample)?.map(|actual| actual as f64 * self.sample_rate_reciprocal()))
    }

    /// Decoder-owned retained/high-water memory accounting. Implementations
    /// override this when they own dynamic scratch, compressed windows or
    /// indexes.
    fn memory_usage(&self) -> DecoderMemoryUsage {
        DecoderMemoryUsage { persistent_state_bytes: core::mem::size_of_val(self), ..DecoderMemoryUsage::default() }
    }

    /// Explicit non-real-time compaction hook. Decode, reset and seek paths do
    /// not shrink by default, preserving steady-state capacity.
    fn trim_memory(&mut self) {}

    /// Reset using the native seek path when one exists.
    fn reset_decoder(&mut self) -> Result<bool, AudioError> { Ok(self.seek_to_sample(0)?.is_some()) }
}

/// Converts normalized `f32` PCM samples into another sample representation.
///
/// Integer conversions clamp the input to `[-1.0, 1.0]`. NaN maps to
/// silence (or the unsigned midpoint).
pub trait FromF32Sample: Sized {
    fn from_f32_sample(sample: f32) -> Self;
}

impl FromF32Sample for f32 {
    #[inline]
    fn from_f32_sample(sample: f32) -> Self { sample }
}

impl FromF32Sample for f64 {
    #[inline]
    fn from_f32_sample(sample: f32) -> Self { sample as f64 }
}

macro_rules! impl_signed_sample {
    ($type:ty) => {
        impl FromF32Sample for $type {
            #[inline]
            fn from_f32_sample(sample: f32) -> Self {
                if sample.is_nan() { return 0; }
                let sample = sample.clamp(-1.0, 1.0);
                if sample <= -1.0 { return <$type>::MIN; }
                if sample >= 1.0 { return <$type>::MAX; }
                if sample >= 0.0 {
                    (sample as f64 * <$type>::MAX as f64) as $type
                } else {
                    (sample as f64 * -(<$type>::MIN as f64)) as $type
                }
            }
        }
    };
}

impl_signed_sample!(i8);
impl_signed_sample!(i16);
impl_signed_sample!(i32);
impl_signed_sample!(i64);

macro_rules! impl_unsigned_sample {
    ($type:ty) => {
        impl FromF32Sample for $type {
            #[inline]
            fn from_f32_sample(sample: f32) -> Self {
                if sample.is_nan() {
                    return ((<$type>::MAX as u128 + 1) / 2) as $type;
                }
                let normalized = (sample.clamp(-1.0, 1.0) as f64 + 1.0) * 0.5;
                (normalized * <$type>::MAX as f64).round() as $type
            }
        }
    };
}

impl_unsigned_sample!(u8);
impl_unsigned_sample!(u16);
impl_unsigned_sample!(u32);
impl_unsigned_sample!(u64);

#[cfg(test)]
mod allocation_api_tests {
    use super::{AudioFrameBuffer, DecodedFrame, Decoder};
    use crate::AudioError;

    struct DummyDecoder { emitted: bool }
    impl Decoder for DummyDecoder {
        fn sample_rate(&self) -> u32 { 48_000 }
        fn channels(&self) -> u16 { 2 }
        fn maximum_frame_samples(&self) -> Option<usize> { Some(8) }
        fn decode_into<'a>(&mut self, output: &'a mut AudioFrameBuffer) -> Result<DecodedFrame<'a>, AudioError> {
            if self.emitted { return Err(AudioError::NoNextFrame); }
            self.emitted = true;
            let samples = output.resize_for_decode(8);
            for (index, sample) in samples.iter_mut().enumerate() { *sample = index as f32 * 0.125; }
            Ok(DecodedFrame { samples: output.samples(), channels: 2, sample_rate: 48_000 })
        }
    }

    #[test]
    fn reusable_frame_buffer_preserves_capacity() {
        let mut buffer = AudioFrameBuffer::with_capacity(4);
        buffer.resize_for_decode(33);
        let capacity = buffer.capacity();
        buffer.clear();
        buffer.resize_for_decode(8);
        assert_eq!(buffer.capacity(), capacity);
    }

    #[test]
    fn next_frame_is_a_compatibility_wrapper() {
        let mut decoder = DummyDecoder { emitted: false };
        let frame = decoder.next_frame().unwrap();
        assert_eq!(frame.channels, 2);
        assert_eq!(frame.sample_rate, 48_000);
        assert_eq!(frame.samples.len(), 8);
        assert_eq!(frame.samples[7], 0.875);
    }
}
