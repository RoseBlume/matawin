#!/usr/bin/env bash
set -euo pipefail
cargo test --release --all-features
cargo run --release --no-default-features --features benchmarking,all-demuxers --bin mata-audio-demux-fuzz -- performance/demuxer_manifest.tsv
cargo run --release --no-default-features --features benchmarking,all-demuxers --bin mata-audio-demux-perf -- performance/demuxer_manifest.tsv performance/results/demuxers.csv
python3 performance/scripts/check_demuxer_regressions.py performance/results/demuxers.csv performance/demuxer_thresholds.tsv
cargo run --release --no-default-features --features benchmarking,auto-detect,simd,perf-instrumentation --bin mata-audio-perf -- performance/corpus/manifest.tsv performance/results/p217-p359.csv
