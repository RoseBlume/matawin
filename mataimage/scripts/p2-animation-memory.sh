#!/usr/bin/env bash
set -euo pipefail

# The bench executable has a dedicated P2 section that compares 120-frame GIF
# and WebP streaming against all-frame materialization with peak-live tracking.
cargo bench --bench decode
