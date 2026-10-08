#!/usr/bin/env bash
set -euo pipefail
HOST="$(rustc -vV | sed -n 's/^host: //p')"
case "$HOST" in
  *android*) LABEL=android ;;
  x86_64-pc-windows-*) LABEL=windows-x86_64 ;;
  x86_64-unknown-linux-*) LABEL=linux-x86_64 ;;
  aarch64-*) LABEL=arm64 ;;
  *) LABEL=other ;;
esac
mkdir -p performance/results/smoke
cargo test --profile perf --no-default-features --features "all-codecs,all-demuxers,all-metadata,auto-detect,simd,offline"
cargo run --profile perf --no-default-features --features "benchmarking,auto-detect,simd" --bin mata-audio-perf -- performance/corpus/manifest.tsv "performance/results/smoke/${LABEL}.csv"
cargo run --profile perf --no-default-features --features benchmarking --bin mata-audio-math-perf > "performance/results/smoke/${LABEL}-math.txt"
