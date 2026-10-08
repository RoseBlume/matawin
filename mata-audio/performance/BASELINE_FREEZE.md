# Baseline freeze gate — P640–P651

The baseline distributed with earlier milestones was a header-only `performance/results/baseline.csv`; it is **not** a measured known-good baseline. No file is silently promoted to `performance/baselines/known-good.csv`.

## Reproducible release procedure

1. Make the sibling `../mata-playback` crate resolvable, or adjust the dependency to your actual workspace. Provide a working device for device-backed tests.
2. Run `performance/scripts/validate_p602_p651.sh` and verify all Cargo tests (scalar and SIMD) and the CPU benchmark pass.
3. On a supported nightly host, run `performance/scripts/run_unsafe_validation.sh`; review actual Miri, ASan and UBSan logs; skips are **not passes**.
4. Run `performance/scripts/benchmark_thermal.sh 180` and `performance/scripts/profile_final.sh`. Run other pre/post phase profiles on the same pinned hardware and compare results.
5. Collect real output telemetry with `cargo run --release --features benchmarking --bin mata-audio-device-perf -- <file> 180`, and use loopback measurements to add `command_to_audio_ns_p95`, `native_seek_ns_p95` and `loop_boundary_gap_samples` to `performance/results/device-current.csv`. Do **not** substitute synthetic timings for audio output latency.
6. Inspect `performance/results/final-performance-report.md`; calibrate `performance/budgets.json` to your benchmark machine while preserving explicit tolerances.
7. After tests and physical-device measurements pass, run `python3 performance/scripts/report_hardening.py --freeze --tests-passed`. This is the **only** command that writes `performance/baselines/known-good.csv` and its SHA-256 lockfile. Archive source, lockfile and test logs together.

A frozen result includes the current source hash, corpus SHA-256, recorded compiler/flags/feature set/ISA, device metrics SHA-256 and hardware fingerprint. Re-run the entire gate after changes to the source or corpus.

## P618/P619 constraints

`rustup` nightly with the Miri component and `rust-src` are needed for Miri and sanitizer runs. ASan/UBSan behavior and supported targets vary by Rust toolchain/platform. The provided script records output when those components exist; no automated check claims a passing run if components were missing.

## Future change policy — P647

Every `src/**/*.rs` hot-path change must add a report under `performance/benchmark-notes/` documenting hardware, settings, before/after throughput, allocations and PCM verification. `check_benchmark_notes.py <base-commit>` provides a git-diff check; reviewers should reject incomplete notes.

## P648 instrumentation

`perf-instrumentation` remains opt-in. Normal `release` and default builds do not enable it; production callbacks have no stage timers. Profile runs explicitly pass `--features perf-instrumentation`.
