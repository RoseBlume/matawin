//! ARM/AArch64 NEON implementations.
//!
//! Runtime detection happens once in the parent dispatch layer. All routines in
//! this module are entered only after NEON/ASIMD support has been confirmed.

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

#[cfg(target_arch = "aarch64")]
use core::arch::aarch64::*;

#[cfg(target_arch = "arm")]
use core::arch::arm::*;

#[inline]
fn available() -> bool {
    #[cfg(target_arch = "aarch64")]
    {
        return std::arch::is_aarch64_feature_detected!("neon");
    }
    #[cfg(target_arch = "arm")]
    {
        return std::arch::is_arm_feature_detected!("neon");
    }
    #[allow(unreachable_code)]
    false
}

pub(super) fn detect_png() -> PngSimd {
    if !available() {
        return PngSimd {
            up: scalar_up,
            sub: scalar_sub,
            rgba8: scalar_rgba8,
            rgb8: scalar_rgb8,
        };
    }

    #[cfg(target_endian = "little")]
    {
        PngSimd {
            up: png_up_neon,
            // PNG Sub has a byte dependency chain. Keep the carefully tuned
            // scalar implementation rather than adding vector setup that loses
            // on the short scanlines where it is most common.
            sub: scalar_sub,
            rgba8: rgba8_to_u32_neon,
            #[cfg(target_arch = "aarch64")]
            rgb8: rgb8_to_u32_neon,
            #[cfg(target_arch = "arm")]
            rgb8: scalar_rgb8,
        }
    }

    #[cfg(target_endian = "big")]
    {
        PngSimd {
            up: png_up_neon,
            sub: scalar_sub,
            rgba8: scalar_rgba8,
            rgb8: scalar_rgb8,
        }
    }
}

pub(super) fn detect_pixels() -> PixelSimd {
    if !available() {
        return PixelSimd {
            rgba8: scalar_rgba8,
            rgb8: scalar_rgb8,
            gray8: scalar_gray8,
            bgra8: scalar_bgra8,
            planar_rgb8: scalar_planar_rgb8,
            argb_rotate: scalar_argb_rotate,
            rgb8_argb: scalar_rgb8_argb,
            rgba8_argb: scalar_rgba8_argb,
        };
    }

    #[cfg(target_endian = "little")]
    {
        PixelSimd {
            rgba8: rgba8_to_u32_neon,
            #[cfg(target_arch = "aarch64")]
            rgb8: rgb8_to_u32_neon,
            #[cfg(target_arch = "arm")]
            rgb8: scalar_rgb8,
            gray8: gray8_to_u32_neon,
            bgra8: bgra8_to_u32_neon,
            planar_rgb8: planar_rgb8_to_u32_neon,
            argb_rotate: argb_rotate_neon,
            #[cfg(target_arch = "aarch64")]
            rgb8_argb: rgb8_to_argb_neon,
            #[cfg(target_arch = "arm")]
            rgb8_argb: scalar_rgb8_argb,
            #[cfg(target_arch = "aarch64")]
            rgba8_argb: rgba8_to_argb_neon,
            #[cfg(target_arch = "arm")]
            rgba8_argb: scalar_rgba8_argb,
        }
    }

    #[cfg(target_endian = "big")]
    {
        PixelSimd {
            rgba8: scalar_rgba8,
            rgb8: scalar_rgb8,
            gray8: gray8_to_u32_neon,
            bgra8: scalar_bgra8,
            planar_rgb8: planar_rgb8_to_u32_neon,
            argb_rotate: argb_rotate_neon,
            rgb8_argb: scalar_rgb8_argb,
            rgba8_argb: scalar_rgba8_argb,
        }
    }
}

#[target_feature(enable = "neon")]
unsafe fn png_up_neon(row: &mut [u8], previous: &[u8]) {
    let mut i = 0usize;
    while i + 16 <= row.len() {
        let a = unsafe { vld1q_u8(row.as_ptr().add(i)) };
        let b = unsafe { vld1q_u8(previous.as_ptr().add(i)) };
        unsafe { vst1q_u8(row.as_mut_ptr().add(i), vaddq_u8(a, b)) };
        i += 16;
    }
    unsafe { scalar_up(&mut row[i..], &previous[i..]) };
}

