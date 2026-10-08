#!/usr/bin/env bash
set -euo pipefail
OUT=performance/results/assembly
mkdir -p "$OUT"
RUSTFLAGS="${RUSTFLAGS:--C target-cpu=native}" cargo rustc --profile perf --no-default-features --features "auto-detect,simd" --lib -- --emit=asm
find target/perf/deps -maxdepth 1 -name 'mata_audio-*.s' -print -exec cp {} "$OUT/" \;
# Symbol/source-name probes correspond to P543-P548. Missing names are printed,
# not treated as failure, because LTO/mangling may fold a hot kernel.
for token in pcm_i16 flac restore_fixed rice L3_huffman dequant synthesis imdct render_linear_kernel mix_add; do
  echo "===== $token ====="
  grep -Rni "$token" "$OUT" || true
done
