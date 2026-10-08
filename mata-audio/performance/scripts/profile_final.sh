#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
command -v cargo >/dev/null || { echo 'Missing Cargo; no real profiler run recorded' >&2; exit 1; }
command -v perf >/dev/null || { echo 'Missing Linux perf; use native host profiler and provide ranked report' >&2; exit 1; }
mkdir -p performance/results/profiles
cargo build --profile perf --no-default-features --features 'benchmarking,auto-detect,simd,perf-instrumentation' --bin mata-audio-perf
perf record -g -o performance/results/profiles/final.data -- \
  target/perf/mata-audio-perf performance/corpus/manifest.tsv performance/results/profile-final.csv
perf report --stdio --no-children -i performance/results/profiles/final.data > performance/results/profiles/final.txt
python3 performance/scripts/rank_hotspots.py performance/results/profiles/final.txt