#[target_feature(enable = "neon")]
unsafe fn rgba8_to_u32_neon(src: &[u8], dst: &mut [u32]) {
    let mut i = 0usize;
    while i + 4 <= dst.len() {
        let rgba = unsafe { vld1q_u8(src.as_ptr().add(i * 4)) };
        // Reverse each 32-bit group in memory. On little-endian ARM this turns
        // RGBA bytes into the memory representation of numeric 0xRRGGBBAA.
        let packed = vrev32q_u8(rgba);
        unsafe { vst1q_u32(dst.as_mut_ptr().add(i), vreinterpretq_u32_u8(packed)) };
        i += 4;
    }
    unsafe { scalar_rgba8(&src[i * 4..], &mut dst[i..]) };
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn rgb8_to_u32_neon(src: &[u8], dst: &mut [u32]) {
    let indices = [
        0x80, 2, 1, 0,
        0x80, 5, 4, 3,
        0x80, 8, 7, 6,
        0x80, 11, 10, 9,
    ];
    let alpha_bytes = [
        0xff, 0, 0, 0,
        0xff, 0, 0, 0,
        0xff, 0, 0, 0,
        0xff, 0, 0, 0,
    ];
    let table_index = unsafe { vld1q_u8(indices.as_ptr()) };
    let alpha = unsafe { vld1q_u8(alpha_bytes.as_ptr()) };
    let mut i = 0usize;

    while i + 4 <= dst.len() {
        let offset = i * 3;
        if offset + 16 > src.len() {
            break;
        }
        let rgb = unsafe { vld1q_u8(src.as_ptr().add(offset)) };
        let shuffled = vqtbl1q_u8(rgb, table_index);
        let packed = vorrq_u8(shuffled, alpha);
        unsafe { vst1q_u8(dst.as_mut_ptr().add(i) as *mut u8, packed) };
        i += 4;
    }

    unsafe { scalar_rgb8(&src[i * 3..], &mut dst[i..]) };
}

#[target_feature(enable = "neon")]
unsafe fn gray8_to_u32_neon(src: &[u8], dst: &mut [u32]) {
    let alpha = vdupq_n_u32(0xff);
    let mut i = 0usize;

    while i + 8 <= dst.len() {
        let gray8 = unsafe { vld1_u8(src.as_ptr().add(i)) };
        let gray16 = vmovl_u8(gray8);
        let lo = vmovl_u16(vget_low_u16(gray16));
        let hi = vmovl_u16(vget_high_u16(gray16));

        let packed_lo = vorrq_u32(
            vorrq_u32(vshlq_n_u32(lo, 24), vshlq_n_u32(lo, 16)),
            vorrq_u32(vshlq_n_u32(lo, 8), alpha),
        );
        let packed_hi = vorrq_u32(
            vorrq_u32(vshlq_n_u32(hi, 24), vshlq_n_u32(hi, 16)),
            vorrq_u32(vshlq_n_u32(hi, 8), alpha),
        );

        unsafe {
            vst1q_u32(dst.as_mut_ptr().add(i), packed_lo);
            vst1q_u32(dst.as_mut_ptr().add(i + 4), packed_hi);
        }
        i += 8;
    }

    unsafe { scalar_gray8(&src[i..], &mut dst[i..]) };
}

#[target_feature(enable = "neon")]
unsafe fn bgra8_to_u32_neon(src: &[u8], dst: &mut [u32]) {
    let mut i = 0usize;
    while i + 4 <= dst.len() {
        // Four BGRA pixels loaded as little-endian u32 values are AARRGGBB.
        // rotate-left by eight yields the crate's RRGGBBAA representation.
        let bytes = unsafe { vld1q_u8(src.as_ptr().add(i * 4)) };
        let p = vreinterpretq_u32_u8(bytes);
        let out = vorrq_u32(vshlq_n_u32(p, 8), vshrq_n_u32(p, 24));
        unsafe { vst1q_u32(dst.as_mut_ptr().add(i), out) };
        i += 4;
    }
    unsafe { scalar_bgra8(&src[i * 4..], &mut dst[i..]) };
}

#[target_feature(enable = "neon")]
unsafe fn planar_rgb8_to_u32_neon(r: &[u8], g: &[u8], b: &[u8], dst: &mut [u32]) {
    let alpha = vdupq_n_u32(0xff);
    let mut i = 0usize;

    while i + 8 <= dst.len() {
        let r16 = vmovl_u8(unsafe { vld1_u8(r.as_ptr().add(i)) });
        let g16 = vmovl_u8(unsafe { vld1_u8(g.as_ptr().add(i)) });
        let b16 = vmovl_u8(unsafe { vld1_u8(b.as_ptr().add(i)) });

        let rlo = vmovl_u16(vget_low_u16(r16));
        let rhi = vmovl_u16(vget_high_u16(r16));
        let glo = vmovl_u16(vget_low_u16(g16));
        let ghi = vmovl_u16(vget_high_u16(g16));
        let blo = vmovl_u16(vget_low_u16(b16));
        let bhi = vmovl_u16(vget_high_u16(b16));

        let packed_lo = vorrq_u32(
            vorrq_u32(vshlq_n_u32(rlo, 24), vshlq_n_u32(glo, 16)),
            vorrq_u32(vshlq_n_u32(blo, 8), alpha),
        );
        let packed_hi = vorrq_u32(
            vorrq_u32(vshlq_n_u32(rhi, 24), vshlq_n_u32(ghi, 16)),
            vorrq_u32(vshlq_n_u32(bhi, 8), alpha),
        );

        unsafe {
            vst1q_u32(dst.as_mut_ptr().add(i), packed_lo);
            vst1q_u32(dst.as_mut_ptr().add(i + 4), packed_hi);
        }
        i += 8;
    }

    unsafe { scalar_planar_rgb8(&r[i..], &g[i..], &b[i..], &mut dst[i..]) };
}

#[target_feature(enable = "neon")]
unsafe fn argb_rotate_neon(pixels: &mut [u32]) {
    let mut i = 0usize;
    while i + 4 <= pixels.len() {
        let p = unsafe { vld1q_u32(pixels.as_ptr().add(i)) };
        let out = vorrq_u32(vshlq_n_u32(p, 8), vshrq_n_u32(p, 24));
        unsafe { vst1q_u32(pixels.as_mut_ptr().add(i), out) };
        i += 4;
    }
    unsafe { scalar_argb_rotate(&mut pixels[i..]) };
}


#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn rgb8_to_argb_neon(src: &[u8], dst: &mut [u32]) {
    let indices = [
        2, 1, 0, 0x80,
        5, 4, 3, 0x80,
        8, 7, 6, 0x80,
        11, 10, 9, 0x80,
    ];
    let alpha_bytes = [
        0, 0, 0, 0xff,
        0, 0, 0, 0xff,
        0, 0, 0, 0xff,
        0, 0, 0, 0xff,
    ];
    let table_index = unsafe { vld1q_u8(indices.as_ptr()) };
    let alpha = unsafe { vld1q_u8(alpha_bytes.as_ptr()) };
    let mut i = 0usize;
    while i + 4 <= dst.len() {
        let offset = i * 3;
        if offset + 16 > src.len() {
            break;
        }
        let rgb = unsafe { vld1q_u8(src.as_ptr().add(offset)) };
        let packed = vorrq_u8(vqtbl1q_u8(rgb, table_index), alpha);
        unsafe { vst1q_u8(dst.as_mut_ptr().add(i) as *mut u8, packed) };
        i += 4;
    }
    unsafe { scalar_rgb8_argb(&src[i * 3..], &mut dst[i..]) };
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn rgba8_to_argb_neon(src: &[u8], dst: &mut [u32]) {
    let indices = [
        2, 1, 0, 3,
        6, 5, 4, 7,
        10, 9, 8, 11,
        14, 13, 12, 15,
    ];
    let table_index = unsafe { vld1q_u8(indices.as_ptr()) };
    let mut i = 0usize;
    while i + 4 <= dst.len() {
        let rgba = unsafe { vld1q_u8(src.as_ptr().add(i * 4)) };
        let packed = vqtbl1q_u8(rgba, table_index);
        unsafe { vst1q_u8(dst.as_mut_ptr().add(i) as *mut u8, packed) };
        i += 4;
    }
    unsafe { scalar_rgba8_argb(&src[i * 4..], &mut dst[i..]) };
}
