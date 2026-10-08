//! Packed pixel-format conversion utilities.
//!
//! The conversion traits in this module treat source pixels as `0xRRGGBBAA` and provide
//! compact conversions to common 8-, 16-, 24-, and 32-bit channel layouts.


/// Packs four 8-bit channels into the crate's canonical `0xRRGGBBAA` pixel layout.
#[inline(always)]
pub(crate) const fn pack_rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | a as u32
}

/// Extension methods for converting a packed `0xRRGGBBAA` pixel into
/// other packed pixel formats.
///
/// The source pixel is always assumed to have the following layout:
///
/// ```text
/// 31             24 23             16 15              8 7               0
/// +----------------+----------------+----------------+----------------+
/// |       R        |       G        |       B        |       A        |
/// +----------------+----------------+----------------+----------------+
/// ```
///
/// For example:
///
/// ```text
/// 0x990030FF
///
/// R = 0x99
/// G = 0x00
/// B = 0x30
/// A = 0xFF
/// ```
///
/// The return type is the smallest Rust integer capable of containing
/// the destination pixel:
///
/// - 8-bit formats return `u8`.
/// - 16-bit formats return `u16`.
/// - 24-bit formats return `u32`.
/// - 32-bit formats return `u32`.
pub trait RgbaConvert {
    // ============================================================
    // 8-bit
    // ============================================================

    /// Converts `0xRRGGBBAA` into 8-bit `RGB332`.
    ///
    /// Layout:
    ///
    /// ```text
    /// RRRGGGBB
    /// ```
    fn rgba_to_u8_rgb(self) -> u8;

    /// Converts `0xRRGGBBAA` into 8-bit `BGR233`.
    ///
    /// Layout:
    ///
    /// ```text
    /// BBBGGGRR
    /// ```
    fn rgba_to_u8_bgr(self) -> u8;

    /// Converts `0xRRGGBBAA` into an 8-bit grayscale value.
    ///
    /// The conversion uses an integer approximation of:
    ///
    /// ```text
    /// Y = 0.299R + 0.587G + 0.114B
    /// ```
    fn rgba_to_u8_gray(self) -> u8;

    /// Extracts the alpha channel from `0xRRGGBBAA`.
    fn rgba_to_u8_alpha(self) -> u8;

    // ============================================================
    // 16-bit
    // ============================================================

    /// Converts `0xRRGGBBAA` into 16-bit `RGB565`.
    ///
    /// Layout:
    ///
    /// ```text
    /// RRRRRGGG GGGBBBBB
    /// ```
    fn rgba_to_u16_rgb(self) -> u16;

    /// Converts `0xRRGGBBAA` into 16-bit `BGR565`.
    ///
    /// Layout:
    ///
    /// ```text
    /// BBBBBGGG GGGRRRRR
    /// ```
    fn rgba_to_u16_bgr(self) -> u16;

    /// Converts `0xRRGGBBAA` into 16-bit `RGBA4444`.
    ///
    /// Layout:
    ///
    /// ```text
    /// RRRRGGGG BBBBAAAA
    /// ```
    fn rgba_to_u16_rgba(self) -> u16;

    /// Converts `0xRRGGBBAA` into 16-bit `BGRA4444`.
    ///
    /// Layout:
    ///
    /// ```text
    /// BBBBGGGG RRRRAAAA
    /// ```
    fn rgba_to_u16_bgra(self) -> u16;

    /// Converts `0xRRGGBBAA` into 16-bit `ARGB4444`.
    ///
    /// Layout:
    ///
    /// ```text
    /// AAAARRRR GGGGBBBB
    /// ```
    fn rgba_to_u16_argb(self) -> u16;

    /// Converts `0xRRGGBBAA` into 16-bit `ABGR4444`.
    ///
    /// Layout:
    ///
    /// ```text
    /// AAAABBBB GGGGRRRR
    /// ```
    fn rgba_to_u16_abgr(self) -> u16;

    /// Converts `0xRRGGBBAA` into 16-bit `ARGB1555`.
    ///
    /// Layout:
    ///
    /// ```text
    /// ARRRRRGG GGGBBBBB
    /// ```
    ///
    /// Alpha values `>= 128` become `1`.
    fn rgba_to_u16_argb1555(self) -> u16;

    /// Converts `0xRRGGBBAA` into 16-bit `RGBA5551`.
    ///
    /// Layout:
    ///
    /// ```text
    /// RRRRRGGG GGBBBBBA
    /// ```
    ///
    /// Alpha values `>= 128` become `1`.
    fn rgba_to_u16_rgba5551(self) -> u16;

    // ============================================================
    // 24-bit
    // ============================================================

    /// Converts `0xRRGGBBAA` into 24-bit RGB.
    ///
    /// The result is stored in a `u32` because Rust has no native
    /// 24-bit integer type.
    ///
    /// Layout:
    ///
    /// ```text
    /// 0x00RRGGBB
    /// ```
    fn rgba_to_u24_rgb(self) -> u32;

