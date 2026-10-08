#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$ROOT"
mkdir -p performance/results/profiles
manifest=performance/corpus/.profile-playback.tsv
head -n 1 performance/corpus/manifest.tsv > "$manifest"
trap 'rm -f "$manifest"' EXIT INT TERM
perf record -g -o performance/results/profiles/playback.data -- \
  cargo run --release --no-default-features \
    --features benchmarking,auto-detect,simd,perf-instrumentation \
    --bin mata-audio-perf -- \
    "$manifest" performance/results/profile-playback.csv
