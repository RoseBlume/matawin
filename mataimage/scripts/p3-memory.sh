#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Streaming PNG should avoid a full encoded-file copy while keeping the final
# packed output plus bounded DEFLATE/scanline scratch. Long animations compare
# streaming frame iteration against all-frame materialization in the same bench.
for filter in 4k_rgba.png 8k_rgba.png png_many_idat.png gif_120_frames.gif webp_120_frames.webp; do
  echo "=== $filter ==="
  MATAIMAGE_BENCH_FILTER="$filter" cargo bench --bench decode
  echo
done
