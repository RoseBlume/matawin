#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
OUT="$ROOT/performance/results/metadata/current.csv"
mkdir -p "$(dirname "$OUT")"
echo 'feature,family,rounds,total_us,fields,allocations,allocated_bytes' > "$OUT"
for feature in metadata-ape metadata-id3v1 metadata-id3v2 metadata-iso-mp4 metadata-riff metadata-vorbis-flac metadata-vorbis-ogg all-metadata; do
  TMP=$(mktemp)
  cargo run --release --quiet --no-default-features --features "benchmarking,$feature" --bin mata-audio-metadata-perf -- "$ROOT/performance/corpus" 100 > "$TMP"
  tail -n +2 "$TMP" | sed "s/^/$feature,/" >> "$OUT"
  rm -f "$TMP"
  cargo run --release --quiet --no-default-features --features "benchmarking,$feature" --bin mata-audio-metadata-fuzz
done
if [ -f "$ROOT/performance/results/metadata/baseline.csv" ]; then
  python3 "$ROOT/performance/scripts/check_metadata_regressions.py" \
    "$ROOT/performance/results/metadata/baseline.csv" "$OUT" "$ROOT/performance/metadata/thresholds.tsv"
fi
