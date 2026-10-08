# P121–P216 implementation map

This file maps the second optimization tranche to concrete source/harness work. The checkboxes in `todo.md` represent implementation completion; the integration/performance suite can still be rerun on any Rust-enabled host.

## P121–P145 — RIFF/WAVE + PCM

- WAV PCM input bytes reuse `WavPcm::byte_scratch`; the initial capacity is derived from the tuned demux chunk size.
- `PcmDecodePlan` validates the format once and caches bytes/sample, valid-bit shift, and normalization scale. WAV decoding uses the prevalidated plan directly on every chunk.
- PCM conversion dispatch has optimized paths for unsigned 8-bit, signed 16/24/32-bit, f32, and f64. Native little-endian f32 uses a bulk-copy fast path when every sample is already finite and normalized.
- The existing root SIMD dispatch supplies 16/24/32-bit and float conversion kernels with scalar fallbacks.
- `WavDemuxer` tunes chunks around a 32 KiB target, aligns frame counts to a 256-frame playback quantum, caches total frames/duration/block alignment, and avoids steady-state format validation/division work.
- `WavDemuxer::seek_frame` is an O(1) byte seek and `reset` reuses it without dropping scratch capacity.
- The benchmark binary contains WAV per-bit-depth conversion regressions (`wav_u8`, `wav_s16`, `wav_s24`, `wav_s32`, `wav_f32`, `wav_f64`).

## P146–P165 — AIFF/AIFC + PCM

- AIFF PCM byte scratch is retained exactly like WAV and decoded directly through a cached `PcmDecodePlan`.
- AIFF sample-size shift/normalization values are precomputed once.
- Signed 8/16/24/32-bit big-endian paths use the shared scalar/SIMD PCM kernels.
- AIFC `sowt` is supported as little-endian PCM; `NONE`/`twos` remain big-endian.
- AIFF chunk sizing uses the same cache/playback tuning policy as WAV and caches duration/block alignment.
- `AiffDemuxer::seek_frame` provides direct O(1) seeking and reset preserves decoder allocations.
- The 80-bit extended sample-rate decoder includes common-rate fast paths plus a direct power-of-two construction for normal f64 exponents.
- The benchmark binary covers AIFF/AIFC 8/16/24/32-bit big/little-endian conversion cases.

## P166–P216 — FLAC

- `flac::BitReader` is persistent across frames, batches 8 KiB source reads, and uses a u64 reservoir. Unary prefixes are scanned with `leading_zeros` instead of per-bit loops.
- Raw per-frame byte capture was removed. CRC-8 and CRC-16 are updated incrementally as bytes are pulled from the buffered reader, avoiding second-pass frame scans.
- `FlacScratch` preallocates a flat `channels: Vec<i64>` plus residual scratch from `STREAMINFO.max_block_size`. Subframes decode into slices instead of returning `Vec`s.
- Constant/verbatim subframes fill provided slices. Fixed predictors 0–4 have specialized reconstruction loops; order 0 decodes residuals directly into channel storage.
- LPC coefficients use a fixed `[i64; 32]` stack buffer. Reconstruction uses checked i64 multiply-accumulate under FLAC's bounded order/precision constraints.
- Residual decoding writes into reusable scratch, only overwrites the active span, specializes Rice parameter zero, batches partitions, and handles escape-coded residuals without temporary vectors.
- Left-side and right-side stereo restoration use runtime-dispatched AVX2 i64 kernels on capable x86 hosts and scalar fallback elsewhere. Mid-side restoration is fused with normalization/interleave.
- Normalization uses a precomputed reciprocal and fuses planar-to-interleaved output in a single traversal.
- FLAC SEEKTABLE metadata is parsed into `SeekPoint`s. `Flac::seek_to_sample` chooses the closest SEEKTABLE or learned index point at/before the requested sample.
- A bounded lazy frame index records observed byte/sample positions and survives reset along with all scratch allocations.
- The corpus contains constant, fixed-predictor, LPC, Rice partition-order 0/4, mono/stereo, 16-bit and 24-bit FLAC cases. `flac -a` was used while constructing the corpus to verify the intended predictor/residual families.
- Stage timers now separate `flac.header`, `flac.subframes`, `flac.restore-normalize`, and the outer `flac.decode` stage when `perf-instrumentation` is enabled.
