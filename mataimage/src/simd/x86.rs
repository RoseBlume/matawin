//! x86/x86_64 SIMD implementations.
//!
//! Public functions in this module perform runtime CPU feature detection.
//! The actual implementations are guarded by `#[target_feature]`, ensuring
//! that AVX2/SSE2 instructions are only executed on CPUs that support them.

// use std::arch::x86_64::{
//     __m128i,
//     _mm_add_epi8,
//     _mm_loadu_si128,
//     _mm_storeu_si128,
//     _mm_set_epi8,
//     _mm_shuffle_epi8
// };

// #[cfg(target_arch = "x86")]
// use std::arch::x86::{
//     __m128i,
//     _mm_add_epi8,
//     _mm_loadu_si128,
//     _mm_storeu_si128,
//     _mm_set_epi8,
//     _mm_shuffle_epi8
// };

use super::{
    PixelSimd,
    PngSimd,
    scalar_argb_rotate,
    scalar_bgra8,
    scalar_gray8,
    scalar_planar_rgb8,
    scalar_rgb8,
    scalar_rgb8_argb,
    scalar_rgba8,
    scalar_rgba8_argb,
    scalar_sub,
    scalar_up,
};

use std::sync::OnceLock;

#[cfg(target_arch = "x86")]
use std::arch::x86::*;

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum X86Level {
    Scalar,
    Sse2,
    Ssse3,
    Avx2,
}

static LEVEL: OnceLock<X86Level> = OnceLock::new();

#[inline]
fn level() -> X86Level {
    *LEVEL.get_or_init(|| {
        // One feature ladder for the whole image crate. AVX2-capable CPUs also
        // provide the lower levels used by the remaining kernels.
        if std::arch::is_x86_feature_detected!("avx2") {
            X86Level::Avx2
        } else if std::arch::is_x86_feature_detected!("ssse3") {
            X86Level::Ssse3
        } else if std::arch::is_x86_feature_detected!("sse2") {
            X86Level::Sse2
        } else {
            X86Level::Scalar
        }
    })
}

pub(super) fn detect_png() -> PngSimd {
    match level() {
        X86Level::Avx2 => PngSimd {
            up: png_up_avx2,
            sub: png_sub_sse2,
            rgba8: rgba8_to_u32_avx2,
            rgb8: rgb8_to_u32_ssse3,
        },
        X86Level::Ssse3 => PngSimd {
            up: png_up_sse2,
            sub: png_sub_sse2,
            rgba8: rgba8_to_u32_ssse3,
            rgb8: rgb8_to_u32_ssse3,
        },
        X86Level::Sse2 => PngSimd {
            up: png_up_sse2,
            sub: png_sub_sse2,
            rgba8: scalar_rgba8,
            rgb8: scalar_rgb8,
        },
        X86Level::Scalar => PngSimd {
            up: scalar_up,
            sub: scalar_sub,
            rgba8: scalar_rgba8,
            rgb8: scalar_rgb8,
        },
    }
}

pub(super) fn detect_pixels() -> PixelSimd {
    match level() {
        X86Level::Avx2 => PixelSimd {
            rgba8: rgba8_to_u32_avx2,
            _rgb8: rgb8_to_u32_ssse3,
            gray8: gray8_to_u32_avx2,
            bgra8: bgra8_to_u32_ssse3,
            planar_rgb8: planar_rgb8_to_u32_avx2,
            argb_rotate: argb_rotate_avx2,
            _rgb8_argb: rgb8_to_argb_ssse3,
            _rgba8_argb: rgba8_to_argb_ssse3,
        },
        X86Level::Ssse3 => PixelSimd {
            rgba8: rgba8_to_u32_ssse3,
            _rgb8: rgb8_to_u32_ssse3,
            gray8: gray8_to_u32_sse2,
            bgra8: bgra8_to_u32_ssse3,
            planar_rgb8: planar_rgb8_to_u32_sse2,
            argb_rotate: argb_rotate_sse2,
            _rgb8_argb: rgb8_to_argb_ssse3,
            _rgba8_argb: rgba8_to_argb_ssse3,
        },
        X86Level::Sse2 => PixelSimd {
            rgba8: scalar_rgba8,
            _rgb8: scalar_rgb8,
            gray8: gray8_to_u32_sse2,
            bgra8: scalar_bgra8,
            planar_rgb8: planar_rgb8_to_u32_sse2,
            argb_rotate: argb_rotate_sse2,
            _rgb8_argb: scalar_rgb8_argb,
            _rgba8_argb: scalar_rgba8_argb,
        },
        X86Level::Scalar => PixelSimd {
            rgba8: scalar_rgba8,
            _rgb8: scalar_rgb8,
            gray8: scalar_gray8,
            bgra8: scalar_bgra8,
            planar_rgb8: scalar_planar_rgb8,
            argb_rotate: scalar_argb_rotate,
            _rgb8_argb: scalar_rgb8_argb,
            _rgba8_argb: scalar_rgba8_argb,
        },
    }
}

