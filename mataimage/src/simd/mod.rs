//! Optional runtime-dispatched SIMD primitives.
//!
//! The public decoder modules call this layer unconditionally. When the
//! `simd` Cargo feature is disabled, every entry in the dispatch tables points
//! at the scalar implementation. When the feature is enabled, the best backend
//! is selected once per process and cached in a `OnceLock`.
//!
//! Keeping dispatch here has two important properties: format hot loops do not
//! repeatedly ask the CPU which instructions it supports, and every SIMD path
//! has an identical scalar fallback.
#![allow(unused)]
use std::sync::OnceLock;

#[cfg(all(feature = "simd", any(target_arch = "x86", target_arch = "x86_64")))]
mod x86;

#[cfg(all(feature = "simd", any(target_arch = "arm", target_arch = "aarch64")))]
mod neon;

type UpFn = unsafe fn(&mut [u8], &[u8]);
type SubFn = unsafe fn(&mut [u8], usize);
type Rgba8Fn = unsafe fn(&[u8], &mut [u32]);
type Rgb8Fn = unsafe fn(&[u8], &mut [u32]);
type Gray8Fn = unsafe fn(&[u8], &mut [u32]);
type Bgra8Fn = unsafe fn(&[u8], &mut [u32]);
type PlanarRgb8Fn = unsafe fn(&[u8], &[u8], &[u8], &mut [u32]);
type ArgbRotateFn = unsafe fn(&mut [u32]);
type Rgb8ArgbFn = unsafe fn(&[u8], &mut [u32]);
type Rgba8ArgbFn = unsafe fn(&[u8], &mut [u32]);

/// SIMD functions used by PNG scanline reconstruction and common PNG color
/// conversion paths.
#[derive(Clone, Copy)]
pub(crate) struct PngSimd {
    pub(super) up: UpFn,
    pub(super) sub: SubFn,
    pub(super) rgba8: Rgba8Fn,
    pub(super) rgb8: Rgb8Fn,
}

/// SIMD functions shared by image formats after their compressed data has been
/// reconstructed into byte/component planes.
#[derive(Clone, Copy)]
pub(super) struct PixelSimd {
    pub(super) rgba8: Rgba8Fn,
    pub(super) _rgb8: Rgb8Fn,
    pub(super) gray8: Gray8Fn,
    pub(super) bgra8: Bgra8Fn,
    pub(super) planar_rgb8: PlanarRgb8Fn,
    pub(super) argb_rotate: ArgbRotateFn,
    pub(super) _rgb8_argb: Rgb8ArgbFn,
    pub(super) _rgba8_argb: Rgba8ArgbFn,
}

static PNG_DISPATCH: OnceLock<PngSimd> = OnceLock::new();
static PIXEL_DISPATCH: OnceLock<PixelSimd> = OnceLock::new();

impl PngSimd {
    /// Selects the PNG implementation once for the current process.
    #[inline]
    pub(crate) fn detect() -> Self {
        *PNG_DISPATCH.get_or_init(select_png)
    }

    #[inline(always)]
    pub(crate) fn up(self, row: &mut [u8], previous: &[u8]) {
        debug_assert_eq!(row.len(), previous.len());
        unsafe { (self.up)(row, previous) }
    }

    #[inline(always)]
    pub(crate) fn sub(self, row: &mut [u8], bpp: usize) {
        unsafe { (self.sub)(row, bpp) }
    }

    #[inline(always)]
    pub(crate) fn rgba8_to_u32(self, src: &[u8], dst: &mut [u32]) {
        debug_assert_eq!(src.len(), dst.len() * 4);
        unsafe { (self.rgba8)(src, dst) }
    }

    #[inline(always)]
    pub(crate) fn rgb8_to_u32(self, src: &[u8], dst: &mut [u32]) {
        debug_assert_eq!(src.len(), dst.len() * 3);
        unsafe { (self.rgb8)(src, dst) }
    }
}

#[inline]
fn pixels() -> PixelSimd {
    *PIXEL_DISPATCH.get_or_init(select_pixels)
}

