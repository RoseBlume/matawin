#!/usr/bin/env bash
set -euo pipefail
mkdir -p performance/results/panic
for mode in unwind abort; do
  CARGO_PROFILE_PERF_PANIC="$mode" cargo run --profile perf --no-default-features --features "benchmarking,auto-detect,simd" --bin mata-audio-perf -- performance/corpus/manifest.tsv "performance/results/panic/${mode}.csv"
done