// ---------------------------------------------------------
// PNG Up
// ---------------------------------------------------------

#[target_feature(enable = "avx2")]
/// Reconstructs a PNG Up-filtered scanline with AVX2 vector operations.
unsafe fn png_up_avx2(row: &mut [u8], previous: &[u8]) {
    let len = row.len();

    let dst = row.as_mut_ptr();
    let src = previous.as_ptr();

    let mut i = 0;

    // Process 128 bytes at a time.
    //
    // The unrolling gives the CPU multiple independent additions to execute
    // simultaneously, which improves instruction-level parallelism.
    while i + 128 <= len {
        unsafe {
            let a0 = _mm256_loadu_si256(dst.add(i) as *const __m256i);
            let a1 = _mm256_loadu_si256(dst.add(i + 32) as *const __m256i);
            let a2 = _mm256_loadu_si256(dst.add(i + 64) as *const __m256i);
            let a3 = _mm256_loadu_si256(dst.add(i + 96) as *const __m256i);

            let b0 = _mm256_loadu_si256(src.add(i) as *const __m256i);
            let b1 = _mm256_loadu_si256(src.add(i + 32) as *const __m256i);
            let b2 = _mm256_loadu_si256(src.add(i + 64) as *const __m256i);
            let b3 = _mm256_loadu_si256(src.add(i + 96) as *const __m256i);

            let c0 = _mm256_add_epi8(a0, b0);
            let c1 = _mm256_add_epi8(a1, b1);
            let c2 = _mm256_add_epi8(a2, b2);
            let c3 = _mm256_add_epi8(a3, b3);

            _mm256_storeu_si256(dst.add(i) as *mut __m256i, c0);
            _mm256_storeu_si256(dst.add(i + 32) as *mut __m256i, c1);
            _mm256_storeu_si256(dst.add(i + 64) as *mut __m256i, c2);
            _mm256_storeu_si256(dst.add(i + 96) as *mut __m256i, c3);
        }

        i += 128;
    }

    while i + 32 <= len {
        unsafe {
            let a = _mm256_loadu_si256(dst.add(i) as *const __m256i);
            let b = _mm256_loadu_si256(src.add(i) as *const __m256i);

            let c = _mm256_add_epi8(a, b);

            _mm256_storeu_si256(
                dst.add(i) as *mut __m256i,
                c,
            );
        }

        i += 32;
    }

    while i < len {
        unsafe {
            *dst.add(i) = (*dst.add(i)).wrapping_add(*src.add(i));
        }

        i += 1;
    }
}

