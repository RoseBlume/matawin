//! RIFF/WAVE demuxer.

mod format;

use std::io::{Cursor, ErrorKind, Read, Seek, SeekFrom};
use crate::{AudioError, demuxers::{DemuxerSeek, SeekCapability}};

pub use format::{WavEncoding, WavFormat};
use format::parse_fmt_chunk;

const DEFAULT_TARGET_CHUNK_BYTES: usize = 32 * 1024;
const PLAYBACK_FRAME_QUANTUM: usize = 256;
const MIN_FRAMES_PER_CHUNK: usize = 256;
const MAX_FRAMES_PER_CHUNK: usize = 8192;

/// Streaming RIFF/WAVE demuxer for PCM/IEEE-float payloads.
pub struct WavDemuxer<R: Read + Seek> {
    reader: R,
    format: WavFormat,
    data_start: u64,
    data_length: u64,
    data_read: u64,
    total_frames: u64,
    duration_seconds: f64,
    block_align: u64,
    frames_per_chunk: usize,
}

impl<R: Read + Seek> WavDemuxer<R> {
    pub fn new(mut reader: R) -> Result<Self, AudioError> {
        reader.seek(SeekFrom::Start(0))?;
        let mut riff_header = [0u8; 12];
        read_exact_checked(&mut reader, &mut riff_header)?;
        if &riff_header[0..4] != b"RIFF" && &riff_header[0..4] != b"RF64" {
            return Err(AudioError::InvalidHeader("Missing RIFF/RF64 signature".into()));
        }
        if &riff_header[8..12] != b"WAVE" {
            return Err(AudioError::InvalidHeader("RIFF file is not WAVE".into()));
        }

        let riff_size = u32::from_le_bytes(riff_header[4..8].try_into().unwrap()) as u64;
        let file_end = reader.seek(SeekFrom::End(0))?;
        let riff_end = if &riff_header[0..4] == b"RF64" || riff_size == u32::MAX as u64 {
            file_end
        } else {
            8u64.checked_add(riff_size)
                .ok_or_else(|| AudioError::InvalidFormat("RIFF size overflow".into()))?
                .min(file_end)
        };
        reader.seek(SeekFrom::Start(12))?;

        let mut format = None;
        let mut data_start = None;
        let mut data_length = None;
        let mut ds64_data_size = None;
        let mut position = 12u64;

        while position.saturating_add(8) <= riff_end {
            let mut chunk_header = [0u8; 8];
            read_exact_checked(&mut reader, &mut chunk_header)?;
            position = position.saturating_add(8);
            let chunk_id: [u8; 4] = chunk_header[0..4].try_into().unwrap();
            let raw_size = u32::from_le_bytes(chunk_header[4..8].try_into().unwrap()) as u64;
            let chunk_data_start = position;

            if &chunk_id == b"ds64" && raw_size >= 28 {
                let mut ds64 = [0u8; 28];
                read_exact_checked(&mut reader, &mut ds64)?;
                ds64_data_size = Some(u64::from_le_bytes(ds64[8..16].try_into().unwrap()));
            } else if &chunk_id == b"fmt " {
                if raw_size > 4096 {
                    return Err(AudioError::InvalidFormat(format!("Unreasonably large fmt chunk: {raw_size}")));
                }
                let mut data = vec![0u8; raw_size as usize];
                read_exact_checked(&mut reader, &mut data)?;
                format = Some(parse_fmt_chunk(&data)?);
            } else if &chunk_id == b"data" && data_start.is_none() {
                data_start = Some(chunk_data_start);
                data_length = Some(if raw_size == u32::MAX as u64 {
                    ds64_data_size.unwrap_or(file_end.saturating_sub(chunk_data_start))
                } else {
                    raw_size
                });
            }

            let effective_size = if &chunk_id == b"data" && raw_size == u32::MAX as u64 {
                data_length.unwrap_or(0)
            } else {
                raw_size
            };
            let padded = effective_size.checked_add(effective_size & 1)
                .ok_or_else(|| AudioError::InvalidFormat("WAV chunk size overflow".into()))?;
            let next = chunk_data_start.checked_add(padded)
                .ok_or_else(|| AudioError::InvalidFormat("WAV chunk offset overflow".into()))?;
            if next > riff_end { break; }
            reader.seek(SeekFrom::Start(next))?;
            position = next;
        }

        let format = format.ok_or_else(|| AudioError::InvalidFormat("Missing fmt chunk".into()))?;
        let data_start = data_start.ok_or_else(|| AudioError::InvalidFormat("Missing data chunk".into()))?;
        let data_length = data_length.unwrap_or(0);
        let block_align = format.block_align as u64;
        if data_length % block_align != 0 {
            return Err(AudioError::InvalidFormat(format!(
                "data chunk size ({data_length}) is not divisible by block alignment ({})",
                format.block_align
            )));
        }
        let total_frames = data_length / block_align;
        let duration_seconds = total_frames as f64 / format.sample_rate as f64;
        let frames_per_chunk = tune_frames_per_chunk(format.block_align as usize);
        reader.seek(SeekFrom::Start(data_start))?;
        Ok(Self {
            reader,
            format,
            data_start,
            data_length,
            data_read: 0,
            total_frames,
            duration_seconds,
            block_align,
            frames_per_chunk,
        })
    }

