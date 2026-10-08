#!/bin/sh
set -eu
cargo test --test seek_timeline
cargo run --release --no-default-features --features benchmarking,auto-detect,simd --bin mata-audio-seek-perf -- performance/corpus