#[target_feature(enable = "sse2")]
/// Reconstructs a PNG Up-filtered scanline with SSE2 vector operations.
unsafe fn png_up_sse2(row: &mut [u8], previous: &[u8]) {
    let len = row.len();

    let dst = row.as_mut_ptr();
    let src = previous.as_ptr();

    let mut i = 0;

    while i + 64 <= len {
        unsafe {
            let a0 = _mm_loadu_si128(dst.add(i) as *const __m128i);
            let a1 = _mm_loadu_si128(dst.add(i + 16) as *const __m128i);
            let a2 = _mm_loadu_si128(dst.add(i + 32) as *const __m128i);
            let a3 = _mm_loadu_si128(dst.add(i + 48) as *const __m128i);

            let b0 = _mm_loadu_si128(src.add(i) as *const __m128i);
            let b1 = _mm_loadu_si128(src.add(i + 16) as *const __m128i);
            let b2 = _mm_loadu_si128(src.add(i + 32) as *const __m128i);
            let b3 = _mm_loadu_si128(src.add(i + 48) as *const __m128i);

            _mm_storeu_si128(
                dst.add(i) as *mut __m128i,
                _mm_add_epi8(a0, b0),
            );

            _mm_storeu_si128(
                dst.add(i + 16) as *mut __m128i,
                _mm_add_epi8(a1, b1),
            );

            _mm_storeu_si128(
                dst.add(i + 32) as *mut __m128i,
                _mm_add_epi8(a2, b2),
            );

            _mm_storeu_si128(
                dst.add(i + 48) as *mut __m128i,
                _mm_add_epi8(a3, b3),
            );
        }

        i += 64;
    }

    while i + 16 <= len {
        unsafe {
            let a = _mm_loadu_si128(dst.add(i) as *const __m128i);
            let b = _mm_loadu_si128(src.add(i) as *const __m128i);

            _mm_storeu_si128(
                dst.add(i) as *mut __m128i,
                _mm_add_epi8(a, b),
            );
        }

        i += 16;
    }

    while i < len {
        unsafe {
            *dst.add(i) = (*dst.add(i)).wrapping_add(*src.add(i));
        }

        i += 1;
    }
}

// ---------------------------------------------------------
// PNG Sub
// ---------------------------------------------------------

#[target_feature(enable = "sse2")]
/// Reconstructs a PNG Sub-filtered scanline with SSE2 acceleration where possible.
unsafe fn png_sub_sse2(row: &mut [u8], bpp: usize) {
    match bpp {
        1 => unsafe { png_sub_1(row) },
        2 => unsafe { png_sub_2(row) },
        3 => unsafe { png_sub_3(row) },
        4 => unsafe { png_sub_4(row) },
        6 => unsafe { png_sub_6(row) },
        8 => unsafe { png_sub_8(row) },

        _ => unsafe {
            scalar_sub(row, bpp);
        },
    }
}

#[target_feature(enable = "sse2")]
/// Reconstructs a PNG Sub-filtered scanline for a one-byte pixel stride.
unsafe fn png_sub_1(row: &mut [u8]) {
    if row.len() <= 1 {
        return;
    }

    let mut i = 1;

    while i + 16 <= row.len() {
        let mut v = unsafe {
            _mm_loadu_si128(row.as_ptr().add(i) as *const __m128i)
        };

        v = _mm_add_epi8(v, _mm_slli_si128(v, 1));
        v = _mm_add_epi8(v, _mm_slli_si128(v, 2));
        v = _mm_add_epi8(v, _mm_slli_si128(v, 4));
        v = _mm_add_epi8(v, _mm_slli_si128(v, 8));

        let carry = _mm_set1_epi8(row[i - 1] as i8);

        v = _mm_add_epi8(v, carry);

        unsafe {
            _mm_storeu_si128(
                row.as_mut_ptr().add(i) as *mut __m128i,
                v,
            );
        }

        i += 16;
    }

    while i < row.len() {
        row[i] = row[i].wrapping_add(row[i - 1]);
        i += 1;
    }
}

