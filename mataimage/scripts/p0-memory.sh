#!/usr/bin/env bash
set -euo pipefail

# P0 ownership/memory regression sweep. The benchmark executable reports
# allocation count, total allocated bytes, temporary bytes, peak live heap and
# Linux VmHWM when available. Keep the output from the commit before P0 and
# compare it with the current run rather than recording machine-specific numbers
# in the repository.

run_case() {
    local filter="$1"
    printf '\n=== P0 memory case: %s ===\n' "$filter"
    MATAIMAGE_BENCH_FILTER="$filter" cargo bench --bench decode --all-features
}

run_case "4k_rgba.png"
run_case "8k_rgba.png"
run_case "gif_120_frames.gif"
run_case "webp_120_frames.webp"
