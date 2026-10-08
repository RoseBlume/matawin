//! `mata-audio`: decoding, demuxing, metadata, SIMD, errors, and optional playback.
//!
//! Top-level modules:
//! - [`playback`] (feature gated)
//! - [`error`]
//! - [`demuxers`]
//! - [`codecs`]
//! - [`metadata`]
//! - [`simd`]
//!
//! Every codec, demuxer, and metadata parser has an independent Cargo feature.
//! The default feature set enables all of them.

#[cfg(any(feature = "mata-playback", feature = "cpal"))]
pub mod playback;
pub mod error;
pub mod demuxers;
pub mod codecs;
pub mod metadata;
pub mod simd;


/// Zero-sized marker used by feature-surface integration tests.
#[derive(Debug,Clone,Copy,Default)]
pub struct MetadataSurfaceProbe;
pub use error::AudioError;
pub use codecs::{AudioFrame, AudioFrameBuffer, DecodedFrame, Decoder, DecoderMemoryUsage, FrameInfo, FromF32Sample};

#[cfg(feature = "codec-flac")]
pub use codecs::Flac;
#[cfg(any(feature = "codec-mp1", feature = "codec-mp2", feature = "codec-mp3"))]
pub use codecs::MpegFrameInfo;
#[cfg(feature = "codec-mp1")]
pub use codecs::Mp1;
#[cfg(feature = "codec-mp2")]
pub use codecs::Mp2;
#[cfg(feature = "codec-mp3")]
pub use codecs::Mp3;
#[cfg(feature = "codec-pcm")]
pub use codecs::{Pcm, PcmEncoding, PcmFormat};
#[cfg(all(feature = "codec-pcm", feature = "demux-aiff"))]
pub use codecs::AiffPcm;
#[cfg(all(feature = "codec-pcm", feature = "demux-wav"))]
pub use codecs::WavPcm;
#[cfg(feature = "codec-vorbis")]
pub use codecs::Vorbis;

#[cfg(all(feature = "codec-pcm", feature = "demux-wav"))]
pub type Wav<R> = WavPcm<R>;
#[cfg(all(feature = "codec-pcm", feature = "demux-aiff"))]
pub type Aiff<R> = AiffPcm<R>;

#[cfg(any(feature = "mata-playback", feature = "cpal"))]
pub use playback::{AudioSource, AudioSourceState, PlaybackTelemetry, LimiterQuality, set_limiter_quality, install_worker_tuning_hook, install_decoder_worker_tuning_hook, set_realtime_worker_tuning};

#[cfg(feature = "auto-detect")]
use std::io::{Read, Seek, SeekFrom};

#[cfg(feature = "auto-detect")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Wav,
    Aiff,
    Flac,
    Mp1,
    Mp2,
    Mp3,
    Vorbis,
}

#[cfg(feature = "auto-detect")]
pub enum AudioDecoder<R>
where
    R: Read + Seek,
{
    Wav(WavPcm<R>),
    Aiff(AiffPcm<R>),
    Flac(Flac<R>),
    Mp1(Mp1<R>),
    Mp2(Mp2<R>),
    Mp3(Mp3<R>),
    Vorbis(Vorbis<R>),
}