#[target_feature(enable = "sse2")]
/// Reconstructs a PNG Sub-filtered scanline for a two-byte pixel stride.
unsafe fn png_sub_2(row: &mut [u8]) {
    if row.len() <= 2 {
        return;
    }

    let mut i = 2;

    while i + 16 <= row.len() {
        let mut v = unsafe {
            _mm_loadu_si128(row.as_ptr().add(i) as *const __m128i)
        };

        v = _mm_add_epi8(v, _mm_slli_si128(v, 2));
        v = _mm_add_epi8(v, _mm_slli_si128(v, 4));
        v = _mm_add_epi8(v, _mm_slli_si128(v, 8));

        let carry = u16::from_le_bytes([
            row[i - 2],
            row[i - 1],
        ]);

        v = _mm_add_epi8(
            v,
            _mm_set1_epi16(carry as i16),
        );

        unsafe {
            _mm_storeu_si128(
                row.as_mut_ptr().add(i) as *mut __m128i,
                v,
            );
        }

        i += 16;
    }

    while i < row.len() {
        row[i] = row[i].wrapping_add(row[i - 2]);
        i += 1;
    }
}

#[target_feature(enable = "sse2")]
/// Reconstructs a PNG Sub-filtered scanline for a three-byte pixel stride.
unsafe fn png_sub_3(row: &mut [u8]) {
    if row.len() <= 3 {
        return;
    }

    let mut i = 3;

    // Decode four RGB pixels = 12 bytes.
    //
    // We load 16 bytes but only write the first 12 bytes.
    while i + 16 <= row.len() {
        let mut v = unsafe {
            _mm_loadu_si128(
                row.as_ptr().add(i) as *const __m128i,
            )
        };

        v = _mm_add_epi8(
            v,
            _mm_slli_si128(v, 3),
        );

        v = _mm_add_epi8(
            v,
            _mm_slli_si128(v, 6),
        );

        let r = row[i - 3] as i8;
        let g = row[i - 2] as i8;
        let b = row[i - 1] as i8;

        let carry = _mm_setr_epi8(
            r, g, b,
            r, g, b,
            r, g, b,
            r, g, b,
            0, 0, 0, 0,
        );

        v = _mm_add_epi8(v, carry);

        unsafe {
            // First 8 bytes.
            _mm_storel_epi64(
                row.as_mut_ptr().add(i) as *mut __m128i,
                v,
            );

            // Remaining 4 bytes.
            let high = _mm_srli_si128(v, 8);
            let remaining = _mm_cvtsi128_si32(high);

            std::ptr::write_unaligned(
                row.as_mut_ptr().add(i + 8) as *mut i32,
                remaining,
            );
        }

        i += 12;
    }

    while i < row.len() {
        row[i] = row[i].wrapping_add(row[i - 3]);
        i += 1;
    }
}

#[target_feature(enable = "sse2")]
/// Reconstructs a PNG Sub-filtered scanline for a four-byte pixel stride.
unsafe fn png_sub_4(row: &mut [u8]) {
    if row.len() <= 4 {
        return;
    }

    let mut i = 4;

    while i + 16 <= row.len() {
        let mut v = unsafe {
            _mm_loadu_si128(row.as_ptr().add(i) as *const __m128i)
        };

        v = _mm_add_epi8(v, _mm_slli_si128(v, 4));
        v = _mm_add_epi8(v, _mm_slli_si128(v, 8));

        let carry = u32::from_le_bytes([
            row[i - 4],
            row[i - 3],
            row[i - 2],
            row[i - 1],
        ]);

        v = _mm_add_epi8(
            v,
            _mm_set1_epi32(carry as i32),
        );

        unsafe {
            _mm_storeu_si128(
                row.as_mut_ptr().add(i) as *mut __m128i,
                v,
            );
        }

        i += 16;
    }

    while i < row.len() {
        row[i] = row[i].wrapping_add(row[i - 4]);
        i += 1;
    }
}

