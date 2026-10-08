#!/usr/bin/env bash
set -euo pipefail
mkdir -p performance/results/math
cargo run --profile perf --no-default-features --features benchmarking --bin mata-audio-math-perf | tee performance/results/math/denormal-fastmath.txt