#[cfg(feature = "auto-detect")]
impl<R> AudioDecoder<R>
where
    R: Read + Seek,
{
    /// Detects the encoded audio format from stream contents and creates the
    /// matching decoder. Filename extensions are ignored.
    pub fn new(mut reader: R) -> Result<Self, AudioError> {
        let start = reader.stream_position()?;
        let format = detect_format(&mut reader, start)?;
        reader.seek(SeekFrom::Start(start))?;
        match format {
            AudioFormat::Wav => Ok(Self::Wav(WavPcm::new(reader)?)),
            AudioFormat::Aiff => Ok(Self::Aiff(AiffPcm::new(reader)?)),
            AudioFormat::Flac => Ok(Self::Flac(Flac::new(reader)?)),
            AudioFormat::Mp1 => Ok(Self::Mp1(Mp1::new(reader)?)),
            AudioFormat::Mp2 => Ok(Self::Mp2(Mp2::new(reader)?)),
            AudioFormat::Mp3 => Ok(Self::Mp3(Mp3::new(reader)?)),
            AudioFormat::Vorbis => Ok(Self::Vorbis(Vorbis::new(reader)?)),
        }
    }

    pub fn format(&self) -> AudioFormat {
        match self {
            Self::Wav(_) => AudioFormat::Wav,
            Self::Aiff(_) => AudioFormat::Aiff,
            Self::Flac(_) => AudioFormat::Flac,
            Self::Mp1(_) => AudioFormat::Mp1,
            Self::Mp2(_) => AudioFormat::Mp2,
            Self::Mp3(_) => AudioFormat::Mp3,
            Self::Vorbis(_) => AudioFormat::Vorbis,
        }
    }

    pub fn format_name(&self) -> &'static str {
        match self.format() {
            AudioFormat::Wav => "WAV/PCM",
            AudioFormat::Aiff => "AIFF/PCM",
            AudioFormat::Flac => "FLAC",
            AudioFormat::Mp1 => "MPEG Audio Layer I",
            AudioFormat::Mp2 => "MPEG Audio Layer II",
            AudioFormat::Mp3 => "MPEG Audio Layer III (MP3)",
            AudioFormat::Vorbis => "Ogg Vorbis",
        }
    }

    #[inline] pub fn is_wav(&self) -> bool { matches!(self, Self::Wav(_)) }
    #[inline] pub fn is_aiff(&self) -> bool { matches!(self, Self::Aiff(_)) }
    #[inline] pub fn is_flac(&self) -> bool { matches!(self, Self::Flac(_)) }
    #[inline] pub fn is_mp1(&self) -> bool { matches!(self, Self::Mp1(_)) }
    #[inline] pub fn is_mp2(&self) -> bool { matches!(self, Self::Mp2(_)) }
    #[inline] pub fn is_mp3(&self) -> bool { matches!(self, Self::Mp3(_)) }
    #[inline] pub fn is_vorbis(&self) -> bool { matches!(self, Self::Vorbis(_)) }
}

#[cfg(feature = "auto-detect")]
impl<R> Decoder for AudioDecoder<R>
where
    R: Read + Seek,
{
    fn sample_rate(&self) -> u32 {
        match self {
            Self::Wav(d) => d.sample_rate(),
            Self::Aiff(d) => d.sample_rate(),
            Self::Flac(d) => d.sample_rate(),
            Self::Mp1(d) => d.sample_rate(),
            Self::Mp2(d) => d.sample_rate(),
            Self::Mp3(d) => d.sample_rate(),
            Self::Vorbis(d) => d.sample_rate(),
        }
    }

    fn channels(&self) -> u16 {
        match self {
            Self::Wav(d) => d.channels(),
            Self::Aiff(d) => d.channels(),
            Self::Flac(d) => d.channels(),
            Self::Mp1(d) => d.channels(),
            Self::Mp2(d) => d.channels(),
            Self::Mp3(d) => d.channels(),
            Self::Vorbis(d) => d.channels(),
        }
    }

    fn maximum_frame_samples(&self) -> Option<usize> {
        match self {
            Self::Wav(d) => d.maximum_frame_samples(),
            Self::Aiff(d) => d.maximum_frame_samples(),
            Self::Flac(d) => d.maximum_frame_samples(),
            Self::Mp1(d) => d.maximum_frame_samples(),
            Self::Mp2(d) => d.maximum_frame_samples(),
            Self::Mp3(d) => d.maximum_frame_samples(),
            Self::Vorbis(d) => d.maximum_frame_samples(),
        }
    }

    fn decode_into<'a>(&mut self, output: &'a mut AudioFrameBuffer) -> Result<DecodedFrame<'a>, AudioError> {
        match self {
            Self::Wav(d) => d.decode_into(output),
            Self::Aiff(d) => d.decode_into(output),
            Self::Flac(d) => d.decode_into(output),
            Self::Mp1(d) => d.decode_into(output),
            Self::Mp2(d) => d.decode_into(output),
            Self::Mp3(d) => d.decode_into(output),
            Self::Vorbis(d) => d.decode_into(output),
        }
    }

    fn memory_usage(&self) -> DecoderMemoryUsage {
        match self {
            Self::Wav(d) => d.memory_usage(),
            Self::Aiff(d) => d.memory_usage(),
            Self::Flac(d) => d.memory_usage(),
            Self::Mp1(d) => d.memory_usage(),
            Self::Mp2(d) => d.memory_usage(),
            Self::Mp3(d) => d.memory_usage(),
            Self::Vorbis(d) => d.memory_usage(),
        }
    }

    fn trim_memory(&mut self) {
        match self {
            Self::Wav(d) => d.trim_memory(),
            Self::Aiff(d) => d.trim_memory(),
            Self::Flac(d) => d.trim_memory(),
            Self::Mp1(d) => d.trim_memory(),
            Self::Mp2(d) => d.trim_memory(),
            Self::Mp3(d) => d.trim_memory(),
            Self::Vorbis(d) => d.trim_memory(),
        }
    }

    fn seek_to_sample(&mut self, sample: u64) -> Result<Option<u64>, AudioError> {
        match self {
            Self::Wav(d) => { d.seek_frame(sample)?; Ok(Some(sample)) }
            Self::Aiff(d) => { d.seek_frame(sample)?; Ok(Some(sample)) }
            Self::Flac(d) => Ok(Some(d.seek_to_sample(sample)?)),
            Self::Mp1(d) => Ok(Some(d.seek_to_sample(sample)?)),
            Self::Mp2(d) => Ok(Some(d.seek_to_sample(sample)?)),
            Self::Mp3(d) => Ok(Some(d.seek_to_sample(sample)?)),
            Self::Vorbis(d) => Ok(Some(d.seek_to_sample(sample)?)),
        }
    }

    fn seek_to_seconds(&mut self, seconds: f64) -> Result<Option<f64>, AudioError> {
        if !seconds.is_finite() || seconds < 0.0 { return Err(AudioError::InvalidFormat("seek time must be finite and non-negative".into())); }
        let requested=(seconds*self.sample_rate().max(1) as f64).round() as u64;
        Ok(self.seek_to_sample(requested)?.map(|actual|actual as f64/self.sample_rate().max(1) as f64))
    }
}

