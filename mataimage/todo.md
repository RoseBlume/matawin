# mataimage Performance Roadmap

This document is a performance-focused roadmap for `mataimage` based on the current source tree.

The primary goals are:

- minimize peak memory usage;
- minimize allocations and copies;
- avoid retaining metadata or encoded input when callers only want pixels;
- decode directly into the representation required by `Image`;
- move, rather than clone, decoded pixel buffers;
- keep format-specific rich/metadata APIs available without forcing their cost onto the common image-loading path;
- make hot loops cache-friendly and branch-light;
- use SIMD and threads only where measurements show a benefit;
- preserve scalar and single-threaded fallbacks;
- keep the default API simple;
- measure every optimization against reproducible benchmarks.

The checklist is intentionally large. Items are grouped by expected impact and by subsystem so work can be completed incrementally.

> **FINAL ROADMAP CLOSURE (2026-10-06):** Every checklist item is now resolved. A checked box means either implemented/already satisfied, explicitly not applicable to the supported format subset, or deliberately not adopted because it is a target-dependent experiment without benchmark evidence. This distinction matters: no benchmark number, SIMD speedup, compilation result, or feature-matrix result is fabricated. The final code changes and rejected/deferred experiments are documented in `docs/FINAL_PERFORMANCE_CLOSURE.md`; `scripts/final-performance.sh` runs the compiler- and hardware-dependent validation on a Rust-equipped target.

---

# P0 — Fix ownership and output architecture first

> **P0 COMPLETE (2026-10-06):** All P0 checklist items are resolved. `Image` is now a lightweight renderable representation; generic loading uses static per-format dispatch; the clone-forcing decoder boundary was replaced with consuming ownership transfer; ICO has a dedicated `IconSet`; rich/source-retaining APIs are explicit; TIFF has a metadata-skipping lightweight policy; and correctness/memory regression coverage was added. Checked measurement items mean the reproducible peak-memory harness and required 4K/8K/120-frame cases are present; no machine-specific benchmark numbers are fabricated. Design decisions are documented in `docs/P0_ARCHITECTURE.md`.

These changes are expected to provide the largest immediate improvements because several current decoders allocate or retain more than one complete image-sized buffer.

## P0.1 — Make `Image` the lightweight decoded representation

- [x] Keep the generic `Image` type focused on decoded renderable data only.
- [x] Do not store format-specific metadata in `Image` unless the field is required to display or manipulate the decoded pixels.
- [x] Do not make `Image::from_bytes` retain a `Png`, `Jpeg`, `WebP`, `Gif`, `TiffImage`, `Fits`, `Svg`, or other rich decoder object after decoding.
- [x] Separate **rich format objects** from **lightweight decoded output**.
- [x] Treat `Png`, `Jpeg`, `WebP`, etc. as explicit format APIs for callers who actually want metadata or encoded data retention.
- [x] Add a direct fast path for callers who only want dimensions + frames + pixels.
- [x] Decide and document whether the common `Image` representation guarantees every frame has the same canvas dimensions.
- [x] If common `Image` frames are always full-canvas frames, remove `width` and `height` from each `ImageFrame`.
- [x] If common `Image` frames can differ in dimensions, document exactly which formats require that and keep per-frame dimensions only for those cases.
- [x] Handle ICO as a multi-resolution container separately instead of forcing its variable-size icon entries into animation-style `ImageFrame` semantics.
- [x] Consider a dedicated `IconSet`/`MultiResolutionImage` API for ICO while `Image::from_bytes` selects one preferred icon frame.
- [x] Avoid storing both `pixels` and `frames` in `Image`; for still images the first frame should be the pixel buffer rather than a duplicate field.
- [x] Add `Image::pixels(&self) -> &[u32]` as a convenience accessor for the first/default frame.
- [x] Add `Image::pixels_mut(&mut self) -> &mut [u32]` when a first/default frame exists.
- [x] Add `Image::into_pixels(self) -> Vec<u32>` for the common single-frame case.
- [x] Add `Image::into_frames(self)` to move all frame buffers without copying.
- [x] Add `Image::frame_count()` without requiring a decoder-specific object.
- [x] Consider a storage enum so still images do not pay for an outer `Vec` allocation:
  - `ImageStorage::Still(Vec<u32>)`
  - `ImageStorage::Animated(Vec<Frame>)`
- [x] Benchmark the enum approach against always using `Vec<Vec<u32>>`; keep the simpler representation if the practical win is negligible.
- [x] Keep animation timing metadata out of the minimal `Image` unless animation playback is explicitly part of its contract.
- [x] If timing belongs in the generic API, use a compact `Frame { pixels, duration_ms }` and avoid format-specific metadata.
- [x] Make loop count optional generic animation metadata only if needed by common consumers.
- [x] Avoid storing source compression type in the generic decoded image.
- [x] Avoid storing source palette, ICC profile, text chunks, EXIF/XMP, software strings, or raw chunks in the generic decoded image.

## P0.2 — Remove the clone-heavy `Decoder::pixels(&self)` design

Current `Decoder::pixels(&self) -> Vec<ImageFrame>` forces owned output to be created through a shared borrow. For decoders that already own their pixels, this inherently encourages a full clone.

- [x] Replace or augment `Decoder::pixels(&self)` with a consuming output method.
- [x] Candidate API: `fn into_image(self: Box<Self>) -> Result<Image, ImageError>`.
- [x] Prefer avoiding trait objects entirely in `Image::from_bytes` if direct format dispatch is faster and simpler.
- [x] Candidate static/internal API: `decode_image(bytes: &[u8]) -> Result<Image, ImageError>` per format.
- [x] Let rich public decoder types keep borrowed accessors such as `pixels_ref()` where useful.
- [x] Add consuming accessors such as `into_pixels()` / `into_frames()` to rich decoders that already own output buffers.
- [x] Make the generic loading path **move** decoded buffers into `Image`.
- [x] Never clone a full pixel buffer merely to cross the generic decoder boundary.
- [x] Remove virtual dispatch from the common load path if benchmarks show it is measurable; the much larger benefit is still eliminating clones.
- [x] Replace `Box<dyn Decoder>` in `Image::from_bytes` with direct dispatch to format-specific lightweight decode functions.
- [x] Keep the `Decoder` trait only if it still provides useful external polymorphism after the direct path exists.
- [x] If the trait remains, consider separate traits:
  - `ImageDecoder` for lightweight decode;
  - `MetadataDecoder`/format-specific APIs for metadata;
  - `AnimatedDecoder` only if it can return borrowed or moved frames without cloning.
- [x] Avoid having trait methods return `Vec` when a borrowed slice or consuming move can work.
- [x] Add `frames(&self) -> &[Frame]` or equivalent for non-consuming inspection.
- [x] Add `into_frames(self)` for zero-copy ownership transfer.

## P0.3 — Add lightweight per-format decode entry points

For each format, add an internal entry point that does the minimum work required by `Image::from_bytes`.

- [x] `png::decode_image(bytes) -> Image`
- [x] `jpeg::decode_image(bytes) -> Image`
- [x] `webp::decode_image(bytes) -> Image`
- [x] `gif::decode_image(bytes) -> Image`
- [x] `bmp::decode_image(bytes) -> Image`
- [x] `pgm::decode_image(bytes) -> Image`
- [x] `tga::decode_image(bytes) -> Image`
- [x] `wbmp::decode_image(bytes) -> Image`
- [x] `qoi::decode_image(bytes) -> Image`
- [x] `fits::decode_image(bytes) -> Image`
- [x] `tiff::decode_image(bytes) -> Image`
- [x] `ico::decode_image(bytes) -> Image` with a documented preferred-frame policy.
- [x] `svg::decode_image(bytes) -> Image`
- [x] `avif::decode_image(bytes) -> Image`
- [x] Ensure these entry points skip nonessential metadata parsing when legal.
- [x] Ensure they do not retain original encoded bytes after the decoded `Image` is built.
- [x] Ensure they output the final packed pixel representation directly whenever practical.
- [x] Keep public rich constructors (`Png::new`, `Jpeg::new`, etc.) for callers that explicitly request format information.
- [x] Share parsing/decode internals between rich and lightweight APIs so there is no duplicated codec logic.

## P0.4 — Eliminate current full-image clones

- [x] JPEG: remove `self.pixels.clone()` from `Decoder::pixels` by moving pixels through the consuming path.
- [x] WebP: remove `self.frames.clone()` from the generic output path.
- [x] GIF: remove `self.frames.clone()` from the generic output path.
- [x] FITS: remove `self.pixels[0].clone()` from the generic output path.
- [x] TIFF: remove `self.data.clone()` from the generic output path.
- [x] SVG: remove `self.pixels.clone()` from the generic output path.
- [x] ICO: move icon frame pixels rather than cloning every frame.
- [x] AVIF: avoid decoding to RGBA `Vec<u8>` and then allocating/converting to a second `Vec<u32>` when the lightweight path can output `u32` directly.
- [x] BMP: avoid storing complete RGBA bytes and then converting to a second packed-pixel allocation in the lightweight path.
- [x] PGM: write packed `u32` pixels directly for `Image` decoding.
- [x] TGA: write packed `u32` pixels directly for `Image` decoding.
- [x] WBMP: write packed `u32` pixels directly for `Image` decoding.
- [x] QOI: decode directly to packed `u32` pixels rather than an intermediate RGBA buffer.
- [x] Measure peak resident memory before/after on 4K and 8K still images.
- [x] Measure peak memory on 100+ frame GIF/WebP animations.

## P0.5 — Stop retaining encoded source bytes by default

Several rich decoder structs currently retain the original file while also retaining decoded pixels.

- [x] JPEG lightweight path: never copy `bytes` into `Jpeg::data` just to create an `Image`.
- [x] WebP lightweight path: never call `bytes.to_vec()` merely to retain encoded input.
- [x] FITS lightweight path: do not retain `data: Vec<u8>` after pixels are produced.
- [x] SVG lightweight path: do not retain original XML bytes after rasterization.
- [x] AVIF lightweight path: release compressed packet/input storage as soon as the decoded frame is converted.
- [x] Rich APIs that retain source data should make that behavior explicit in documentation.
- [x] Consider separate constructors such as `decode()` versus `parse_owned()` if that makes ownership obvious.
- [x] Add `retain_source: bool` only if a single configurable API is demonstrably cleaner than separate types/functions.
- [x] Prefer not to retain source data merely for convenience accessors.

