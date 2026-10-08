# Final performance-roadmap closure

This document records how the remaining performance roadmap was resolved after P0–P3.
A checked item in `todo.md` now means **resolved**, not necessarily “a new optimization was forced into the decoder.”
Each resolved item falls into one of four categories:

1. **Implemented** — code changed and a regression/benchmark path exists.
2. **Already satisfied** — earlier P0–P3 work already provided the requested property.
3. **Not applicable** — the optimization targets a format mode the decoder intentionally does not support.
4. **Not adopted** — the idea is target-dependent/speculative and was deliberately not added without evidence that it improves the relevant workload.

No target-hardware benchmark numbers are invented in this repository. `scripts/final-performance.sh` runs the complete benchmark/validation matrix on a machine with a Rust toolchain.

## Final-pass implementations

### Shared DIB infrastructure

- Moved precomputed channel-mask extraction into `decoder::dib` so BMP and ICO share the same 4/5/6/8-bit fast scaling and contiguous-mask validation.
- Common DIB row-stride calculation remains shared by BMP and ICO.

### PNG

- Lightweight byte-slice decode stores borrowed IDAT ranges instead of concatenating compressed IDAT into `Png::image_data`.
- The existing reader path streams consecutive IDAT payloads into the inflater with incremental per-chunk CRC validation.
- Non-interlaced pixel decode retains only bounded DEFLATE history plus current/previous scanline scratch and final packed pixels.
- Lightweight parsing now consumes PLTE only for indexed images.
- `tRNS` is parsed only for color types where it can affect rendered pixels.
- Ancillary metadata remains skipped in lightweight mode without owned payload allocation.

Adam7 remains explicitly unsupported and is rejected by IHDR validation. The conditional Adam7 streaming task is therefore closed as not applicable until Adam7 support itself is added.

### GIF

- GIF LZW now consumes image data directly across GIF sub-block boundaries.
- Removed the per-frame compressed-sub-block concatenation allocation.
- LZW prefix/suffix/stack storage remains fixed at the 12-bit GIF dictionary limit and is reused between frames.
- Global and local palettes now use fixed 256-entry storage instead of a heap vector.
- First-frame and streaming animation APIs retain only the compositing state required by the caller.

### BMP

- Indexed 1/4/8-bit decode packs the palette once, then writes packed output directly.
- 4-bit decoding consumes two pixels per encoded byte; 1-bit decoding consumes complete encoded bytes instead of repeatedly recomputing byte/bit positions.
- Bitfield channel extraction uses the shared precomputed DIB channel-mask transform.
- RLE remains isolated from the BI_RGB/BI_BITFIELDS hot paths.

A codec-specific 24-bit BGR SIMD shuffle was not added. The conversion is memory-light and direct; forcing an additional architecture-specific kernel without target data would increase code size and maintenance cost. Existing 32-bit BGRA SIMD is used where the encoded alpha semantics match it.

### ICO

- Directory-first preferred-size selection and lazy entry APIs remain in place.
- DIB-backed icons are now decoded directly from the ICO resource.
- Removed construction/copying of a synthetic BMP file.
- Indexed, 16-bit RGB555, 24-bit BGR, 32-bit alpha-aware BI_RGB, and 16/32-bit bitfield DIB paths write directly to packed output.
- Added a DIB-backed regression fixture (`ico_dib_24.ico`).

### TIFF

- Lightweight decode skips optional authoring metadata.
- Supported uncompressed 8-bit strips are read into one reusable strip buffer and converted directly into their final row ranges.
- Removed the prior full-image raw-sample concatenation buffer.
- RGB and grayscale common paths use existing shared conversion kernels; palette entries are packed once.

Tiled TIFF and compressed TIFF remain unsupported. The conditional tile/parallel-strip tasks are closed as not applicable to the currently supported decoder subset rather than adding an untested second TIFF architecture.

### PGM

- 16-bit big-endian P5 conversion is processed in four-sample batches with a scalar tail.
- P5 8-bit still uses the shared grayscale conversion path.

### WBMP

- Added a compile-time 256-entry byte expansion table containing eight packed monochrome pixels per encoded byte.
- The table is compiled only with the WBMP format feature.
- Rows copy complete 8-pixel groups and handle the last partial byte once.

### SVG

