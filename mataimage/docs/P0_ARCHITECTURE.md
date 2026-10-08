# P0 ownership and output architecture

P0 separates the common renderable image representation from format-specific
containers and metadata. The generic path is intentionally biased toward low
peak memory and ownership transfer rather than preserving every property of the
source file.

## `Image` contract

`Image` contains only:

- one canvas `width` and `height`;
- one primary packed `Vec<u32>` in logical `0xRRGGBBAA` order;
- zero or more additional full-canvas animation frames.

Every generic animation frame is fully composited and has exactly
`width * height` pixels. `ImageFrame` therefore contains only its pixel buffer;
per-frame dimensions are deliberately absent. Still images do not allocate a
one-element outer frame vector: the primary pixels live directly in
`Image::pixels`, while `Image::frames` stores only frames after the first.

The common representation deliberately does **not** retain source compression,
palettes, text chunks, ICC profiles, EXIF/XMP, original encoded bytes, software
strings, or other format-specific metadata. It also does not currently carry
animation timing or loop count. Callers that need those properties should use
`Gif`, `WebP`, `Png`, `Jpeg`, `TiffImage`, `Fits`, `Svg`, `Ico`/`IconSet`, or the
other explicit format APIs.

A separate `ImageStorage::{Still, Animated}` enum was evaluated but not adopted.
The current primary-buffer-plus-additional-frames layout already gives the main
benefit: a still image has no outer frame backing allocation. Keeping the fields
flat also preserves a simpler public API and avoids an extra branch on every
primary-pixel access.

## Generic decode path

`Image::from_bytes[_with_options]` performs static `ImageFormat` dispatch to one
internal `decode_image` function per enabled format. It never constructs
`Box<dyn Decoder>` and it does not retain a rich decoder after output is built.

The lightweight entry points are:

- PNG, JPEG, WebP, GIF, BMP;
- PGM, TGA, WBMP, QOI;
- FITS, TIFF, ICO, SVG, and AVIF/AV1 when their features are enabled.

Where practical they write the final packed `u32` representation directly.
PGM, TGA, WBMP, QOI, BMP and AVIF/AV1 no longer require an RGBA8-to-packed
second full-image conversion on the generic path. JPEG, GIF, WebP, FITS, TIFF,
SVG and ICO move their final packed allocation into `Image`.

PNG is special because its public rich object intentionally represents parsed
chunks plus compressed IDAT data. Its generic path constructs only the
pixel-affecting state and discards it after one pixel decode. Ancillary text,
ICC, physical and similar metadata is not allocated by the generic path.

TIFF's generic path uses a pixels-only parse policy. It still reads tags needed
for dimensions, strips, compression, color interpretation, palettes and alpha,
but skips optional Software, DateTime, Artist, Copyright, X/YResolution,
FreeOffsets and FreeByteCounts payloads.

## `Decoder` trait

The old `Decoder::pixels(&self) -> Vec<ImageFrame>` API was removed because an
owned return value from a shared borrow forced rich decoders such as JPEG, GIF,
WebP, FITS, TIFF, SVG and ICO to clone full pixel buffers.

The polymorphic API now uses:

```text
fn into_image(self: Box<Self>) -> Result<Image, ImageError>
```

This consumes the rich decoder so owned output can be transferred. The common
`Image::from_bytes` path is still faster/simpler because it avoids trait objects
entirely.

Rich decoders expose borrowed and/or consuming accessors where their storage
makes that useful, such as `pixels_ref`, `frames_ref`, `into_pixels`,
`into_frames`, or `into_rgba8`.

## ICO is not animation

ICO entries are resolutions, not temporal frames. `Image::from_bytes` selects
one preferred ICO entry:

1. an explicitly requested `DecodeOptions::preferred_icon_size` when possible;
2. otherwise the largest/highest-quality directory entry.

`IconSet` is the explicit multi-resolution API. Every `IcoFrame` keeps its own
width and height. Generic `Image` never exposes the other ICO resolutions as
animation frames.

## Source retention

The lightweight path never retains the encoded input after decode.

Some rich APIs intentionally retain encoded source data because it is part of
their public representation: JPEG, WebP, FITS and SVG. Their owned constructors
(`from_bytes`/`parse_owned`) make this behavior explicit and avoid an extra copy
when the caller already owns the encoded `Vec<u8>`. Borrowed constructors copy
source bytes only when the rich object requires ownership.

A `retain_source: bool` switch was deliberately not added. There are already two
clear use cases with different return types: `Image::from_bytes` for lightweight
renderable output and the rich format constructors for metadata/source-aware
work. A flag would make one type have two substantially different ownership
profiles.

## Invariants and limits

All common constructors validate pixel counts against `width * height` and the
configured decode limits before returning. Debug/test builds additionally assert
that the primary and every additional generic frame exactly match the full
canvas. Checked arithmetic protects pixel counts, RGBA byte counts, DIB row
strides and frame limits.

QOI's public `data` buffer is verified by regression tests to contain decoded
RGBA8 bytes, not the encoded opcode stream. Zero-dimension rejection is covered
for representative formats that forbid it, while a zero-sized generic canvas
remains representable as an internal data structure.

## Memory measurement

`benches/decode.rs` records allocation count, total allocated bytes, temporary
bytes, peak live heap and Linux `VmHWM` when available. P0 adds deterministic
120-frame GIF and WebP cases and `scripts/p0-memory.sh`, which runs the focused
4K, 8K, 120-frame GIF and 120-frame WebP memory sweep.

Machine-specific before/after numbers are intentionally not committed as if they
were universal. Run the script on the same compiler/profile/machine before and
after a change and retain that output with the benchmark record.