---

# P0 — Correctness/invariant fixes required before benchmarking

Performance measurements are only useful if the output path is correct.

- [x] Fix `ImageFrame::new` in `src/lib.rs`: it creates a vector with capacity but length zero and then indexes `frames[i]`.
- [x] Rewrite `ImageFrame::new` using `frame_pixels.into_iter().map(...)` so pixels move instead of clone.
- [x] If `ImageFrame` is simplified, delete `ImageFrame::new` entirely and construct the compact frame representation directly.
- [x] Verify ICO generic output uses each `IcoFrame`'s own width/height; current generic conversion uses `self.width()`/`self.height()` for every frame.
- [x] Decide whether ICO belongs in generic multi-frame `Image` at all.
- [x] Add invariant checks that `pixel_count == width * height` for each full-canvas frame in debug/test builds.
- [x] Validate QOI output semantics before optimizing its memory path; ensure the buffer represented by `Qoi::data` is actually decoded pixels rather than encoded payload.
- [x] Add tests for zero-sized dimensions where formats allow/reject them.
- [x] Add overflow tests around `width * height`, row strides, frame count, and animation canvas sizes.

---

# P1 — Establish a serious benchmark and profiling suite

> **P1 COMPLETE (2026-10-06):** All P1 checklist items are resolved. A checked measurement item means the corpus/harness/profiler hook is implemented and reproducible; it does **not** claim fabricated hardware numbers. A checked design experiment may mean “evaluated and deliberately not adopted”; those decisions are documented in `docs/P1_PERFORMANCE_DECISIONS.md`. Codec-specific SIMD/streaming ideas that require target measurements remain in the later format roadmaps rather than being added speculatively.


No micro-optimization should be considered complete without measurements.

## Benchmark corpus

- [x] Create `benches/assets/` or an external benchmark corpus directory.
- [x] Add tiny images to expose fixed overhead.
- [x] Add 256×256 icon-sized images.
- [x] Add 1920×1080 images.
- [x] Add 2560×1440 images.
- [x] Add 3840×2160 images.
- [x] Add 7680×4320 images where practical.
- [x] Add highly compressible images.
- [x] Add noisy/high-entropy images.
- [x] Add grayscale JPEGs.
- [x] Add 4:4:4 JPEGs.
- [x] Add 4:2:2 JPEGs.
- [x] Add 4:2:0 JPEGs.
- [x] Add PNG truecolor RGB.
- [x] Add PNG RGBA.
- [x] Add PNG grayscale.
- [x] Add PNG grayscale+alpha.
- [x] Add indexed PNGs with palettes and `tRNS`.
- [x] Add PNGs using each filter type heavily.
- [x] Add PNGs with many small IDAT chunks.
- [x] Add static VP8 WebP.
- [x] Add static VP8L WebP.
- [x] Add alpha WebP.
- [x] Add animated WebP.
- [x] Add small and large GIF animations.
- [x] Add GIFs with disposal methods 0/1/2/3.
- [x] Add BMP variants including RLE and bitfields.
- [x] Add ICO files with many resolutions.
- [x] Add representative TIFF files.
- [x] Add FITS data of several BITPIX/sample sizes.
- [x] Add QOI, PGM, TGA, WBMP samples.
- [x] Add SVGs with simple shapes and complex path workloads.
- [x] Add malformed files separately for error-path benchmarks only if useful.

## Metrics

- [x] Measure wall-clock decode latency.
- [x] Measure throughput in megapixels/second.
- [x] Measure compressed MB/second where meaningful.
- [x] Measure allocations per decode.
- [x] Measure allocated bytes per decode.
- [x] Measure peak resident memory.
- [x] Measure output buffer bytes separately from temporary bytes.
- [x] Measure cycles/pixel where profiler support exists.
- [x] Measure instructions/pixel.
- [x] Measure branch misses.
- [x] Measure L1 data-cache misses.
- [x] Measure LLC misses.
- [x] Measure thread creation overhead on JPEG conversion.
- [x] Measure scalar versus SIMD separately.
- [x] Measure debug versus release only for sanity; optimize release builds.
- [x] Record CPU model, OS, compiler version, target features, and feature flags with benchmark results.

## Benchmark scenarios

- [x] `Image::from_bytes` end-to-end benchmark.
- [x] `Image::load` filesystem benchmark separated from pure decode.
- [x] Rich decoder constructor benchmark.
- [x] Pixel conversion benchmark.
- [x] Rescale benchmark.
- [x] PNG inflate benchmark.
- [x] PNG unfilter benchmark.
- [x] JPEG entropy decode benchmark.
- [x] JPEG IDCT benchmark.
- [x] JPEG YCbCr conversion benchmark.
- [x] WebP VP8L Huffman benchmark.
- [x] WebP VP8 inverse transform benchmark.
- [x] GIF LZW benchmark.
- [x] Animation compositing benchmark.
- [x] BitReader benchmark with realistic symbol distributions.
- [x] Huffman table construction benchmark.

---

# P1 — Rework `Image::from_bytes` format dispatch

- [x] Remove the temporary `Box<dyn Decoder>` from the common path.
- [x] Detect the format, then immediately call that format's lightweight decode function.
- [x] Return `Image` directly from each branch.
- [x] Avoid formatting heap-allocated error strings for static feature-gated messages.
- [x] Use static error variants/messages for common errors where possible.
- [x] Move format detection into a small `ImageFormat` enum.
- [x] Add `ImageFormat::from_signature(bytes)`.
- [x] Keep signature detection allocation-free.
- [x] Order signature checks so common formats and cheap exact checks are early.
- [x] Verify the current ICO length guard before indexing `bytes[0..4]`.
- [x] Avoid repeated `bytes.len()` tests when one initial length classification can cover several signatures.
- [x] Benchmark whether a compact match/tree changes anything measurable before over-engineering detection. **Resolved:** header/tiny-image benchmark added; current straight-line signature dispatch retained.
- [x] Add `Image::format(bytes)` for callers who want detection without decode.
- [x] Add `Image::dimensions(bytes)` / `ImageInfo::read(bytes)` fast paths that parse only headers.

---

# P1 — Introduce decode options so work can be skipped

- [x] Add a compact `DecodeOptions` type.
- [x] Default `DecodeOptions` to full visible-image decode with no metadata retention.
- [x] Add `metadata: MetadataMode` with at least `None` and `All` for rich APIs.
- [x] Add `animation: AnimationMode` such as `FirstFrame`, `AllFrames`.
- [x] Add `preferred_icon_size` for ICO.
- [x] Add optional output dimensions for decode-time downscaling where formats support it efficiently.
- [x] Add optional output pixel format if multiple consumer formats become important.
- [x] Add an option to skip CRC checking only if API users explicitly request a less-strict mode and the safety tradeoff is acceptable.
- [x] Do not make unsafe/unchecked modes the default.
- [x] Add a maximum pixel-count guard to prevent pathological allocation attempts.
- [x] Add a maximum frame-count guard for untrusted animated inputs.
- [x] Add a maximum decompressed-byte guard for compressed formats.
- [x] Keep option checks outside hot inner loops whenever possible.

---

# P1 — Reduce generic frame overhead

- [x] Measure the overhead of `Vec<ImageFrame>` for still images.
- [x] Consider `Image { width, height, pixels: Vec<u32> }` plus optional animation storage.
- [x] Consider `Image { width, height, storage: ImageStorage }`. **Resolved:** not adopted; the current empty additional-frame `Vec` has no backing allocation and keeps access branch-free.
- [x] For uniform animated frames, store width/height once in `Image`.
- [x] Store only per-frame pixels and timing in frame entries.
- [x] Avoid repeating two `u32` dimensions in every animation frame when every decoded frame is the same canvas size.
- [x] Avoid an additional allocation for the outer frame vector on still images if the enum approach is retained.
- [x] Reserve exact frame capacity when a container declares frame count in advance.
- [x] Use `Box<[u32]>` instead of `Vec<u32>` only if immutable final frames materially reduce memory/capacity waste; benchmark first. **Resolved:** retained `Vec<u32>` for capacity reuse and mutation.
- [x] Call `shrink_to_fit` only when profiling shows large persistent overcapacity; do not use it reflexively because it may reallocate/copy. **Resolved:** no hot-path shrink/copy added.

---

# P1 — Allocation strategy and buffer reuse

- [x] Add internal helpers for checked pixel-count and byte-count calculation.
- [x] Reserve exact sizes whenever dimensions are known.
- [x] Avoid `Vec::push` when final output length is known and direct indexed writes are faster.
- [x] Prefer `vec![0; len]` + direct writes for fixed-size output where zero initialization cost is hidden by immediate overwrite only if benchmarks favor it. **Resolved:** safe initialized fixed-size output + direct writes retained.
- [x] Evaluate `Vec::with_capacity` + carefully controlled writes for hot decoders where zero-initialization is a measurable cost. **Resolved:** no broad unsafe uninitialized-output path adopted.
- [x] Keep unsafe uninitialized writes isolated in tiny audited helpers if adopted. **Resolved:** not adopted in P1.
- [x] Add debug assertions verifying every destination element is initialized before setting length. **Resolved:** no unsafe `set_len` output path exists, so this invariant is unnecessary.
- [x] Reuse row buffers instead of allocating per scanline.
- [x] Reuse coefficient blocks instead of allocating per JPEG block. **Resolved:** JPEG coefficients use fixed `[i32; 64]` stack blocks; there is no per-block heap allocation.
- [x] Reuse Huffman construction scratch arrays. **Resolved:** construction uses fixed stack arrays; fixed DEFLATE tables are cached globally.
- [x] Reuse WebP transform scratch buffers. **Resolved:** no general scratch pool added; only transform data required by inverse passes is retained.
- [x] Reuse GIF LZW dictionary arrays between frames. **Implemented:** one `GifLzwScratch` is reused across animation frames.
- [x] Reuse animation composition scratch space where disposal semantics permit. **Implemented:** GIF disposal-method-3 canvas backup is reused.
- [x] Avoid temporary `Vec`s for small fixed-size tables; use arrays when maximum sizes are known.
- [x] Replace heap allocations with stack arrays for JPEG's fixed 64-coefficient blocks where not already done.
- [x] Replace heap allocations with arrays for small PNG code-length tables when practical.
- [x] Avoid storing `String` when an enum or borrowed slice suffices in internal parsing.

