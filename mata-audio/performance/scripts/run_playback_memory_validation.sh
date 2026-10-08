#!/usr/bin/env bash
set -euo pipefail
FILE="${MATA_AUDIO_PLAYBACK_FILE:-performance/corpus/wav_s16_stereo_44100.wav}"
LONG_SECONDS="${MATA_AUDIO_PLAYBACK_MEMORY_SECONDS:-3600}"
SCALE_SECONDS="${MATA_AUDIO_PLAYBACK_SCALE_SECONDS:-120}"
mkdir -p performance/results
cargo run --profile perf --features benchmarking --bin mata-audio-playback-memory-perf -- "$FILE" "$LONG_SECONDS" 1 performance/results/playback-memory-1voice.csv
cargo run --profile perf --features benchmarking --bin mata-audio-playback-memory-perf -- "$FILE" "$SCALE_SECONDS" 128 performance/results/playback-memory-128voices.csv
python3 performance/scripts/check_playback_memory.py performance/results/playback-memory-1voice.csv performance/results/playback-memory-128voices.csv
