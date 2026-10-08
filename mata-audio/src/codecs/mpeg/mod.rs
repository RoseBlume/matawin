#[cfg(not(feature = "simd"))]
mod engine;
#[cfg(feature = "simd")]
use crate::simd::mpeg as engine;
pub(crate) mod tables;

use std::io::{Cursor, Read, Seek, SeekFrom};

use crate::{
    AudioError,
    codecs::{AudioFrame, AudioFrameBuffer, DecodedFrame, Decoder, DecoderMemoryUsage},
};

use engine::{mp3dec_decode_frame, mp3dec_frame_info_t, mp3dec_scratch_t, mp3dec_t};
use super::support::{ByteWindow, RefillPolicy, StageTimer};

/// MPEG's codec-specific compressed-input refill policy. Refill begins below
/// 8 KiB, targets at least 16 KiB of live lookahead, and issues reads in 32 KiB
/// blocks so the following frame is normally resident already.
const MPEG_REFILL: RefillPolicy = RefillPolicy::new(8 * 1024, 16 * 1024, 32 * 1024);
const LOOKAHEAD_BYTES: usize = MPEG_REFILL.high_watermark;

/// Kept when no complete frame was found in the current lookahead window.
/// The reference core searches up to 2304 bytes when determining a
/// free-format frame size, so 8192 leaves enough overlap for a frame and
/// following headers to survive a refill boundary.
const PRESERVE_TAIL_BYTES: usize = 8 * 1024;