#[target_feature(enable = "sse2")]
/// Reconstructs a PNG Sub-filtered scanline for a six-byte pixel stride.
unsafe fn png_sub_6(row: &mut [u8]) {
    if row.len() <= 6 {
        return;
    }

    let mut i = 6;

    // Two RGB16 pixels = 12 bytes.
    while i + 16 <= row.len() {
        let mut v = unsafe {
            _mm_loadu_si128(
                row.as_ptr().add(i) as *const __m128i,
            )
        };

        v = _mm_add_epi8(
            v,
            _mm_slli_si128(v, 6),
        );

        let a = row[i - 6] as i8;
        let b = row[i - 5] as i8;
        let c = row[i - 4] as i8;
        let d = row[i - 3] as i8;
        let e = row[i - 2] as i8;
        let f = row[i - 1] as i8;

        let carry = _mm_setr_epi8(
            a, b, c, d, e, f,
            a, b, c, d, e, f,
            0, 0, 0, 0,
        );

        v = _mm_add_epi8(v, carry);

        unsafe {
            _mm_storel_epi64(
                row.as_mut_ptr().add(i) as *mut __m128i,
                v,
            );

            let high = _mm_srli_si128(v, 8);
            let remaining = _mm_cvtsi128_si32(high);

            std::ptr::write_unaligned(
                row.as_mut_ptr().add(i + 8) as *mut i32,
                remaining,
            );
        }

        i += 12;
    }

    while i < row.len() {
        row[i] = row[i].wrapping_add(row[i - 6]);
        i += 1;
    }
}

#[target_feature(enable = "sse2")]
/// Reconstructs a PNG Sub-filtered scanline for an eight-byte pixel stride.
unsafe fn png_sub_8(row: &mut [u8]) {
    if row.len() <= 8 {
        return;
    }

    let mut i = 8;

    while i + 16 <= row.len() {
        let mut v = unsafe {
            _mm_loadu_si128(row.as_ptr().add(i) as *const __m128i)
        };

        v = _mm_add_epi8(
            v,
            _mm_slli_si128(v, 8),
        );

        let previous = unsafe {
            _mm_loadl_epi64(
                row.as_ptr().add(i - 8) as *const __m128i,
            )
        };

        let carry = _mm_unpacklo_epi64(previous, previous);

        v = _mm_add_epi8(v, carry);

        unsafe {
            _mm_storeu_si128(
                row.as_mut_ptr().add(i) as *mut __m128i,
                v,
            );
        }

        i += 16;
    }

    while i < row.len() {
        row[i] = row[i].wrapping_add(row[i - 8]);
        i += 1;
    }
}

// ---------------------------------------------------------
// RGBA8 -> 0xRRGGBBAA
// ---------------------------------------------------------

#[target_feature(enable = "avx2")]
/// Packs RGBA8 byte samples into `u32` pixels with AVX2 acceleration.
unsafe fn rgba8_to_u32_avx2(src: &[u8], dst: &mut [u32]) {
    let shuffle = _mm256_setr_epi8(
        3, 2, 1, 0,
        7, 6, 5, 4,
        11, 10, 9, 8,
        15, 14, 13, 12,

        3, 2, 1, 0,
        7, 6, 5, 4,
        11, 10, 9, 8,
        15, 14, 13, 12,
    );

    let mut pixel = 0;

    while pixel + 8 <= dst.len() {
        let offset = pixel * 4;

        let rgba = unsafe {
            _mm256_loadu_si256(
                src.as_ptr().add(offset) as *const __m256i,
            )
        };

        let packed = _mm256_shuffle_epi8(
            rgba,
            shuffle,
        );

        unsafe {
            _mm256_storeu_si256(
                dst.as_mut_ptr().add(pixel) as *mut __m256i,
                packed,
            );
        }

        pixel += 8;
    }

    while pixel < dst.len() {
        let i = pixel * 4;

        dst[pixel] =
            ((src[i] as u32) << 24)
            | ((src[i + 1] as u32) << 16)
            | ((src[i + 2] as u32) << 8)
            | src[i + 3] as u32;

        pixel += 1;
    }
}

