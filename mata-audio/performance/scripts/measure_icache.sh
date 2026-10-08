#!/usr/bin/env bash
set -euo pipefail
if ! command -v perf >/dev/null 2>&1; then echo 'Linux perf is unavailable; i-cache run skipped'; exit 0; fi
mkdir -p performance/results/icache
run(){
  local label="$1"; shift
  env "$@" cargo build --profile perf --no-default-features --features "benchmarking,auto-detect,simd" --bin mata-audio-perf
  perf stat -x, -e instructions,cycles,cache-references,cache-misses,L1-icache-load-misses \
    -o "performance/results/icache/${label}.csv" -- \
    target/perf/mata-audio-perf performance/corpus/manifest.tsv "performance/results/icache/${label}-results.csv"
}
run portable
run native RUSTFLAGS="-C target-cpu=native"
