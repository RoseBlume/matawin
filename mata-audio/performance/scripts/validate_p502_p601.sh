#!/usr/bin/env bash
set -euo pipefail
python3 performance/scripts/verify_corpus.py
python3 performance/scripts/static_validate.py
python3 -m py_compile performance/scripts/*.py
for script in performance/scripts/*.sh; do bash -n "$script"; done
python3 - <<'PY'
import re
from pathlib import Path
text=Path('todo.md').read_text()
phases=[int(x) for x in re.findall(r'\*\*P(\d+)\s+—',text)]
assert phases==list(range(652)), 'P0-P651 numbering mismatch'
for n in range(602):
    assert re.search(rf'^- \[x\] \*\*P{n} —',text,re.M), f'P{n} is not marked complete'
for n in range(602,652):
    assert re.search(rf'^- \[ \] \*\*P{n} —',text,re.M), f'P{n} should remain pending'
print('todo status OK: P0-P601 complete; P602-P651 pending')
PY
if command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1; then
  cargo test --no-default-features --features "all-codecs,all-demuxers,all-metadata,auto-detect,simd,offline"
  cargo check --no-default-features --features "all-codecs,all-demuxers,all-metadata,auto-detect,simd,offline,prefetch"
  cargo check --no-default-features --features "all-codecs,all-demuxers,all-metadata,auto-detect,simd,offline,fast-math"
  cargo check --no-default-features --features "all-codecs,all-demuxers,all-metadata,auto-detect,simd,offline,mmap"
else
  echo 'cargo/rustc unavailable: Rust integration/benchmark execution skipped; static validation completed.'
fi
