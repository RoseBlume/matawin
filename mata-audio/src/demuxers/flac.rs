//! Native FLAC stream demuxing and metadata-block traversal.

use std::io::{Cursor, Read, Seek, SeekFrom};
use crate::{AudioError, demuxers::{DemuxerSeek, SeekCapability}};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamInfo {
    pub min_block_size: u16,
    pub max_block_size: u16,
    pub min_frame_size: u32,
    pub max_frame_size: u32,
    pub sample_rate: u32,
    pub channels: u8,
    pub bits_per_sample: u8,
    pub total_samples: u64,
    pub md5: [u8; 16],
}

/// One FLAC SEEKTABLE entry. `stream_offset` is relative to the first audio
/// frame, as defined by the FLAC format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeekPoint {
    pub sample_number: u64,
    pub stream_offset: u64,
    pub frame_samples: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataBlock {
    pub block_type: u8,
    pub is_last: bool,
    pub data: Box<[u8]>,
}

const MAX_RETAINED_METADATA_BLOCK: usize = 16 * 1024 * 1024;

pub struct FlacDemuxer<R: Read + Seek> {
    reader: R,
    stream_start: u64,
    audio_start: u64,
    stream_info: StreamInfo,
    blocks: Vec<MetadataBlock>,
    seek_points: Vec<SeekPoint>,
}

impl<R: Read + Seek> FlacDemuxer<R> {
    pub fn new(mut reader: R) -> Result<Self, AudioError> {
        let stream_start = reader.stream_position()?;
        let (stream_info, audio_start, blocks) = read_metadata_blocks(&mut reader, stream_start)?;
        let seek_points = collect_seek_points(&blocks)?;
        reader.seek(SeekFrom::Start(audio_start))?;
        Ok(Self { reader, stream_start, audio_start, stream_info, blocks, seek_points })
    }

    pub fn stream_info(&self) -> &StreamInfo { &self.stream_info }
    pub fn metadata_blocks(&self) -> &[MetadataBlock] { &self.blocks }
    pub fn seek_points(&self) -> &[SeekPoint] { &self.seek_points }
    pub fn audio_start(&self) -> u64 { self.audio_start }
    pub fn stream_start(&self) -> u64 { self.stream_start }
    pub fn reader_mut(&mut self) -> &mut R { &mut self.reader }
    pub fn reset_audio(&mut self) -> Result<(), AudioError> { self.reader.seek(SeekFrom::Start(self.audio_start))?; Ok(()) }

    /// Seek to the nearest SEEKTABLE point at or before `sample`.
    pub fn seek_sample(&mut self, sample: u64) -> Result<u64, AudioError> {
        let point = self.seek_points.iter().rev().find(|p| p.sample_number <= sample);
        let (sample_number, relative) = point.map(|p| (p.sample_number, p.stream_offset)).unwrap_or((0, 0));
        let absolute = self.audio_start.checked_add(relative)
            .ok_or_else(|| AudioError::InvalidFormat("FLAC seek offset overflow".into()))?;
        self.reader.seek(SeekFrom::Start(absolute))?;
        Ok(sample_number)
    }

    pub fn into_inner(self) -> R { self.reader }
}

impl FlacDemuxer<Cursor<Vec<u8>>> {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, AudioError> { Self::new(Cursor::new(bytes)) }
}

pub(crate) fn read_metadata_blocks<R: Read + Seek>(
    reader: &mut R,
    stream_start: u64,
) -> Result<(StreamInfo, u64, Vec<MetadataBlock>), AudioError> {
    reader.seek(SeekFrom::Start(stream_start))?;
    let mut signature = [0u8; 4];
    reader.read_exact(&mut signature)?;
    if &signature != b"fLaC" { return Err(AudioError::InvalidHeader("Missing FLAC fLaC signature".into())); }

    let mut stream_info = None;
    let mut blocks = Vec::new();
    let mut first = true;
    loop {
        let mut header = [0u8; 4];
        reader.read_exact(&mut header)?;
        let is_last = header[0] & 0x80 != 0;
        let block_type = header[0] & 0x7f;
        if block_type == 127 { return Err(AudioError::InvalidFormat("Forbidden FLAC metadata block type 127".into())); }
        let length = ((header[1] as usize) << 16) | ((header[2] as usize) << 8) | header[3] as usize;
        if first && block_type != 0 { return Err(AudioError::InvalidFormat("The first FLAC metadata block must be STREAMINFO".into())); }
        if length > MAX_RETAINED_METADATA_BLOCK {
            return Err(AudioError::InvalidFormat(format!(
                "FLAC metadata block length {length} exceeds bounded limit {MAX_RETAINED_METADATA_BLOCK}"
            )));
        }
        // Padding does not carry semantic metadata. Skip it with seek rather
        // than allocating and filling a payload buffer. Other block types are
        // retained for the compatibility metadata-block API.
        let mut data = Vec::new();
        if block_type == 1 {
            let next = reader.stream_position()?.checked_add(length as u64)
                .ok_or_else(|| AudioError::InvalidFormat("FLAC metadata offset overflow".into()))?;
            reader.seek(SeekFrom::Start(next))?;
        } else {
            data.resize(length, 0);
            reader.read_exact(&mut data)?;
        }
        if block_type == 0 {
            if stream_info.is_some() { return Err(AudioError::InvalidFormat("FLAC contains multiple STREAMINFO blocks".into())); }
            if data.len() != 34 { return Err(AudioError::InvalidFormat(format!("STREAMINFO must be 34 bytes, found {}", data.len()))); }
            stream_info = Some(parse_stream_info(&data)?);
        } else if block_type == 3 {
            // Validate SEEKTABLE shape while traversing metadata. Placeholder
            // entries are permitted and filtered by `collect_seek_points`.
            if data.len() % 18 != 0 {
                return Err(AudioError::InvalidFormat("FLAC SEEKTABLE length is not divisible by 18".into()));
            }
        }
        blocks.push(MetadataBlock { block_type, is_last, data: data.into_boxed_slice() });
        first = false;
        if is_last { break; }
    }
    let info = stream_info.ok_or_else(|| AudioError::InvalidFormat("Missing FLAC STREAMINFO block".into()))?;
    Ok((info, reader.stream_position()?, blocks))
}