/// MPEG Audio Layer II/III emit at most 1152 samples per channel per frame.
/// Layer I emits 384.
const MAX_SAMPLES_PER_CHANNEL: usize = 1152;
const MAX_CHANNELS: usize = 2;
const MPEG_INDEX_STRIDE_FRAMES: u64 = 32;
const MPEG_MAX_INDEX_POINTS: usize = 4096;
const MPEG_LAYER3_PREROLL_POINTS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MpegSeekPoint {
    /// Absolute byte offset of a synchronized MPEG frame header.
    pub byte_offset: u64,
    /// Decoded sample-frame position at this header.
    pub sample: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MpegFrameInfo {
    /// Number of compressed bytes consumed from the buffered input,
    /// including bytes skipped before the frame while resynchronizing.
    pub frame_bytes: usize,

    /// Offset of the MPEG frame header inside the consumed input.
    pub frame_offset: usize,

    pub channels: u16,
    pub sample_rate: u32,
    pub layer: u8,
    pub bitrate_kbps: u32,

    /// Number of decoded PCM samples per channel in this MPEG frame.
    pub samples_per_channel: usize,
}

struct MpegAudio<R>
where
    R: Read + Seek,
{
    reader: R,
    decoder: mp3dec_t,
    scratch: mp3dec_scratch_t,

    buffer: ByteWindow,
    eof: bool,
    stream_start: u64,
    data_start: u64,
    buffer_origin: u64,

    sample_rate: u32,
    channels: u16,
    layer: u8,

    last_frame_info: Option<MpegFrameInfo>,
    decoded_frames: u64,
    sample_position: u64,
    sparse_index: Vec<MpegSeekPoint>,
}

impl<R> MpegAudio<R>
where
    R: Read + Seek,
{
    pub fn new(mut reader: R) -> Result<Self, AudioError> {
        let stream_start = reader.stream_position()?;
        let data_start = skip_leading_id3v2(&mut reader, stream_start)?;

        let mut mp3 = Self {
            reader,
            decoder: mp3dec_t::new(),
            scratch: mp3dec_scratch_t::new(),
            buffer: ByteWindow::with_capacity(LOOKAHEAD_BYTES * 2),
            eof: false,
            stream_start,
            data_start,
            buffer_origin: data_start,
            sample_rate: 0,
            channels: 0,
            layer: 0,
            last_frame_info: None,
            decoded_frames: 0,
            sample_position: 0,
            sparse_index: Vec::with_capacity(128),
        };

        mp3.probe()?;

        Ok(mp3)
    }

    pub fn last_frame_info(&self) -> Option<MpegFrameInfo> {
        self.last_frame_info
    }

    pub fn decoded_frames(&self) -> u64 {
        self.decoded_frames
    }

    #[inline]
    pub fn sample_position(&self) -> u64 { self.sample_position }

    #[inline]
    pub fn sparse_index(&self) -> &[MpegSeekPoint] { &self.sparse_index }

    /// Seeks to the nearest indexed MPEG frame at or before `sample`. Layer
    /// III backs up several index points to warm the bit reservoir before the
    /// requested neighborhood. The returned value is the indexed sample
    /// position actually selected.
    pub fn seek_to_sample(&mut self, sample: u64) -> Result<u64, AudioError> {
        let mut idx = self.sparse_index.partition_point(|p| p.sample <= sample).saturating_sub(1);
        if self.layer == 3 { idx = idx.saturating_sub(MPEG_LAYER3_PREROLL_POINTS); }
        let point = self.sparse_index.get(idx).copied().unwrap_or(MpegSeekPoint {
            byte_offset: self.data_start,
            sample: 0,
        });
        self.reader.seek(SeekFrom::Start(point.byte_offset))?;
        self.decoder = mp3dec_t::new();
        self.scratch = mp3dec_scratch_t::new();
        self.buffer.clear();
        self.buffer_origin = point.byte_offset;
        self.eof = false;
        self.last_frame_info = None;
        self.sample_position = point.sample;
        self.decoded_frames = point.sample / MAX_SAMPLES_PER_CHANNEL as u64;
        Ok(point.sample)
    }

    pub fn seek_to_seconds(&mut self, seconds: f64) -> Result<f64, AudioError> {
        let requested = (seconds.max(0.0) * self.sample_rate.max(1) as f64) as u64;
        let actual = self.seek_to_sample(requested)?;
        Ok(actual as f64 / self.sample_rate.max(1) as f64)
    }

    /// MPEG Audio layer: 1, 2, or 3.
    #[inline]
    pub fn layer(&self) -> u8 { self.layer }

    /// Rewind to the byte position at which `MpegAudio::new` was called.
    pub fn reset(&mut self) -> Result<(), AudioError> {
        self.reader.seek(SeekFrom::Start(self.data_start))?;
        self.decoder = mp3dec_t::new();
        self.scratch = mp3dec_scratch_t::new();
        // `clear` retains ByteWindow capacity; the learned sparse index is also
        // deliberately preserved for subsequent seeks/resets.
        self.buffer.clear();
        self.buffer_origin = self.data_start;
        self.eof = false;
        self.last_frame_info = None;
        self.decoded_frames = 0;
        self.sample_position = 0;
        Ok(())
    }

    fn fill_to(&mut self, target: usize) -> Result<(), AudioError> {
        if self.eof { return Ok(()); }
        let target = MPEG_REFILL.target(target);
        if self.buffer.len() >= target && !MPEG_REFILL.needs_refill(self.buffer.len()) {
            return Ok(());
        }
        let read = self.buffer.read_from(&mut self.reader, target, MPEG_REFILL.read_block)?;
        if read == 0 {
            self.eof = true;
            trim_trailing_tags(&mut self.buffer);
        }
        Ok(())
    }

    fn preserve_tail_for_refill(&mut self) {
        self.buffer.keep_tail(PRESERVE_TAIL_BYTES);
    }

    /// Locate the first MPEG Audio Layer I, II, or III frame without consuming
    /// it from the public decoder state. This establishes sample rate/channels.
    fn probe(&mut self) -> Result<(), AudioError> {
        loop {
            self.fill_to(LOOKAHEAD_BYTES)?;

            if self.buffer.is_empty() {
                return Err(AudioError::InvalidHeader(
                    "No MPEG Audio Layer I, II, or III frame was found".into(),
                ));
            }

            let mut info = mp3dec_frame_info_t::default();
            let samples = mp3dec_decode_frame(
                &mut self.decoder,
                &mut self.scratch,
                self.buffer.as_slice(),
                &mut [],
                &mut info,
            );

            if samples > 0 && info.frame_bytes != 0 {
                if !(1..=3).contains(&info.layer) {
                    return Err(AudioError::InvalidFormat(format!(
                        "Unsupported MPEG Audio layer {}",
                        info.layer,
                    )));
                }
                if info.hz <= 0 || !(1..=2).contains(&info.channels) {
                    return Err(AudioError::InvalidFormat(
                        "Invalid MPEG Audio sample rate or channel count".into(),
                    ));
                }

                self.sample_rate = info.hz as u32;
                self.channels = info.channels as u16;
                self.layer = info.layer;
                if info.frame_offset != 0 {
                    self.consume(info.frame_offset)?;
                }
                if self.sparse_index.is_empty() {
                    self.sparse_index.push(MpegSeekPoint { byte_offset: self.buffer_origin, sample: 0 });
                }
                return Ok(());
            }

            if info.frame_bytes != 0
                && info.layer != 0
                && !(1..=3).contains(&info.layer)
            {
                return Err(AudioError::InvalidFormat(format!(
                    "Unsupported MPEG Audio layer {}",
                    info.layer,
                )));
            }

            if self.eof {
                return Err(AudioError::InvalidHeader(
                    "No complete MPEG Audio Layer I, II, or III frame was found".into(),
                ));
            }

            self.preserve_tail_for_refill();
        }
    }

    fn validate_stream_format(
        &self,
        info: &mp3dec_frame_info_t,
    ) -> Result<(), AudioError> {
        if !(1..=3).contains(&info.layer) {
            return Err(AudioError::InvalidFormat(format!(
                "Unsupported MPEG Audio layer {}",
                info.layer,
            )));
        }

        if info.layer != self.layer {
            return Err(AudioError::InvalidFormat(format!(
                "MPEG Audio layer changed from {} to {}",
                self.layer, info.layer,
            )));
        }

        if info.hz <= 0 || !(1..=2).contains(&info.channels) {
            return Err(AudioError::InvalidFormat(
                "Invalid MP3 frame sample rate or channel count".into(),
            ));
        }

        if info.hz as u32 != self.sample_rate {
            return Err(AudioError::InvalidFormat(format!(
                "MPEG Audio sample rate changed from {} Hz to {} Hz",
                self.sample_rate,
                info.hz,
            )));
        }

        if info.channels as u16 != self.channels {
            return Err(AudioError::InvalidFormat(format!(
                "MPEG Audio channel count changed from {} to {}",
                self.channels,
                info.channels,
            )));
        }

        Ok(())
    }

    fn consume(&mut self, bytes: usize) -> Result<(), AudioError> {
        if bytes == 0 || bytes > self.buffer.len() {
            return Err(AudioError::InvalidFormat(format!(
                "MPEG Audio decoder reported an invalid consumed-byte count of {bytes}",
            )));
        }

        self.buffer.consume(bytes)?;
        self.buffer_origin = self.buffer_origin.saturating_add(bytes as u64);
        Ok(())
    }
}

fn skip_leading_id3v2<R: Read + Seek>(reader: &mut R, start: u64) -> Result<u64, AudioError> {
    let mut cursor = start;
    loop {
        reader.seek(SeekFrom::Start(cursor))?;
        let mut header = [0u8; 10];
        match reader.read_exact(&mut header) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                reader.seek(SeekFrom::Start(cursor))?;
                return Ok(cursor);
            }
            Err(error) => return Err(AudioError::Io(error)),
        }
        if &header[..3] != b"ID3" {
            reader.seek(SeekFrom::Start(cursor))?;
            return Ok(cursor);
        }
        if header[6..10].iter().any(|b| b & 0x80 != 0) {
            return Err(AudioError::InvalidHeader("invalid ID3v2 synchsafe size".into()));
        }
        let payload = ((header[6] as u64) << 21)
            | ((header[7] as u64) << 14)
            | ((header[8] as u64) << 7)
            | header[9] as u64;
        let footer = if header[5] & 0x10 != 0 { 10 } else { 0 };
        cursor = cursor.checked_add(10 + payload + footer)
            .ok_or_else(|| AudioError::InvalidHeader("ID3v2 size overflow".into()))?;
    }
}

