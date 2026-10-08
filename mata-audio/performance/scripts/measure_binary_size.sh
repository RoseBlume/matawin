#!/usr/bin/env bash
set -euo pipefail
mkdir -p performance/results/size
measure(){
  local label="$1" features="$2" flags="${3:-}"
  if [[ -n "$flags" ]]; then RUSTFLAGS="$flags" cargo build --profile perf --no-default-features --features "$features"; else cargo build --profile perf --no-default-features --features "$features"; fi
  local lib
  lib=$(find target/perf/deps -maxdepth 1 \( -name 'libmata_audio*.rlib' -o -name 'mata_audio*.lib' \) -printf '%s %p\n' 2>/dev/null | sort -nr | head -1 || true)
  printf '%s,%s\n' "$label" "${lib:-unavailable}" >> performance/results/size/simd-size.csv
}
: > performance/results/size/simd-size.csv
measure scalar "auto-detect"
measure simd "auto-detect,simd"
measure simd-prefetch "auto-detect,simd,prefetch"
measure simd-native "auto-detect,simd" "-C target-cpu=native"
