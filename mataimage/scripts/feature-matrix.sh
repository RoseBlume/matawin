#!/usr/bin/env bash
set -euo pipefail

# Verifies that each format is independently feature-gated and that SIMD can be
# compiled out. Run from the crate root.
features=(bmp pgm tga png wbmp qoi jpeg webp gif fits ico tiff svg)

cargo check --no-default-features
for feature in "${features[@]}"; do
    cargo check --no-default-features --features "$feature"
done
cargo check --no-default-features --features "png,simd"
cargo check --no-default-features --features "all-formats"

# AVIF intentionally lives in its own dependency-bearing check.
cargo check --no-default-features --features avif