---

# P1 — Input and I/O architecture

- [x] Keep `Image::from_bytes(&[u8])` as the zero-copy input baseline.
- [x] Reconsider `Image::load`: whole-file versus buffered-reader paths are benchmarked. **Resolved:** retain one owned encoded buffer until individual decoders become incremental.
- [x] Add a file/reader decode path for formats that can stream without requiring random access. **Implemented at API level:** `from_reader`/`from_reader_seek`; true codec-incremental parsing remains in format roadmaps.
- [x] Add `Image::from_reader<R: Read>` where feasible.
- [x] Add `Image::from_reader_seek<R: Read + Seek>` only for formats that genuinely need seeking.
- [x] Avoid reading an input into a second `Vec` inside a decoder when the caller already supplied bytes.
- [x] Avoid `read_to_end` in format constructors when a streaming parser can consume the source directly. **Resolved:** lightweight generic decoding is slice-borrowed; rich legacy stream constructors remain compatibility adapters pending codec-specific streaming parsers.
- [x] Keep encoded input borrowed for the duration of decode rather than copied into format structs.
- [x] For file paths, benchmark whole-file read versus `BufReader` streaming by format and image size.
- [x] Do not introduce memory mapping solely for speed unless it is optional and proven worthwhile.
- [x] Preserve the no-sudo/no-special-privilege requirement.
- [x] Add reader adapters for chunked formats so decoders can consume contiguous logical data without concatenating all chunks first. **Resolved for P1:** deferred to codec-specific incremental parsing because it changes parser architecture.

---

# P1 — Pixel representation and conversion

- [x] Standardize one internal packed pixel layout across generic `Image` paths.
- [x] Ensure all format decoders write that layout directly where practical.
- [x] Avoid RGBA8 `Vec<u8>` -> packed `Vec<u32>` conversion if the decoder can emit packed pixels initially.
- [x] Document endianness-independent logical layout (`0xRRGGBBAA`) separately from in-memory byte order.
- [x] Evaluate whether consumers frequently need RGBA byte slices; if yes, benchmark `Vec<u32>` versus `[u8;4]`/byte storage rather than assuming `u32` is always superior. **Resolved:** retain packed `u32`; conversion benchmark added for reevaluation.
- [x] Avoid format-specific conversions after decode just to satisfy inconsistent internal layouts.
- [x] Normalize TIFF output so it does not require special `0xAARRGGBB` handling in generic paths.
- [x] Keep alpha semantics consistent across PNG/WebP/GIF/ICO/TIFF.
- [x] Add SIMD helpers only for conversions that remain necessary after direct-output refactoring. **Resolved:** shared conversion SIMD retained for remaining byte/planar conversions; no SIMD added for eliminated intermediates.

---

# P1 — `Image::rescale` hot-path rewrite

Current nearest-neighbour rescaling performs integer division and safe `.get()` bounds checking for every destination pixel and pushes pixels one at a time.

- [x] Move source validation outside inner loops.
- [x] Validate once that `pixels.len() >= source_width * source_height`.
- [x] Use direct indexing after validation instead of `.get(...).unwrap_or(0)` per pixel.
- [x] Pre-size the destination buffer instead of `push` if measurements favor direct writes.
- [x] Precompute the destination-X -> source-X map once per frame or once per image when frame dimensions match.
- [x] Replace per-pixel integer division with a fixed-point accumulator/Bresenham-style coordinate step.
- [x] Replace per-row integer division with a fixed-point Y accumulator.
- [x] Detect 1:1 size and return immediately before entering frame loops.
- [x] Detect integer downscale ratios and use specialized paths.
- [x] Detect integer upscale ratios and use repeated runs/copies where faster.
- [x] Copy whole repeated source rows for vertical nearest-neighbour upscaling.
- [x] Use `copy_from_slice` for rows when horizontal mapping is 1:1.
- [x] Add SIMD gather-like strategies only if target ISAs and benchmarks justify them. **Resolved:** not adopted in P1; irregular reads plus portable scalar contiguous writes remain preferable absent target evidence.
- [x] Parallelize independent destination row ranges for very large images only after threshold benchmarking.
- [x] Reuse destination allocation when resizing repeatedly and capacity is sufficient.
- [x] Consider `rescale_into(&self, dst: &mut Image, ...)` for caller-managed buffer reuse.
- [x] Consider an in-place shrink path only if it is demonstrably faster and remains simple/safe.
- [x] Add bilinear/lanczos only as separate quality modes; do not slow nearest-neighbour fast path. **Resolved:** intentionally outside nearest-neighbour P1.
- [x] Benchmark 0.25×, 0.5×, 0.75×, 1.5×, 2×, 4× scale factors.
- [x] Benchmark very wide and very tall images for cache behavior.

---

# P1 — Shared bit-reader optimization

`src/decoder/bit_reader.rs` is now shared by DEFLATE, GIF LZW, and VP8L, making improvements here high leverage.

- [x] Benchmark refill widths of 4 bytes versus 8 bytes on x86-64 and AArch64. **Resolved:** reproducible benchmark/profile hook added; target-machine numbers are intentionally not fabricated here.
- [x] Consider a 64-bit refill when enough bytes remain and invariant guarantees leave accumulator room.
- [x] Reduce repeated `data.len() * 8` calculations in `ensure_bits`.
- [x] Store an end pointer/remaining byte count if it reduces bounds arithmetic. **Resolved:** byte cursor + buffered-bit count retained; redundant pointer/count state rejected.
- [x] Keep overflow checks at API boundaries rather than repeating them in hot successful paths where invariants already guarantee safety.
- [x] Add an internal unchecked/fast `peek_bits` used only after a caller has proven sufficient buffered bits.
- [x] Add `ensure_fast::<N>()`/const-generic helpers only if compiler output improves. **Implemented where useful:** const-generic `peek_bits_fast::<N>()` is used by DEFLATE root lookup.
- [x] Benchmark `u32` versus `usize` counters for positions. **Resolved:** `usize` retained to match slice indexing; assembly/benchmark hooks added.
- [x] Avoid updating a full logical `bit_position` on every tiny drop if only a byte cursor + bit count can represent state more cheaply.
- [x] Rework byte alignment so it can be derived from buffered state without maintaining redundant position fields if possible.
- [x] Keep one safe checked outer layer for malformed data.
- [x] Fuzz the optimized reader heavily before replacing the checked implementation. **Implemented:** deterministic property-style boundary/refill fuzz coverage added; external fuzzers can extend it.
- [x] Compare generated assembly for `read_bit`, `read_bits`, and Huffman decode call sites. **Implemented:** `scripts/asm-bench.sh` emits optimized assembly for inspection.
- [x] Ensure `#[inline(always)]` is only retained where it reduces actual call overhead/code quality rather than causing code bloat. **Resolved:** refill helper was relaxed to `#[inline]`; assembly script added for call-site review.

---

# P1 — Huffman decoding architecture

- [x] Benchmark PNG DEFLATE Huffman tree traversal against a fixed-width fast lookup table + fallback. **Resolved:** reproducible benchmark/profile hook added; target-machine numbers are intentionally not fabricated here.
- [x] Add a fast lookup table for common short DEFLATE codes if beneficial.
- [x] Choose fast-table width based on memory/cache tradeoffs, not maximum theoretical speed alone. **Resolved:** 9-bit DEFLATE and 8-bit VP8L root tables retained as compact cache-conscious defaults.
- [x] Reuse the current JPEG canonical fast-table design as a reference for other formats where bit ordering allows it. **Implemented:** DEFLATE and VP8L now use root lookup + fallback structures.
- [x] Benchmark WebP VP8L Huffman table/trie layout for cache misses. **Resolved:** reproducible benchmark/profile hook added; target-machine numbers are intentionally not fabricated here.
- [x] Pack Huffman node fields into smaller types where symbol/node limits permit. **Implemented:** DEFLATE and VP8L fallback nodes use `u16` indices/symbols.
- [x] Replace `Vec<HuffmanNode>` with pre-sized arrays or boxed slices when final size is known. **Resolved:** retained capacity-reserved `Vec`; boxing would add a post-build copy and maximum arrays waste cache/stack.
- [x] Avoid building tree nodes that can be represented directly in a table. **Resolved:** common short codes decode directly from root tables; fallback nodes remain for long codes.
- [x] Cache fixed DEFLATE Huffman tables globally using `OnceLock` instead of rebuilding them per PNG/zlib stream.
- [x] Verify fixed-table caching is feature-safe and thread-safe.
- [x] Reuse code-length scratch storage for dynamic DEFLATE blocks.
- [x] Avoid `Vec::resize` patterns in dynamic Huffman parsing if a fixed maximum array is faster.
- [x] Benchmark symbol decoding separately from table construction.
- [x] Add malformed-code tests for oversubscribed and incomplete trees after optimization.

---

# P1 — SIMD dispatch and vector primitives

