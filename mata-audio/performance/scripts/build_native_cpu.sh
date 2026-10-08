#!/usr/bin/env bash
set -euo pipefail
# Compile-time specialization is deliberately an opt-in artifact. Normal
# `cargo build --release` remains portable and runtime-dispatched.
RUSTFLAGS="${RUSTFLAGS:--C target-cpu=native}" cargo build --profile perf "$@"
