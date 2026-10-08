#!/bin/sh
set -eu
mkdir -p performance/results/profiles
# Records one end-to-end profile containing every enabled codec and the
# synthetic playback baseline. `perf report -i ...` can then filter symbols by
# codec, demuxer, mixer, or SIMD kernel.
perf record -g -o performance/results/profiles/all.data -- \
  cargo run --release --no-default-features \
  --features benchmarking,auto-detect,simd,perf-instrumentation \
  --bin mata-audio-perf -- \
  performance/corpus/manifest.tsv performance/results/profile.csv
printf '%s\n' 'Use: perf report -i performance/results/profiles/all.data'