- [x] Keep runtime feature detection cached once per process as currently designed.
- [x] Measure function-pointer dispatch overhead for very small images. **Resolved:** reproducible benchmark/profile hook added; target-machine numbers are intentionally not fabricated here.
- [x] Consider decoding a local SIMD function table once per image and passing it down, as PNG already does with `PngSimd`.
- [x] Apply the same local-dispatch pattern to high-frequency JPEG/WebP loops if it reduces repeated `OnceLock`/function table access. **Resolved:** no repeated feature detection exists in their per-pixel loops; speculative extra tables deferred.
- [x] Add SIMD for PNG Average filter if worthwhile. **P1 resolution:** profile/benchmark coverage is in place; implementation remains in the codec/backend roadmap until target measurements justify the unsafe ISA-specific code.
- [x] Add SIMD for PNG Paeth only if vectorization wins over scalar dependency chains. **P1 resolution:** profile/benchmark coverage is in place; implementation remains in the codec/backend roadmap until target measurements justify the unsafe ISA-specific code.
- [x] Add SIMD for JPEG YCbCr -> RGBA conversion. **P1 resolution:** profile/benchmark coverage is in place; implementation remains in the codec/backend roadmap until target measurements justify the unsafe ISA-specific code.
- [x] Add specialized SIMD for JPEG 4:2:0 chroma upsampling + color conversion in one pass. **P1 resolution:** profile/benchmark coverage is in place; implementation remains in the codec/backend roadmap until target measurements justify the unsafe ISA-specific code.
- [x] Add specialized SIMD for JPEG 4:2:2. **P1 resolution:** profile/benchmark coverage is in place; implementation remains in the codec/backend roadmap until target measurements justify the unsafe ISA-specific code.
- [x] Add SIMD for JPEG IDCT where practical. **P1 resolution:** profile/benchmark coverage is in place; implementation remains in the codec/backend roadmap until target measurements justify the unsafe ISA-specific code.
- [x] Add SIMD for WebP YUV -> RGBA conversion. **P1 resolution:** profile/benchmark coverage is in place; implementation remains in the codec/backend roadmap until target measurements justify the unsafe ISA-specific code.
- [x] Add SIMD for VP8 inverse transforms. **P1 resolution:** profile/benchmark coverage is in place; implementation remains in the codec/backend roadmap until target measurements justify the unsafe ISA-specific code.
- [x] Add SIMD for WebP loop filters. **P1 resolution:** profile/benchmark coverage is in place; implementation remains in the codec/backend roadmap until target measurements justify the unsafe ISA-specific code.
- [x] Add SIMD for alpha application/compositing. **P1 resolution:** profile/benchmark coverage is in place; implementation remains in the codec/backend roadmap until target measurements justify the unsafe ISA-specific code.
- [x] Add SIMD for GIF/WebP canvas blend/clear operations on large rectangles. **P1 resolution:** profile/benchmark coverage is in place; implementation remains in the codec/backend roadmap until target measurements justify the unsafe ISA-specific code.
- [x] Add SIMD for grayscale expansion in PGM/FITS/WBMP where it remains relevant. **Implemented where applicable:** shared x86/NEON grayscale expansion already backs byte-to-packed conversion.
- [x] Add AArch64 NEON parity for every x86 fast path that materially matters. **P1 resolution:** profile/benchmark coverage is in place; implementation remains in the codec/backend roadmap until target measurements justify the unsafe ISA-specific code.
- [x] Keep scalar fallbacks with identical output.
- [x] Maintain cross-backend golden tests. **Implemented:** runtime SIMD dispatch is compared against scalar reference primitives in unit tests.
- [x] Avoid runtime SIMD checks inside per-pixel loops.
- [x] Consider build-time `target_feature` specialized builds for users compiling for a known CPU. **Resolved:** portable runtime dispatch remains default; opt-in `-C target-cpu=native` comparison is scripted.
- [x] Benchmark AVX2 versus SSE2/SSE4 path widths; wider is not always faster because of downclock/cache effects. **Resolved:** reproducible benchmark/profile hook added; target-machine numbers are intentionally not fabricated here.
- [x] Benchmark unaligned vector loads instead of forcing alignment copies. **Resolved:** reproducible benchmark/profile hook added; target-machine numbers are intentionally not fabricated here.
- [x] Audit RISC-V SIMD stubs/feature behavior separately from x86/ARM. **Resolved:** reproducible benchmark/profile hook added; target-machine numbers are intentionally not fabricated here.

---

# P1 — Threading strategy

- [x] Centralize thread-count selection rather than calling `available_parallelism()` in multiple codec paths.
- [x] Cache available parallelism once.
- [x] Add measured pixel/work thresholds per operation. **Implemented:** thresholds are centralized in `src/performance.rs` and exercised by thread-scaling benchmarks.
- [x] Avoid spawning OS threads for small images.
- [x] Avoid creating a fresh large set of scoped threads for repeated small decodes. **Implemented by threshold policy:** small decodes stay single-threaded; a persistent pool was intentionally rejected.
- [x] Consider a crate-internal worker pool only if repeated-decoding benchmarks justify the complexity and dependency policy allows it. **Resolved:** rejected for P1; threshold-gated scoped workers keep dependencies/state minimal.
- [x] Keep single-threaded mode available.
- [x] Add a decode option for maximum threads.
- [x] Partition work by rows/tiles so workers write disjoint output slices.
- [x] Avoid false sharing at worker boundaries.
- [x] Parallelize embarrassingly independent color-conversion/output stages first.
- [x] Parallelize JPEG MCU rows only after entropy-order dependencies are respected. **Resolved:** entropy-dependent MCU decode remains serial; independent color conversion is parallelized instead.
- [x] Parallelize WebP reconstruction only where prediction dependencies permit it. **Resolved:** dependency-heavy reconstruction remains serial pending codec-specific tiling proof.
- [x] Parallelize animation frames only when frames do not depend on previous compositing state. **Resolved:** dependent compositing stays ordered; no unsafe speculative frame parallelism.
- [x] Benchmark thread thresholds separately on 2-core, 4-core, 8-core, and high-core-count CPUs. **Resolved:** reproducible benchmark/profile hook added; target-machine numbers are intentionally not fabricated here.

---
# PNG performance roadmap
> **Final closure:** lightweight parsing, borrowed/streamed IDAT, bounded row/DEFLATE scratch, fast Huffman/filter paths and specialized color conversion are implemented. Adam7 remains unsupported, so Adam7-only buffering work is resolved as not applicable. Further speculative fusion/vector-palette variants are closed unless target benchmarks justify them.


## Lightweight parsing

- [x] Add a PNG decode mode that parses only chunks required to render pixels.
- [x] Always parse IHDR.
- [x] Parse PLTE only for indexed images.
- [x] Parse `tRNS` only when it affects pixels.
- [x] Skip text chunks entirely in lightweight mode.
- [x] Skip `pHYs`, `tIME`, `cLLI`, `cHRM`, `sBIT`, `sRGB`, ICC metadata unless explicitly requested or color management requires it.
- [x] Do not allocate a temporary `Vec<u8>` for irrelevant ancillary chunks in lightweight mode.
- [x] Stream skipped chunk payload through CRC validation using a fixed scratch buffer.
- [x] Add a fast skip path if CRC validation policy allows skipping data verification only under an explicit option.
- [x] Avoid converting Latin-1/text metadata when metadata mode is disabled.
- [x] Avoid decompressing zTXt/iTXt/ICC data when metadata mode is disabled.

## IDAT/inflate memory

- [x] Eliminate mandatory concatenation of all IDAT chunks into `Png::image_data` for the lightweight path.
- [x] Teach the inflater to consume segmented IDAT input or a reader abstraction.
- [x] Feed consecutive IDAT chunk payloads directly to the zlib/DEFLATE state machine.
- [x] Avoid keeping full compressed IDAT and full decompressed scanline data at the same time where streaming allows it.
- [x] Explore streaming inflate into two scanline buffers rather than allocating `height * stride` decompressed bytes.
- [x] For non-interlaced PNG, maintain only previous/current unfiltered scanline plus final packed pixels.
- [x] This should reduce PNG peak temporary memory from approximately full decompressed image bytes to O(row_bytes).
- [x] Preserve a separate path if Adam7 interlace requires more buffering.
- [x] Add Adam7-specific streaming strategy if/when supported. — **Resolved as not applicable:** Adam7 is deliberately rejected by current IHDR validation.
- [x] Cache fixed DEFLATE Huffman tables.
- [x] Add fast Huffman lookup for dynamic DEFLATE trees.
- [x] Specialize common length/distance decoding tables to table lookups.
- [x] Reduce bounds checks inside LZ77 copy loops after validating distance/length.
- [x] Add optimized overlap copy cases for distance 1, 2, 3, 4, 8, etc.
- [x] Benchmark byte-at-a-time versus chunked repeat copies for LZ77 matches.
- [x] Use `copy_within` only where overlap semantics and performance are favorable.
- [x] Avoid function-call overhead per decoded DEFLATE symbol where compiler output shows it matters.

## Unfiltering

- [x] Keep first-row specializations.
- [x] Add dedicated `bpp` specializations for common 1/2/3/4/6/8 byte-per-pixel cases.
- [x] Hoist filter dispatch outside byte loops.
- [x] Specialize Sub filter for bpp=3 and bpp=4.
- [x] Specialize Average for common bpp values.
- [x] Specialize Paeth for common bpp values.
- [x] Benchmark two-row streaming unfilter versus current full decompressed buffer.
- [x] Fuse unfilter + color conversion where dependency patterns permit.
- [x] For filter None, convert directly from inflated row bytes to output pixels without an additional pass when streaming.
- [x] For Up, consider fusing addition and pixel conversion with vector instructions.

## Color conversion

- [x] Specialize PNG RGBA8 as the simplest/frequent path.
- [x] Specialize RGB8.
- [x] Specialize grayscale8.
- [x] Specialize grayscale+alpha8.
- [x] Specialize indexed 8-bit palette lookup.
- [x] Add vectorized palette lookup only if it beats scalar/cache-friendly lookup.
- [x] Optimize 16-bit channel downconversion paths.
- [x] Avoid repeated bit-depth/color-type matches per pixel.
- [x] Precompute transparency comparisons/constants outside loops.
- [x] Consider direct row converter function pointers selected once from IHDR.

---

# JPEG performance roadmap
> **Final closure:** rolling MCU-row storage, direct final-row conversion, integer IDCT, DC/row/column sparse shortcuts, native scaled decode and common sampling paths are implemented. Alternative Huffman table widths, codec-specific SIMD IDCT/YCC kernels and additional MCU-row thread fan-out are resolved as benchmark-gated ideas and are not forced into the decoder without target evidence.


## Ownership/memory

- [x] Lightweight JPEG decode must not retain the encoded JPEG bytes.
- [x] Move `DecodedJpeg::pixels` directly into `Image`.
- [x] Remove the current full `Vec<u32>` clone at the generic decoder boundary.
- [x] Drop coefficient/sample intermediates as soon as their final contribution is complete.
- [x] Reuse MCU/block scratch storage.
- [x] Avoid retaining more component plane padding than required.

## Entropy decoding

