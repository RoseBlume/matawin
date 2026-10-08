//! TrueType font rendering for MataWin software widgets.
//!
//! Glyphs are decoded and rasterized by `matafonts`; no platform font APIs are
//! used by ordinary widgets.

use std::{ collections::HashMap, fs, path::{ Path, PathBuf } };
use matafonts::decoders::{ Character, ttf::Ttf };
use crate::Framebuffer;
use super::{ Rect, WidgetError, style::{ FontStretch, FontStyle, FontVariant, FontWeight } };

/// Constant used for glyph cache limit.
///
/// Visibility: **private**.
const GLYPH_CACHE_LIMIT: usize = 384;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Stores the state and data for glyph key.
///
/// Visibility: **private**.
struct GlyphKey {
    /// Stores pt.
    ///
    /// Visibility: **private**.
    pt: u16,
    /// Stores ch.
    ///
    /// Visibility: **private**.
    ch: char,
}

/// Cached `matafonts` TrueType renderer.
#[derive(Debug, Clone)]
pub struct FontRenderer {
    /// Stores font.
    ///
    /// Visibility: **private**.
    font: Ttf,
    /// Stores source.
    ///
    /// Visibility: **private**.
    source: Option<PathBuf>,
    /// Stores glyphs.
    ///
    /// Visibility: **private**.
    glyphs: HashMap<GlyphKey, Character>,
}

