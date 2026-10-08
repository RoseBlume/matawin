# P3 performance implementation and decisions

P3 combines implemented hot-path changes with several explicitly investigative checklist items. A checked investigative item means it was resolved with an engineering decision; it does **not** mean every speculative optimization was adopted.

## PNG streaming and DEFLATE

`Image::load`/`Image::from_reader` now special-case PNG after an eight-byte prefix and pass the original `Read` source into a streaming PNG parser. Consecutive IDAT chunks are exposed as one logical zlib stream without concatenating their payloads. IDAT CRC state is updated while bytes are copied into the inflater refill buffer. The zlib wrapper keeps only the four-byte Adler trailer back, and DEFLATE keeps a 32 KiB history window plus a scanline-sized chunk.

Non-interlaced PNG decode uses the emitted scanline chunk plus one previous-row buffer. Inflated data therefore flows directly through unfiltering and packed-pixel conversion. The slice API remains available for callers that already own encoded bytes.

The DEFLATE core is shared by slice-backed and `Read`-backed bit readers. Stored blocks use bulk writes, fixed trees are process-wide immutable `OnceLock` tables, dynamic code lengths use RFC-bounded stack arrays, and the Huffman decoder uses a 9-bit root table with a compact trie fallback. This is the selected two-level table design: a larger root would increase cache footprint for relatively few long codes.

Adler-32 processes RFC-recommended bounded blocks and unrolls the byte loop before modulo reduction. PNG CRC32 uses slicing-by-8 in the portable path. AArch64's matching IEEE CRC extension is used when available; x86 SSE4.2 CRC is deliberately **not** used because it implements CRC32C/Castagnoli rather than PNG's IEEE polynomial.

## JPEG

Baseline JPEG component storage is now bounded to one MCU row per component instead of complete padded component planes. Each completed MCU row is chroma-upsampled/color-converted directly into the final packed output and then reused for the next entropy row. Baseline spatial reconstruction has no dependency on earlier MCU rows, so no historical component plane is required.

`JpegScale::{Full, Half, Quarter, Eighth}` and `Jpeg::from_bytes_scaled` expose native decoder-level thumbnail sizing. Generic `DecodeOptions::output_dimensions` chooses a 1/2, 1/4, or 1/8 JPEG reconstruction size before the optional exact final resize, avoiding a full-resolution packed image and full-resolution component storage.

The existing 10-bit Huffman front table already makes common AC/DC symbol decode table-driven. The all-zero-AC path is propagated from entropy decode into IDCT so constant blocks avoid scanning and full transform work.

Restart-interval parallel entropy decode was evaluated but not adopted in P3. Correct partitioning requires first indexing restart segments and maintaining independent predictor/marker state. For ordinary files, segment size is often too small to amortize that machinery. The benchmark suite retains thread-scaling measurements so this decision can be revisited with data.

A second run/size lookup table after the existing Huffman table was also not added: the current direct Huffman entry already returns the combined AC run/size byte, so another table would add memory traffic without eliminating entropy reads.

## WebP

Lossy VP8 no longer clones the reconstructed Y/U/V planes when returning from the VP8 core. A full macroblock-row prediction/filter pipeline was investigated. VP8 intra prediction requires unfiltered reconstructed neighbors while loop filtering mutates block edges; releasing rows safely therefore requires a duplicated boundary window and a substantial filter rewrite. P3 leaves the now-copy-free full planes in place until profiling shows that complexity is justified.

VP8L Huffman tables now use the actual root width for short alphabets. Meta-Huffman group selection caches the active meta tile/group rather than reloading it for every symbol in a tile. When subtract-green is the final inverse transform, it is fused with ARGB-to-RRGGBBAA conversion, removing one complete pixel-buffer pass.

## Trusted-input fast path

P3 intentionally does **not** add a public unchecked/trusted decoder. The default public APIs keep full validation. Internal unsafe indexing is used only after checked dimensions/table construction has established the required invariants. This provides most bounds-check-hoisting benefit without creating a second externally reachable parser whose safety contract would need separate maintenance and fuzzing. Because no trusted public path exists, safe-vs-trusted differential fuzzing is not applicable.

## Allocation and peak-memory diagnostics

The benchmark allocator records allocation count, total allocated bytes, current/peak live bytes, and the largest individual allocation. End-to-end rows report theoretical packed output bytes and peak/output ratio. `scripts/p3-memory.sh` focuses on large streaming PNGs and 120-frame GIF/WebP cases.

Targets are architectural rather than fabricated measurements: still-image lightweight decode should approach final packed output + bounded codec scratch; streaming PNG should additionally avoid a complete encoded-file copy; streaming animation should stay near one/two canvas frames + bounded codec scratch.

## Correctness and comparison infrastructure

P3 adds randomized PNG `Read` chunk-boundary tests, GIF image-data sub-block repartition tests, all-bit-offset bit-reader properties, randomized Huffman construction tests, optimized bulk-copy truncation/overflow tests, JPEG scale tests, rich-vs-lightweight comparisons, and malformed-input no-panic checks.

`benches/assets/GOLDEN_PIXELS.tsv` contains first-frame RGBA reference hashes for every valid benchmark image. The references are generated independently (Pillow for supported raster formats, CairoSVG for SVG, and a tiny WBMP reference decoder). Lossy/reference implementations may differ by rounding, so these hashes are reference diagnostics rather than an assertion that all independent decoders must be bit-identical.

`src/simd` already contains byte-for-byte runtime SIMD-vs-scalar golden tests. `scripts/p3-sanitizers.sh` provides Miri, AddressSanitizer, and cargo-fuzz entry points. The fuzz target invokes both full decode and header-only inspection with all formats enabled.

The benchmark executable now compares mataimage and the `image` crate on representative PNG, JPEG, WebP, and GIF inputs via the `image` crate plus QOI via a tiny local spec reference in the same process, reporting both time and allocator peak/largest-allocation values. Tiny PNG is tracked separately from large-image throughput.
