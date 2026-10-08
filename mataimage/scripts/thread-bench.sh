#!/usr/bin/env sh
set -eu
# The benchmark binary contains explicit 1/2/4/8 max-thread JPEG runs. Under
# Linux, repeat those runs with taskset so the scheduler is also constrained to
# 1/2/4/8 logical CPUs. MATAIMAGE_BENCH_FILTER limits end-to-end corpus output.
for cores in 1 2 4 8; do
  echo "=== ${cores} logical CPU(s) ==="
  if command -v taskset >/dev/null 2>&1; then
    last=$((cores - 1))
    MATAIMAGE_BENCH_FILTER=jpeg taskset -c "0-${last}" cargo bench --bench decode
  else
    MATAIMAGE_BENCH_FILTER=jpeg cargo bench --bench decode
    break
  fi
done
