#!/bin/sh
set -eu
# Linux profiler helper. `perf` is optional and intentionally not a Rust dependency.
mkdir -p performance/results
perf record -g -- cargo run --release --no-default-features --features benchmarking,auto-detect,simd --bin mata-audio-perf -- performance/corpus/manifest.tsv performance/results/profile.csv
perf report
