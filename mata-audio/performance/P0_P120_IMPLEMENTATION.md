# P0-P120 implementation map

This file maps the P0-P120 implementation pass to the source and performance
infrastructure. Formal phase check-off is still governed by `todo.md`'s
completion rule.

## P0-P30 — measurement and regression infrastructure

- `src/bin/mata-audio-perf.rs`: open latency, decode throughput, `next_frame`
  and `decode_into` latency, allocation accounting, allocated bytes/audio
  second, peak RSS, correctness hashes/tolerances, timeline baselines,
  synthetic mixer/resampling/channel-layout/underrun/command workloads,
  long-duration playback workload, simultaneous decoder startup, steady-state
  decode loops, scalar-vs-SIMD dispatch measurements, CSV output and Markdown
  summary.
- `performance/corpus/manifest.tsv`: representative WAV, AIFF, FLAC, MPEG
  Layer I/II/III and Ogg Vorbis inputs with SHA-256 and tolerances.
- `performance/reference/`: deterministic interleaved f32 reference output.
- `performance/scripts/run_baseline.sh`: full validated baseline entry point.
- `performance/scripts/compare_results.py`: default 5% regression gate and
  exact correctness-hash gate.
- `performance/scripts/capture_hardware.py`: pinned host/compiler environment.
- `performance/scripts/profile_codecs.sh` / `profile_playback.sh`: per-path
  profiler recordings.

## P31-P60 — reusable decode/allocation architecture

- `src/codecs/mod.rs`: `AudioFrameBuffer`, `FrameInfo`, borrowed
  `DecodedFrame`, `Decoder::decode_into`, compatibility `next_frame`, maximum
  and preferred frame-size hints, sample-rate reciprocal and cached usize
  channel helpers.
- `src/codecs/pcm.rs`: persistent byte scratch and direct output decode.
- `src/codecs/mpeg/mod.rs`: direct retained PCM output for the stream decoder.
- `src/codecs/flac/mod.rs` and `src/codecs/vorbis/mod.rs`: ownership transfer
  into reusable output without an extra PCM copy.
- `src/playback.rs`: one retained decode buffer per voice and allocation swaps
  into empty staging buffers.
- `src/codecs/support/output.rs`: contiguous planar scratch plus planar and
  interleaved writers.
- `performance/COPY_AUDIT.md`: PCM-copy audit and remaining codec-local work.

## P61-P90 — I/O, compressed buffers, bit reading and indexing

- `src/codecs/support/input.rs`: tuned `BufReader`, tracked logical position,
  and reset-to-known-offset helper.
- `src/codecs/support/buffer.rs`: head-indexed `ByteWindow`, threshold
  compaction, direct spare-capacity reads, geometric growth and refill policy.
- `src/codecs/mpeg/mod.rs`: codec-specific low/high refill watermarks and read
  block size; compressed prefix draining removed.
- `src/codecs/support/bits.rs`: 64-bit reservoir, experimental 128-bit
  reservoir, common-width fast reads, aligned/unaligned reads, unary scanning,
  coarse bounds checks and tightly-scoped unchecked loads.
- `src/codecs/support/index.rs`: bounded lazy byte/sample sparse index.
- `AudioDecoder::open_small_file_in_memory`: opt-in bounded memory-resident
  local-file comparison path.
- Benchmark wrappers in `codecs::benchmark_support` cover sequential read,
  consume/compact/refill and 64/128-bit reservoir throughput.

## P91-P120 — SIMD and dispatch

- `src/simd/mod.rs`: one-time `OnceLock` dispatch table, scalar references,
  safe wrappers, unaligned/tail tests and randomized scalar equivalence tests.
- `src/simd/x86.rs`: SSE2, SSSE3, SSE4.1, AVX2 and FMA kernels for the
  requested conversion, mix/overlap, interleave/deinterleave, interpolation,
  butterfly, gain, limiter and channel-layout operations.
- `src/simd/neon.rs`: AArch64 NEON implementations plus ARMv7 NEON fallbacks
  where supported.
- `mata-audio-perf`: direct scalar-vs-dispatch gain timing and cached runtime
  ISA lookup timing.

## Validation entry point

On a Rust-enabled benchmark host run:

```sh
performance/scripts/run_baseline.sh
```

That command performs static/corpus validation, the Cargo feature matrix, Rust
tests, SIMD and scalar benchmark runs, regression comparison, and baseline
hardware pinning. Once it succeeds and records the required results, P0-P120
may be formally checked off under the completion rule.
