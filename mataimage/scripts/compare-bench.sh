#!/usr/bin/env sh
set -eu
# P1 scalar/SIMD/native comparison matrix. The benchmark prints compiler, CPU,
# target and feature information into each result file for reproducibility.
mkdir -p bench-results

cargo bench --bench decode --no-default-features --features all-formats \
  > bench-results/scalar-release.txt
cargo bench --bench decode \
  > bench-results/simd-release.txt

# Debug is only a correctness/sanity comparison; optimization decisions should
# use release/bench results. `cargo test --bench` executes harness=false benches.
if [ "${MATAIMAGE_DEBUG_BENCH:-0}" = "1" ]; then
  cargo test --bench decode --no-default-features --features all-formats \
    > bench-results/scalar-debug.txt
fi

# Opt-in known-CPU build. This intentionally remains separate from the portable
# default so published numbers do not accidentally depend on the local CPU.
if [ "${MATAIMAGE_NATIVE:-0}" = "1" ]; then
  MATAIMAGE_TARGET_CPU=native RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=native" \
    cargo bench --bench decode > bench-results/native-release.txt
fi

echo "Wrote bench-results/scalar-release.txt and bench-results/simd-release.txt"