#[target_feature(enable = "ssse3")]
/// Packs RGBA8 byte samples into `u32` pixels with SSSE3 acceleration.
unsafe fn rgba8_to_u32_ssse3(src: &[u8], dst: &mut [u32]) {
    let shuffle = _mm_setr_epi8(
        3, 2, 1, 0,
        7, 6, 5, 4,
        11, 10, 9, 8,
        15, 14, 13, 12,
    );

    let mut pixel = 0;

    while pixel + 4 <= dst.len() {
        let offset = pixel * 4;

        let rgba = unsafe {
            _mm_loadu_si128(
                src.as_ptr().add(offset) as *const __m128i,
            )
        };

        let packed = _mm_shuffle_epi8(
            rgba,
            shuffle,
        );

        unsafe {
            _mm_storeu_si128(
                dst.as_mut_ptr().add(pixel) as *mut __m128i,
                packed,
            );
        }

        pixel += 4;
    }

    while pixel < dst.len() {
        let i = pixel * 4;

        dst[pixel] =
            ((src[i] as u32) << 24)
            | ((src[i + 1] as u32) << 16)
            | ((src[i + 2] as u32) << 8)
            | src[i + 3] as u32;

        pixel += 1;
    }
}

// ---------------------------------------------------------
// RGB8 -> 0xRRGGBBFF
// ---------------------------------------------------------

#[target_feature(enable = "ssse3")]
/// Expands RGB8 byte samples to opaque packed `u32` pixels with SSSE3 acceleration.
unsafe fn rgb8_to_u32_ssse3(src: &[u8], dst: &mut [u32]) {
    let shuffle = _mm_setr_epi8(
        -1, 2, 1, 0,
        -1, 5, 4, 3,
        -1, 8, 7, 6,
        -1, 11, 10, 9,
    );

    let alpha = _mm_set1_epi32(0x000000ff);

    let mut pixel = 0;

    while pixel + 4 <= dst.len() {
        let offset = pixel * 3;

        // Loading 16 bytes requires at least 16 input bytes remaining.
        if offset + 16 > src.len() {
            break;
        }

        let rgb = unsafe {
            _mm_loadu_si128(
                src.as_ptr().add(offset) as *const __m128i,
            )
        };

        let rgba = _mm_shuffle_epi8(
            rgb,
            shuffle,
        );

        let rgba = _mm_or_si128(
            rgba,
            alpha,
        );

        unsafe {
            _mm_storeu_si128(
                dst.as_mut_ptr().add(pixel) as *mut __m128i,
                rgba,
            );
        }

        pixel += 4;
    }

    while pixel < dst.len() {
        let i = pixel * 3;

        dst[pixel] =
            ((src[i] as u32) << 24)
            | ((src[i + 1] as u32) << 16)
            | ((src[i + 2] as u32) << 8)
            | 0xff;

        pixel += 1;
    }
}

// #[inline]
// pub unsafe fn bgra_to_rgba_16(src: *const u8, dst: *mut u8) -> bool {
//     let v = unsafe { _mm_loadu_si128(src as *const __m128i) };

//     // BGRA → RGBA shuffle mask
//     let shuffle = unsafe { _mm_set_epi8(
//         14, 13, 12, 15,
//         10, 9, 8, 11,
//         6, 5, 4, 7,
//         2, 1, 0, 3,
//     ) };

//     let res = unsafe { _mm_shuffle_epi8(v, shuffle) };
//     unsafe { _mm_storeu_si128(dst as *mut __m128i, res) };

//     true
// }


// ---------------------------------------------------------
// Shared image-format kernels
// ---------------------------------------------------------

#[target_feature(enable = "sse2")]
unsafe fn gray8_to_u32_sse2(src: &[u8], dst: &mut [u32]) {
    let zero = _mm_setzero_si128();
    let alpha = _mm_set1_epi32(0xff);
    let mut i = 0usize;

    while i + 4 <= dst.len() {
        let bytes = unsafe { std::ptr::read_unaligned(src.as_ptr().add(i) as *const i32) };
        let v8 = _mm_cvtsi32_si128(bytes);
        let v16 = _mm_unpacklo_epi8(v8, zero);
        let v32 = _mm_unpacklo_epi16(v16, zero);
        let packed = _mm_or_si128(
            _mm_or_si128(_mm_slli_epi32(v32, 24), _mm_slli_epi32(v32, 16)),
            _mm_or_si128(_mm_slli_epi32(v32, 8), alpha),
        );
        unsafe { _mm_storeu_si128(dst.as_mut_ptr().add(i) as *mut __m128i, packed) };
        i += 4;
    }

    unsafe { scalar_gray8(&src[i..], &mut dst[i..]) };
}