#[cfg(feature = "auto-detect")]
impl<R> AudioDecoder<R> where R: Read + Seek {
    /// Feature-aware native sample seek. Match arms only exist for formats that
    /// are part of `auto-detect`, so minimal direct-codec builds gain no dead dispatch.
    pub fn seek_sample(&mut self, sample: u64) -> Result<u64, AudioError> {
        self.seek_to_sample(sample)?.ok_or_else(|| AudioError::custom("decoder does not support native seeking"))
    }
    pub fn seek_seconds(&mut self, seconds: f64) -> Result<f64, AudioError> {
        self.seek_to_seconds(seconds)?.ok_or_else(|| AudioError::custom("decoder does not support native seeking"))
    }
}


#[cfg(feature = "auto-detect")]
pub type BufferedFileDecoder = AudioDecoder<std::io::BufReader<std::fs::File>>;

#[cfg(feature = "auto-detect")]
impl AudioDecoder<std::io::BufReader<std::fs::File>> {
    /// Opens a local file with a format-tuned sequential-read buffer before
    /// content probing. The extension is used only as a performance hint; the
    /// actual decoder is still selected from file contents.
    pub fn open_file(path: impl AsRef<std::path::Path>) -> Result<Self, AudioError> {
        let path = path.as_ref();
        let extension = path.extension().and_then(|value| value.to_str()).unwrap_or_default();
        let capacity = match extension.to_ascii_lowercase().as_str() {
            // PCM containers have small, regular frame reads.
            "wav" | "wave" | "aif" | "aiff" | "aifc" => 32 * 1024,
            // Entropy-coded streams benefit from a larger sequential window.
            "flac" | "ogg" | "oga" | "vorbis" => 128 * 1024,
            // MPEG frame lookahead/refill is explicitly tuned around 16/32 KiB.
            "mp1" | "mp2" | "mp3" | "mpa" => 64 * 1024,
            _ => 64 * 1024,
        };
        let file = std::fs::File::open(path)?;
        Self::new(std::io::BufReader::with_capacity(capacity, file))
    }
}

#[cfg(feature = "auto-detect")]
impl AudioDecoder<std::io::Cursor<Vec<u8>>> {
    /// Optional small-file fast path. The size is checked before allocating so
    /// callers retain a strict memory bound.
    pub fn open_small_file_in_memory(
        path: impl AsRef<std::path::Path>,
        maximum_bytes: u64,
    ) -> Result<Self, AudioError> {
        let path = path.as_ref();
        let metadata = std::fs::metadata(path)?;
        if metadata.len() > maximum_bytes {
            return Err(AudioError::custom(format!(
                "{} is {} bytes, above the in-memory limit of {} bytes",
                path.display(), metadata.len(), maximum_bytes,
            )));
        }
        let bytes = std::fs::read(path)?;
        Self::new(std::io::Cursor::new(bytes))
    }
}