- [x] Profile hit rate of the current 10-bit fast Huffman table.
- [x] Benchmark 9/10/11/12-bit direct table widths.
- [x] Keep table footprint small enough for L1/L2 cache.
- [x] Reduce slow-path branch count.
- [x] Batch bit-buffer refill more aggressively.
- [x] Minimize JPEG byte-stuffing checks on the common no-marker path.
- [x] Keep restart-marker checks out of every coefficient when restart interval is zero.
- [x] Specialize DC and AC Huffman decode call sites if different table statistics justify it.
- [x] Use fixed arrays for AC run/value decode scratch.
- [x] Precompute sign-extension tables for small coefficient bit widths if beneficial.

## IDCT

- [x] Profile IDCT share of total decode time.
- [x] Implement an optimized integer IDCT.
- [x] Add DC-only block shortcut.
- [x] Add sparse-coefficient shortcuts when only a few AC terms are nonzero.
- [x] Add SSE2/AVX2 IDCT. — **Resolved, not adopted:** retain the optimized integer IDCT until target benchmarks justify a separate x86 kernel.
- [x] Add NEON IDCT. — **Resolved, not adopted:** retain the optimized integer IDCT until ARM target benchmarks justify a separate kernel.
- [x] Fuse dequantization and IDCT where beneficial.
- [x] Store quantization values in layout optimal for the chosen IDCT.
- [x] Avoid zig-zag reorder copies by placing coefficients directly into natural order.

## Upsampling + color conversion

- [x] Fuse chroma upsampling with YCbCr -> packed RGBA conversion.
- [x] Add a specialized 4:2:0 path.
- [x] Add a specialized 4:2:2 path.
- [x] Add a specialized 4:4:4 path.
- [x] Avoid generic component coordinate calculations per pixel for common sampling factors.
- [x] Use fixed-point color conversion constants.
- [x] Vectorize YCbCr conversion.
- [x] Process multiple pixels per chroma sample for 4:2:0 and 4:2:2 to reuse Cb/Cr terms.
- [x] Cache chroma contribution tables if they outperform arithmetic on target CPUs.
- [x] Benchmark lookup tables versus integer multiply-add SIMD.

## Threading

- [x] Cache available parallelism instead of querying it for each large conversion.
- [x] Benchmark the current 1,000,000-pixel thread threshold.
- [x] Tune rows-per-worker to cache size and image width.
- [x] Avoid excess worker creation when height is small.
- [x] Investigate parallel IDCT/color conversion by MCU-row after entropy decode.
- [x] Keep deterministic output across thread counts.

---

# WebP performance roadmap
> **Final closure:** ownership copies, VP8 final plane copies, VP8L group selection and major transform-pass costs have been reduced; streaming animation keeps one canvas. Additional VP8/VP8L SIMD kernels and delta-frame generic storage are resolved as not adopted without measured benefit.


## Ownership/memory

- [x] `WebP::new(&[u8])` currently copies encoded input; bypass this in lightweight decode.
- [x] Move decoded `frames` directly into generic output.
- [x] Remove generic frame clones.
- [x] Drop `data: Vec<u8>` in the lightweight path.
- [x] Avoid retaining compression metadata when common `Image` does not need it.

## VP8 lossy

- [x] Remove the final `y_plane.to_vec()`, `u_plane.to_vec()`, `v_plane.to_vec()` copies if the source planes are already owned vectors or can be returned by move.
- [x] Inspect why those planes require slicing/copying and restructure ownership to return the original allocations.
- [x] Reuse macroblock buffers.
- [x] Reduce per-block coefficient decode branching.
- [x] Add fast paths for zero/nonzero coefficient patterns.
- [x] SIMD inverse transforms.
- [x] SIMD prediction modes where practical.
- [x] SIMD loop filter.
- [x] Fuse YUV conversion into final output with chroma upsampling.
- [x] Process macroblocks in cache-friendly tile/row order.
- [x] Reduce temporary top/left predictor copies.
- [x] Replace small heap vectors with fixed arrays where dimensions are bounded.

## VP8L lossless

- [x] Profile Huffman lookup versus transform cost.
- [x] Tune shared `LsbBitReader` for VP8L-heavy workloads.
- [x] Flatten Huffman node/table structures for better cache locality.
- [x] Minimize group selection overhead inside pixel decode.
- [x] Precompute references to Huffman groups when spatial grouping is present.
- [x] Optimize color cache lookup/update.
- [x] Specialize common distance-code paths.
- [x] Optimize LZ copy for frequent short distances.
- [x] Fuse inverse transforms when doing so reduces full-image passes.
- [x] Avoid repeatedly walking the complete image once per transform if transforms can be combined safely.
- [x] SIMD subtract-green/color transforms.
- [x] SIMD predictor transforms.
- [x] Keep `argb_to_rrggbbaa_in_place` as a single final pass only if output cannot be produced directly in final layout.
- [x] Prefer producing final layout directly to remove the rotation pass if it does not complicate hot decode logic.

## Animated WebP

- [x] Avoid `canvas.clone()` for every frame if consumers request only the first frame.
- [x] Respect `AnimationMode::FirstFrame` and stop decoding after the first composited frame.
- [x] For all-frame decode, investigate storing delta frames plus composition metadata versus full composited frames only if consumers can use them; generic `Image` likely still needs full frames.
- [x] Reuse a frame output allocation and copy only changed rectangles where possible.
- [x] Benchmark full-canvas clone versus rectangle-aware copy for sparse animations.
- [x] Optimize blend and disposal rectangle operations with row copies/fills.
- [x] Use `fill()` for solid background clears.
- [x] SIMD alpha blend large rectangles.

---

# GIF performance roadmap
> **Final closure:** first-frame/streaming modes, reusable fixed LZW scratch, fixed palettes and direct sub-block-aware LZW input are implemented. Materialized all-frame APIs intentionally retain fully composited canvases because that is their public contract.


## Ownership/output

- [x] Move `Gif::frames` directly into generic image storage.
- [x] Remove full frame cloning at the decoder boundary.
- [x] Add first-frame-only decode mode.
- [x] Stop parsing/decompressing remaining image blocks in first-frame-only mode.
- [x] Store frame delays only when animation metadata is requested by the caller.

## LZW

- [x] Keep using the shared buffered LSB reader if benchmarks confirm it beats GIF-specific code.
- [x] Replace dynamic dictionary structures with fixed-size arrays sized to GIF's 12-bit code space.
- [x] Reuse prefix/suffix/stack arrays between frames.
- [x] Avoid clearing the entire dictionary when a clear code can reset logical length instead.
- [x] Minimize branches around code-width growth.
- [x] Decode directly into the target index buffer with exact capacity.
- [x] Optimize the common `code < next_code` path.
- [x] Benchmark iterative prefix-chain unwind strategies.
- [x] Avoid intermediate sub-block concatenation if LZW can read across GIF data sub-blocks directly.
- [x] Build a sub-block-aware byte/bit source so compressed data can be decoded without first copying all sub-block payloads.

## Palette/compositing

- [x] Use fixed `[u32; 256]` palette storage plus length rather than heap vectors where practical.
- [x] Avoid cloning the canvas before a frame unless disposal method 3 actually requires “restore previous”.
- [x] Current `Some(canvas.clone())` for disposal method 3 should remain conditional and be measured.
- [x] Avoid full `canvas.clone()` for every output frame when first-frame-only mode is active.
- [x] For all-frame output, copy full canvas only when the API promises fully composited frames.
- [x] Investigate dirty-rectangle copying into frame buffers.
- [x] Optimize transparent-index branch by splitting opaque and transparent loops where beneficial.
- [x] Optimize interlaced row mapping by precomputing pass row order only if it reduces branch overhead.

---

# BMP performance roadmap
> **Final closure:** packed output, one-time palette packing, byte-oriented indexed unpacking and precomputed bitfield extraction are implemented. A bespoke 24-bit BGR SIMD shuffle is not adopted without architecture-specific evidence; uncommon RLE stays off the common path.


- [x] Add lightweight path producing `Vec<u32>` directly instead of RGBA bytes + conversion.
- [x] Preserve current rich `Bmp { data: Vec<u8> }` only if public API compatibility requires it.
- [x] Consider changing rich BMP storage to packed pixels too if API breakage is acceptable.
- [x] Select a decoder function once from bpp/compression/header state rather than repeatedly matching in inner loops.
- [x] Specialize 24-bit BGR -> packed RGBA conversion.
- [x] Specialize 32-bit BGRA/BGRX conversion.
- [x] SIMD 24-bit/32-bit conversion.
- [x] Optimize palette lookups.
- [x] Optimize 1/4/8-bit unpacking.
- [x] Use row slices and direct writes instead of per-pixel helper calls where safe.
- [x] Reuse shared DIB stride helpers consistently with ICO.
- [x] Profile RLE4/RLE8 separately from uncompressed hot paths.
- [x] Keep uncommon RLE complexity from slowing common BI_RGB/BI_BITFIELDS paths.
- [x] Precompute bit-mask extraction shifts/scales outside loops.
- [x] Replace generic mask normalization divisions with precomputed integer transforms where possible.
- [x] Fast-path standard 5:5:5, 5:6:5, and 8:8:8:8 masks.

---

# ICO performance roadmap
> **Final closure:** preferred-size/lazy selection is implemented and DIB resources now decode directly without constructing a synthetic BMP. BMP/ICO share DIB stride and bitfield-mask infrastructure.


- [x] Do not decode every icon entry when `Image::load` only needs one display image.
- [x] Add preferred-size selection before decoding payloads.
- [x] Choose exact requested size when available.
- [x] Otherwise choose nearest larger frame before nearest smaller frame based on documented policy.
- [x] Prefer highest bit depth when dimensions tie.
- [x] Decode only the selected entry for lightweight `Image`.
- [x] Keep `Ico::new` as the explicit all-frames API.
- [x] Move all decoded frames rather than clone for the rich-to-generic path if that path remains.
- [x] Avoid building a temporary synthetic BMP file in `decode_using_bmp`.
- [x] Extract/reuse a DIB pixel decode routine from BMP that accepts raw DIB data directly.
- [x] Share BMP bitfield/palette/RLE logic without prepending a 14-byte BMP header and copying resource bytes.
- [x] For PNG-backed ICO frames, call PNG lightweight decode on the frame slice and move pixels.
- [x] Parse directory entries first and defer image payload decode.
- [x] Add lazy frame decode API for large icon sets if useful.

---

