#!/usr/bin/env bash
set -euo pipefail
mkdir -p performance/results/prefetch
BASE="benchmarking,auto-detect,simd"
cargo run --profile perf --no-default-features --features "$BASE" --bin mata-audio-perf -- performance/corpus/manifest.tsv performance/results/prefetch/off.csv
cargo run --profile perf --no-default-features --features "$BASE,prefetch" --bin mata-audio-perf -- performance/corpus/manifest.tsv performance/results/prefetch/on.csv
python3 performance/scripts/compare_results.py performance/results/prefetch/off.csv performance/results/prefetch/on.csv > performance/results/prefetch/comparison.txt
