#!/usr/bin/env bash
set -euo pipefail
mkdir -p performance/results/codegen
FEATURES="benchmarking,auto-detect,simd"
run_profile(){
  local profile="$1"; local label="$2"; shift 2
  local out="performance/results/codegen/${label}.csv"
  env "$@" cargo run --profile "$profile" --no-default-features --features "$FEATURES" --bin mata-audio-perf -- performance/corpus/manifest.tsv "$out"
}
run_profile perf o3-thin-cgu1
run_profile perf-size os-thin-cgu1
run_profile perf-fat o3-fat-cgu1
run_profile perf-parallel o3-thin-cgu8
run_profile perf o3-thin-cgu1-native RUSTFLAGS="-C target-cpu=native"
python3 performance/scripts/summarize_codegen.py performance/results/codegen/*.csv
