#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
if ! command -v rustup >/dev/null || ! rustup toolchain list | grep -q nightly; then
  echo 'SKIP: nightly toolchain is required for Miri and sanitizer build-std'; exit 0
fi
mkdir -p performance/results/unsafe
FEATURES='benchmarking,auto-detect,simd'
if cargo +nightly miri --version >/dev/null 2>&1; then
  # Run non-SIMD stateful buffering tests and pure reference transforms with Miri.
  MIRIFLAGS='-Zmiri-disable-isolation' cargo +nightly miri test --no-default-features --features 'auto-detect' --lib \
    | tee performance/results/unsafe/miri.log
else
  echo 'SKIP Miri: rustup +nightly component add miri'; fi
if rustup +nightly component list --installed | grep -q rust-src; then
  for kind in address undefined; do
    RUSTFLAGS="-Zsanitizer=$kind" cargo +nightly test -Zbuild-std --target "$(rustc -vV | sed -n 's/^host: //p')" \
      --no-default-features --features "$FEATURES" --test hardening_decode \
      | tee "performance/results/unsafe/$kind.log"
  done
else
  echo 'SKIP sanitizers: rustup +nightly component add rust-src'; fi
