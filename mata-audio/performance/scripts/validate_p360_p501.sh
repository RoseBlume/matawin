#!/usr/bin/env sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$ROOT"

python3 performance/scripts/static_validate.py
python3 performance/scripts/check_feature_surface.py
python3 performance/scripts/verify_corpus.py

# Script syntax is part of the source-level handoff and works without Rust.
for script in performance/scripts/*.sh; do sh -n "$script"; done
python3 -m py_compile performance/scripts/*.py

if command -v cargo >/dev/null 2>&1; then
  performance/scripts/check_feature_matrix.sh
  performance/scripts/verify_disabled_api.sh
  performance/scripts/run_metadata_matrix.sh
  cargo test --test metadata_position
  cargo test --test default_feature_surface
  performance/scripts/run_seek_validation.sh
  cargo test --release --all-features
else
  echo "cargo unavailable: Rust integration/benchmark execution skipped; source/static validation completed" >&2
fi