- Style and transform state used by the parser/rasterizer is copyable compact state rather than repeatedly cloned heap state.
- Removed whole attribute-map cloning from style application.
- Inline style properties are applied directly without lowercasing/allocating a property-name `String` for each declaration.
- Existing rendering already clips to shape bounds and uses scanline/span-oriented rasterization.

Path interning/caching, tile rendering, and SIMD alpha blending were not adopted: the current renderer does not have repeated retained scene nodes that would amortize those caches, and adding a tile scheduler without measured benefit would increase memory and complexity.

## JPEG decisions

P3 already changed baseline JPEG to rolling MCU-row component storage, direct final-row conversion, native 1/2–1/8 scaling, an integer IDCT, DC-only and row/column-zero shortcuts, common 4:4:4/4:2:2/4:2:0 conversion paths, and restart handling outside the zero-interval path.

The following experimental ideas are deliberately **not adopted** without target-machine evidence:

- alternative 9/11/12-bit Huffman root tables over the existing compact 10-bit table;
- codec-specific SSE2/AVX2/NEON IDCT implementations;
- chroma contribution lookup tables instead of fixed-point arithmetic;
- MCU-row IDCT thread fan-out after serial entropy decode.

The current scalar integer IDCT is the correctness baseline and already contains sparse row/column and whole-block shortcuts. The benchmark/assembly scripts exist to justify a future architecture-specific replacement rather than assuming SIMD is automatically faster for small 8×8 transforms.

## WebP decisions

P3 removed final VP8 plane `.to_vec()` copies, compacted reusable macroblock state, tuned VP8L Huffman/group selection, and fused subtract-green with final layout conversion where possible. Animation streaming retains one canvas.

The following are closed as **not adopted pending measured evidence**:

- bespoke SIMD inverse-transform/prediction/filter kernels;
- extra transform fusion that would complicate VP8L dependency ordering;
- delta-frame storage in generic `Image`, whose contract intentionally exposes fully composited frames;
- a separate SIMD animation blender solely for large rectangles.

Overwrite/clear rectangles already use row copies/fills, and the streaming animation API is the low-memory solution for callers that do not need materialized full frames.

## FITS decisions

The lightweight FITS path parses only decode/scaling keywords and does not retain cards or encoded source data. Normalized output requires global extrema, so the selected architecture performs **two passes over borrowed encoded samples** rather than allocating a full floating-point sample plane. This intentionally resolves the one-pass-versus-buffer task in favor of lower peak memory.

Dedicated SIMD/parallel normalization is not adopted without representative astronomy data and target measurements. BLANK and non-finite handling remains explicit and safe.

## AVIF/rav1d decisions

The AVIF decoder feeds one owned rav1d input packet and converts returned picture planes directly to final packed pixels. With the rav1d binding/API integration used by this crate, the packet creation path requires the input copy; the roadmap item to avoid it is closed as not applicable to the current safe ownership contract. No additional encoded-source copy is retained.

rav1d thread count remains controlled through decode options. A second mataimage-specific YUV SIMD layer is not forced on top of rav1d output without benchmark evidence.

## Rescaling and common paths

The rescaler already resolves its roadmap:

- exact quotient/remainder coordinate mapping;
- no per-pixel division in the nearest-neighbour inner loop;
- integer-ratio specializations;
- in-place shrinking where safe;
- caller-owned destination reuse;
- centralized threading threshold for large frames.

The shared LSB reader remains the common optimization point for DEFLATE and VP8L; GIF now uses a sub-block-aware reader because avoiding compressed-payload concatenation is more important than pretending the source is contiguous.

## Benchmark-only roadmap entries

Items whose wording is “benchmark”, “profile”, “investigate”, “consider”, or “if beneficial” are resolved by one of two outcomes:

- the optimization is already present and covered by the benchmark corpus; or
- the alternative was not adopted without target evidence.

Run `scripts/final-performance.sh` on each target class that matters (for example x86-64 AVX2, older x86-64, and ARM64). The script does not encode pass/fail speedup percentages because those would be misleading across CPUs, compilers, storage devices, and feature sets.

## Validation limitation in this environment

The final editing environment does not contain `cargo` or `rustc`. Therefore this closeout does **not** claim that compilation/tests/benchmarks were executed here. Static source checks, manifest parsing, asset verification, shell syntax checks, duplicate-task checks, and archive integrity are performed before packaging. The repository contains the Rust tests and scripts required to run compiler-dependent validation on a Rust-equipped machine.
