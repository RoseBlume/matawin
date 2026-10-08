#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
python3 performance/scripts/static_validate.py
python3 performance/scripts/verify_corpus.py
python3 -m py_compile performance/scripts/*.py
for f in performance/scripts/*.sh; do bash -n "$f"; done
python3 performance/scripts/check_hardening.py
if ! command -v cargo >/dev/null || ! command -v rustc >/dev/null; then
  echo 'SKIP: Cargo/rustc absent. Rust tests, benchmarks, Miri, sanitizers and baseline promotion have NOT run.'
  python3 performance/scripts/report_hardening.py --draft
  exit 0
fi
# Decoder tests work without a physical output device (dependency resolution still
# requires the sibling optional mata-playback crate on a standard Cargo checkout).
cargo test --no-default-features --features 'benchmarking,auto-detect,simd' --all-targets
cargo test --no-default-features --features 'benchmarking,auto-detect' --all-targets
cargo test --no-default-features --features 'benchmarking,auto-detect,simd' --test hardening_decode
# Playback stress requires a resolvable local mata-playback dependency.
cargo test --no-default-features --features 'benchmarking,mata-playback,simd' --lib hardening_spsc_tests
mkdir -p performance/results
cargo run --release --no-default-features --features 'benchmarking,auto-detect,simd,perf-instrumentation' --bin mata-audio-perf -- performance/corpus/manifest.tsv performance/results/current.csv
python3 performance/scripts/report_hardening.py --draft --tests-passed
printf '%s\n' 'Cargo tests and CPU benchmarks ran. Baseline freeze still requires real device metrics and successful explicit --freeze.'
