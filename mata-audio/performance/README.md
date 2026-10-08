# Performance suite

This directory is the reproducible baseline/regression harness used by P0-P30, the shared microbenchmarks used by P61-P120, and the WAV/AIFF/FLAC regression/profiling coverage used by P121-P216.

Run the full suite from the crate root:

```sh
cargo run --release --no-default-features --features benchmarking,auto-detect,simd --bin mata-audio-perf -- performance/corpus/manifest.tsv performance/results/current.csv
```

Run a scalar comparison by omitting `simd`. The corpus manifest records input/reference SHA-256 hashes, stream format, channel count, rate, and tolerance. Reference `.f32` files are little-endian interleaved `f32` PCM. `current.csv` records compiler version, feature set and runtime ISA on every row so results remain attributable.

For stable regressions, run on pinned hardware with fixed power mode, no competing workloads, and the same Rust toolchain. Copy an accepted `current.csv` to `performance/results/baseline.csv`, then use `scripts/compare_results.py` with the desired percentage threshold.

The P90 experiment uses the std-only bounded memory-resident file path as the portable mmap-style comparison. It intentionally does not add an mmap dependency to the audio crate; it compares buffered streaming against a local file resident in one contiguous byte region. If a platform-specific true mmap backend is added later, it can use the same `io,mmap_style_*` result slots.

## One-command validation

`performance/scripts/validate_p0_p120.sh` retains the original P0-P120 validation entry point. `performance/scripts/validate_p0_p216.sh` extends validation through the WAV/AIFF/FLAC P121-P216 implementation, running corpus/static validation, the Cargo feature matrix, Rust tests, and an instrumented performance pass. `run_baseline.sh` performs the baseline-oriented checks and initializes the baseline if one has not yet been frozen.

Use `profile_codecs.sh` for one `perf` recording per corpus/codec path, `profile_flac.sh` for the P212-P216 FLAC predictor/Rice/stage corpus, and `profile_playback.sh` for the synthetic playback/mixer workload. These scripts write only under `performance/results/`.

## P217–P359 validation

`./performance/scripts/validate_p217_p359.sh` runs the full-feature tests, deterministic demuxer mutation harness, demuxer-only startup/throughput benchmark and regression thresholds, then the instrumented codec benchmark. See `P217_P359_IMPLEMENTATION.md` for the source-to-phase map.

## P360–P501 validation

`./performance/scripts/validate_p360_p501.sh` adds the metadata feature-isolation matrix, disabled-API/default-surface gates, feature size/compile-time tooling, playback architecture invariants, and native seek/timeline tests. `run_metadata_matrix.sh` records parser timing plus allocator counts per metadata feature. `run_seek_validation.sh` exercises native seek accuracy, repeated-random seek timing, exact loop-prefix handoff and long-duration Q32 timeline drift. See `P360_P501_IMPLEMENTATION.md` for the source-to-phase map.

When Cargo is unavailable, `validate_p360_p501.sh` still runs corpus hashes, feature-surface checks, script syntax checks, TODO numbering, Rust delimiter validation, and the source-level P360–P501 architecture invariants; it reports the skipped Rust integration step explicitly.
