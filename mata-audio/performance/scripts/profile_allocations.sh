#!/usr/bin/env bash
set -euo pipefail
mkdir -p performance/results
# The built-in counting allocator gives portable allocation counts/bytes.
cargo run --profile perf --no-default-features --features "benchmarking,auto-detect,simd" --bin mata-audio-perf -- performance/corpus/manifest.tsv performance/results/allocation-profile.csv
# Optional external heap profiler for deeper call stacks.
if command -v heaptrack >/dev/null 2>&1; then
  heaptrack target/perf/mata-audio-perf performance/corpus/manifest.tsv performance/results/heaptrack.csv || true
elif command -v valgrind >/dev/null 2>&1; then
  valgrind --tool=massif --massif-out-file=performance/results/massif.out target/perf/mata-audio-perf performance/corpus/manifest.tsv performance/results/massif.csv || true
fi
