#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$ROOT"
python3 performance/scripts/static_validate.py
python3 performance/scripts/verify_corpus.py
performance/scripts/check_feature_matrix.sh
cargo test --no-default-features --features auto-detect,simd,benchmarking
cargo run --release --no-default-features \
  --features benchmarking,auto-detect,simd,perf-instrumentation \
  --bin mata-audio-perf -- \
  performance/corpus/manifest.tsv performance/results/current.csv
cargo run --release --no-default-features \
  --features benchmarking,auto-detect \
  --bin mata-audio-perf -- \
  performance/corpus/manifest.tsv performance/results/scalar-current.csv
if [ -s performance/results/baseline.csv ] && [ "$(wc -l < performance/results/baseline.csv)" -gt 1 ]; then
  python3 performance/scripts/compare_results.py performance/results/baseline.csv performance/results/current.csv
fi
