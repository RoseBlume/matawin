# P2 performance architecture

P2 turns the P0/P1 lightweight representation into a set of explicit low-memory APIs. The normal `Image` path remains simple; advanced callers opt into metadata indexing, reusable scratch, lazy page/frame readers, caller-owned output, or streaming animation only when those capabilities are useful.

## Metadata policy

`DecodeOptions::default()` uses `MetadataMode::None`. The lightweight pixel decoders parse only structures required to render pixels. `ImageMetadata::read` is the explicit metadata path and returns borrowed encoded payloads rather than eagerly decoding text, EXIF, XMP, or ICC data. `MetadataMode::Essential` selects interpretation/container metadata; `All` selects every metadata block currently recognized. `Image::from_bytes_and_metadata` exists for callers that intentionally want both results.

The metadata index intentionally does not decompress PNG text/ICC payloads. Rich format objects remain the owned API when metadata must outlive the source byte slice.

## Error allocation policy

`ImageError` has a machine-readable `ImageErrorKind` plus `Cow<'static, str>`. Static diagnostics borrow their literals. Dynamic values use `format!` only at the point where the value improves the diagnostic. Error constructors are cold and non-inlined so malformed-input handling does not unnecessarily pollute success-path instruction layout.

## Animation policy

`AnimationMode::FirstFrame` is the default. `Image::from_bytes_animated` / `AnimationMode::AllFrames` are explicit materialization paths. `AnimationDecoder` is the low-memory playback path for GIF and animated WebP and exposes `next_frame(&mut self, &mut [u32])`.

GIF keeps one compositing canvas, an optional reusable disposal-method-3 restore canvas, and one reusable 4096-entry LZW dictionary/stack. WebP keeps one compositing canvas and decodes/drops each ANMF frame before advancing. The caller owns the display buffer. Disposal is applied only after the displayed composited canvas is copied to that output.

`benches/assets/gif_120_frames.gif` and `webp_120_frames.webp`, plus the P2 benchmark section and `scripts/p2-animation-memory.sh`, provide the long-animation memory comparison. No machine-specific benchmark result is hard-coded into the repository.

## Reusable decode scratch

`DecodeScratch` deliberately hides codec internals. PNG currently uses it across independent decodes for the 32 KiB DEFLATE history, streaming inflate chunk, and previous PNG row. Capacity retention is bounded by `max_retained_bytes`; `clear()` preserves useful bounded capacity and `shrink()` releases variable buffers.

Other P2 scratch candidates were audited rather than forced into one oversized public scratch object:

- GIF LZW prefix/suffix/stack arrays are fixed 4096-entry state already reused for every frame of a streaming decoder.
- JPEG coefficient/MCU working arrays are decoder-local fixed/reused working storage; retaining them across unrelated files would not remove a meaningful heap allocation in the current implementation.
- Huffman decode tables are retained for the lifetime of the compressed stream that uses them. Cross-file retention would require tying format-specific table shapes to the public scratch ABI.
- WebP transform/macroblock storage is frame/stream dependent and is dropped as soon as it is no longer useful. Streaming animation removes the much larger all-frame retention cost first.

Those decisions satisfy P2 without freezing speculative format-specific fields into the public API. New cross-decode fields can be added privately when the benchmark suite demonstrates a real allocation win.

## Header-only and lazy APIs

`ImageInfo::from_bytes`/`read` never decodes compressed pixels. It reports dimensions and, where cheap to determine, frame count, alpha capability, animation state, and all ICO entry sizes.

Lazy format APIs:

- `IcoReader`: indexes the icon directory and decodes only the selected entry.
- `TiffPages`: walks the IFD chain and decodes only the selected IFD/page.
- `FitsReader`: indexes HDUs and decodes only the selected image HDU or plane.
- `AnimationDecoder`: advances GIF/WebP sequentially without retaining prior display frames.

The generic `Image::from_bytes` therefore never needs to decode every page/entry/frame when page/frame 0 is enough.

## Low-copy output APIs

Decoded owners expose borrowed accessors and consuming accessors rather than cloning getters. `Image::frame_iter` borrows existing full-canvas frames; `into_frame_iter` moves them. `Image::decode_into` validates caller storage once from `ImageInfo` and writes the primary decoded canvas into persistent caller storage. `decode_into_with_scratch` combines caller-owned output with reusable codec scratch.

`decode_into` still permits a codec-internal final `Image` allocation when a decoder does not yet have a native write-into implementation. It removes the need for the caller to create a second persistent result allocation and provides a stable path for individual formats to become direct-write later.

## Data layout decisions

P2 keeps large pixel and plane buffers contiguous and row-major. DEFLATE Huffman nodes were already compacted in P1. WebP VP8 lossy macroblock segment IDs are stored as `u8` rather than `usize`, reducing per-macroblock metadata footprint.

The code does not impose stronger-than-natural heap alignment. Runtime SIMD backends use load strategies appropriate for their targets and retain scalar fallbacks. Pass fusion is used only where it removes meaningful memory traffic without obscuring codec dependencies; PNG continues to inflate -> unfilter -> convert one row at a time instead of materializing a decompressed frame.

## Features and dependency footprint

Every decoder module is behind its Cargo feature. Architecture-specific SIMD modules require `simd`. `rav1d` is optional and enabled only by `avif`. TIFF/SVG/AVIF are grouped under `experimental-formats` and are not part of `all-formats`.

A minimal PNG-only consumer can use:

```toml
mataimage = { version = "0.1.0", default-features = false, features = ["png"] }
```

PNG plus runtime SIMD:

```toml
mataimage = { version = "0.1.0", default-features = false, features = ["png", "simd"] }
```

`all-formats` remains the default for API ergonomics; footprint-sensitive applications should select only the formats they accept. `scripts/feature-matrix.sh` verifies independent feature combinations when a Rust toolchain is available, and `scripts/size-matrix.sh` records compile time and output size for representative feature sets.

## Code-generation policy

The normal release profile uses `opt-level = 3` and thin LTO. `release-fat` uses fat LTO and one codegen unit for distribution experiments; `release-size` is an explicit size-oriented profile. The library never forces `panic = "abort"` because unwinding policy belongs to the final application.

Portable releases should keep runtime SIMD dispatch. For local deployment-specific builds, callers may benchmark:

```sh
RUSTFLAGS="-C target-cpu=native" cargo bench --bench decode
```

`target-cpu=native` must not be used for a portable binary. `scripts/codegen-matrix.sh` sweeps LTO and codegen units and includes an application-only panic-abort comparison. `scripts/asm-bench.sh` emits optimized assembly for the hot-loop audit. Speed results should always be considered together with `scripts/size-matrix.sh`, because aggressive inlining/LTO can trade instruction-cache footprint for throughput.

## P2 validation

`tests/p2_performance.rs` covers metadata separation, static error messages, header-only info, PNG scratch reuse, default-first-frame behavior, streamed-vs-materialized GIF/WebP compositing, and lazy ICO/FITS/TIFF readers. The benchmark suite has dedicated P2 header, metadata, scratch, and long-animation memory cases.
