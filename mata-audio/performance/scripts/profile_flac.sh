#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$ROOT"
mkdir -p performance/results/flac-profiles
header=$(head -n 1 performance/corpus/manifest.tsv)
awk -F '\t' 'NR > 1 && $3 == "flac"' performance/corpus/manifest.tsv | while IFS='\t' read -r id file codec channels sample_rate reference input_sha reference_sha tolerance; do
  manifest="performance/corpus/.profile-flac-${id}.tsv"
  printf '%s\n' "$header" > "$manifest"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$id" "$file" "$codec" "$channels" "$sample_rate" "$reference" "$input_sha" "$reference_sha" "$tolerance" >> "$manifest"
  perf record -g -o "performance/results/flac-profiles/${id}.data" -- \
    cargo run --release --no-default-features \
      --features benchmarking,auto-detect,simd,perf-instrumentation \
      --bin mata-audio-perf -- "$manifest" "performance/results/flac-${id}.csv"
  rm -f "$manifest"
done
