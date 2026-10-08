#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
OUT="$ROOT/performance/results/features/feature-costs.csv"
mkdir -p "$(dirname "$OUT")"
echo 'feature,check_ms,rlib_bytes' > "$OUT"
FEATURES='no-default codec-flac codec-mp1 codec-mp2 codec-mp3 codec-pcm codec-vorbis demux-aiff demux-flac demux-matroska demux-ogg demux-wav metadata-ape metadata-id3v1 metadata-id3v2 metadata-iso-mp4 metadata-riff metadata-vorbis-flac metadata-vorbis-ogg all-codecs all-demuxers all-metadata auto-detect auto-detect-simd mata-playback cpal default'
for feature in $FEATURES; do
  cargo clean -p mata-audio >/dev/null 2>&1 || true
  start=$(python3 -c 'import time;print(time.monotonic_ns())')
  case "$feature" in
    no-default) cargo check --no-default-features >/dev/null;;
    default) cargo check >/dev/null;;
    auto-detect-simd) cargo check --no-default-features --features auto-detect,simd >/dev/null;;
    *) cargo check --no-default-features --features "$feature" >/dev/null;;
  esac
  stop=$(python3 -c 'import time;print(time.monotonic_ns())')
  ms=$(( (stop-start)/1000000 ))
  case "$feature" in
    no-default) cargo build --release --no-default-features >/dev/null;;
    default) cargo build --release >/dev/null;;
    auto-detect-simd) cargo build --release --no-default-features --features auto-detect,simd >/dev/null;;
    *) cargo build --release --no-default-features --features "$feature" >/dev/null;;
  esac
  rlib=$(find "$ROOT/target/release/deps" -name 'libmata_audio-*.rlib' -printf '%s\n' 2>/dev/null | sort -nr | head -1)
  echo "$feature,$ms,${rlib:-0}" >> "$OUT"
done
