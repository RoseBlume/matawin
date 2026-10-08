mod bit_reader;
mod crc;
mod frame;
mod residual;
mod subframe;

use std::io::{Cursor, Read, Seek};

use crate::{
    AudioError,
    codecs::{AudioFrameBuffer, DecodedFrame, Decoder, DecoderMemoryUsage},
};

pub use crate::demuxers::flac::{SeekPoint, StreamInfo};
use crate::demuxers::flac::{collect_seek_points, read_metadata_blocks};
use super::support::StageTimer;
use bit_reader::BitReader;
use frame::{decode_frame_into, FlacScratch};

const MAX_LAZY_INDEX_POINTS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlacIndexPoint {
    pub sample_number: u64,
    pub byte_offset: u64,
}

pub struct Flac<R: Read + Seek> {
    bits: BitReader<R>,
    stream_info: StreamInfo,
    audio_start: u64,
    decoded_samples: u64,
    scratch: FlacScratch,
    seek_points: Vec<SeekPoint>,
    frame_index: Vec<FlacIndexPoint>,
    index_sample_interval: u64,
    next_index_sample: u64,
}

impl<R: Read + Seek> Flac<R> {
    pub fn new(mut reader: R) -> Result<Self, AudioError> {
        let stream_start = reader.stream_position()?;
        let (stream_info, audio_start, blocks) = read_metadata_blocks(&mut reader, stream_start)?;
        let seek_points = collect_seek_points(&blocks)?;
        let scratch = FlacScratch::new(&stream_info)?;
        let index_sample_interval = stream_info.sample_rate.max(1) as u64;
        let bits = BitReader::new(reader, audio_start);
        Ok(Self {
            bits,
            stream_info,
            audio_start,
            decoded_samples: 0,
            scratch,
            seek_points,
            frame_index: Vec::new(),
            index_sample_interval,
            next_index_sample: 0,
        })
    }

    pub fn stream_info(&self) -> &StreamInfo { &self.stream_info }
    pub fn bits_per_sample(&self) -> u8 { self.stream_info.bits_per_sample }
    pub fn total_samples(&self) -> u64 { self.stream_info.total_samples }
    pub fn decoded_samples(&self) -> u64 { self.decoded_samples }
    pub fn seek_points(&self) -> &[SeekPoint] { &self.seek_points }
    pub fn frame_index(&self) -> &[FlacIndexPoint] { &self.frame_index }

    pub fn remaining_samples(&self) -> Option<u64> {
        if self.stream_info.total_samples == 0 {
            None
        } else {
            Some(self.stream_info.total_samples.saturating_sub(self.decoded_samples))
        }
    }

    pub fn duration_seconds(&self) -> Option<f64> {
        if self.stream_info.total_samples == 0 {
            None
        } else {
            Some(self.stream_info.total_samples as f64 / self.stream_info.sample_rate as f64)
        }
    }

    /// Reset without dropping any decoder scratch, SEEKTABLE data, or the
    /// lazily learned fallback frame index.
    pub fn reset(&mut self) -> Result<(), AudioError> {
        self.bits.seek_to(self.audio_start)?;
        self.decoded_samples = 0;
        self.next_index_sample = 0;
        Ok(())
    }

    /// Seek to the closest indexed FLAC frame at or before `sample`.
    ///
    /// The returned value is the exact sample number of the frame where decode
    /// will resume. SEEKTABLE points are preferred when they are closer than
    /// the lazily observed fallback frame index.
    pub fn seek_to_sample(&mut self, sample: u64) -> Result<u64, AudioError> {
        if self.stream_info.total_samples != 0 && sample > self.stream_info.total_samples {
            return Err(AudioError::InvalidFormat(format!(
                "FLAC sample seek {sample} exceeds total sample count {}",
                self.stream_info.total_samples
            )));
        }

        let mut best = FlacIndexPoint { sample_number: 0, byte_offset: self.audio_start };
        for point in &self.seek_points {
            if point.sample_number > sample { break; }
            let byte_offset = self.audio_start.checked_add(point.stream_offset)
                .ok_or_else(|| AudioError::InvalidFormat("FLAC SEEKTABLE offset overflow".into()))?;
            if point.sample_number >= best.sample_number {
                best = FlacIndexPoint { sample_number: point.sample_number, byte_offset };
            }
        }
        if let Some(point) = self.frame_index.iter().rev().find(|point| point.sample_number <= sample) {
            if point.sample_number >= best.sample_number { best = *point; }
        }

        self.bits.seek_to(best.byte_offset)?;
        self.decoded_samples = best.sample_number;
        self.next_index_sample = best.sample_number.saturating_add(self.index_sample_interval);
        Ok(best.sample_number)
    }

