#!/usr/bin/env bash
set -euo pipefail
FILE="${1:-performance/corpus/flac_s16_stereo_44100.flac}"
mkdir -p performance/results/offline
cargo run --profile perf --no-default-features --features "benchmarking,offline,simd" --bin mata-audio-offline-perf -- "$FILE" 5 > performance/results/offline/read-ahead.csv
# mmap is intentionally opt-in and benchmark-only; it is never selected by the
# normal decoder or realtime playback path.
cargo run --profile perf --no-default-features --features "benchmarking,offline,mmap,simd" --bin mata-audio-offline-perf -- "$FILE" 5 > performance/results/offline/mmap.csv