    /// Converts `0xRRGGBBAA` into 24-bit BGR.
    ///
    /// The result is stored in a `u32` because Rust has no native
    /// 24-bit integer type.
    ///
    /// Layout:
    ///
    /// ```text
    /// 0x00BBGGRR
    /// ```
    fn rgba_to_u24_bgr(self) -> u32;

    // ============================================================
    // 32-bit
    // ============================================================

    /// Returns the original `RGBA8888` pixel.
    ///
    /// Layout:
    ///
    /// ```text
    /// 0xRRGGBBAA
    /// ```
    fn rgba_to_u32_rgba(self) -> u32;

    /// Converts `0xRRGGBBAA` into `BGRA8888`.
    ///
    /// Layout:
    ///
    /// ```text
    /// 0xBBGGRRAA
    /// ```
    fn rgba_to_u32_bgra(self) -> u32;

    /// Converts `0xRRGGBBAA` into `ARGB8888`.
    ///
    /// Layout:
    ///
    /// ```text
    /// 0xAARRGGBB
    /// ```
    fn rgba_to_u32_argb(self) -> u32;

    /// Converts `0xRRGGBBAA` into `ABGR8888`.
    ///
    /// Layout:
    ///
    /// ```text
    /// 0xAABBGGRR
    /// ```
    fn rgba_to_u32_abgr(self) -> u32;

    /// Converts `0xRRGGBBAA` into `RGBX8888`.
    ///
    /// The unused byte is set to `0xFF`.
    ///
    /// Layout:
    ///
    /// ```text
    /// 0xRRGGBBFF
    /// ```
    fn rgba_to_u32_rgbx(self) -> u32;

    /// Converts `0xRRGGBBAA` into `BGRX8888`.
    ///
    /// The unused byte is set to `0xFF`.
    ///
    /// Layout:
    ///
    /// ```text
    /// 0xBBGGRRFF
    /// ```
    fn rgba_to_u32_bgrx(self) -> u32;

    /// Converts `0xRRGGBBAA` into `XRGB8888`.
    ///
    /// The unused byte is set to `0xFF`.
    ///
    /// Layout:
    ///
    /// ```text
    /// 0xFFRRGGBB
    /// ```
    fn rgba_to_u32_xrgb(self) -> u32;

    /// Converts `0xRRGGBBAA` into `XBGR8888`.
    ///
    /// The unused byte is set to `0xFF`.
    ///
    /// Layout:
    ///
    /// ```text
    /// 0xFFBBGGRR
    /// ```
    fn rgba_to_u32_xbgr(self) -> u32;
}