fn trim_trailing_tags(buffer: &mut ByteWindow) {
    // ID3v1 is conventionally the final 128 bytes. Removing it first exposes
    // a preceding APE footer in the common APEv2 + ID3v1 layout.
    if buffer.len() >= 128 && &buffer.slice_from(buffer.len() - 128)[..3] == b"TAG" {
        buffer.truncate_live(buffer.len() - 128);
    }
    if buffer.len() >= 32 {
        let footer = buffer.slice_from(buffer.len() - 32);
        if footer.len() >= 32 && &footer[..8] == b"APETAGEX" {
            let size = u32::from_le_bytes([footer[12], footer[13], footer[14], footer[15]]) as usize;
            if (32..=buffer.len()).contains(&size) {
                buffer.truncate_live(buffer.len() - size);
            }
        }
    }
}

fn push_sparse_index(index: &mut Vec<MpegSeekPoint>, point: MpegSeekPoint) {
    if index.last().is_some_and(|last| last.byte_offset == point.byte_offset) { return; }
    index.push(point);
    if index.len() > MPEG_MAX_INDEX_POINTS {
        // Keep the origin and every second learned point. This bounds memory
        // while retaining progressively coarser coverage for very long files.
        let mut write = 1usize;
        for read in (2..index.len()).step_by(2) {
            index[write] = index[read];
            write += 1;
        }
        index.truncate(write);
    }
}