impl FontRenderer {
    /// Performs the from bytes operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the from bytes operation. Visibility: public."]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WidgetError> {
        let font = Ttf::new(bytes).map_err(|e|
            WidgetError(format!("matafonts failed to decode TTF: {e}"))
        )?;
        Ok(Self { font, source: None, glyphs: HashMap::with_capacity(96) })
    }

    /// Performs the from file operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the from file operation. Visibility: public."]
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, WidgetError> {
        let path = path.as_ref();
        let bytes = fs
            ::read(path)
            .map_err(|e| WidgetError(format!("failed to read font {}: {e}", path.display())))?;
        let mut renderer = Self::from_bytes(&bytes)?;
        renderer.source = Some(path.to_path_buf());
        Ok(renderer)
    }

    /// Finds a normal system TTF using filesystem paths only, then lets
    /// `matafonts` decode it.
    #[doc = "Implements the system default operation. Visibility: public."]
    pub fn system_default() -> Result<Self, WidgetError> {
        for path in system_font_candidates() {
            if path.is_file() {
                if let Ok(font) = Self::from_file(&path) {
                    return Ok(font);
                }
            }
        }
        Err(
            WidgetError(
                "no usable system TTF found; call WidgetWindow::set_font_file() after attaching it to a Window".into()
            )
        )
    }

    /// Finds a system TrueType font whose filename best matches a CSS-like
    /// family/style/weight request. The font is still decoded only by
    /// `matafonts`; platform font APIs are not used.
    #[doc = "Implements the system family operation. Visibility: public."]
    pub fn system_family(
        family: &str,
        style: FontStyle,
        weight: FontWeight
    ) -> Result<Self, WidgetError> {
        let wanted = normalize_font_name(family);
        if wanted.is_empty() {
            return Self::system_default();
        }
        let mut best: Option<(i32, PathBuf)> = None;
        for path in system_font_files() {
            let Some(name) = path.file_stem().and_then(|v| v.to_str()) else {
                continue;
            };
            let normalized = normalize_font_name(name);
            if !normalized.contains(&wanted) {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            let mut score = 100i32 - ((normalized.len() as i32) - (wanted.len() as i32)).abs();
            let italic = lower.contains("italic") || lower.contains("oblique");
            if matches!(style, FontStyle::Italic | FontStyle::Oblique) == italic {
                score += 25;
            } else if italic {
                score -= 10;
            }
            let bold = lower.contains("bold") || lower.contains("black") || lower.contains("heavy");
            let wants_bold = weight_strength(weight) >= 700;
            if bold == wants_bold {
                score += 20;
            } else if bold {
                score -= 8;
            }
            if best.as_ref().is_none_or(|(s, _)| score > *s) {
                best = Some((score, path));
            }
        }
        if let Some((_, path)) = best {
            Self::from_file(path)
        } else {
            Self::system_default()
        }
    }

    /// Performs the source operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the source operation. Visibility: public."]
    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    /// Performs the glyph operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the glyph operation. Visibility: private/internal."]
    fn glyph(&mut self, pt: u16, ch: char) -> Result<&Character, WidgetError> {
        let key = GlyphKey { pt, ch };
        if !self.glyphs.contains_key(&key) {
            // Responsive font sizes can otherwise retain a complete glyph set for
            // every intermediate size seen during a resize. Prefer the current
            // point size and keep the cache bounded.
            if self.glyphs.len() >= GLYPH_CACHE_LIMIT {
                self.glyphs.retain(|cached, _| cached.pt == pt);
                if self.glyphs.len() >= GLYPH_CACHE_LIMIT {
                    self.glyphs.clear();
                }
            }
            let glyph = self.font
                .character(pt, ch)
                .map_err(|e| WidgetError(format!("matafonts failed to rasterize {ch:?}: {e}")))?;
            self.glyphs.insert(key, glyph);
        }
        Ok(self.glyphs.get(&key).expect("cached glyph missing"))
    }

    /// Measures UTF-8 text using TrueType advance width and line metrics.
    #[doc = "Implements the measure operation. Visibility: public."]
    pub fn measure(&mut self, text: &str, pt: u16) -> Result<(u32, u32), WidgetError> {
        if text.is_empty() || pt == 0 {
            return Ok((0, 0));
        }
        let mut width = 0u32;
        let mut max_width = 0u32;
        let mut line_height = 0u32;
        let mut height = 0u32;
        for ch in text.chars() {
            if ch == '\n' {
                max_width = max_width.max(width);
                height = height.saturating_add(line_height.max(1));
                width = 0;
                line_height = 0;
                continue;
            }
            let glyph = self.glyph(pt, ch)?;
            width = width.saturating_add(glyph.advance_width);
            line_height = line_height.max(glyph.line_height.max(glyph.height));
        }
        max_width = max_width.max(width);
        height = height.saturating_add(line_height.max(1));
        Ok((max_width, height))
    }

    /// Measures text while applying font variant, weight, and stretch.
    #[doc = "Implements the measure styled operation. Visibility: public."]
    pub fn measure_styled(
        &mut self,
        text: &str,
        pt: u16,
        variant: FontVariant,
        weight: FontWeight,
        stretch: FontStretch,
        line_height: Option<u32>
    ) -> Result<(u32, u32), WidgetError> {
        let owned = apply_variant(text, variant);
        let text: &str = &owned;
        if text.is_empty() || pt == 0 {
            return Ok((0, 0));
        }
        let scale = stretch_factor(stretch);
        let extra = weight_extra(weight);
        let mut width = 0u32;
        let mut max_width = 0u32;
        let mut current_line = 0u32;
        let mut height = 0u32;
        for ch in text.chars() {
            if ch == '\n' {
                max_width = max_width.max(width);
                height = height.saturating_add(line_height.unwrap_or(current_line.max(1)));
                width = 0;
                current_line = 0;
                continue;
            }
            let glyph = self.glyph(pt, ch)?;
            let advance = ((glyph.advance_width as f32) * scale).round().max(0.0) as u32;
            width = width.saturating_add(advance.saturating_add(extra));
            current_line = current_line.max(glyph.line_height.max(glyph.height));
        }
        max_width = max_width.max(width);
        height = height.saturating_add(line_height.unwrap_or(current_line.max(1)));
        Ok((max_width, height))
    }

    /// Draws text while applying font style, variant, weight, and stretch.
    #[doc = "Draws text styled. Visibility: public."]
    pub fn draw_text_styled(
        &mut self,
        target: &mut Framebuffer,
        text: &str,
        x: i32,
        y: i32,
        pt: u16,
        color: u32,
        clip: Option<Rect>,
        style: FontStyle,
        variant: FontVariant,
        weight: FontWeight,
        stretch: FontStretch,
        line_height: Option<u32>
    ) -> Result<(), WidgetError> {
        if text.is_empty() || pt == 0 || target.width == 0 || target.height == 0 {
            return Ok(());
        }
        let owned = apply_variant(text, variant);
        let text: &str = &owned;
        let clip = clip.unwrap_or(Rect::new(0, 0, target.width, target.height));
        let (default_line, initial_baseline) = match self.glyph(pt, 'M') {
            Ok(g) => (g.line_height.max(g.height).max(1), g.baseline),
            Err(_) => (pt as u32, pt as u32),
        };
        let step = line_height.unwrap_or(default_line).max(1);
        let mut baseline = initial_baseline;
        let scale = stretch_factor(stretch);
        let extra = weight_extra(weight);
        let mut pen_x = x;
        let mut line_top = y;
        for ch in text.chars() {
            if ch == '\n' {
                pen_x = x;
                line_top = line_top.saturating_add(step as i32);
                continue;
            }
            let glyph = self.glyph(pt, ch)?;
            baseline = baseline.max(glyph.baseline);
            let gx = pen_x.saturating_add(((glyph.bearing_x as f32) * scale).round() as i32);
            let gy = line_top.saturating_add(baseline as i32).saturating_sub(glyph.bearing_y);
            draw_glyph_styled(target, glyph, gx, gy, color, clip, style, weight, stretch);
            let advance = ((glyph.advance_width as f32) * scale).round().max(0.0) as i32;
            pen_x = pen_x.saturating_add(advance.saturating_add(extra as i32));
        }
        Ok(())
    }

    /// Draws text into a MataWin `0xAARRGGBB` framebuffer.
    #[doc = "Draws text. Visibility: public."]
    pub fn draw_text(
        &mut self,
        target: &mut Framebuffer,
        text: &str,
        x: i32,
        y: i32,
        pt: u16,
        color: u32,
        clip: Option<Rect>
    ) -> Result<(), WidgetError> {
        if text.is_empty() || pt == 0 || target.width == 0 || target.height == 0 {
            return Ok(());
        }
        let clip = clip.unwrap_or(Rect::new(0, 0, target.width, target.height));
        let (mut line_height, mut baseline) = match self.glyph(pt, 'M') {
            Ok(g) => (g.line_height.max(g.height).max(1), g.baseline),
            Err(_) => ((pt as u32).max(1), pt as u32),
        };
        let mut pen_x = x;
        let mut line_top = y;

        for ch in text.chars() {
            if ch == '\n' {
                pen_x = x;
                line_top = line_top.saturating_add(line_height as i32);
                continue;
            }
            let glyph = self.glyph(pt, ch)?;
            line_height = line_height.max(glyph.line_height.max(glyph.height));
            baseline = baseline.max(glyph.baseline);
            let gx = pen_x.saturating_add(glyph.bearing_x);
            let gy = line_top.saturating_add(baseline as i32).saturating_sub(glyph.bearing_y);
            draw_glyph(target, glyph, gx, gy, color, clip);
            pen_x = pen_x.saturating_add(glyph.advance_width as i32);
        }
        Ok(())
    }
}