impl RgbaConvert for u32 {
    // ============================================================
    // 8-bit
    // ============================================================

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u8_rgb` destination layout.
    fn rgba_to_u8_rgb(self) -> u8 {
        let r = ((self >> 24) & 0xff) as u8;
        let g = ((self >> 16) & 0xff) as u8;
        let b = ((self >> 8) & 0xff) as u8;

        (r & 0xe0)
            | ((g >> 3) & 0x1c)
            | (b >> 6)
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u8_bgr` destination layout.
    fn rgba_to_u8_bgr(self) -> u8 {
        let r = ((self >> 24) & 0xff) as u8;
        let g = ((self >> 16) & 0xff) as u8;
        let b = ((self >> 8) & 0xff) as u8;

        (b & 0xe0)
            | ((g >> 3) & 0x1c)
            | (r >> 6)
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u8_gray` destination layout.
    fn rgba_to_u8_gray(self) -> u8 {
        let r = (self >> 24) & 0xff;
        let g = (self >> 16) & 0xff;
        let b = (self >> 8) & 0xff;

        ((r * 77 + g * 150 + b * 29) >> 8) as u8
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u8_alpha` destination layout.
    fn rgba_to_u8_alpha(self) -> u8 {
        self as u8
    }

    // ============================================================
    // 16-bit
    // ============================================================

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u16_rgb` destination layout.
    fn rgba_to_u16_rgb(self) -> u16 {
        let r = ((self >> 24) & 0xff) as u16;
        let g = ((self >> 16) & 0xff) as u16;
        let b = ((self >> 8) & 0xff) as u16;

        ((r >> 3) << 11)
            | ((g >> 2) << 5)
            | (b >> 3)
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u16_bgr` destination layout.
    fn rgba_to_u16_bgr(self) -> u16 {
        let r = ((self >> 24) & 0xff) as u16;
        let g = ((self >> 16) & 0xff) as u16;
        let b = ((self >> 8) & 0xff) as u16;

        ((b >> 3) << 11)
            | ((g >> 2) << 5)
            | (r >> 3)
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u16_rgba` destination layout.
    fn rgba_to_u16_rgba(self) -> u16 {
        let r = ((self >> 28) & 0x0f) as u16;
        let g = ((self >> 20) & 0x0f) as u16;
        let b = ((self >> 12) & 0x0f) as u16;
        let a = ((self >> 4) & 0x0f) as u16;

        (r << 12)
            | (g << 8)
            | (b << 4)
            | a
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u16_bgra` destination layout.
    fn rgba_to_u16_bgra(self) -> u16 {
        let r = ((self >> 28) & 0x0f) as u16;
        let g = ((self >> 20) & 0x0f) as u16;
        let b = ((self >> 12) & 0x0f) as u16;
        let a = ((self >> 4) & 0x0f) as u16;

        (b << 12)
            | (g << 8)
            | (r << 4)
            | a
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u16_argb` destination layout.
    fn rgba_to_u16_argb(self) -> u16 {
        let r = ((self >> 28) & 0x0f) as u16;
        let g = ((self >> 20) & 0x0f) as u16;
        let b = ((self >> 12) & 0x0f) as u16;
        let a = ((self >> 4) & 0x0f) as u16;

        (a << 12)
            | (r << 8)
            | (g << 4)
            | b
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u16_abgr` destination layout.
    fn rgba_to_u16_abgr(self) -> u16 {
        let r = ((self >> 28) & 0x0f) as u16;
        let g = ((self >> 20) & 0x0f) as u16;
        let b = ((self >> 12) & 0x0f) as u16;
        let a = ((self >> 4) & 0x0f) as u16;

        (a << 12)
            | (b << 8)
            | (g << 4)
            | r
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u16_argb1555` destination layout.
    fn rgba_to_u16_argb1555(self) -> u16 {
        let r = ((self >> 24) & 0xff) as u16;
        let g = ((self >> 16) & 0xff) as u16;
        let b = ((self >> 8) & 0xff) as u16;
        let a = (self & 0xff) as u16;

        let a = if a >= 128 { 1 } else { 0 };

        (a << 15)
            | ((r >> 3) << 10)
            | ((g >> 3) << 5)
            | (b >> 3)
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u16_rgba5551` destination layout.
    fn rgba_to_u16_rgba5551(self) -> u16 {
        let r = ((self >> 24) & 0xff) as u16;
        let g = ((self >> 16) & 0xff) as u16;
        let b = ((self >> 8) & 0xff) as u16;
        let a = (self & 0xff) as u16;

        let a = if a >= 128 { 1 } else { 0 };

        ((r >> 3) << 11)
            | ((g >> 3) << 6)
            | ((b >> 3) << 1)
            | a
    }

    // ============================================================
    // 24-bit
    // ============================================================

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u24_rgb` destination layout.
    fn rgba_to_u24_rgb(self) -> u32 {
        self >> 8
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u24_bgr` destination layout.
    fn rgba_to_u24_bgr(self) -> u32 {
        let r = (self >> 24) & 0xff;
        let g = (self >> 16) & 0xff;
        let b = (self >> 8) & 0xff;

        (b << 16)
            | (g << 8)
            | r
    }

    // ============================================================
    // 32-bit
    // ============================================================

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u32_rgba` destination layout.
    fn rgba_to_u32_rgba(self) -> u32 {
        self
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u32_bgra` destination layout.
    fn rgba_to_u32_bgra(self) -> u32 {
        // Original:
        // RR GG BB AA
        //
        // Result:
        // BB GG RR AA

        (self & 0x00ff_00ff)
            | ((self & 0xff00_0000) >> 16)
            | ((self & 0x0000_ff00) << 16)
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u32_argb` destination layout.
    fn rgba_to_u32_argb(self) -> u32 {
        // RRGGBBAA -> AARRGGBB
        self.rotate_right(8)
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u32_abgr` destination layout.
    fn rgba_to_u32_abgr(self) -> u32 {
        let r = (self >> 24) & 0xff;
        let g = (self >> 16) & 0xff;
        let b = (self >> 8) & 0xff;
        let a = self & 0xff;

        (a << 24)
            | (b << 16)
            | (g << 8)
            | r
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u32_rgbx` destination layout.
    fn rgba_to_u32_rgbx(self) -> u32 {
        self | 0xff
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u32_bgrx` destination layout.
    fn rgba_to_u32_bgrx(self) -> u32 {
        let r = (self >> 24) & 0xff;
        let g = (self >> 16) & 0xff;
        let b = (self >> 8) & 0xff;

        (b << 24)
            | (g << 16)
            | (r << 8)
            | 0xff
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u32_xrgb` destination layout.
    fn rgba_to_u32_xrgb(self) -> u32 {
        0xff00_0000 | (self >> 8)
    }

    #[inline(always)]
    /// Converts a packed `0xRRGGBBAA` pixel into the `u32_xbgr` destination layout.
    fn rgba_to_u32_xbgr(self) -> u32 {
        let r = (self >> 24) & 0xff;
        let g = (self >> 16) & 0xff;
        let b = (self >> 8) & 0xff;

        0xff00_0000
            | (b << 16)
            | (g << 8)
            | r
    }
}
