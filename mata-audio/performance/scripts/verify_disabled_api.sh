#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
TMP=${TMPDIR:-/tmp}/mata-audio-disabled-api-$$
trap 'rm -rf "$TMP"' EXIT INT TERM
mkdir -p "$TMP/src"
cat > "$TMP/Cargo.toml" <<TOML
[package]
name="mata-audio-disabled-api-check"
version="0.0.0"
edition="2024"
[dependencies]
mata-audio={path="$ROOT",default-features=false}
TOML
cat > "$TMP/src/main.rs" <<'RS'
fn main(){let _=core::mem::size_of::<mata_audio::codecs::Flac<std::io::Cursor<Vec<u8>>>>();}
RS
if cargo check --manifest-path "$TMP/Cargo.toml" >/dev/null 2>&1; then echo "disabled codec unexpectedly exposed" >&2; exit 1; fi
cat > "$TMP/src/main.rs" <<'RS'
fn main(){let _=mata_audio::metadata::MetadataKind::Id3V1;let _=mata_audio::metadata::id3v1::read(&mut std::io::Cursor::new(Vec::<u8>::new()));}
RS
if cargo check --manifest-path "$TMP/Cargo.toml" >/dev/null 2>&1; then echo "disabled metadata module unexpectedly exposed" >&2; exit 1; fi
cat > "$TMP/src/main.rs" <<'RS'
fn main(){let _=mata_audio::demuxers::WavDemuxer::new(std::io::Cursor::new(Vec::<u8>::new()));}
RS
if cargo check --manifest-path "$TMP/Cargo.toml" >/dev/null 2>&1; then echo "disabled demuxer unexpectedly exposed" >&2; exit 1; fi
echo "disabled codec/demuxer/metadata API compile-fail checks: OK"