/// Draws glyph.
///
/// Visibility: **private**.
#[doc = "Draws glyph. Visibility: private/internal."]
fn draw_glyph(target: &mut Framebuffer, glyph: &Character, x: i32, y: i32, color: u32, clip: Rect) {
    let ca = (color >> 24) & 0xff;
    let rgb = color & 0x00ff_ffff;
    let left = clip.x.max(0);
    let top = clip.y.max(0);
    let right = clip.x.saturating_add(clip.width as i32).min(target.width as i32);
    let bottom = clip.y.saturating_add(clip.height as i32).min(target.height as i32);
    for gy in 0..glyph.height as i32 {
        let ty = y + gy;
        if ty < top || ty >= bottom {
            continue;
        }
        for gx in 0..glyph.width as i32 {
            let tx = x + gx;
            if tx < left || tx >= right {
                continue;
            }
            let coverage = glyph.pixels[((gy as u32) * glyph.width + (gx as u32)) as usize] & 0xff;
            if coverage == 0 {
                continue;
            }
            let alpha = (coverage * ca + 127) / 255;
            let src = (alpha << 24) | rgb;
            let i = (ty as usize) * (target.width as usize) + (tx as usize);
            target.pixels[i] = alpha_over(src, target.pixels[i]);
        }
    }
}