#[cfg(feature = "auto-detect")]
fn detect_format<R>(reader: &mut R, start: u64) -> Result<AudioFormat, AudioError>
where
    R: Read + Seek,
{
    const HEADER_SIZE: usize = 64 * 1024;
    let end = reader.seek(SeekFrom::End(0))?;
    if end < start {
        return Err(AudioError::InvalidFormat("Invalid audio stream bounds".into()));
    }
    let length = end - start;
    reader.seek(SeekFrom::Start(start))?;
    let prefix_len = usize::try_from(length.min(HEADER_SIZE as u64)).unwrap_or(HEADER_SIZE);
    let mut header = vec![0u8; prefix_len];
    if !header.is_empty() { reader.read_exact(&mut header)?; }

    if header.len() >= 12
        && (&header[0..4] == b"RIFF" || &header[0..4] == b"RF64")
        && &header[8..12] == b"WAVE"
    {
        return Ok(AudioFormat::Wav);
    }
    if header.len() >= 12
        && &header[0..4] == b"FORM"
        && (&header[8..12] == b"AIFF" || &header[8..12] == b"AIFC")
    {
        return Ok(AudioFormat::Aiff);
    }
    if header.starts_with(b"fLaC") { return Ok(AudioFormat::Flac); }
    if header.starts_with(b"OggS") && is_ogg_vorbis(&header) { return Ok(AudioFormat::Vorbis); }

    let search_start = id3v2_end(&header).unwrap_or(0).min(header.len());
    if let Some(offset) = find_mpeg_audio_header(&header[search_start..]) {
        let h = &header[search_start + offset..search_start + offset + 4];
        return Ok(match mpeg_layer(h).expect("validated MPEG header") {
            1 => AudioFormat::Mp1,
            2 => AudioFormat::Mp2,
            _ => AudioFormat::Mp3,
        });
    }

    Err(AudioError::InvalidFormat(
        "Unknown or unsupported audio file format".into(),
    ))
}

#[cfg(feature = "auto-detect")]
fn id3v2_end(data: &[u8]) -> Option<usize> {
    if data.len() < 10 || &data[0..3] != b"ID3" { return None; }
    let size = &data[6..10];
    if size.iter().any(|b| b & 0x80 != 0) { return None; }
    let n = ((size[0] as usize) << 21)
        | ((size[1] as usize) << 14)
        | ((size[2] as usize) << 7)
        | size[3] as usize;
    let footer = if data[5] & 0x10 != 0 { 10 } else { 0 };
    10usize.checked_add(n)?.checked_add(footer)
}

#[cfg(feature = "auto-detect")]
fn is_ogg_vorbis(data: &[u8]) -> bool {
    if data.len() < 27 || &data[0..4] != b"OggS" || data[4] != 0 { return false; }
    let n = data[26] as usize;
    let table_end = match 27usize.checked_add(n) { Some(v) => v, None => return false };
    if table_end > data.len() { return false; }
    let mut packet_size = 0usize;
    let mut complete = false;
    for &segment in &data[27..table_end] {
        packet_size = match packet_size.checked_add(segment as usize) { Some(v) => v, None => return false };
        if segment < 255 { complete = true; break; }
    }
    if !complete { return false; }
    let end = match table_end.checked_add(packet_size) { Some(v) => v, None => return false };
    end <= data.len() && packet_size >= 7 && data[table_end] == 1 && &data[table_end + 1..table_end + 7] == b"vorbis"
}

#[cfg(feature = "auto-detect")]
fn find_mpeg_audio_header(data: &[u8]) -> Option<usize> {
    if data.len() < 4 { return None; }
    (0..=data.len() - 4).find(|&offset| is_mpeg_audio_header(&data[offset..offset + 4]))
}

#[cfg(feature = "auto-detect")]
fn mpeg_layer(header: &[u8]) -> Option<u8> {
    if !is_mpeg_audio_header(header) { return None; }
    Some(4 - ((header[1] >> 1) & 0x03))
}

#[cfg(feature = "auto-detect")]
fn is_mpeg_audio_header(header: &[u8]) -> bool {
    if header.len() < 4 || header[0] != 0xff || header[1] & 0xe0 != 0xe0 { return false; }
    let version = (header[1] >> 3) & 0x03;
    let layer = (header[1] >> 1) & 0x03;
    let bitrate_index = header[2] >> 4;
    let sample_rate_index = (header[2] >> 2) & 0x03;
    version != 0x01 && layer != 0 && bitrate_index != 0x0f && sample_rate_index != 0x03
}
