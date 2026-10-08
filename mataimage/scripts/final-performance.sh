#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo is required to run the final performance validation" >&2
    exit 127
fi

mkdir -p performance-results
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT="performance-results/${STAMP}"
mkdir -p "$OUT"

run() {
    local name="$1"; shift
    echo "==> $name"
    { printf '$'; printf ' %q' "$@"; printf '\n'; "$@"; } 2>&1 | tee "$OUT/${name}.log"
}

run test-default cargo test
run test-all-features cargo test --all-features
run test-no-default cargo test --no-default-features
run bench-decode cargo bench --bench decode

for script in \
    scripts/feature-matrix.sh \
    scripts/size-matrix.sh \
    scripts/codegen-matrix.sh \
    scripts/thread-bench.sh \
    scripts/compare-bench.sh; do
    if [[ -x "$script" ]]; then
        name="$(basename "$script" .sh)"
        run "$name" "$script"
    fi
done

if command -v rustc >/dev/null 2>&1; then
    rustc -Vv > "$OUT/rustc.txt"
fi
cargo -V > "$OUT/cargo.txt"
{
    uname -a || true
    printf '\nCPU:\n'
    if [[ -r /proc/cpuinfo ]]; then grep -m1 -E 'model name|Hardware|Processor' /proc/cpuinfo || true; fi
} > "$OUT/system.txt"

printf 'Final validation logs: %s\n' "$OUT"
