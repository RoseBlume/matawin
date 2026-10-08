#!/usr/bin/env bash
set -euo pipefail
mkdir -p performance/results/backends
run(){ local feature="$1"; local name="$2"; cargo run --profile perf --no-default-features --features "benchmarking,simd,all-codecs,all-demuxers,auto-detect,$feature" --bin mata-audio-perf -- performance/corpus/manifest.tsv "performance/results/backends/${name}.csv"; }
run mata-playback mata-playback
run cpal cpal
python3 performance/scripts/summarize_codegen.py performance/results/backends/*.csv
