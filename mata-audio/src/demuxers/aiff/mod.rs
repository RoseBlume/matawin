//! AIFF/AIFF-C demuxer for uncompressed PCM streams.

mod extended;
mod format;

use std::io::{Cursor, ErrorKind, Read, Seek, SeekFrom};
use crate::{AudioError, demuxers::{DemuxerSeek, SeekCapability}};

pub use format::{AiffEncoding, AiffFormat};
use format::parse_comm_chunk;

const DEFAULT_TARGET_CHUNK_BYTES: usize = 32 * 1024;
const PLAYBACK_FRAME_QUANTUM: usize = 256;
const MIN_FRAMES_PER_CHUNK: usize = 256;
const MAX_FRAMES_PER_CHUNK: usize = 8192;

pub struct AiffDemuxer<R: Read + Seek> {
    reader: R,
    format: AiffFormat,
    data_start: u64,
    data_length: u64,
    data_read: u64,
    block_size: u32,
    block_align: u64,
    total_frames: u64,
    duration_seconds: f64,
    frames_per_chunk: usize,
}

impl<R: Read + Seek> AiffDemuxer<R> {
    pub fn new(mut reader: R) -> Result<Self, AudioError> {
        reader.seek(SeekFrom::Start(0))?;
        let mut form = [0u8; 12];
        read_exact_checked(&mut reader, &mut form)?;
        if &form[0..4] != b"FORM" { return Err(AudioError::InvalidHeader("Missing AIFF FORM signature".into())); }
        let is_aifc = &form[8..12] == b"AIFC";
        if &form[8..12] != b"AIFF" && !is_aifc {
            return Err(AudioError::InvalidHeader("FORM is not AIFF/AIFC".into()));
        }
        let form_size = u32::from_be_bytes(form[4..8].try_into().unwrap()) as u64;
        let form_end = 8u64.checked_add(form_size)
            .ok_or_else(|| AudioError::InvalidFormat("AIFF FORM size overflow".into()))?;
        let file_end = reader.seek(SeekFrom::End(0))?;
        let form_end = form_end.min(file_end);
        reader.seek(SeekFrom::Start(12))?;

        let mut format = None;
        let mut data_start = None;
        let mut available_bytes = None;
        let mut block_size = 0u32;
        let mut position = 12u64;

        while position.saturating_add(8) <= form_end {
            let mut h = [0u8; 8];
            read_exact_checked(&mut reader, &mut h)?;
            position = position.saturating_add(8);
            let id: [u8; 4] = h[0..4].try_into().unwrap();
            let size = u32::from_be_bytes(h[4..8].try_into().unwrap()) as u64;
            let start = position;
            let padded = size.checked_add(size & 1)
                .ok_or_else(|| AudioError::InvalidFormat("AIFF chunk size overflow".into()))?;
            let next = start.checked_add(padded)
                .ok_or_else(|| AudioError::InvalidFormat("AIFF chunk offset overflow".into()))?;
            if next > form_end { break; }

            match &id {
                b"COMM" => {
                    if size < 18 || size > 4096 {
                        return Err(AudioError::InvalidFormat(format!("Invalid AIFF COMM chunk size: {size}")));
                    }
                    let mut data = vec![0u8; size as usize];
                    read_exact_checked(&mut reader, &mut data)?;
                    format = Some(parse_comm_chunk(&data, is_aifc)?);
                }
                b"SSND" => {
                    if size < 8 { return Err(AudioError::InvalidFormat("AIFF SSND chunk is smaller than 8 bytes".into())); }
                    let mut header = [0u8; 8];
                    read_exact_checked(&mut reader, &mut header)?;
                    let offset = u32::from_be_bytes(header[0..4].try_into().unwrap()) as u64;
                    block_size = u32::from_be_bytes(header[4..8].try_into().unwrap());
                    let payload = size - 8;
                    if offset > payload { return Err(AudioError::InvalidFormat("AIFF SSND offset exceeds payload".into())); }
                    if data_start.is_none() {
                        data_start = Some(start + 8 + offset);
                        available_bytes = Some(payload - offset);
                    }
                }
                _ => {}
            }
            reader.seek(SeekFrom::Start(next))?;
            position = next;
        }

        let format = format.ok_or_else(|| AudioError::InvalidFormat("Missing AIFF COMM chunk".into()))?;
        let data_start = data_start.ok_or_else(|| AudioError::InvalidFormat("Missing AIFF SSND chunk".into()))?;
        let block_align = format.block_align() as u64;
        let total_frames = format.sample_frames as u64;
        let expected = total_frames.checked_mul(block_align)
            .ok_or_else(|| AudioError::InvalidFormat("AIFF data length overflow".into()))?;
        if available_bytes.unwrap_or(0) < expected { return Err(AudioError::TruncatedData); }
        let duration_seconds = total_frames as f64 / format.sample_rate as f64;
        let frames_per_chunk = tune_frames_per_chunk(block_align as usize);
        reader.seek(SeekFrom::Start(data_start))?;
        Ok(Self {
            reader,
            format,
            data_start,
            data_length: expected,
            data_read: 0,
            block_size,
            block_align,
            total_frames,
            duration_seconds,
            frames_per_chunk,
        })
    }

