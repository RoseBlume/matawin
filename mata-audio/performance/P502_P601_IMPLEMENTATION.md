# P502-P601 implementation map

## P502-P526 — memory footprint and cache locality

- `src/codecs/mod.rs`: `DecoderMemoryUsage`, decoder memory/trim hooks, reusable `AudioFrameBuffer` capacity/high-water accounting.
- `src/codecs/support/memory.rs`: bounded checked growth plus 64-byte `AlignedScratch` for hot float/integer workspaces.
- FLAC/Vorbis/MPEG/PCM implementations report persistent, scratch, compressed, index and metadata/setup capacity; normal reset/seek retains buffers.
- Vorbis uses compact codebook integers/fast-table entries and boxes cold setup/comments; FLAC metadata blocks use exact boxed payloads.
- `src/playback.rs`: Voice retained/high-water telemetry, shared `Arc<PathBuf>`, cold-path memory trimming, cold error strings.
- `mata-audio-memory-perf`, `run_memory_validation.sh`, `run_playback_memory_validation.sh` and `profile_allocations.sh` cover decoder flatness, playback flatness, 128-source scaling and allocation call stacks.

## P527-P551 — compiler/code generation

- `Cargo.toml`: portable release plus `perf`, `perf-size`, `perf-fat`, and `perf-parallel` profiles.
- `benchmark_codegen.sh`: O3/Os, ThinLTO/FatLTO, CGU=1/8 and native-CPU variants.
- `build_native_cpu.sh`: explicit `target-cpu=native` opt-in; normal release remains runtime-dispatched.
- `CODEGEN.md`: inlining/cold-path policy. Error construction/formatting is `#[cold] #[inline(never)]`; tiny kernels retain narrow inlining annotations.
- `inspect_assembly.sh`: PCM, FLAC Rice/LPC, MPEG Huffman/dequant/synthesis, Vorbis IMDCT and mixer/resampling assembly probes.
- `measure_binary_size.sh`, `measure_icache.sh`, and `benchmark_panic_strategy.sh`: SIMD/native size, instruction-cache and panic-mode comparisons.

## P552-P574 — backend/OS efficiency

- mata-playback remains the default native backend; CPAL is separately feature gated for A/B measurements.
- `OutputSession` caches device format/ring/period. Device enumeration is startup/recovery work, and recovery retains source decoders.
- Config scoring prefers native f32 and rates near active source rates.
- Platform output policies cover Windows, direct Linux/ALSA, PulseAudio/PipeWire and Android burst/ring sizing.
- Worker realtime/affinity tuning is opt-in through the installed tuning hook and safely falls back to normal scheduling.
- Non-normal PCM sanitization flushes denormals outside the common branch and publishes a telemetry counter.
- `benchmark_backends.sh`, `benchmark_denormals.sh`, `platform_smoke.sh`, `cross_target_smoke.sh`, and `performance-smoke.yml` provide backend and Windows/Linux/ARM64/Android smoke coverage.

## P575-P601 — advanced/offline pipeline

- `src/codecs/offline.rs`: bounded compressed read-ahead worker, generation-safe seeking, parallel independent-file decode, thresholded independent-channel sanitization, optional mmap path.
- Realtime playback already uses bounded decoder-worker/SPSC stages and stays independent from offline read-ahead.
- PCM, channel routing, resampling and limiter kernels are selected once and cached as function pointers.
- `prefetch` and `fast-math` are explicit non-default experiment features; default math/dispatch stays portable/exact.
- Q32 resampling/timeline arithmetic, f32 hot DSP, blocked Vorbis IMDCT, fixed FLAC stack coefficients and setup-arena/small-stack benchmark experiments cover P591-P599.
- `benchmark_offline_pipeline.sh`, `benchmark_prefetch.sh`, `benchmark_math_strategies.sh` provide reproducible adoption tests; mmap is optional only.

A Cargo integration/benchmark run is still required on a Rust-enabled host to produce machine-specific performance numbers. Per project-owner instruction, implementation-complete phases may be checked before that integration run.
