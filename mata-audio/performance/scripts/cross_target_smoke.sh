#!/usr/bin/env bash
set -euo pipefail
# Compile-smoke the portable decoder surface for the selected targets. Native
# benchmark execution is still performed by platform_smoke.sh on real hardware.
TARGETS=(
  x86_64-pc-windows-msvc
  x86_64-unknown-linux-gnu
  aarch64-unknown-linux-gnu
  aarch64-linux-android
)
for target in "${TARGETS[@]}"; do
  echo "== $target =="
  rustup target add "$target" || true
  cargo check --target "$target" --no-default-features --features "all-codecs,all-demuxers,all-metadata,auto-detect,simd,offline" || {
    echo "target smoke unavailable for $target on this host/toolchain" >&2
    exit 1
  }
done
