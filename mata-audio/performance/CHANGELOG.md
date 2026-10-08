# mata-audio performance changelog

This log records *implemented* changes separately from *measured* results.

## P602–P651 hardening implementation (2026-10-07)

- Added independently pinned PCM reference comparisons for every corpus entry, plus deterministic format-aware truncation, size-field and bit-flip mutation tests.
- Added randomized FLAC MSB / Vorbis LSB reader differential tests and MPEG `ByteWindow` sequence-model tests.
- Added wraparound, producer-consumer concurrency, forced-underrun and 128-queue synthetic stress tests for playback SPSC infrastructure.
- Added exhaustive SIMD short-tail/misalignment/nonfinite conversion checks.
- Added reproducible Miri/sanitizer, sustained thermal and final Linux `perf` entry points, automated five-hotspot ranking and follow-on issue template.
- Added per-codec/mixer/underrun/startup/seek/loop budgets and a report generator that refuses to freeze an unmeasured baseline.
- Result status: **source/test infrastructure prepared; Rust execution and device-backed benchmarks not run in this environment**.

## Previous phases

See `P0_P120_IMPLEMENTATION.md`, `P121_P216_IMPLEMENTATION.md`, `P217_P359_IMPLEMENTATION.md`, `P360_P501_IMPLEMENTATION.md`, and `P502_P601_IMPLEMENTATION.md` for the preceding milestones. No unmeasured speedup percentages are claimed.