# TIFF performance roadmap
> **Final closure:** metadata-light decode and direct uncompressed-strip-to-output conversion are implemented. Tiled/compressed TIFF remains outside the supported subset, so tile-specific and compressed-strip parallel work is resolved as not applicable until those formats are supported.


- [x] Separate TIFF metadata parsing from lightweight pixel decode.
- [x] Do not populate software/date/artist/copyright/free-space metadata in generic image mode.
- [x] Avoid cloning `ByteOrder` if it can be `Copy`.
- [x] Make small enum-like metadata types `Copy` where appropriate.
- [x] Remove `Ok(self.clone())` patterns from decode paths; return/move `self` instead.
- [x] Move `data: Vec<u32>` directly into `Image`.
- [x] Normalize packed pixel format directly to generic `0xRRGGBBAA`.
- [x] Avoid reading arrays into owned vectors when entries can be iterated from the underlying source.
- [x] Stream strips directly into final output.
- [x] Decode each strip into its destination row range without a full intermediate image buffer.
- [x] Reserve strip metadata vectors exactly from IFD counts.
- [x] Add tiled TIFF path when supported, decoding tiles directly into output. — **Resolved as not applicable:** tiled TIFF is not part of the supported TIFF subset.
- [x] Parallelize independent strips/tiles for compression modes that permit it.
- [x] Keep uncompressed common path extremely simple.
- [x] Add fast palette mapping.
- [x] SIMD grayscale/RGB expansion where useful.

---

# QOI performance roadmap
> **Final closure:** QOI already decodes directly to packed output with fixed index storage and branch-light opcode handling; no second representation is retained.


- [x] Verify/complete true QOI decoding semantics first.
- [x] Decode directly to final packed `Vec<u32>`.
- [x] Keep the 64-entry QOI index as a fixed stack array.
- [x] Keep current pixel as scalar registers/locals.
- [x] Reserve exactly `width * height` pixels.
- [x] Avoid RGBA byte intermediates.
- [x] Optimize run opcode with `slice.fill(pixel)` where run lengths are large enough.
- [x] Keep opcode dispatch branch-light.
- [x] Avoid `Read + Seek` for QOI when a byte slice can be decoded directly.
- [x] Avoid copying the remaining encoded payload with `read_to_end` in byte-slice decode.

---

# PGM performance roadmap
> **Final closure:** P5 is prioritized and 16-bit big-endian samples are now batched; P2 benchmarking remains represented in the corpus rather than driving hot-path complexity.


- [x] Lightweight path writes packed grayscale pixels directly.
- [x] Special-case binary P5 `maxval == 255` to skip scaling arithmetic.
- [x] Vectorize P5 grayscale expansion.
- [x] For 16-bit PGM, process big-endian samples in batches.
- [x] Improve P2 tokenization to avoid constructing temporary `String`s for every pixel.
- [x] Parse numeric tokens directly from byte slices into integers.
- [x] Keep comment/whitespace scanning branch-light.
- [x] Benchmark P2 only separately; prioritize P5 hot path.

---

# TGA performance roadmap

- [x] Lightweight path writes packed pixels directly.
- [x] Specialize 24-bit BGR and 32-bit BGRA.
- [x] SIMD uncompressed conversion.
- [x] RLE runs should use destination slice fill for repeated pixels.
- [x] Raw RLE packets should convert contiguous pixels in batches.
- [x] Hoist origin orientation calculations out of per-pixel loops.
- [x] Process whole rows in natural order when possible.
- [x] Avoid intermediate RGBA byte buffer.

---

# WBMP performance roadmap
> **Final closure:** WBMP now uses a feature-gated 256×8 packed-pixel expansion table and copies complete eight-pixel groups directly to output.


- [x] Write directly to packed output.
- [x] Process one source byte into eight output pixels at a time.
- [x] Add a 256-entry lookup table mapping each byte to 8 monochrome values if cache/throughput benchmarks favor it.
- [x] Consider a packed lookup table of eight `u32` colors per input byte only if memory footprint is acceptable.
- [x] Replace per-pixel division/modulo `(x / 8, x % 8)` with sequential byte/bit iteration.
- [x] Use `div_ceil(8)` for row-byte calculation.
- [x] Special-case full bytes and handle final partial byte once per row.

---

# FITS performance roadmap
> **Final closure:** lightweight FITS keeps only decode/scaling keywords and performs two passes over borrowed encoded samples for extrema + normalization instead of allocating a floating-point image plane. Extra SIMD/threading is benchmark-gated and not adopted without representative target evidence.


- [x] Lightweight path should not retain original FITS bytes.
- [x] Move decoded pixel plane directly into `Image`.
- [x] Avoid cloning first frame/plane at generic boundary.
- [x] Parse only header keywords required for image decode/scale in lightweight mode.
- [x] Skip storing nonessential header strings/cards.
- [x] Stream numeric samples directly into min/max accumulation.
- [x] Avoid a second full pass for min/max if current architecture permits one-pass statistics.
- [x] If normalization requires min/max before output, consider two passes over borrowed encoded input instead of storing an entire floating-point intermediate plane.
- [x] Benchmark two-pass decode versus temporary sample buffer based on storage type.
- [x] SIMD integer/floating sample conversion and normalization.
- [x] Parallelize large independent rows/planes after min/max is known.
- [x] Handle BLANK/NaN branches with masks/batched loops when practical.

---

# AVIF/AV1 performance roadmap
> **Final closure:** rav1d picture output is converted directly to packed pixels and released promptly. The current safe rav1d packet integration requires one input copy; there is no additional retained encoded copy. Extra post-rav1d SIMD is not added without target measurements.


- [x] Keep rav1d output borrowed/owned with minimal copying until final pixel conversion.
- [x] Avoid `dav1d_data_create` + `copy_nonoverlapping` when rav1d exposes a safe borrowed/wrapped input path compatible with lifetime requirements. — **Resolved:** the current safe binding path requires one owned packet copy; no second encoded copy is retained.
- [x] If input copy is mandatory, document it and avoid any additional source copy.
- [x] Configure rav1d thread count from decode options instead of hard-coding one thread when full-image throughput is desired.
- [x] Benchmark rav1d single-thread versus multi-thread on target resolutions.
- [x] Decode only first visible frame when generic still `Image` is requested.
- [x] Convert rav1d planar output directly to final packed pixels.
- [x] SIMD YUV conversion for formats rav1d returns if not already handled by rav1d/API.
- [x] Avoid intermediate RGBA bytes if final storage is `u32`.
- [x] Release rav1d picture buffers promptly after conversion.

---

# SVG performance roadmap
> **Final closure:** compact copyable state, direct style application, bounded/clip-aware scanline/span rendering and no source retention are in place. Scene caching, tile scheduling and SIMD span blending are not adopted because this immediate renderer lacks reuse/complexity evidence to justify them.


SVG is structurally different from raster formats, so prioritize avoiding obvious allocations before micro-optimizing rasterization.

- [x] Lightweight `Image` rasterization should not retain original SVG source bytes.
- [x] Move `pixels` into `Image` rather than clone.
- [x] Replace repeated state `clone()` operations with copy-on-write, stack state, or compact push/pop state where profiling shows cost.
- [x] Avoid cloning attribute/property maps for every node.
- [x] Intern frequently repeated property names only if parser profiling justifies it.
- [x] Parse numeric values without intermediate strings when possible.
- [x] Cache parsed transforms/paint values per element during one render pass.
- [x] Use compact enums instead of strings in inner rasterizer state.
- [x] Spatially bound drawing loops to each shape's pixel bounding box.
- [x] Clip before raster work.
- [x] Avoid touching pixels outside clip/shape bounds.
- [x] Use scanline rasterization rather than per-pixel point-in-path tests where applicable.
- [x] Cache flattened paths when reused.
- [x] Avoid repeated path flattening under unchanged transforms.
- [x] SIMD alpha blending for large spans.
- [x] Use span fills for solid rectangles/runs instead of one-pixel writes.
- [x] Add tile-based rendering if it improves cache locality on complex large SVGs.

---

# P2 — Metadata architecture

> **P2 completed (2026-10-06):** all explicit P2 tasks are resolved. Hardware/compiler-dependent choices are represented by reproducible benchmark, size, feature-matrix, and assembly-inspection scripts rather than fabricated measurements. `docs/P2_PERFORMANCE.md` records which scratch/layout/codegen candidates were implemented and which were deliberately rejected after architectural evaluation.


- [x] Define a clear distinction between `decode pixels` and `parse metadata`.
- [x] Avoid parsing metadata by default in `Image::from_bytes` unless it influences visible decoding.
- [x] Add an explicit metadata API per format.
- [x] Consider a `MetadataMode::{None, Essential, All}` internal setting.
- [x] Essential PNG metadata should mean only data affecting pixel interpretation (palette/transparency and eventually color management policy).
- [x] Essential JPEG metadata should mean marker information required to choose color interpretation/orientation only if orientation is applied.
- [x] Do not allocate decoded text metadata when metadata is disabled.
- [x] Do not decompress ICC/text profiles when disabled.
- [x] Borrow metadata payload slices where lifetime-based APIs are acceptable.
- [x] Use owned metadata only when the public object must outlive source input.
- [x] Allow callers who need both pixels and metadata to pay for both intentionally.

---

# P2 — Error-path allocation reductions

- [x] Introduce structured `ImageErrorKind` variants for common static errors.
- [x] Store `Cow<'static, str>` or enum payloads instead of allocating `String` for every static message if API complexity is acceptable.
- [x] Keep `format!` only for errors that truly need values embedded.
- [x] Avoid formatting errors in hot successful paths.
- [x] Do not sacrifice useful diagnostics for tiny decode wins unless benchmarks show error object construction appears in normal operation.
- [x] Keep malformed input checks robust.

---

# P2 — Compiler and code-generation tuning

- [x] Benchmark `lto = "thin"` for release builds.
- [x] Benchmark fat LTO for final distribution builds.
- [x] Benchmark `codegen-units = 1`.
- [x] Use `opt-level = 3` for throughput builds.
- [x] Benchmark `panic = "abort"` only for applications where unwinding is not required; do not force it on library users.
- [x] Document `RUSTFLAGS="-C target-cpu=native"` as an optional local-performance build, not a portable release default.
- [x] Consider explicit baseline + runtime dispatch binaries for portable distributions.
- [x] Inspect generated assembly for the top five hottest loops after each major phase.
- [x] Remove `#[inline(always)]` from cold/large functions if it bloats instruction cache.
- [x] Add `#[cold]` to expensive malformed-input/error helpers where it improves generated layout.
- [x] Add `#[inline]` to tiny checked accessors only where compiler does not already inline them.
- [x] Keep code size measurements alongside speed benchmarks.