impl MpegAudio<Cursor<Vec<u8>>> {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, AudioError> {
        Self::new(Cursor::new(bytes))
    }
}

impl<R> Decoder for MpegAudio<R>
where
    R: Read + Seek,
{
    fn sample_rate(&self) -> u32 { self.sample_rate }
    fn channels(&self) -> u16 { self.channels }
    fn maximum_frame_samples(&self) -> Option<usize> { Some(MAX_SAMPLES_PER_CHANNEL * MAX_CHANNELS) }

    fn memory_usage(&self) -> DecoderMemoryUsage {
        let compressed = self.buffer.capacity();
        DecoderMemoryUsage {
            persistent_state_bytes: core::mem::size_of::<Self>()
                .saturating_sub(core::mem::size_of::<ByteWindow>())
                .saturating_sub(core::mem::size_of::<Vec<MpegSeekPoint>>()),
            scratch_capacity_bytes: core::mem::size_of_val(&self.scratch),
            compressed_capacity_bytes: compressed,
            index_capacity_bytes: self.sparse_index.capacity().saturating_mul(core::mem::size_of::<MpegSeekPoint>()),
            metadata_capacity_bytes: 0,
            peak_scratch_capacity_bytes: core::mem::size_of_val(&self.scratch),
            peak_compressed_capacity_bytes: self.buffer.peak_capacity(),
        }
    }

    fn trim_memory(&mut self) {
        self.buffer.shrink_to_fit_non_realtime();
        self.sparse_index.shrink_to_fit();
    }

    fn decode_into<'a>(&mut self, output: &'a mut AudioFrameBuffer) -> Result<DecodedFrame<'a>, AudioError> {
        let _stage_timer = StageTimer::start("mpeg.decode");
        loop {
            self.fill_to(LOOKAHEAD_BYTES)?;
            if self.buffer.is_empty() { return Err(AudioError::NoNextFrame); }

            let pcm = output.resize_for_decode(MAX_SAMPLES_PER_CHANNEL * MAX_CHANNELS);
            let mut info = mp3dec_frame_info_t::default();
            let samples_per_channel = mp3dec_decode_frame(
                &mut self.decoder,
                &mut self.scratch,
                self.buffer.as_slice(),
                pcm,
                &mut info,
            );

            if samples_per_channel > 0 {
                self.validate_stream_format(&info)?;
                let total_samples = (samples_per_channel as usize)
                    .checked_mul(info.channels as usize)
                    .ok_or_else(|| AudioError::InvalidFormat("MPEG Audio output sample count overflow".into()))?;
                if total_samples > output.len() {
                    return Err(AudioError::InvalidFormat(format!(
                        "MPEG Audio decoder produced {total_samples} samples, but the output buffer holds only {}",
                        output.len(),
                    )));
                }
                output.truncate(total_samples);
                let frame_byte_offset = self.buffer_origin.saturating_add(info.frame_offset as u64);
                if self.decoded_frames % MPEG_INDEX_STRIDE_FRAMES == 0 {
                    push_sparse_index(&mut self.sparse_index, MpegSeekPoint {
                        byte_offset: frame_byte_offset,
                        sample: self.sample_position,
                    });
                }
                let frame_info = MpegFrameInfo {
                    frame_bytes: info.frame_bytes,
                    frame_offset: info.frame_offset,
                    channels: info.channels as u16,
                    sample_rate: info.hz as u32,
                    layer: info.layer,
                    bitrate_kbps: info.bitrate_kbps.max(0) as u32,
                    samples_per_channel: samples_per_channel as usize,
                };
                self.consume(info.frame_bytes)?;
                self.last_frame_info = Some(frame_info);
                self.decoded_frames = self.decoded_frames.saturating_add(1);
                self.sample_position = self.sample_position.saturating_add(samples_per_channel as u64);
                return Ok(DecodedFrame {
                    samples: output.samples(),
                    channels: frame_info.channels,
                    sample_rate: frame_info.sample_rate,
                });
            }

            output.clear();
            if info.frame_bytes != 0 {
                if info.layer != 0 && !(1..=3).contains(&info.layer) {
                    return Err(AudioError::InvalidFormat(format!(
                        "Unsupported MPEG Audio layer {}", info.layer,
                    )));
                }
                if info.frame_bytes < self.buffer.len() || self.eof {
                    self.consume(info.frame_bytes)?;
                    if self.buffer.is_empty() && self.eof { return Err(AudioError::NoNextFrame); }
                    continue;
                }
            }
            if self.eof {
                self.buffer.clear();
                return Err(AudioError::NoNextFrame);
            }
            self.preserve_tail_for_refill();
        }
    }
}