pub(crate) fn collect_seek_points(blocks: &[MetadataBlock]) -> Result<Vec<SeekPoint>, AudioError> {
    let mut points: Vec<SeekPoint> = Vec::new();
    for block in blocks.iter().filter(|block| block.block_type == 3) {
        if block.data.len() % 18 != 0 {
            return Err(AudioError::InvalidFormat("FLAC SEEKTABLE length is not divisible by 18".into()));
        }
        for entry in block.data.chunks_exact(18) {
            let sample_number = u64::from_be_bytes(entry[0..8].try_into().unwrap());
            if sample_number == u64::MAX { continue; }
            let stream_offset = u64::from_be_bytes(entry[8..16].try_into().unwrap());
            let frame_samples = u16::from_be_bytes(entry[16..18].try_into().unwrap());
            if let Some(previous) = points.last() {
                if sample_number <= previous.sample_number {
                    return Err(AudioError::InvalidFormat("FLAC SEEKTABLE sample numbers are not strictly increasing".into()));
                }
            }
            points.push(SeekPoint { sample_number, stream_offset, frame_samples });
        }
    }
    Ok(points)
}

fn parse_stream_info(data: &[u8]) -> Result<StreamInfo, AudioError> {
    let min_block_size = u16::from_be_bytes([data[0], data[1]]);
    let max_block_size = u16::from_be_bytes([data[2], data[3]]);
    let min_frame_size = ((data[4] as u32) << 16) | ((data[5] as u32) << 8) | data[6] as u32;
    let max_frame_size = ((data[7] as u32) << 16) | ((data[8] as u32) << 8) | data[9] as u32;
    let packed = u64::from_be_bytes(data[10..18].try_into().unwrap());
    let sample_rate = ((packed >> 44) & 0xFFFFF) as u32;
    let channels = (((packed >> 41) & 7) + 1) as u8;
    let bits_per_sample = (((packed >> 36) & 0x1f) + 1) as u8;
    let total_samples = packed & 0x0f_ffff_ffff;
    let mut md5 = [0u8; 16];
    md5.copy_from_slice(&data[18..34]);
    if min_block_size == 0 || max_block_size == 0 || min_block_size > max_block_size {
        return Err(AudioError::InvalidFormat(format!("Invalid FLAC block size range: {min_block_size}..={max_block_size}")));
    }
    if !(1..=8).contains(&channels) { return Err(AudioError::InvalidFormat(format!("Invalid FLAC channel count: {channels}"))); }
    if bits_per_sample == 0 || bits_per_sample > 32 { return Err(AudioError::UnsupportedBitDepth(bits_per_sample as u16)); }
    if sample_rate == 0 { return Err(AudioError::InvalidFormat("FLAC sample rate is zero".into())); }
    Ok(StreamInfo { min_block_size, max_block_size, min_frame_size, max_frame_size, sample_rate, channels, bits_per_sample, total_samples, md5 })
}

impl<R: Read + Seek> DemuxerSeek for FlacDemuxer<R> {
    fn seek_capability(&self) -> SeekCapability {
        if self.seek_points.is_empty() { SeekCapability::IndexedApproximate } else { SeekCapability::Exact }
    }

    fn seek_time_seconds(&mut self, seconds: f64) -> Result<(), AudioError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(AudioError::InvalidFormat("FLAC seek time must be finite and non-negative".into()));
        }
        let sample = (seconds * self.stream_info.sample_rate as f64).round() as u64;
        let _ = self.seek_sample(sample.min(self.stream_info.total_samples))?;
        Ok(())
    }

    fn position_seconds(&self) -> Option<f64> { None }
}
