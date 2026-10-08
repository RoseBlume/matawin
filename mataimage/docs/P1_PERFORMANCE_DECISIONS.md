# P1 performance decisions

This file records the P1 items that required a design decision rather than a
blind code change. In `todo.md`, a checked P1 item means one of three things:

1. the optimization was implemented;
2. the benchmark/test/profiling instrumentation needed to evaluate it was added;
3. the proposed optimization was evaluated and deliberately not adopted in P1.

Hardware-dependent numbers are intentionally not invented. Run the checked-in
benchmark and profiling scripts on the target x86-64/AArch64 systems before
changing thresholds or choosing one ISA-specific implementation over another.

## Generic image representation

The generic `Image` remains a small decoded-data object: canvas dimensions,
primary packed pixels, and additional full-canvas animation frames. It does not
contain the rich PNG/JPEG/WebP/GIF decoder object or its format metadata.
Still images leave the additional-frame `Vec` empty, so there is no backing
allocation for a one-element outer frame collection.

An `ImageStorage` enum was evaluated but not adopted in P1. The current layout
already has a zero-allocation still-image frame list, keeps field access simple,
and avoids introducing a branch into common callers. The benchmark suite prints
`size_of::<Image>()`, `ImageFrame`, `Vec<u32>`, and `Box<[u32]>` so this decision
can be revisited with measurements.

`Vec<u32>` is retained instead of `Box<[u32]>`. Capacity reuse is useful for
`rescale_into`, animations, and caller-owned buffers; converting to boxed slices
would trade away reuse for a small capacity-field/storage benefit. P1 also does
not call `shrink_to_fit` in decode hot paths because that can add a full copy.

## Output allocation safety

Fixed-size outputs use safe initialized vectors and direct indexed/slice writes.
P1 evaluated `Vec::with_capacity` + `set_len`/uninitialized output but did not
adopt it. The zero-initialization cost must first show up materially in release
profiles, and avoiding a broad unsafe initialization surface is preferable.
Because the unsafe path was rejected, there is no `set_len` initialization
invariant requiring debug assertions.

JPEG coefficient blocks are fixed `[i32; 64]` stack values rather than heap
vectors. DEFLATE code-length/count scratch is fixed-size stack storage. GIF now
keeps one reusable 4096-entry LZW scratch block for the whole animation and one
reusable disposal-method-3 canvas backup instead of recreating/allocating them
per frame. WebP transform maps remain owned only as long as their inverse
transform needs them; a general scratch-pool abstraction was rejected for P1
because it would increase state/complexity without a demonstrated allocation
hot spot.

## I/O and streaming

`Image::from_bytes` remains the zero-copy encoded-input baseline. `Image::load`,
`fs::read + from_bytes`, and `BufReader + from_reader` are all represented in the
P1 benchmark suite so file-I/O overhead can be separated from decode cost.

A generic `Read` API cannot become truly streaming while the format decoders are
slice-based, so `from_reader` currently owns one encoded buffer. P1 deliberately
does not add a second decoder-internal copy. True incremental readers for PNG
chunk streams, JPEG entropy segments, GIF sub-blocks, and similar formats are
left as codec-specific follow-up work because they change parser architecture,
not just performance plumbing. Memory mapping remains unnecessary and optional.

## Pixel representation

Generic pixels remain numeric `0xRRGGBBAA` `u32` values. This gives one internal
layout, direct per-pixel indexing, and efficient `Vec<u32>` ownership. It is not
claimed that the in-memory byte order is RGBA on every host. If byte-oriented
consumers become dominant, the benchmark suite's conversion scenario provides a
place to compare a byte-backed representation before changing the public model.

Existing SIMD conversion helpers are retained for conversions that still exist
(gray/RGB/RGBA/BGRA/planar RGB and ARGB rotation). New conversion SIMD is not
added merely to preserve an intermediate representation that direct packed
output has already removed.

## Nearest-neighbour rescaling

P1 implements:

- early 1:1 return;
- one-time source validation;
- destination pre-sizing and capacity reuse;
- precomputed X mapping for generic ratios;
- exact quotient/remainder (Bresenham-style) X/Y stepping with no per-pixel division;
- exact integer upscale run/row replication;
- exact integer downscale source stepping;
- in-place pure shrinking, including non-integer shrink ratios;
- `rescale_into` for caller-managed reusable storage;
- independent-row scoped threading above a centralized work threshold.

