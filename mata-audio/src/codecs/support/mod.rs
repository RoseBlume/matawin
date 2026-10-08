//! Internal performance-oriented codec support primitives.
//!
//! These are kept under `codecs` so the crate's public root remains focused on
//! decoding, demuxing, metadata, playback, errors and SIMD.

pub(crate) mod bits;
pub(crate) mod buffer;
pub(crate) mod index;
pub(crate) mod input;
pub(crate) mod instrumentation;
pub(crate) mod memory;
pub(crate) mod output;

pub(crate) use bits::{BitReservoir64, BitReservoir128};
pub(crate) use buffer::{ByteWindow, RefillPolicy};
pub(crate) use index::{PositionIndex, PositionPoint};
pub(crate) use input::{TunedBufReader, DEFAULT_IO_CAPACITY};
pub(crate) use instrumentation::StageTimer;
pub(crate) use memory::{AlignedScratch, checked_allocation_len, checked_setup_len, MAX_DECODER_SCRATCH_BYTES, MAX_SETUP_ALLOCATION_BYTES};
pub(crate) use output::{InterleavedWriter, PlanarAudioScratch, PlanarWriter};
