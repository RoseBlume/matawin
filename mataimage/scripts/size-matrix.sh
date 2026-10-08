#!/usr/bin/env bash
set -euo pipefail

# Measures compile time and produced library/example size for representative
# feature sets. GNU /usr/bin/time is used when available.
measure() {
  local name="$1"; shift
  cargo clean >/dev/null 2>&1 || true
  echo "=== $name ==="
  if [[ -x /usr/bin/time ]]; then
    /usr/bin/time -f 'elapsed=%e maxrss_kb=%M' cargo build --release "$@"
  else
    time cargo build --release "$@"
  fi
  find target/release -maxdepth 2 -type f \( -name '*.rlib' -o -name 'pixdump*' \) -printf '%s %p\n' 2>/dev/null | sort -n
}

measure default
measure minimal-png --no-default-features --features png
measure png-simd --no-default-features --features png,simd
measure all-stable --no-default-features --features all-formats
