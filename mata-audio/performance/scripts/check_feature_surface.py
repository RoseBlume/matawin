#!/usr/bin/env python3
"""Static feature-surface gate used before compiler-level matrix validation."""
from pathlib import Path
import re,tomllib
root=Path(__file__).resolve().parents[2]
cargo_text=(root/'Cargo.toml').read_text()
cargo=tomllib.loads(cargo_text)
features=cargo['features']
required={'all-codecs','all-demuxers','all-metadata','auto-detect','simd'}
assert required <= set(features)
for feature,module in [
 ('metadata-ape','ape'),('metadata-id3v1','id3v1'),('metadata-id3v2','id3v2'),('metadata-iso-mp4','iso_mp4'),('metadata-riff','riff'),
 ('demux-wav','wav'),('demux-aiff','aiff'),('demux-flac','flac'),('demux-ogg','ogg'),('demux-matroska','matroska')]:
    text=(root/'src'/('metadata' if feature.startswith('metadata') else 'demuxers')/'mod.rs').read_text()
    assert feature in text and module in text
expected_auto={
 'codec-flac','codec-mp1','codec-mp2','codec-mp3','codec-pcm','codec-vorbis',
 'demux-aiff','demux-wav',
}
assert set(features['auto-detect'])==expected_auto,(features['auto-detect'],expected_auto)
assert set(features['codec-flac'])=={'demux-flac'}
assert set(features['codec-vorbis'])=={'demux-ogg'}
print('feature surface: OK')
