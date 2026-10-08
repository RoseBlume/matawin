# Supported feature combinations

`--no-default-features` builds only the common error/core shell. Codec, demuxer and metadata modules remain absent unless their exact feature is enabled.

- `codec-pcm,demux-wav` — WAV PCM only.
- `codec-pcm,demux-aiff` — AIFF/AIFC PCM only.
- `codec-flac` — native FLAC decoder and required FLAC demuxer.
- `codec-vorbis` — Vorbis decoder and required Ogg demuxer.
- `codec-mp1`, `codec-mp2`, `codec-mp3` — independent MPEG layers.
- `all-metadata` — all metadata parsers without playback.
- `auto-detect` — all supported decoders plus the demuxers needed by content detection.
- `simd` — runtime-dispatched SIMD; safe to omit for scalar-only builds.
- `mata-playback` or `cpal` — playback backend plus `auto-detect`.

The default set enables playback, SIMD, every codec/demuxer/metadata parser and auto-detection.
