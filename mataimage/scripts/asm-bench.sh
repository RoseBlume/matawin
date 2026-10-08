#!/usr/bin/env sh
set -eu
# Emit optimized assembly for checking BitReader/Huffman hot call sites and
# SIMD dispatch. Output location is printed by rustc/cargo and normally lands
# under target/release/deps/.
mkdir -p bench-results
RUSTFLAGS="${RUSTFLAGS:-} -C debuginfo=1" \
  cargo rustc --release --lib -- --emit=asm
find target/release/deps -maxdepth 1 -name 'mataimage-*.s' -print \
  > bench-results/assembly-files.txt
cat bench-results/assembly-files.txt
