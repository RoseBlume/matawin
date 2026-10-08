#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
seconds="${1:-180}"
case "$seconds" in ''|*[!0-9]*) echo 'duration must be seconds as a positive integer' >&2; exit 2;; esac
if (( seconds < 60 )); then echo 'Use at least 60 seconds for thermal stability' >&2; exit 2; fi
command -v cargo >/dev/null || { echo 'Cargo is required' >&2; exit 1; }
mkdir -p performance/results/thermal
cargo build --release --no-default-features --features 'benchmarking,auto-detect,simd' --bin mata-audio-perf
started=$(date +%s); iteration=0
printf 'iteration,epoch_seconds,elapsed_seconds,run_seconds\n' > performance/results/thermal/runs.csv
while (( $(date +%s) - started < seconds )); do
  t=$(date +%s)
  target="performance/results/thermal/run-${iteration}.csv"
  target/release/mata-audio-perf performance/corpus/manifest.tsv "$target"
  now=$(date +%s)
  printf '%d,%d,%d,%d\n' "$iteration" "$now" "$((now-started))" "$((now-t))" >> performance/results/thermal/runs.csv
  iteration=$((iteration+1))
done
printf 'Completed %s sustained iterations in at least %s seconds\n' "$iteration" "$seconds"
