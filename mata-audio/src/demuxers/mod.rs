//! Container/elementary-stream demuxers that can be implemented without a
//! per-use royalty payment.
//!
//! Codec licensing is intentionally separate from container parsing. A
//! Matroska or Ogg file can contain a codec this crate does not decode.
//! Each demuxer has an independent Cargo feature; `all-demuxers` enables all
//! of them and is part of the default feature set.

use crate::AudioError;

mod support;

#[cfg(feature = "demux-aiff")]
pub mod aiff;
#[cfg(feature = "demux-flac")]
pub mod flac;
#[cfg(feature = "demux-matroska")]
pub mod matroska;
#[cfg(feature = "demux-ogg")]
pub mod ogg;
#[cfg(feature = "demux-wav")]
pub mod wav;

/// Coarse seek/index capabilities exposed by demuxers.
///
/// The common interface deliberately uses media time rather than codec sample
/// indices because not every container has a PCM frame concept. Demuxers with
/// exact frame/granule APIs keep those format-specific methods as well.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekCapability {
    None,
    Exact,
    IndexedApproximate,
}

/// Common demuxer timeline interface used by playback and benchmarks.
pub trait DemuxerSeek {
    fn seek_capability(&self) -> SeekCapability;
    fn seek_time_seconds(&mut self, seconds: f64) -> Result<(), AudioError>;
    fn position_seconds(&self) -> Option<f64>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerFormat {
    #[cfg(feature = "demux-aiff")]
    Aiff,
    #[cfg(feature = "demux-flac")]
    Flac,
    #[cfg(feature = "demux-matroska")]
    Matroska,
    #[cfg(feature = "demux-ogg")]
    Ogg,
    #[cfg(feature = "demux-wav")]
    RiffWave,
}
