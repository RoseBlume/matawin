#!/bin/sh
set -eu
cargo check --no-default-features
for feature in \
  codec-flac codec-mp1 codec-mp2 codec-mp3 codec-pcm codec-vorbis \
  demux-aiff demux-flac demux-matroska demux-ogg demux-wav \
  metadata-ape metadata-id3v1 metadata-id3v2 metadata-iso-mp4 metadata-riff \
  metadata-vorbis-flac metadata-vorbis-ogg
 do cargo check --no-default-features --features "$feature"; done
cargo check --no-default-features --features all-codecs
cargo check --no-default-features --features all-demuxers
cargo check --no-default-features --features all-metadata
for feature in codec-flac codec-mp1 codec-mp2 codec-mp3 codec-pcm codec-vorbis; do
  cargo check --no-default-features --features "$feature"
  cargo check --no-default-features --features "$feature,simd"
done
cargo check --no-default-features --features auto-detect
cargo check --no-default-features --features auto-detect,simd
cargo check --no-default-features --features mata-playback
cargo check --no-default-features --features cpal
cargo check
