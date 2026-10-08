#!/usr/bin/env bash
set -euo pipefail
SECONDS_TO_RUN="${MATA_AUDIO_MEMORY_SECONDS:-3600}"
mkdir -p performance/results
cargo run --profile perf --no-default-features --features "benchmarking,auto-detect,simd" --bin mata-audio-memory-perf -- performance/corpus/manifest.tsv performance/results/memory.csv "$SECONDS_TO_RUN"
python3 performance/scripts/check_memory_flatness.py performance/results/memory.csv