fn select_png() -> PngSimd {
    let scalar = PngSimd {
        up: scalar_up,
        sub: scalar_sub,
        rgba8: scalar_rgba8,
        rgb8: scalar_rgb8,
    };

    #[cfg(all(feature = "simd", any(target_arch = "x86", target_arch = "x86_64")))]
    {
        return x86::detect_png();
    }

    #[cfg(all(feature = "simd", any(target_arch = "arm", target_arch = "aarch64")))]
    {
        return neon::detect_png();
    }

    #[allow(unreachable_code)]
    scalar
}

fn select_pixels() -> PixelSimd {
    let scalar = PixelSimd {
        rgba8: scalar_rgba8,
        _rgb8: scalar_rgb8,
        gray8: scalar_gray8,
        bgra8: scalar_bgra8,
        planar_rgb8: scalar_planar_rgb8,
        argb_rotate: scalar_argb_rotate,
        _rgb8_argb: scalar_rgb8_argb,
        _rgba8_argb: scalar_rgba8_argb,
    };

    #[cfg(all(feature = "simd", any(target_arch = "x86", target_arch = "x86_64")))]
    {
        return x86::detect_pixels();
    }

    #[cfg(all(feature = "simd", any(target_arch = "arm", target_arch = "aarch64")))]
    {
        return neon::detect_pixels();
    }

    #[allow(unreachable_code)]
    scalar
}

/// Converts tightly packed RGBA8 bytes to the crate's `0xRRGGBBAA` pixels.
#[inline]
pub(crate) fn rgba8_to_vec(src: &[u8]) -> Vec<u32> {
    let mut dst = vec![0u32; src.len() / 4];
    rgba8_to_u32(src, &mut dst);
    dst
}

#[inline]
pub(crate) fn rgba8_to_u32(src: &[u8], dst: &mut [u32]) {
    debug_assert!(src.len() >= dst.len() * 4);
    unsafe { (pixels().rgba8)(&src[..dst.len() * 4], dst) }
}

#[inline]
pub(crate) fn _rgb8_to_u32(src: &[u8], dst: &mut [u32]) {
    debug_assert!(src.len() >= dst.len() * 3);
    unsafe { (pixels()._rgb8)(&src[..dst.len() * 3], dst) }
}

#[inline]
pub(crate) fn gray8_to_u32(src: &[u8], dst: &mut [u32]) {
    debug_assert!(src.len() >= dst.len());
    unsafe { (pixels().gray8)(&src[..dst.len()], dst) }
}

#[inline]
pub(crate) fn bgra8_to_u32(src: &[u8], dst: &mut [u32]) {
    debug_assert!(src.len() >= dst.len() * 4);
    unsafe { (pixels().bgra8)(&src[..dst.len() * 4], dst) }
}

#[inline]
pub(crate) fn planar_rgb8_to_u32(r: &[u8], g: &[u8], b: &[u8], dst: &mut [u32]) {
    let len = dst.len();
    debug_assert!(r.len() >= len && g.len() >= len && b.len() >= len);
    unsafe { (pixels().planar_rgb8)(&r[..len], &g[..len], &b[..len], dst) }
}

#[inline]
pub(crate) fn argb_to_rrggbbaa_in_place(pixels_: &mut [u32]) {
    unsafe { (pixels().argb_rotate)(pixels_) }
}

#[inline]
pub(crate) fn _rgb8_to_argb(src: &[u8], dst: &mut [u32]) {
    debug_assert!(src.len() >= dst.len() * 3);
    unsafe { (pixels()._rgb8_argb)(&src[..dst.len() * 3], dst) }
}

#[inline]
pub(crate) fn _rgba8_to_argb(src: &[u8], dst: &mut [u32]) {
    debug_assert!(src.len() >= dst.len() * 4);
    unsafe { (pixels()._rgba8_argb)(&src[..dst.len() * 4], dst) }
}

pub(super) unsafe fn scalar_up(row: &mut [u8], previous: &[u8]) {
    for i in 0..row.len() {
        row[i] = row[i].wrapping_add(previous[i]);
    }
}

pub(super) unsafe fn scalar_sub(row: &mut [u8], bpp: usize) {
    if bpp == 0 {
        return;
    }
    for i in bpp..row.len() {
        row[i] = row[i].wrapping_add(row[i - bpp]);
    }
}

pub(super) unsafe fn scalar_rgba8(src: &[u8], dst: &mut [u32]) {
    for (pixel, rgba) in dst.iter_mut().zip(src.chunks_exact(4)) {
        *pixel = u32::from_be_bytes([rgba[0], rgba[1], rgba[2], rgba[3]]);
    }
}