    #[inline] pub fn format(&self) -> &AiffFormat { &self.format }
    #[inline] pub fn block_size(&self) -> u32 { self.block_size }
    #[inline] pub fn total_frames(&self) -> u64 { self.total_frames }
    #[inline] pub fn current_frame(&self) -> u64 { self.data_read / self.block_align }
    #[inline] pub fn remaining_frames(&self) -> u64 { (self.data_length - self.data_read) / self.block_align }
    #[inline] pub fn duration_seconds(&self) -> f64 { self.duration_seconds }
    #[inline] pub fn frames_per_chunk(&self) -> usize { self.frames_per_chunk }

    pub fn set_frames_per_chunk(&mut self, frames: usize) -> Result<(), AudioError> {
        if frames == 0 { return Err(AudioError::custom("frames_per_chunk cannot be zero")); }
        self.frames_per_chunk = frames;
        Ok(())
    }

    /// Seek directly to an AIFF PCM frame in O(1) time.
    pub fn seek_frame(&mut self, frame: u64) -> Result<(), AudioError> {
        if frame > self.total_frames {
            return Err(AudioError::InvalidFormat(format!(
                "AIFF frame seek {frame} exceeds total frame count {}", self.total_frames
            )));
        }
        let byte_offset = frame.checked_mul(self.block_align)
            .ok_or_else(|| AudioError::InvalidFormat("AIFF seek offset overflow".into()))?;
        let absolute = self.data_start.checked_add(byte_offset)
            .ok_or_else(|| AudioError::InvalidFormat("AIFF seek offset overflow".into()))?;
        self.reader.seek(SeekFrom::Start(absolute))?;
        self.data_read = byte_offset;
        Ok(())
    }

    #[inline]
    pub fn reset(&mut self) -> Result<(), AudioError> { self.seek_frame(0) }

    pub fn next_pcm_chunk_into(&mut self, data: &mut Vec<u8>) -> Result<(), AudioError> {
        if self.data_read >= self.data_length { return Err(AudioError::NoNextFrame); }
        let remaining_bytes = self.data_length - self.data_read;
        let wanted_bytes = (self.frames_per_chunk as u64).checked_mul(self.block_align)
            .ok_or_else(|| AudioError::InvalidFormat("AIFF chunk size overflow".into()))?;
        let bytes_to_read = usize::try_from(remaining_bytes.min(wanted_bytes))
            .map_err(|_| AudioError::InvalidFormat("AIFF chunk too large".into()))?;
        data.resize(bytes_to_read, 0);
        read_exact_checked(&mut self.reader, data)?;
        self.data_read += bytes_to_read as u64;
        Ok(())
    }

    pub fn next_pcm_chunk(&mut self) -> Result<Vec<u8>, AudioError> {
        let mut data = Vec::with_capacity(self.frames_per_chunk.saturating_mul(self.format.block_align()));
        self.next_pcm_chunk_into(&mut data)?;
        Ok(data)
    }

    pub fn into_inner(self) -> R { self.reader }
}

impl AiffDemuxer<Cursor<Vec<u8>>> {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, AudioError> { Self::new(Cursor::new(bytes)) }
}

fn tune_frames_per_chunk(block_align: usize) -> usize {
    if block_align == 0 { return PLAYBACK_FRAME_QUANTUM; }
    let raw = (DEFAULT_TARGET_CHUNK_BYTES / block_align)
        .clamp(MIN_FRAMES_PER_CHUNK, MAX_FRAMES_PER_CHUNK);
    let aligned = (raw / PLAYBACK_FRAME_QUANTUM) * PLAYBACK_FRAME_QUANTUM;
    aligned.max(PLAYBACK_FRAME_QUANTUM)
}

fn read_exact_checked<R: Read>(reader: &mut R, buffer: &mut [u8]) -> Result<(), AudioError> {
    match reader.read_exact(buffer) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => Err(AudioError::TruncatedData),
        Err(e) => Err(AudioError::Io(e)),
    }
}

impl<R: Read + Seek> DemuxerSeek for AiffDemuxer<R> {
    fn seek_capability(&self) -> SeekCapability { SeekCapability::Exact }

    fn seek_time_seconds(&mut self, seconds: f64) -> Result<(), AudioError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(AudioError::InvalidFormat("AIFF seek time must be finite and non-negative".into()));
        }
        let frame = (seconds * self.format.sample_rate as f64).round() as u64;
        self.seek_frame(frame.min(self.total_frames))
    }

    fn position_seconds(&self) -> Option<f64> {
        Some(self.current_frame() as f64 / self.format.sample_rate.max(1) as f64)
    }
}