/// Incremental MPEG Audio decoder for container formats such as AVI.
///
/// Unlike `MpegAudio<R>`, this decoder does not own a `Read + Seek` source. Compressed
/// bytes are appended as the container demuxer encounters them, while the MPEG
/// decoder state (Layer III reservoir, MDCT overlap, synthesis state, etc.) is
/// preserved continuously across packet boundaries.
pub(crate) struct MpegAudioStream {
    decoder: mp3dec_t,
    scratch: mp3dec_scratch_t,
    frame_buffer: AudioFrameBuffer,
    buffer: ByteWindow,
    sample_rate: u32,
    channels: u16,
    last_frame_info: Option<MpegFrameInfo>,
    decoded_frames: u64,
}

impl Default for MpegAudioStream {
    fn default() -> Self {
        Self::new()
    }
}

impl MpegAudioStream {
    pub fn new() -> Self {
        Self {
            decoder: mp3dec_t::new(),
            scratch: mp3dec_scratch_t::new(),
            frame_buffer: AudioFrameBuffer::with_capacity(MAX_SAMPLES_PER_CHANNEL * MAX_CHANNELS),
            buffer: ByteWindow::with_capacity(LOOKAHEAD_BYTES * 2),
            sample_rate: 0,
            channels: 0,
            last_frame_info: None,
            decoded_frames: 0,
        }
    }

    #[inline]
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    #[inline]
    pub fn buffered_bytes(&self) -> usize {
        self.buffer.len()
    }

    #[inline]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    #[inline]
    pub fn channels(&self) -> u16 {
        self.channels
    }

    #[inline]
    pub fn last_frame_info(&self) -> Option<MpegFrameInfo> {
        self.last_frame_info
    }

    #[inline]
    pub fn decoded_frames(&self) -> u64 {
        self.decoded_frames
    }

    pub fn reset(&mut self) {
        self.decoder = mp3dec_t::new();
        self.scratch = mp3dec_scratch_t::new();
        self.frame_buffer.clear();
        self.buffer.clear();
        self.sample_rate = 0;
        self.channels = 0;
        self.last_frame_info = None;
        self.decoded_frames = 0;
    }

