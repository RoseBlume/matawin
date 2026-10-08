#!/usr/bin/env bash
set -euo pipefail
mkdir -p performance/results/math
cargo run --profile perf --no-default-features --features benchmarking --bin mata-audio-math-perf > performance/results/math/exact.txt
cargo run --profile perf --no-default-features --features "benchmarking,fast-math" --bin mata-audio-math-perf > performance/results/math/fast.txt
# P598/P599 experiments are deliberately benchmark-only; they do not force
# stack/arena strategies into codecs unless measured results justify them.
cargo run --profile perf --no-default-features --features benchmarking --bin mata-audio-memory-strategies > performance/results/math/memory-strategies.txt