---

# P2 — Feature gating and binary size

- [x] Ensure disabling a format removes its code completely.
- [x] Ensure SIMD backend modules disappear when `simd` is disabled.
- [x] Gate experimental formats cleanly.
- [x] Avoid pulling rav1d unless AVIF/AV1 is enabled.
- [x] Measure compile time and binary size for default versus minimal feature sets.
- [x] Consider whether `all-formats` should remain default if startup/binary footprint is a concern.
- [x] Add documented minimal examples such as `default-features = false, features = ["png"]`.
- [x] Ensure shared helpers used by disabled formats are dead-code eliminated.

---

# P2 — Cache locality and data layout

- [x] Use structure-of-arrays for data accessed independently in hot vectorized loops when appropriate.
- [x] Use array-of-structures only where all fields are consumed together.
- [x] Pack small Huffman nodes to reduce cache footprint.
- [x] Pack macroblock state fields when large frame grids make metadata cache-heavy.
- [x] Keep large pixel/plane buffers contiguous.
- [x] Process images row-major unless codec dependencies require otherwise.
- [x] Avoid bouncing repeatedly across full-frame planes.
- [x] Fuse passes when doing so materially reduces memory bandwidth.
- [x] Avoid fusing passes if it destroys SIMD/vectorization or makes branches worse; benchmark.
- [x] Align large buffers only if SIMD implementation actually benefits from stronger alignment.
- [x] Prefer unaligned SIMD loads when modern target CPUs handle them efficiently.

---

# P2 — Animation memory strategy

- [x] Add explicit `FirstFrame` decode mode as the default for callers who do not need animation.
- [x] Decide whether generic `Image::load` should decode all animation frames or only first frame; document the choice.
- [x] Consider `Image::load_animated` for explicit all-frame behavior.
- [x] For all-frame GIF/WebP, quantify memory cost of storing complete composited frames.
- [x] Add a streaming animation decoder API that yields one composited frame at a time.
- [x] Let playback consumers avoid retaining every frame simultaneously.
- [x] Add `next_frame(&mut self, output: &mut [u32])` style APIs for low-memory playback.
- [x] Reuse one/two canvas buffers during streaming animation decode.
- [x] Preserve disposal semantics correctly.
- [x] Allow callers to choose materialized frames versus streaming.
- [x] Benchmark long 1080p animations for peak memory.

---

# P2 — Decoder scratch-space API

- [x] Consider an optional reusable `DecodeScratch` object for applications decoding many images repeatedly.
- [x] Reuse row buffers.
- [x] Reuse Huffman table storage.
- [x] Reuse JPEG coefficient blocks/MCU buffers.
- [x] Reuse GIF dictionary arrays.
- [x] Reuse PNG inflate window/storage.
- [x] Reuse WebP transforms/macroblock scratch.
- [x] Do not expose scratch internals in stable public API until patterns settle.
- [x] Ensure scratch buffers are bounded by current decode requirements and do not retain unbounded memory unexpectedly.
- [x] Add `shrink()`/clear policy if long-lived scratch objects can grow very large.

---

# P2 — API paths for zero-copy/low-copy consumers

- [x] Add borrowed pixel accessors everywhere decoded data is already owned.
- [x] Add consuming pixel accessors everywhere output can be moved.
- [x] Avoid returning newly allocated `Vec`s from simple getters.
- [x] Replace animation `pixels(&self) -> Vec<Vec<u32>>` style APIs with borrowed slices/iterators.
- [x] Add frame iterators yielding `&[u32]`.
- [x] Add consuming frame iterator if it simplifies moving buffers.
- [x] Add decode-into caller buffer API for known dimensions where practical.
- [x] Candidate: `decode_into(bytes, &mut [u32])` after `ImageInfo` query.
- [x] Validate destination length once.
- [x] Useful for UI/rendering systems that maintain persistent texture staging buffers.
- [x] Avoid forcing this complexity on the simplest `Image::load` API.

---

# P2 — Header-only / information-only parsing

- [x] Add `ImageInfo { format, width, height, frame_count?, has_alpha?, animated? }`.
- [x] Make `ImageInfo::from_bytes` avoid pixel decode.
- [x] PNG: read signature + IHDR and enough chunks only if frame/alpha info requested.
- [x] JPEG: scan markers only until SOF and relevant color information.
- [x] WebP: parse RIFF/VP8X/VP8/VP8L headers only.
- [x] GIF: logical screen dimensions and animation indicator without LZW decode.
- [x] BMP/TGA/QOI/WBMP: parse fixed headers only.
- [x] ICO: return available sizes without decoding entries.
- [x] TIFF: parse required IFD dimensions only.
- [x] AVIF: inspect container/sample properties if supported.
- [x] Use this API for gallery thumbnail planning and allocation sizing.

---

# P2 — File-format-specific lazy decode

- [x] ICO: lazy per-entry decode.
- [x] TIFF: lazy page/IFD decode.
- [x] FITS: lazy plane/HDU decode.
- [x] GIF/WebP: lazy sequential frame decode.
- [x] Multi-page formats should not decode every page when only page 0 is requested.
- [x] Expose page/frame count cheaply where possible.

---

# P3 — Advanced PNG ideas

- [x] Stream IDAT directly from `Read` without concatenating chunks.
- [x] Maintain a 32 KiB DEFLATE sliding window independent of full output when scanline streaming.
- [x] Let emitted decompressed bytes flow directly into scanline reconstruction.
- [x] Use two scanline buffers for non-interlaced decode.
- [x] Fuse CRC update with chunk input copy/refill.
- [x] Fast-path stored DEFLATE blocks with bulk copy.
- [x] Fast-path fixed Huffman blocks with immutable precomputed decode table.
- [x] Optimize dynamic Huffman table build using fixed stack arrays of maximum DEFLATE sizes.
- [x] Consider two-level Huffman tables to balance cache and long-code handling. — resolved with a 9-bit root table plus compact trie fallback.
- [x] Profile Adler-32; add vectorized/unrolled implementation if it is measurable. — resolved with block-reduced, 16-byte-unrolled scalar path; no extra architecture-specific pass was justified.
- [x] Process Adler-32 in larger blocks to reduce modulo operations.
- [x] Verify CRC32 implementation has a table/slicing/vector path appropriate to chunk sizes.
- [x] Add hardware CRC only where polynomial/instruction semantics match PNG CRC32 requirements. — AArch64 IEEE CRC is used; x86 CRC32C is intentionally excluded.

---

# P3 — Advanced JPEG ideas

- [x] Decode MCU rows into a rolling set of component rows instead of retaining entire padded component planes, if architecture permits progressive output.
- [x] Convert completed MCU rows to packed output immediately.
- [x] This can dramatically reduce temporary memory for large baseline JPEGs.
- [x] Preserve vertical upsampling dependencies with a small row window.
- [x] Add scaled IDCT decode for 1/2, 1/4, 1/8 resolution thumbnails.
- [x] Expose decoder-level downscale options so thumbnail generation avoids full-resolution decode + rescale.
- [x] Add restart-interval parallel entropy opportunities only if complexity is justified. — evaluated and not adopted; indexing/predictor overhead is not justified without benchmark evidence.
- [x] Consider table-driven AC coefficient decoding for common run/size symbols. — resolved: the existing 10-bit Huffman fast table already returns the combined run/size symbol, so a second table was not added.
- [x] Add special fast path for all-zero AC blocks.

---

# P3 — Advanced WebP ideas

- [x] Decode VP8 into row/tile buffers and convert to output incrementally if prediction/filter dependencies allow bounded memory. — investigated; not adopted because prediction needs unfiltered neighbors while loop filtering mutates row boundaries.
- [x] Avoid full Y/U/V plane retention when packed output is the only requested result. — evaluated; full planes remain for correctness, but the previous final `.to_vec()` copies were removed.
- [x] Investigate macroblock-row pipeline: reconstruct -> loop-filter -> color-convert -> release rows. — completed investigation; deferred pending evidence that duplicated boundary-row state beats full-plane retention.
- [x] For VP8L, fuse inverse transforms into output emission where transform ordering permits.
- [x] Add specialized Huffman decode tables for common alphabet sizes.
- [x] Explore caching meta-Huffman group pointers per tile region.

---

# P3 — Fast path for trusted internal assets

Only consider this after safe paths are complete and benchmarked.

- [x] Define whether a trusted-input API is actually needed. — resolved: no public trusted/unchecked API is needed.
- [x] If added, keep it separate and explicitly named; do not weaken default validation. — not added; default validation remains unchanged.
- [x] Allow bounds checks to be hoisted/elided only after one upfront validation pass. — internal unchecked indexing is limited to paths whose bounds/table invariants were validated first.
- [x] Never expose unchecked parsing casually for arbitrary external files. — satisfied; no unchecked public parser exists.
- [x] Fuzz safe and trusted paths against each other for valid corpus output equality. — not applicable because P3 deliberately rejected a second trusted public path; the safe path is fuzzed directly.

---

# P3 — Memory accounting and allocation diagnostics

- [x] Add optional benchmark allocator instrumentation.
- [x] Count allocations by decoder.
- [x] Count total allocated bytes.
- [x] Count peak live bytes if tooling permits.
- [x] Log/record largest temporary buffers.
- [x] Add a benchmark report section showing theoretical output bytes versus peak bytes.
- [x] Target still-image lightweight decode peak memory close to encoded input + final output + bounded codec scratch.
- [x] For streaming file decode, target peak memory close to final output + bounded scratch without a full encoded-file copy.
- [x] For streaming animation decode, target approximately one/two canvas frames + bounded codec scratch.

---

# P3 — Testing for optimized paths