#[target_feature(enable = "avx2")]
unsafe fn gray8_to_u32_avx2(src: &[u8], dst: &mut [u32]) {
    let alpha = _mm256_set1_epi32(0xff);
    let mut i = 0usize;

    while i + 8 <= dst.len() {
        let bytes = unsafe { _mm_loadl_epi64(src.as_ptr().add(i) as *const __m128i) };
        let v = _mm256_cvtepu8_epi32(bytes);
        let packed = _mm256_or_si256(
            _mm256_or_si256(_mm256_slli_epi32(v, 24), _mm256_slli_epi32(v, 16)),
            _mm256_or_si256(_mm256_slli_epi32(v, 8), alpha),
        );
        unsafe { _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, packed) };
        i += 8;
    }

    unsafe { gray8_to_u32_sse2(&src[i..], &mut dst[i..]) };
}

#[target_feature(enable = "ssse3")]
unsafe fn bgra8_to_u32_ssse3(src: &[u8], dst: &mut [u32]) {
    let shuffle = _mm_setr_epi8(
        3, 0, 1, 2,
        7, 4, 5, 6,
        11, 8, 9, 10,
        15, 12, 13, 14,
    );
    let mut i = 0usize;

    while i + 4 <= dst.len() {
        let v = unsafe { _mm_loadu_si128(src.as_ptr().add(i * 4) as *const __m128i) };
        let packed = _mm_shuffle_epi8(v, shuffle);
        unsafe { _mm_storeu_si128(dst.as_mut_ptr().add(i) as *mut __m128i, packed) };
        i += 4;
    }

    unsafe { scalar_bgra8(&src[i * 4..], &mut dst[i..]) };
}

#[target_feature(enable = "sse2")]
unsafe fn planar_rgb8_to_u32_sse2(r: &[u8], g: &[u8], b: &[u8], dst: &mut [u32]) {
    let zero = _mm_setzero_si128();
    let alpha = _mm_set1_epi32(0xff);
    let mut i = 0usize;

    while i + 4 <= dst.len() {
        let rl = unsafe { std::ptr::read_unaligned(r.as_ptr().add(i) as *const i32) };
        let gl = unsafe { std::ptr::read_unaligned(g.as_ptr().add(i) as *const i32) };
        let bl = unsafe { std::ptr::read_unaligned(b.as_ptr().add(i) as *const i32) };
        let rv = _mm_unpacklo_epi16(_mm_unpacklo_epi8(_mm_cvtsi32_si128(rl), zero), zero);
        let gv = _mm_unpacklo_epi16(_mm_unpacklo_epi8(_mm_cvtsi32_si128(gl), zero), zero);
        let bv = _mm_unpacklo_epi16(_mm_unpacklo_epi8(_mm_cvtsi32_si128(bl), zero), zero);
        let packed = _mm_or_si128(
            _mm_or_si128(_mm_slli_epi32(rv, 24), _mm_slli_epi32(gv, 16)),
            _mm_or_si128(_mm_slli_epi32(bv, 8), alpha),
        );
        unsafe { _mm_storeu_si128(dst.as_mut_ptr().add(i) as *mut __m128i, packed) };
        i += 4;
    }

    unsafe { scalar_planar_rgb8(&r[i..], &g[i..], &b[i..], &mut dst[i..]) };
}