    #[inline] pub fn format(&self) -> &WavFormat { &self.format }
    #[inline] pub fn data_length(&self) -> u64 { self.data_length }
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

    /// Seek directly to a PCM frame in O(1) time.
    pub fn seek_frame(&mut self, frame: u64) -> Result<(), AudioError> {
        if frame > self.total_frames {
            return Err(AudioError::InvalidFormat(format!(
                "WAV frame seek {frame} exceeds total frame count {}", self.total_frames
            )));
        }
        let byte_offset = frame.checked_mul(self.block_align)
            .ok_or_else(|| AudioError::InvalidFormat("WAV seek offset overflow".into()))?;
        let absolute = self.data_start.checked_add(byte_offset)
            .ok_or_else(|| AudioError::InvalidFormat("WAV seek offset overflow".into()))?;
        self.reader.seek(SeekFrom::Start(absolute))?;
        self.data_read = byte_offset;
        Ok(())
    }

    #[inline]
    pub fn reset(&mut self) -> Result<(), AudioError> { self.seek_frame(0) }

    /// Fill a caller-owned scratch buffer with the next complete PCM frame span.
    pub fn next_pcm_chunk_into(&mut self, data: &mut Vec<u8>) -> Result<(), AudioError> {
        if self.data_read >= self.data_length { return Err(AudioError::NoNextFrame); }
        let remaining_bytes = self.data_length - self.data_read;
        let wanted_frames = self.frames_per_chunk as u64;
        let wanted_bytes = wanted_frames.checked_mul(self.block_align)
            .ok_or_else(|| AudioError::InvalidFormat("WAV chunk size overflow".into()))?;
        let bytes_to_read = remaining_bytes.min(wanted_bytes);
        let bytes_to_read = usize::try_from(bytes_to_read)
            .map_err(|_| AudioError::InvalidFormat("WAV chunk too large".into()))?;
        data.resize(bytes_to_read, 0);
        read_exact_checked(&mut self.reader, data)?;
        self.data_read += bytes_to_read as u64;
        Ok(())
    }

    pub fn next_pcm_chunk(&mut self) -> Result<Vec<u8>, AudioError> {
        let mut data = Vec::with_capacity(self.frames_per_chunk.saturating_mul(self.format.block_align as usize));
        self.next_pcm_chunk_into(&mut data)?;
        Ok(data)
    }

    pub fn into_inner(self) -> R { self.reader }
}

impl WavDemuxer<Cursor<Vec<u8>>> {
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
        Err(error) if error.kind() == ErrorKind::UnexpectedEof => Err(AudioError::TruncatedData),
        Err(error) => Err(AudioError::Io(error)),
    }
}

impl<R: Read + Seek> DemuxerSeek for WavDemuxer<R> {
    fn seek_capability(&self) -> SeekCapability { SeekCapability::Exact }

    fn seek_time_seconds(&mut self, seconds: f64) -> Result<(), AudioError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(AudioError::InvalidFormat("WAV seek time must be finite and non-negative".into()));
        }
        let frame = (seconds * self.format.sample_rate as f64).round() as u64;
        self.seek_frame(frame.min(self.total_frames))
    }

    fn position_seconds(&self) -> Option<f64> {
        Some(self.current_frame() as f64 / self.format.sample_rate.max(1) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::tune_frames_per_chunk;

    #[test]
    fn chunk_tuning_is_playback_aligned() {
        for align in [1usize, 2, 3, 4, 6, 8, 16, 24, 32] {
            let frames = tune_frames_per_chunk(align);
            assert!(frames >= 256);
            assert_eq!(frames % 256, 0);
            assert!(frames * align <= 64 * 1024 || frames == 256);
        }
    }
}
