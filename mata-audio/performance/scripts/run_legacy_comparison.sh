#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
if (( $# != 1 )); then
  echo 'Usage: performance/scripts/run_legacy_comparison.sh PATH_TO_P0_P120_CRATE' >&2
  exit 2
fi
legacy="$(cd "$1" && pwd)"
if [[ ! -f "$legacy/Cargo.toml" ]]; then echo 'Expected legacy Cargo.toml' >&2; exit 2; fi
command -v cargo >/dev/null || { echo 'Cargo required for true pre/post comparison' >&2; exit 1; }
mkdir -p performance/results/differential/{legacy,current}
# Copy the same exact output emitter into the old crate without overwriting
# an existing example. The user-supplied old workspace is left unchanged.
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
cp -a "$legacy" "$work/mata-audio"
mkdir -p "$work/mata-audio/examples"
cp examples/emit_pcm.rs "$work/mata-audio/examples/emit_pcm.rs"
cat >> "$work/mata-audio/Cargo.toml" <<'TOML'
[[example]]
name = "emit_pcm"
required-features = ["auto-detect"]
TOML
# Optional sibling path dependencies must be provided when Cargo resolves
# manifests, even when the corresponding backend feature is disabled.
if [[ -d "$(dirname "$legacy")/mata-playback" ]]; then
  cp -a "$(dirname "$legacy")/mata-playback" "$work/mata-playback"
fi
while IFS=$'\t' read -r id file codec channels rate reference inputsha refsha tolerance; do
  [[ "$id" == id || -z "$id" ]] && continue
  cargo run --quiet --release --manifest-path "$work/mata-audio/Cargo.toml" --no-default-features --features auto-detect --example emit_pcm -- "$(pwd)/performance/corpus/$file" "$(pwd)/performance/results/differential/legacy/$id.f32"
  cargo run --quiet --release --no-default-features --features auto-detect --example emit_pcm -- "$(pwd)/performance/corpus/$file" "$(pwd)/performance/results/differential/current/$id.f32"
done < performance/corpus/manifest.tsv
python3 performance/scripts/compare_legacy_pcm.py performance/results/differential/legacy performance/results/differential/current