    /// Decode one complete MPEG Audio frame if enough compressed bytes are
    /// currently buffered. `Ok(None)` means the container needs to provide more
    /// bytes; it is not an end-of-stream condition.
    pub fn next_available_into<'a>(
        &mut self,
        output: &'a mut AudioFrameBuffer,
    ) -> Result<Option<DecodedFrame<'a>>, AudioError> {
        loop {
            if self.buffer.is_empty() {
                return Ok(None);
            }

            let pcm = output.resize_for_decode(MAX_SAMPLES_PER_CHANNEL * MAX_CHANNELS);
            let mut info = mp3dec_frame_info_t::default();

            let samples_per_channel = mp3dec_decode_frame(
                &mut self.decoder,
                &mut self.scratch,
                self.buffer.as_slice(),
                pcm,
                &mut info,
            );

            if samples_per_channel > 0 {
                self.validate_incremental_format(&info)?;

                if info.frame_bytes == 0 || info.frame_bytes > self.buffer.len() {
                    return Err(AudioError::InvalidFormat(format!(
                        "MPEG Audio decoder reported an invalid consumed-byte count of {}",
                        info.frame_bytes,
                    )));
                }

                let total_samples = (samples_per_channel as usize)
                    .checked_mul(info.channels as usize)
                    .ok_or_else(|| {
                        AudioError::InvalidFormat(
                            "MPEG Audio output sample count overflow".into(),
                        )
                    })?;

                if total_samples > output.len() {
                    return Err(AudioError::InvalidFormat(format!(
                        "MPEG Audio decoder produced {total_samples} samples, but the output buffer holds only {}",
                        output.len(),
                    )));
                }

                output.truncate(total_samples);

                let frame_info = MpegFrameInfo {
                    frame_bytes: info.frame_bytes,
                    frame_offset: info.frame_offset,
                    channels: info.channels as u16,
                    sample_rate: info.hz as u32,
                    layer: info.layer,
                    bitrate_kbps: info.bitrate_kbps.max(0) as u32,
                    samples_per_channel: samples_per_channel as usize,
                };

                self.buffer.consume(info.frame_bytes)?;
                self.last_frame_info = Some(frame_info);
                self.decoded_frames = self.decoded_frames.saturating_add(1);

                return Ok(Some(DecodedFrame {
                    samples: output.samples(),
                    channels: frame_info.channels,
                    sample_rate: frame_info.sample_rate,
                }));
            }

            if info.frame_bytes != 0 {
                if info.layer != 0 && !(1..=3).contains(&info.layer) {
                    return Err(AudioError::InvalidFormat(format!(
                        "Unsupported MPEG Audio layer {}",
                        info.layer,
                    )));
                }

                // If the decoder consumed fewer bytes than are buffered, it
                // found a complete frame (or skipped junk before one) but did
                // not output PCM. Layer III can do this while its bit reservoir
                // is warming up. Consume it so the persistent decoder state can
                // advance to the next frame.
                if info.frame_bytes < self.buffer.len() {
                    self.buffer.consume(info.frame_bytes)?;
                    continue;
                }

                // The candidate reaches exactly to the current packet boundary.
                // Wait for another AVI `##wb` packet so the low-level decoder can
                // distinguish a complete boundary-aligned frame from a partial
                // one without throwing away bytes.
                return Ok(None);
            }

            // No complete MPEG frame was found. AVI may split one across `##wb`
            // packets, so retain the bytes and wait for the next packet.
            // Bound malformed input so a corrupt stream cannot grow forever.
            if self.buffer.len() > LOOKAHEAD_BYTES * 8 {
                let keep = PRESERVE_TAIL_BYTES.min(self.buffer.len());
                let remove = self.buffer.len() - keep;
                self.buffer.consume(remove)?;
            }

            return Ok(None);
        }
    }

    /// Owned compatibility path. The decoder keeps its reusable PCM allocation;
    /// only the returned owned `AudioFrame` requires a copy/allocation. Container
    /// integrations should prefer `next_available_into`.
    pub fn next_available(&mut self) -> Result<Option<AudioFrame>, AudioError> {
        let mut frame_buffer = std::mem::take(&mut self.frame_buffer);
        let decoded = self.next_available_into(&mut frame_buffer);
        let result = match decoded {
            Ok(Some(frame)) => Ok(Some(AudioFrame::new(
                frame.samples.to_vec(),
                frame.channels,
                frame.sample_rate,
            ))),
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        };
        self.frame_buffer = frame_buffer;
        result
    }

    /// Decode every complete frame currently available without treating the
    /// current end of the buffer as EOF.
    pub fn decode_available(&mut self) -> Result<Vec<AudioFrame>, AudioError> {
        let mut frames = Vec::new();

        while let Some(frame) = self.next_available()? {
            frames.push(frame);
        }

        Ok(frames)
    }

    fn validate_incremental_format(
        &mut self,
        info: &mp3dec_frame_info_t,
    ) -> Result<(), AudioError> {
        if !(1..=3).contains(&info.layer) {
            return Err(AudioError::InvalidFormat(format!(
                "Unsupported MPEG Audio layer {}",
                info.layer,
            )));
        }

        if info.hz <= 0 || !(1..=2).contains(&info.channels) {
            return Err(AudioError::InvalidFormat(
                "Invalid MPEG Audio sample rate or channel count".into(),
            ));
        }

        let sample_rate = info.hz as u32;
        let channels = info.channels as u16;

        if self.sample_rate == 0 {
            self.sample_rate = sample_rate;
        } else if self.sample_rate != sample_rate {
            return Err(AudioError::InvalidFormat(format!(
                "MPEG Audio sample rate changed from {} Hz to {} Hz",
                self.sample_rate,
                sample_rate,
            )));
        }

        if self.channels == 0 {
            self.channels = channels;
        } else if self.channels != channels {
            return Err(AudioError::InvalidFormat(format!(
                "MPEG Audio channel count changed from {} to {}",
                self.channels,
                channels,
            )));
        }

        Ok(())
    }
}