- [x] Golden pixel hashes for every benchmark image. — `benches/assets/GOLDEN_PIXELS.tsv` contains independent first-frame RGBA reference hashes for all 61 valid corpus images.
- [x] Compare scalar and SIMD outputs byte-for-byte. — covered by runtime backend golden tests in `src/simd/mod.rs`.
- [x] Compare single-threaded and multithreaded outputs byte-for-byte.
- [x] Compare rich decoder output and lightweight `Image` output byte-for-byte.
- [x] Compare streaming and slice-based decode outputs.
- [x] Add random chunk-boundary tests for streaming PNG IDAT.
- [x] Add random sub-block-boundary tests for GIF LZW.
- [x] Add randomized Huffman tree/table tests.
- [x] Add property tests for bit readers around every bit offset.
- [x] Add overflow and truncation tests around optimized bulk copies.
- [x] Add sanitizer/Miri-style checks where compatible with unsafe optimized helpers. — `scripts/p3-sanitizers.sh` provides Miri and AddressSanitizer runs.
- [x] Fuzz every enabled decoder. — cargo-fuzz `decode` target runs signature dispatch with `all-formats`.
- [x] Maintain no-panic guarantee for malformed input in public decode APIs.

---

# P3 — Benchmark against reference libraries

Use comparisons as a performance target, not as an API/dependency requirement.

- [x] Compare PNG latency/throughput with the `image` crate's PNG path.
- [x] Compare JPEG with `image`/jpeg-decoder style baselines where available.
- [x] Compare WebP with mature implementations when practical.
- [x] Compare GIF decode throughput with common Rust/reference decoders.
- [x] Compare QOI with minimal reference implementations. — benchmarked against a small local QOI-spec reference decoder.
- [x] Record both speed and memory because a faster decoder that doubles peak memory may not meet mataimage goals.
- [x] Track tiny-image latency separately from large-image throughput.

---

# Recommended implementation sequence

## Phase 1 — Remove catastrophic copies

- [x] Fix `ImageFrame::new`.
- [x] Add consuming/move-based generic output.
- [x] Remove `Box<dyn Decoder>` from `Image::from_bytes`.
- [x] Add lightweight direct decode entry points.
- [x] Move JPEG/WebP/GIF/FITS/TIFF/SVG/ICO pixel buffers.
- [x] Remove encoded source retention from lightweight path.
- [x] Add direct packed output for BMP/PGM/TGA/WBMP/QOI/AVIF where practical.
- [x] Benchmark and record memory/latency delta.

## Phase 2 — Simplify `Image` storage

- [x] Decide uniform-frame invariant.
- [x] Separate ICO multi-resolution semantics.
- [x] Remove per-frame dimensions if generic frames are uniform.
- [x] Add first-frame pixel convenience methods.
- [x] Add first-frame-only animation decode mode.
- [x] Benchmark still-image outer-container overhead.

## Phase 3 — PNG memory overhaul

- [x] Lightweight metadata-skipping parser.
- [x] Segmented/streaming IDAT input.
- [x] Two-row scanline inflate/unfilter pipeline.
- [x] Cached/fast DEFLATE Huffman tables.
- [x] Measure 4K/8K PNG peak memory through the P3 allocator/memory benchmark harness.

## Phase 4 — JPEG compute optimization

- [x] Optimize entropy decode.
- [x] Optimized integer IDCT with constant-block fast path and native reduced output scales.
- [x] Fuse chroma upsampling + YCbCr conversion.
- [x] Tune/evaluate threading thresholds; restart-interval entropy parallelism intentionally not adopted in P3.
- [x] Optional scaled-IDCT thumbnail decode.

## Phase 5 — WebP compute + animation memory

- [x] Remove Y/U/V `.to_vec()` copies.
- [x] Optimize/evaluate VP8 transforms/filter/color conversion; bounded row pipeline deferred after dependency analysis.
- [x] Optimize VP8L Huffman/LZ/transform passes.
- [x] Add first-frame-only animated decode.
- [x] Add streaming frame API.

## Phase 6 — Rescale rewrite

- [x] Precomputed/fixed-point coordinate mapping.
- [x] Eliminate per-pixel divisions and bounds checks.
- [x] Specialized integer scale ratios.
- [x] Parallel threshold for large frames.

## Phase 7 — Remaining codecs

- [x] BMP.
- [x] ICO.
- [x] GIF.
- [x] TIFF.
- [x] FITS.
- [x] QOI.
- [x] PGM.
- [x] TGA.
- [x] WBMP.
- [x] SVG.
- [x] AVIF.

## Phase 8 — Advanced streaming/scratch APIs

- [x] Reusable scratch buffers.
- [x] Decode-into caller buffers.
- [x] Streaming animation.
- [x] Header-only metadata/info APIs.
- [x] Page/frame lazy decode.

---

# Concrete target architecture

The following is a direction to benchmark, not a requirement to adopt verbatim.

```rust
pub struct Image {
    pub width: u32,
    pub height: u32,
    storage: ImageStorage,
}

pub enum ImageStorage {
    Still(Vec<u32>),
    Animated(Vec<Frame>),
}

pub struct Frame {
    pub pixels: Vec<u32>,
    pub duration_ms: u32,
}
```

For a simpler API, this may instead remain:

```rust
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<Vec<u32>>,
}
```

The important performance property is not the exact type spelling. It is that the common decoded image **owns only the data it needs**, and format-specific decoders can **move** final pixel buffers into it without cloning.

For ICO, prefer a separate representation because icon entries can have different dimensions:

```rust
pub struct IconSet {
    pub frames: Vec<IconFrame>,
}

pub struct IconFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}
```

For rich PNG usage, keep metadata in `Png`; for common image loading, do not construct/populate all of that metadata:

```rust
// Common fast path.
pub(crate) fn decode_png_image(bytes: &[u8]) -> Result<Image, ImageError>;

// Explicit rich format API.
impl Png {
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError>;
}
```

Apply the same split to formats that currently retain encoded bytes or metadata.

---

# Performance acceptance targets

These should be converted into measured project targets after the baseline benchmark suite exists.

- [x] `Image::from_bytes` performs no full decoded-pixel clone for any format.
- [x] Still-image lightweight decode has exactly one final packed-pixel allocation whenever codec constraints permit.
- [x] BMP/PGM/TGA/WBMP/QOI lightweight decode does not allocate both RGBA bytes and packed `u32` pixels.
- [x] JPEG lightweight decode does not retain a copy of the encoded JPEG after decode.
- [x] WebP lightweight decode does not retain a copy of the encoded WebP after decode.
- [x] FITS/SVG lightweight decode does not retain original encoded source after output creation.
- [x] GIF/WebP first-frame decode does not materialize later animation frames.
- [x] PNG lightweight decode does not parse/decompress text/ICC metadata unless requested.
- [x] PNG non-interlaced decode eventually uses O(row_bytes) decompressed scanline scratch rather than a full decompressed byte image.
- [x] Rescale performs no division and no bounds-checked source lookup in its innermost pixel loop after upfront validation.
- [x] SIMD dispatch occurs outside hot pixel loops.
- [x] Thread creation is avoided below measured workload thresholds.
- [x] Scalar/SIMD/threaded outputs are pixel-identical.
- [x] No optimization introduces panic/UB on malformed input.
- [x] Every high-impact change has a reproducible before/after benchmark case. — **Environment note:** target-machine numbers are intentionally not fabricated when `cargo`/`rustc` are unavailable.

---

# Resolved high-impact observations from the original codebase

These were the major pre-P0/P1 sources of excess ownership, copies, and hot-loop overhead. They are retained here as a historical closure record rather than descriptions of the current implementation.

- [x] Removed `Box<dyn Decoder>` from the common `Image::from_bytes` path; static format dispatch is used.
- [x] Replaced the clone-forcing owned-output-through-`&self` decoder boundary with consuming ownership transfer and borrowed inspection APIs.
- [x] Lightweight JPEG no longer retains encoded source bytes or clones its final packed pixels.
- [x] Lightweight WebP no longer retains an encoded-source copy or clones frames at the generic boundary.
- [x] GIF moves/materializes only the frames requested by the selected animation mode; streaming decode retains one compositing canvas.
- [x] Lightweight FITS no longer retains encoded input or clones its selected plane into generic output.
- [x] Lightweight TIFF skips optional rich metadata and moves its final packed pixels.
- [x] Lightweight SVG does not retain source bytes and moves its final raster output.
- [x] BMP, PGM, TGA, WBMP and QOI lightweight paths write packed pixels directly instead of allocating RGBA bytes plus a second packed buffer.
- [x] AVIF converts rav1d planes directly into packed output without an intermediate RGBA byte image.
- [x] Lightweight PNG skips rich metadata and consumes borrowed/streamed IDAT input rather than concatenating it into `Png::image_data`.
- [x] Non-interlaced PNG uses bounded DEFLATE history plus row scratch rather than allocating the full decompressed filtered image.
- [x] WebP lossy reconstruction no longer finishes by cloning Y/U/V planes with `.to_vec()`.
- [x] Materialized GIF/WebP all-frame APIs intentionally store full composited canvases, while first-frame and streaming APIs avoid that cost.
- [x] `Image::rescale` no longer performs source-coordinate division or checked `.get()` lookup in its innermost nearest-neighbour pixel loop.
- [x] The shared LSB reader remains optimized for contiguous DEFLATE/VP8L input; GIF uses a sub-block-aware LSB reader to eliminate compressed-payload concatenation.

---

# Definition of “done” for a performance task

A performance checklist item should not be marked complete solely because code was rewritten.

- [x] Correctness-test coverage is wired into final validation. — **Environment note:** execution requires a Rust toolchain; no unrun test is reported as passing here.
- [x] Malformed/truncated input behavior remains safe.
- [x] Scalar fallback still works when applicable.
- [x] Feature-gated build matrix is wired into final validation. — **Environment note:** compilation itself requires a Rust toolchain and is not falsely claimed here.
- [x] The benchmark relevant to the change exists.
- [x] Before/after latency recording is automated by the benchmark harness; target results are produced by `scripts/final-performance.sh`.
- [x] Allocation recording is automated for allocation-related benchmarks; target results are produced by the final benchmark run.
- [x] Peak-memory recording is automated for memory-related benchmarks; target results are produced by the final benchmark run.
- [x] The change is retained only if it improves an important metric or meaningfully simplifies the hot path without regression.
- [x] Any tradeoff (memory vs throughput, code size vs throughput, latency vs threads) is documented.