Gather SIMD was evaluated and not adopted in P1. Nearest-neighbour source
indices are irregular for generic ratios, ISA gather availability differs, and
current scalar loops already perform contiguous output writes. Bilinear/Lanczos
are quality modes, not optimizations of nearest-neighbour, so they are explicitly
outside this P1 fast path.

## Shared LSB bit reader

The shared reader now uses a `u64` accumulator, eight-byte refill when empty,
four-byte refill otherwise, and a const-generic fast peek for fixed Huffman root
widths. It stores a byte cursor plus buffered-bit count instead of a redundant
logical bit cursor/end pointer. The current `usize` cursor matches slice indexing
and avoids conversions at every byte access.

A deterministic property-style test exercises many source lengths/read widths
across refill boundaries. `scripts/asm-bench.sh` emits release assembly for
manual inspection of `read_bit`, `read_bits`, and Huffman call sites. Four- vs
eight-byte refill performance, `usize` vs `u32`, and exact inlining choices are
hardware/compiler questions; the benchmark/assembly machinery is the P1 result,
not a fabricated universal winner.

## Huffman representation

PNG DEFLATE uses a 9-bit root table with compact `u16` fallback nodes. Fixed
DEFLATE tables are cached in `OnceLock`. Oversubscribed and incomplete/missing
branch behavior has regression coverage. WebP VP8L already uses an 8-bit root
lookup plus compact flat `u16` trie. These choices intentionally keep fallback
storage small while resolving common short codes in one lookup.

P1 retains `Vec` storage for fallback nodes because final trie node count is
known only after code assignment and construction is not the dominant decode
loop. Converting to a boxed slice would add a post-build allocation/copy; a
pre-sized maximum array would increase stack/cache footprint. The benchmark
suite includes Huffman-construction and symbol-heavy stage proxies, while Linux
`perf` can provide cache-miss data.

## SIMD policy

Runtime dispatch remains cached once per process. PNG already passes its local
`PngSimd` table through high-frequency scanline conversion code. Shared pixel
SIMD covers the portable conversions that have clear independent lanes and both
x86 and ARM backends already provide meaningful implementations.

P1 does **not** add speculative unsafe SIMD for dependency-heavy operations such
as PNG Average/Paeth, JPEG IDCT/chroma fusion, VP8 prediction/inverse transforms,
VP8 loop filtering, or animation blending without measurements from the target
architectures. Those operations stay on the codec-specific roadmap. This is a
resolved P1 decision: portable scalar correctness plus benchmark/profile hooks
comes before adding architecture-specific code. `tests` contains golden checks
that runtime-dispatched shared SIMD produces the same results as scalar code.

`-C target-cpu=native` is supported as an opt-in benchmark comparison in
`scripts/compare-bench.sh`; it is not made the portable crate default. AVX2 vs
SSE/SSSE3, unaligned loads, NEON parity, and RISC-V vector work remain measurable
backend-specific follow-ups rather than assumed wins.

## Threading policy

Available parallelism is cached once. Work thresholds are centralized in
`src/performance.rs`, and small operations remain single-threaded. JPEG color
conversion and very large nearest-neighbour rescaling are the first independent
row-oriented stages to use scoped workers. The benchmark suite includes explicit
1/2/4/8 max-thread JPEG runs plus isolated scoped-thread creation overhead, and
`scripts/thread-bench.sh` can constrain the process to 1/2/4/8 logical CPUs on
Linux.

A persistent internal worker pool was evaluated and rejected for P1. It would
add synchronization/lifetime complexity and potentially a dependency merely to
avoid scoped-thread creation in workloads that are already threshold-gated.
Likewise, JPEG entropy/MCU reconstruction, WebP prediction reconstruction, and
animation-frame decoding are not parallelized across dependency boundaries.
Those can be revisited only where the bitstream/compositing dependency graph
proves independence.

## P1 measurement commands

Use these on the actual target machines:

```sh
cargo bench --bench decode
scripts/compare-bench.sh
scripts/profile-bench.sh
scripts/thread-bench.sh
scripts/asm-bench.sh
```

Optional native-code comparison:

```sh
MATAIMAGE_NATIVE=1 scripts/compare-bench.sh
```

Optional debug sanity baseline:

```sh
MATAIMAGE_DEBUG_BENCH=1 scripts/compare-bench.sh
```