macro_rules! layer_decoder {
    ($name:ident, $layer:literal) => {
        pub struct $name<R>
        where
            R: Read + Seek,
        {
            inner: MpegAudio<R>,
        }

        impl<R> $name<R>
        where
            R: Read + Seek,
        {
            pub fn new(reader: R) -> Result<Self, AudioError> {
                let inner = MpegAudio::new(reader)?;
                if inner.layer() != $layer {
                    return Err(AudioError::InvalidFormat(format!(
                        "Expected MPEG Audio Layer {}, found Layer {}",
                        $layer, inner.layer(),
                    )));
                }
                Ok(Self { inner })
            }

            #[inline]
            pub fn last_frame_info(&self) -> Option<MpegFrameInfo> { self.inner.last_frame_info() }
            #[inline]
            pub fn decoded_frames(&self) -> u64 { self.inner.decoded_frames() }
            pub fn reset(&mut self) -> Result<(), AudioError> { self.inner.reset() }
            pub fn seek_to_sample(&mut self, sample: u64) -> Result<u64, AudioError> { self.inner.seek_to_sample(sample) }
            pub fn seek_to_seconds(&mut self, seconds: f64) -> Result<f64, AudioError> { self.inner.seek_to_seconds(seconds) }
            pub fn sparse_index(&self) -> &[MpegSeekPoint] { self.inner.sparse_index() }
            pub fn into_inner(self) -> R { self.inner.reader }
        }

        impl $name<Cursor<Vec<u8>>> {
            pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, AudioError> {
                Self::new(Cursor::new(bytes))
            }
        }

        impl<R> Decoder for $name<R>
        where
            R: Read + Seek,
        {
            #[inline]
            fn sample_rate(&self) -> u32 { self.inner.sample_rate() }
            #[inline]
            fn channels(&self) -> u16 { self.inner.channels() }
            #[inline]
            fn maximum_frame_samples(&self) -> Option<usize> { self.inner.maximum_frame_samples() }
            #[inline]
            fn decode_into<'a>(&mut self, output: &'a mut AudioFrameBuffer) -> Result<DecodedFrame<'a>, AudioError> {
                self.inner.decode_into(output)
            }
            #[inline]
            fn memory_usage(&self) -> DecoderMemoryUsage { self.inner.memory_usage() }
            #[inline]
            fn trim_memory(&mut self) { self.inner.trim_memory(); }
        }
    };
}

#[cfg(feature = "codec-mp1")]
layer_decoder!(Mp1, 1);
#[cfg(feature = "codec-mp2")]
layer_decoder!(Mp2, 2);
#[cfg(feature = "codec-mp3")]
layer_decoder!(Mp3, 3);