#[inline]
/// Implements alpha over.
///
/// Visibility: **private**.
#[doc = "Implements the alpha over operation. Visibility: private/internal."]
fn alpha_over(src: u32, dst: u32) -> u32 {
    let a = (src >> 24) & 0xff;
    if a == 255 {
        return src;
    }
    if a == 0 {
        return dst;
    }
    let inv = 255 - a;
    let sr = (src >> 16) & 0xff;
    let sg = (src >> 8) & 0xff;
    let sb = src & 0xff;
    let dr = (dst >> 16) & 0xff;
    let dg = (dst >> 8) & 0xff;
    let db = dst & 0xff;
    let r = (sr * a + dr * inv + 127) / 255;
    let g = (sg * a + dg * inv + 127) / 255;
    let b = (sb * a + db * inv + 127) / 255;
    0xff00_0000 | (r << 16) | (g << 8) | b
}

/// Implements apply variant.
///
/// Visibility: **private**.
#[doc = "Implements the apply variant operation. Visibility: private/internal."]
fn apply_variant<'a>(text: &'a str, variant: FontVariant) -> std::borrow::Cow<'a, str> {
    match variant {
        FontVariant::Normal => std::borrow::Cow::Borrowed(text),
        FontVariant::SmallCaps =>
            std::borrow::Cow::Owned(text.chars().flat_map(char::to_uppercase).collect()),
    }
}
/// Implements stretch factor.
///
/// Visibility: **private**.
#[doc = "Implements the stretch factor operation. Visibility: private/internal."]
fn stretch_factor(v: FontStretch) -> f32 {
    match v {
        FontStretch::UltraCondensed => 0.5,
        FontStretch::ExtraCondensed => 0.625,
        FontStretch::Condensed => 0.75,
        FontStretch::SemiCondensed => 0.875,
        FontStretch::Normal => 1.0,
        FontStretch::SemiExpanded => 1.125,
        FontStretch::Expanded => 1.25,
        FontStretch::ExtraExpanded => 1.5,
        FontStretch::UltraExpanded => 2.0,
    }
}
/// Implements weight strength.
///
/// Visibility: **private**.
#[doc = "Implements the weight strength operation. Visibility: private/internal."]
fn weight_strength(v: FontWeight) -> u16 {
    match v {
        FontWeight::Thin => 100,
        FontWeight::ExtraLight => 200,
        FontWeight::Light => 300,
        FontWeight::Normal => 400,
        FontWeight::Medium => 500,
        FontWeight::SemiBold => 600,
        FontWeight::Bold => 700,
        FontWeight::ExtraBold => 800,
        FontWeight::Black => 900,
        FontWeight::Numeric(n) => n,
    }
}
/// Implements weight extra.
///
/// Visibility: **private**.
#[doc = "Implements the weight extra operation. Visibility: private/internal."]
fn weight_extra(v: FontWeight) -> u32 {
    match weight_strength(v) {
        0..=599 => 0,
        600..=799 => 1,
        _ => 2,
    }
}
/// Draws glyph styled.
///
/// Visibility: **private**.
#[doc = "Draws glyph styled. Visibility: private/internal."]
fn draw_glyph_styled(
    target: &mut Framebuffer,
    glyph: &Character,
    x: i32,
    y: i32,
    color: u32,
    clip: Rect,
    style: FontStyle,
    weight: FontWeight,
    stretch: FontStretch
) {
    let ca = (color >> 24) & 0xff;
    let rgb = color & 0x00ff_ffff;
    let left = clip.x.max(0);
    let top = clip.y.max(0);
    let right = clip.x.saturating_add(clip.width as i32).min(target.width as i32);
    let bottom = clip.y.saturating_add(clip.height as i32).min(target.height as i32);
    let scale = stretch_factor(stretch);
    let extra = weight_extra(weight) as i32;
    let shear = match style {
        FontStyle::Normal => 0.0,
        FontStyle::Italic => 0.22,
        FontStyle::Oblique => 0.14,
    };
    for gy in 0..glyph.height as i32 {
        let ty = y + gy;
        if ty < top || ty >= bottom {
            continue;
        }
        let sxoff = ((((glyph.height as i32) - gy) as f32) * shear).round() as i32;
        for gx in 0..glyph.width as i32 {
            let coverage = glyph.pixels[((gy as u32) * glyph.width + (gx as u32)) as usize] & 0xff;
            if coverage == 0 {
                continue;
            }
            let start = x + (((gx as f32) * scale).floor() as i32) + sxoff;
            let end =
                x +
                ((((gx + 1) as f32) * scale).ceil() as i32).max(
                    (((gx as f32) * scale).floor() as i32) + 1
                ) +
                sxoff;
            let alpha = (coverage * ca + 127) / 255;
            let src = (alpha << 24) | rgb;
            for tx in start..end + extra {
                if tx < left || tx >= right {
                    continue;
                }
                let i = (ty as usize) * (target.width as usize) + (tx as usize);
                target.pixels[i] = alpha_over(src, target.pixels[i]);
            }
        }
    }
}
/// Implements normalize font name.
///
/// Visibility: **private**.
#[doc = "Implements the normalize font name operation. Visibility: private/internal."]
fn normalize_font_name(v: &str) -> String {
    v.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
/// Implements system font files.
///
/// Visibility: **private**.
#[doc = "Implements the system font files operation. Visibility: private/internal."]
fn system_font_files() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "windows")]
    {
        let root = std::env
            ::var_os("WINDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        roots.push(root.join("Fonts"));
    }
    #[cfg(target_os = "linux")]
    {
        roots.extend([PathBuf::from("/usr/share/fonts"), PathBuf::from("/usr/local/share/fonts")]);
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            roots.push(home.join(".fonts"));
            roots.push(home.join(".local/share/fonts"));
        }
    }
    #[cfg(target_os = "android")]
    {
        // Android system images commonly split fonts across these read-only
        // partitions. Missing directories are harmless and skipped below.
        roots.extend([
            PathBuf::from("/system/fonts"),
            PathBuf::from("/product/fonts"),
            PathBuf::from("/system_ext/fonts"),
            PathBuf::from("/vendor/fonts"),
        ]);
    }
    let mut out = Vec::new();
    for root in roots {
        collect_ttf(&root, 0, &mut out);
    }
    out
}
/// Implements collect ttf.
///
/// Visibility: **private**.
#[doc = "Implements the collect ttf operation. Visibility: private/internal."]
fn collect_ttf(path: &Path, depth: u8, out: &mut Vec<PathBuf>) {
    if depth > 5 {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_ttf(&p, depth + 1, out);
        } else if
            p
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("ttf"))
        {
            out.push(p);
        }
    }
}

/// Implements system font candidates.
///
/// Visibility: **private**.
#[doc = "Implements the system font candidates operation. Visibility: private/internal."]
fn system_font_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(target_os = "windows")]
    {
        let root = std::env
            ::var_os("WINDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let fonts = root.join("Fonts");
        paths.extend([
            fonts.join("segoeui.ttf"),
            fonts.join("arial.ttf"),
            fonts.join("tahoma.ttf"),
        ]);
    }
    #[cfg(target_os = "linux")]
    {
        paths.extend([
            PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
            PathBuf::from("/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf"),
            PathBuf::from("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf"),
            PathBuf::from("/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf"),
            PathBuf::from("/usr/share/fonts/TTF/DejaVuSans.ttf"),
        ]);
    }
    #[cfg(target_os = "android")]
    {
        paths.extend([
            PathBuf::from("/system/fonts/Roboto-Regular.ttf"),
            PathBuf::from("/system/fonts/RobotoStatic-Regular.ttf"),
            PathBuf::from("/system/fonts/NotoSans-Regular.ttf"),
            PathBuf::from("/product/fonts/Roboto-Regular.ttf"),
        ]);
    }
    paths
}