#[target_feature(enable = "avx2")]
unsafe fn planar_rgb8_to_u32_avx2(r: &[u8], g: &[u8], b: &[u8], dst: &mut [u32]) {
    let alpha = _mm256_set1_epi32(0xff);
    let mut i = 0usize;

    while i + 8 <= dst.len() {
        let rr = unsafe { _mm_loadl_epi64(r.as_ptr().add(i) as *const __m128i) };
        let gg = unsafe { _mm_loadl_epi64(g.as_ptr().add(i) as *const __m128i) };
        let bb = unsafe { _mm_loadl_epi64(b.as_ptr().add(i) as *const __m128i) };
        let rv = _mm256_cvtepu8_epi32(rr);
        let gv = _mm256_cvtepu8_epi32(gg);
        let bv = _mm256_cvtepu8_epi32(bb);
        let packed = _mm256_or_si256(
            _mm256_or_si256(_mm256_slli_epi32(rv, 24), _mm256_slli_epi32(gv, 16)),
            _mm256_or_si256(_mm256_slli_epi32(bv, 8), alpha),
        );
        unsafe { _mm256_storeu_si256(dst.as_mut_ptr().add(i) as *mut __m256i, packed) };
        i += 8;
    }

    unsafe { planar_rgb8_to_u32_sse2(&r[i..], &g[i..], &b[i..], &mut dst[i..]) };
}

#[target_feature(enable = "sse2")]
unsafe fn argb_rotate_sse2(pixels: &mut [u32]) {
    let mut i = 0usize;
    while i + 4 <= pixels.len() {
        let v = unsafe { _mm_loadu_si128(pixels.as_ptr().add(i) as *const __m128i) };
        let out = _mm_or_si128(_mm_slli_epi32(v, 8), _mm_srli_epi32(v, 24));
        unsafe { _mm_storeu_si128(pixels.as_mut_ptr().add(i) as *mut __m128i, out) };
        i += 4;
    }
    unsafe { scalar_argb_rotate(&mut pixels[i..]) };
}

#[target_feature(enable = "avx2")]
unsafe fn argb_rotate_avx2(pixels: &mut [u32]) {
    let mut i = 0usize;
    while i + 8 <= pixels.len() {
        let v = unsafe { _mm256_loadu_si256(pixels.as_ptr().add(i) as *const __m256i) };
        let out = _mm256_or_si256(_mm256_slli_epi32(v, 8), _mm256_srli_epi32(v, 24));
        unsafe { _mm256_storeu_si256(pixels.as_mut_ptr().add(i) as *mut __m256i, out) };
        i += 8;
    }
    unsafe { argb_rotate_sse2(&mut pixels[i..]) };
}


#[target_feature(enable = "ssse3")]
unsafe fn rgb8_to_argb_ssse3(src: &[u8], dst: &mut [u32]) {
    let shuffle = _mm_setr_epi8(
        2, 1, 0, -1,
        5, 4, 3, -1,
        8, 7, 6, -1,
        11, 10, 9, -1,
    );
    let alpha = _mm_set1_epi32(0xff00_0000u32 as i32);
    let mut i = 0usize;
    while i + 4 <= dst.len() {
        let offset = i * 3;
        if offset + 16 > src.len() {
            break;
        }
        let v = unsafe { _mm_loadu_si128(src.as_ptr().add(offset) as *const __m128i) };
        let out = _mm_or_si128(_mm_shuffle_epi8(v, shuffle), alpha);
        unsafe { _mm_storeu_si128(dst.as_mut_ptr().add(i) as *mut __m128i, out) };
        i += 4;
    }
    unsafe { scalar_rgb8_argb(&src[i * 3..], &mut dst[i..]) };
}

#[target_feature(enable = "ssse3")]
unsafe fn rgba8_to_argb_ssse3(src: &[u8], dst: &mut [u32]) {
    let shuffle = _mm_setr_epi8(
        2, 1, 0, 3,
        6, 5, 4, 7,
        10, 9, 8, 11,
        14, 13, 12, 15,
    );
    let mut i = 0usize;
    while i + 4 <= dst.len() {
        let v = unsafe { _mm_loadu_si128(src.as_ptr().add(i * 4) as *const __m128i) };
        let out = _mm_shuffle_epi8(v, shuffle);
        unsafe { _mm_storeu_si128(dst.as_mut_ptr().add(i) as *mut __m128i, out) };
        i += 4;
    }
    unsafe { scalar_rgba8_argb(&src[i * 4..], &mut dst[i..]) };
}
