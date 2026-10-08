#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$ROOT"
mkdir -p performance/results/profiles performance/results/profile-manifests
header=$(head -n 1 performance/corpus/manifest.tsv)

tail -n +2 performance/corpus/manifest.tsv | while IFS='\t' read -r id file codec channels sample_rate reference input_sha reference_sha tolerance; do
  manifest="performance/results/profile-manifests/${id}.tsv"
  printf '%s\n' "$header" > "$manifest"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$id" "$file" "$codec" "$channels" "$sample_rate" "$reference" "$input_sha" "$reference_sha" "$tolerance" >> "$manifest"
  # The benchmark expects corpus-relative paths, so place the temporary file in
  # the corpus directory for the duration of the profile.
  corpus_manifest="performance/corpus/.profile-${id}.tsv"
  cp "$manifest" "$corpus_manifest"
  perf record -g -o "performance/results/profiles/${id}.data" -- \
    cargo run --release --no-default-features \
      --features benchmarking,auto-detect,simd,perf-instrumentation \
      --bin mata-audio-perf -- \
      "$corpus_manifest" "performance/results/profile-${id}.csv"
  rm -f "$corpus_manifest"
done
