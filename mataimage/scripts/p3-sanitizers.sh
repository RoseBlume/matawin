#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

case "${1:-miri}" in
  miri)
    cargo +nightly miri test --all-features
    ;;
  address)
    RUSTFLAGS="${RUSTFLAGS:-} -Zsanitizer=address" \
      cargo +nightly test --all-features -Zbuild-std --target "$(rustc -vV | sed -n 's|host: ||p')"
    ;;
  fuzz)
    cargo +nightly fuzz run decode --fuzz-dir fuzz -- -max_total_time="${MATAIMAGE_FUZZ_SECONDS:-60}"
    ;;
  *)
    echo "usage: $0 {miri|address|fuzz}" >&2
    exit 2
    ;;
esac
