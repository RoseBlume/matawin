# P502-P526 memory/allocation audit

The decoder `DecoderMemoryUsage` surface separates persistent object state, reusable scratch capacity, compressed-input windows, seek indexes, and cold metadata/setup capacity. The memory harness records retained and high-water bytes after decode and can repeat full decode/reset passes for an hour without intentionally shrinking buffers.

Hot/cold decisions:
- Vorbis comments and setup live behind `Box` allocations outside the hot `DecoderState`; FLAC metadata blocks use boxed slices and seek metadata is outside frame scratch.
- FLAC integer channel/residual scratch and Vorbis IMDCT float scratch use 64-byte-aligned reusable storage.
- MPEG compressed input uses bounded `ByteWindow` storage with a retained high-water counter.
- Playback keeps UI/error `String` state behind the cold `Mutex<AudioSourceState>` / producer error slot; the callback publishes only atomic error codes.

Playback Arc audit:
- `AudioSource` shares `state`, `fast`, and one `Arc<PathBuf>` with the open command. The previous duplicate `PathBuf` stored on every `Voice` was removed.
- `DecodeProducer`, PCM SPSC rings, engine status and output rings genuinely cross thread ownership boundaries and remain `Arc`-backed.

Allocation policy:
- Stream-derived allocation lengths pass bounded checked helpers before scratch/setup growth.
- Normal decode/reset/seek retains capacity. `Decoder::trim_memory`, `AudioSource::trim_memory`, and `AudioFrameBuffer::shrink_to_fit_non_realtime` are explicit cold-path reclamation APIs.
- Allocation profiling is performed by `mata-audio-perf`'s counting allocator and `scripts/profile_allocations.sh`; steady-state growth is checked by `mata-audio-memory-perf`.