    fn record_frame_position(&mut self, sample_number: u64, byte_offset: u64) {
        if sample_number < self.next_index_sample && !self.frame_index.is_empty() { return; }
        match self.frame_index.binary_search_by_key(&sample_number, |point| point.sample_number) {
            Ok(_) => return,
            Err(position) => self.frame_index.insert(position, FlacIndexPoint { sample_number, byte_offset }),
        }
        self.next_index_sample = sample_number.saturating_add(self.index_sample_interval);

        // Keep the lazy index bounded. If it grows unusually large, retain
        // every second point and double the sampling interval.
        if self.frame_index.len() > MAX_LAZY_INDEX_POINTS {
            let mut write = 1usize;
            for read in (2..self.frame_index.len()).step_by(2) {
                self.frame_index[write] = self.frame_index[read];
                write += 1;
            }
            self.frame_index.truncate(write);
            self.index_sample_interval = self.index_sample_interval.saturating_mul(2).max(1);
            self.next_index_sample = sample_number.saturating_add(self.index_sample_interval);
        }
    }
}

impl Flac<Cursor<Vec<u8>>> {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, AudioError> { Self::new(Cursor::new(bytes)) }
}

impl<R: Read + Seek> Decoder for Flac<R> {
    fn sample_rate(&self) -> u32 { self.stream_info.sample_rate }
    fn channels(&self) -> u16 { self.stream_info.channels as u16 }

    fn maximum_frame_samples(&self) -> Option<usize> {
        (self.stream_info.max_block_size as usize).checked_mul(self.stream_info.channels as usize)
    }

    fn memory_usage(&self) -> DecoderMemoryUsage {
        let scratch = self.scratch.allocated_bytes();
        DecoderMemoryUsage {
            persistent_state_bytes: core::mem::size_of_val(self),
            scratch_capacity_bytes: scratch,
            compressed_capacity_bytes: self.bits.buffered_capacity_bytes(),
            index_capacity_bytes: self.frame_index.capacity().saturating_mul(core::mem::size_of::<FlacIndexPoint>())
                .saturating_add(self.seek_points.capacity().saturating_mul(core::mem::size_of::<SeekPoint>())),
            metadata_capacity_bytes: 0,
            peak_scratch_capacity_bytes: self.scratch.peak_allocated_bytes(),
            peak_compressed_capacity_bytes: self.bits.buffered_capacity_bytes(),
        }
    }

    fn trim_memory(&mut self) {
        self.scratch.trim_memory();
        self.frame_index.shrink_to_fit();
        self.seek_points.shrink_to_fit();
    }

    fn decode_into<'a>(&mut self, output: &'a mut AudioFrameBuffer) -> Result<DecodedFrame<'a>, AudioError> {
        let _stage_timer = StageTimer::start("flac.decode");
        let total = self.stream_info.total_samples;
        if total != 0 && self.decoded_samples >= total { return Err(AudioError::NoNextFrame); }
        if total == 0 && self.bits.is_eof()? { return Err(AudioError::NoNextFrame); }

        let frame_byte_offset = self.bits.byte_position();
        let frame_sample = self.decoded_samples;
        self.record_frame_position(frame_sample, frame_byte_offset);

        let decoded = decode_frame_into(&mut self.bits, &self.stream_info, &mut self.scratch, output)?;
        let frame_samples = decoded.block_size as u64;
        let new_total = self.decoded_samples.checked_add(frame_samples)
            .ok_or_else(|| AudioError::InvalidFormat("Decoded FLAC sample count overflow".into()))?;
        if total != 0 && new_total > total {
            return Err(AudioError::InvalidFormat(format!(
                "FLAC frames contain more samples than STREAMINFO declares ({new_total} > {total})"
            )));
        }
        self.decoded_samples = new_total;
        Ok(DecodedFrame { samples: output.samples(), channels: decoded.channels, sample_rate: decoded.sample_rate })
    }
}
