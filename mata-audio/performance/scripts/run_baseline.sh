#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$ROOT"
mkdir -p performance/results

python3 performance/scripts/static_validate.py
python3 performance/scripts/verify_corpus.py
python3 performance/scripts/capture_hardware.py performance/results/hardware-current.md
performance/scripts/check_feature_matrix.sh

cargo test --no-default-features --features auto-detect,simd,benchmarking
cargo run --release --no-default-features \
  --features benchmarking,auto-detect,simd,perf-instrumentation \
  --bin mata-audio-perf -- \
  performance/corpus/manifest.tsv performance/results/current.csv

# Scalar run provides the before/reference point for SIMD P91-P120 work.
cargo run --release --no-default-features \
  --features benchmarking,auto-detect \
  --bin mata-audio-perf -- \
  performance/corpus/manifest.tsv performance/results/scalar-current.csv

if [ -s performance/results/baseline.csv ] && [ "$(wc -l < performance/results/baseline.csv)" -gt 1 ]; then
  python3 performance/scripts/compare_results.py \
    performance/results/baseline.csv performance/results/current.csv
else
  cp performance/results/current.csv performance/results/baseline.csv
  cp performance/results/current.md performance/results/baseline.md
  cp performance/results/hardware-current.md performance/HARDWARE.md
  printf '%s\n' 'Initialized performance/results/baseline.csv and pinned performance/HARDWARE.md from this validated run.'
fi
