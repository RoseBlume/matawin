# mataimage P1 benchmark corpus

This directory is a deterministic, redistributable synthetic corpus used by the
P1 performance suite. It intentionally contains no user data or downloaded
images.

Coverage:

- `tiny_rgba.png` — 8×8 fixed-overhead case.
- `icon_rgba.png` — 256×256 icon-sized case.
- `1080p_rgba.png`, `1440p_rgba.png`, `4k_rgba.png`, `8k_rgba.png` — scale cases.
- `png_rgb.png`, `png_rgba.png`, `png_gray.png`, `png_gray_alpha.png`,
  `png_indexed_trns.png`, `png_filter_0.png` … `png_filter_4.png`,
  `png_many_idat.png`, `png_noise.png` — PNG color/filter/chunk/entropy cases.
- `jpeg_gray.jpg`, `jpeg_444.jpg`, `jpeg_422.jpg`, `jpeg_420.jpg`,
  `jpeg_noise_420.jpg` — JPEG sampling and entropy cases.
- `webp_vp8.webp`, `webp_vp8l.webp`, `webp_alpha.webp`,
  `webp_animated.webp`, `webp_120_frames.webp` — WebP lossy/lossless/alpha and
  short/100+ frame animation cases.
- `gif_small.gif`, `gif_large.gif`, `gif_120_frames.gif`, `gif_disposal_0.gif` …
  `gif_disposal_3.gif` — GIF LZW/compositing/disposal and 100+ frame cases.
- `bmp_rgb.bmp`, `bmp_rgba.bmp`, `bmp_rle4.bmp`, `bmp_rle8.bmp`,
  `bmp_bitfields_565.bmp` — BMP common/RLE/bitfield cases.
- `ico_many.ico` — multi-resolution icon container.
- `ico_dib_24.ico` — tiny 24-bit DIB-backed ICO used to cover the direct no-synthetic-BMP path.
- `tiff_rgb.tiff`, `tiff_packbits.tiff`, `tiff_gray.tiff`,
  `tiff_palette.tiff` — representative TIFF paths.
- `fits_u8.fits`, `fits_i16.fits`, `fits_i32.fits`, `fits_f32.fits`,
  `fits_f64.fits` — multiple FITS BITPIX/sample types.
- `qoi_rgba.qoi`, `pgm_p2.pgm`, `pgm_p5.pgm`, `tga_rgb.tga`,
  `tga_rle.tga`, `wbmp.wbmp` — smaller format coverage.
- `svg_simple.svg`, `svg_complex.svg` — SVG fixed-overhead and raster-heavy cases.
- `malformed/` — deliberately truncated/random inputs for error-path profiling.

The large images are deliberately highly compressible so the repository does
not balloon in size. `png_noise.png` and `jpeg_noise_420.jpg` provide separate
high-entropy cases.

## P3 golden references

`GOLDEN_PIXELS.tsv` records an independent first-frame RGBA FNV-1a hash for every
valid benchmark image. These are diagnostic references; lossy JPEG/WebP decoders
can legitimately differ by small rounding choices while remaining correct.