pub(super) unsafe fn scalar_rgb8(src: &[u8], dst: &mut [u32]) {
    for (pixel, rgb) in dst.iter_mut().zip(src.chunks_exact(3)) {
        *pixel = ((rgb[0] as u32) << 24)
            | ((rgb[1] as u32) << 16)
            | ((rgb[2] as u32) << 8)
            | 0xff;
    }
}

pub(super) unsafe fn scalar_gray8(src: &[u8], dst: &mut [u32]) {
    for (pixel, &gray) in dst.iter_mut().zip(src) {
        let g = gray as u32;
        *pixel = (g << 24) | (g << 16) | (g << 8) | 0xff;
    }
}

pub(super) unsafe fn scalar_bgra8(src: &[u8], dst: &mut [u32]) {
    for (pixel, bgra) in dst.iter_mut().zip(src.chunks_exact(4)) {
        *pixel = u32::from_be_bytes([bgra[2], bgra[1], bgra[0], bgra[3]]);
    }
}

pub(super) unsafe fn scalar_planar_rgb8(r: &[u8], g: &[u8], b: &[u8], dst: &mut [u32]) {
    for i in 0..dst.len() {
        dst[i] = ((r[i] as u32) << 24)
            | ((g[i] as u32) << 16)
            | ((b[i] as u32) << 8)
            | 0xff;
    }
}

pub(super) unsafe fn scalar_argb_rotate(pixels: &mut [u32]) {
    for pixel in pixels {
        *pixel = pixel.rotate_left(8);
    }
}


pub(super) unsafe fn scalar_rgb8_argb(src: &[u8], dst: &mut [u32]) {
    for (pixel, rgb) in dst.iter_mut().zip(src.chunks_exact(3)) {
        *pixel = 0xff00_0000
            | ((rgb[0] as u32) << 16)
            | ((rgb[1] as u32) << 8)
            | rgb[2] as u32;
    }
}

pub(super) unsafe fn scalar_rgba8_argb(src: &[u8], dst: &mut [u32]) {
    for (pixel, rgba) in dst.iter_mut().zip(src.chunks_exact(4)) {
        *pixel = ((rgba[3] as u32) << 24)
            | ((rgba[0] as u32) << 16)
            | ((rgba[1] as u32) << 8)
            | rgba[2] as u32;
    }
}

#[cfg(test)]
mod golden_tests {
    use super::*;

    fn pseudo_random_bytes(length: usize) -> Vec<u8> {
        let mut state = 0x1234_5678u32;
        (0..length)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect()
    }

    #[test]
    fn runtime_pixel_backend_matches_scalar() {
        let rgba = pseudo_random_bytes(4096 * 4);
        let rgb = pseudo_random_bytes(4096 * 3);
        let gray = pseudo_random_bytes(4096);
        let backend = select_pixels();

        let mut expected = vec![0u32; 4096];
        let mut actual = vec![0u32; 4096];
        unsafe { scalar_rgba8(&rgba, &mut expected) };
        unsafe { (backend.rgba8)(&rgba, &mut actual) };
        assert_eq!(actual, expected);

        unsafe { scalar_rgb8(&rgb, &mut expected) };
        unsafe { (backend._rgb8)(&rgb, &mut actual) };
        assert_eq!(actual, expected);

        unsafe { scalar_gray8(&gray, &mut expected) };
        unsafe { (backend.gray8)(&gray, &mut actual) };
        assert_eq!(actual, expected);
    }

    #[test]
    fn runtime_png_backend_matches_scalar() {
        let backend = select_png();
        let previous = pseudo_random_bytes(4096);
        let source = pseudo_random_bytes(4096);

        let mut expected = source.clone();
        let mut actual = source.clone();
        unsafe { scalar_up(&mut expected, &previous) };
        unsafe { (backend.up)(&mut actual, &previous) };
        assert_eq!(actual, expected);

        for bpp in [1usize, 2, 3, 4, 8] {
            let mut expected = source.clone();
            let mut actual = source.clone();
            unsafe { scalar_sub(&mut expected, bpp) };
            unsafe { (backend.sub)(&mut actual, bpp) };
            assert_eq!(actual, expected, "bpp={bpp}");
        }
    }
}
