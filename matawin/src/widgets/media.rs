//! Mata ecosystem audio facade for widgets.
//!
//! Decoding, streamed mixing, seeking, per-source volume and the output backend
//! are owned by `mata-audio`; MataWin widgets only consume the Mata API.

pub use mata_audio::{ AudioSource, AudioSourceState };
