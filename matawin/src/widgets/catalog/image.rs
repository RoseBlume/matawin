//! Image and picture widgets backed by `mataimage`.

use std::{ path::Path, sync::{ Arc, Mutex } };

use mataimage::Image as DecodedImage;

use crate::Framebuffer;
use crate::widgets::{ Rect, WidgetError };
use super::{ base::{ Widget, WidgetExt }, types::ContentFit };

/// Mata image/picture backing store decoded by `mataimage`.
#[derive(Debug, Clone)]
pub struct ImageData {
    /// Stores width.
    ///
    /// Visibility: **public**.
    pub width: u32,
    /// Stores height.
    ///
    /// Visibility: **public**.
    pub height: u32,
    /// MataWin-native `0xAARRGGBB` pixels.
    pub pixels: Arc<Vec<u32>>,
}

impl ImageData {
    /// Performs the load operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the load operation. Visibility: public."]
    pub fn load(path: impl AsRef<Path>) -> Result<Self, WidgetError> {
        let path = path.as_ref();
        let mut file = std::fs::File::open(path).map_err(|e|
            WidgetError(format!("failed to open image {}: {e}", path.display()))
        )?;
        let decoded = DecodedImage::from_reader(&mut file).map_err(|e|
            WidgetError(format!("mataimage failed to decode {}: {e}", path.display()))
        )?;
        Self::from_decoded(decoded)
    }
    /// Performs the from bytes operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the from bytes operation. Visibility: public."]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WidgetError> {
        let decoded = DecodedImage::from_bytes(bytes).map_err(|e|
            WidgetError(format!("mataimage failed to decode image bytes: {e}"))
        )?;
        Self::from_decoded(decoded)
    }
    /// Performs the from decoded operation.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the from decoded operation. Visibility: private/internal."]
    fn from_decoded(decoded: DecodedImage) -> Result<Self, WidgetError> {
        // `mataimage::Image` keeps the primary display frame directly in
        // `Image::pixels`; `Image::frames` contains only *additional* animation
        // frames. Consume that primary buffer and convert it in place so MataWin
        // does not allocate a second full-size pixel vector.
        let width = decoded.width;
        let height = decoded.height;
        let mut pixels = decoded.into_pixels();
        for pixel in &mut pixels {
            *pixel = rgba_to_matawin(*pixel);
        }
        Ok(Self { width, height, pixels: Arc::new(pixels) })
    }
}

#[inline(always)]
/// Implements rgba to matawin.
///
/// Visibility: **private**.
#[doc = "Implements the rgba to matawin operation. Visibility: private/internal."]
fn rgba_to_matawin(pixel: u32) -> u32 {
    (pixel >> 8) | (pixel << 24)
}

#[derive(Debug, Clone)]
/// Stores the state and data for picture.
///
/// Visibility: **public**.
pub struct Picture {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores image.
    ///
    /// Visibility: **private**.
    image: Arc<Mutex<Option<ImageData>>>,
    /// Stores content fit.
    ///
    /// Visibility: **private**.
    content_fit: Arc<Mutex<ContentFit>>,
    /// Stores keep aspect ratio.
    ///
    /// Visibility: **private**.
    keep_aspect_ratio: Arc<Mutex<bool>>,
    /// Stores alternative text.
    ///
    /// Visibility: **private**.
    alternative_text: Arc<Mutex<Option<String>>>,
}
impl Default for Picture {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new()
    }
}
impl Picture {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        Self {
            widget: Widget::new(),
            image: Arc::new(Mutex::new(None)),
            content_fit: Arc::new(Mutex::new(ContentFit::ScaleDown)),
            keep_aspect_ratio: Arc::new(Mutex::new(true)),
            alternative_text: Arc::new(Mutex::new(None)),
        }
    }
    /// Performs the from file operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the from file operation. Visibility: public."]
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, WidgetError> {
        let this = Self::new();
        this.set_file(path)?;
        Ok(this)
    }
    /// Sets file.
    ///
    /// Visibility: **public**.
    #[doc = "Sets file. Visibility: public."]
    pub fn set_file(&self, path: impl AsRef<Path>) -> Result<(), WidgetError> {
        *self.image.lock().unwrap() = Some(ImageData::load(path)?);
        Ok(())
    }
    /// Sets bytes.
    ///
    /// Visibility: **public**.
    #[doc = "Sets bytes. Visibility: public."]
    pub fn set_bytes(&self, bytes: &[u8]) -> Result<(), WidgetError> {
        *self.image.lock().unwrap() = Some(ImageData::from_bytes(bytes)?);
        Ok(())
    }
    /// Performs the image operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the image operation. Visibility: public."]
    pub fn image(&self) -> Option<ImageData> {
        self.image.lock().unwrap().clone()
    }
    /// Sets content fit.
    ///
    /// Visibility: **public**.
    #[doc = "Sets content fit. Visibility: public."]
    pub fn set_content_fit(&self, fit: ContentFit) {
        *self.content_fit.lock().unwrap() = fit;
    }
    /// Sets keep aspect ratio.
    ///
    /// Visibility: **public**.
    #[doc = "Sets keep aspect ratio. Visibility: public."]
    pub fn set_keep_aspect_ratio(&self, value: bool) {
        *self.keep_aspect_ratio.lock().unwrap() = value;
    }
    /// Sets alternative text.
    ///
    /// Visibility: **public**.
    #[doc = "Sets alternative text. Visibility: public."]
    pub fn set_alternative_text<T: Into<String>>(&self, text: Option<T>) {
        *self.alternative_text.lock().unwrap() = text.map(Into::into);
    }

    /// Renders the primary decoded frame into a MataWin framebuffer using nearest
    /// neighbour scaling and alpha blending.
    #[doc = "Implements the blit operation. Visibility: public."]
    pub fn blit(&self, target: &mut Framebuffer) {
        let Some(image) = self.image() else {
            return;
        };
        let computed = self.widget.computed_style((target.width, target.height), None);
        let bounds = computed.content_bounds;
        if bounds.width == 0 || bounds.height == 0 || image.width == 0 || image.height == 0 {
            return;
        }
        blit_scaled(
            &image,
            target,
            bounds,
            *self.keep_aspect_ratio.lock().unwrap(),
            computed.opacity,
            computed.border_radius
        );
    }
}
impl WidgetExt for Picture {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

#[derive(Debug, Clone, Default)]
/// Stores the state and data for image.
///
/// Visibility: **public**.
pub struct Image {
    /// Stores picture.
    ///
    /// Visibility: **private**.
    picture: Picture,
}
impl Image {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        Self { picture: Picture::new() }
    }
    /// Performs the from file operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the from file operation. Visibility: public."]
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, WidgetError> {
        Ok(Self { picture: Picture::from_file(path)? })
    }
    /// Sets file.
    ///
    /// Visibility: **public**.
    #[doc = "Sets file. Visibility: public."]
    pub fn set_file(&self, path: impl AsRef<Path>) -> Result<(), WidgetError> {
        self.picture.set_file(path)
    }
    /// Sets bytes.
    ///
    /// Visibility: **public**.
    #[doc = "Sets bytes. Visibility: public."]
    pub fn set_bytes(&self, bytes: &[u8]) -> Result<(), WidgetError> {
        self.picture.set_bytes(bytes)
    }
    /// Performs the blit operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the blit operation. Visibility: public."]
    pub fn blit(&self, target: &mut Framebuffer) {
        self.picture.blit(target);
    }
}
impl WidgetExt for Image {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.picture.widget()
    }
}

#[derive(Debug, Clone, Default)]
/// Stores the state and data for svg widget.
///
/// Visibility: **public**.
pub struct SvgWidget {
    /// Stores image.
    ///
    /// Visibility: **private**.
    image: Image,
}
impl SvgWidget {
    /// Performs the from file operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the from file operation. Visibility: public."]
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, WidgetError> {
        Ok(Self { image: Image::from_file(path)? })
    }
    /// Sets file.
    ///
    /// Visibility: **public**.
    #[doc = "Sets file. Visibility: public."]
    pub fn set_file(&self, path: impl AsRef<Path>) -> Result<(), WidgetError> {
        self.image.set_file(path)
    }
    /// Performs the blit operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the blit operation. Visibility: public."]
    pub fn blit(&self, target: &mut Framebuffer) {
        self.image.blit(target);
    }
}
impl WidgetExt for SvgWidget {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        self.image.widget()
    }
}

/// Implements blit scaled.
///
/// Visibility: **private**.
#[doc = "Implements the blit scaled operation. Visibility: private/internal."]
fn blit_scaled(
    image: &ImageData,
    target: &mut Framebuffer,
    bounds: Rect,
    keep_aspect: bool,
    opacity: f32,
    radius: crate::widgets::style::ResolvedBorderRadius
) {
    let mut dw = bounds.width;
    let mut dh = bounds.height;
    let mut ox = bounds.x;
    let mut oy = bounds.y;
    if keep_aspect {
        let sx = (bounds.width as f64) / (image.width as f64);
        let sy = (bounds.height as f64) / (image.height as f64);
        let scale = sx.min(sy);
        dw = ((image.width as f64) * scale).round().max(1.0) as u32;
        dh = ((image.height as f64) * scale).round().max(1.0) as u32;
        ox += ((bounds.width - dw) / 2) as i32;
        oy += ((bounds.height - dh) / 2) as i32;
    }
    for dy in 0..dh {
        let ty = oy + (dy as i32);
        if ty < 0 || ty >= (target.height as i32) {
            continue;
        }
        let sy = (((dy as u64) * (image.height as u64)) / (dh as u64)) as u32;
        for dx in 0..dw {
            let tx = ox + (dx as i32);
            if tx < 0 || tx >= (target.width as i32) {
                continue;
            }
            let sx = (((dx as u64) * (image.width as u64)) / (dw as u64)) as u32;
            if !inside_rounded(bounds, radius, tx, ty) {
                continue;
            }
            let mut src = image.pixels[(sy * image.width + sx) as usize];
            let a = ((((src >> 24) & 0xff) as f32) * opacity.clamp(0.0, 1.0))
                .round()
                .clamp(0.0, 255.0) as u32;
            src = (src & 0x00ff_ffff) | (a << 24);
            let dst_i = ((ty as u32) * target.width + (tx as u32)) as usize;
            target.pixels[dst_i] = alpha_over(src, target.pixels[dst_i]);
        }
    }
}

/// Implements inside rounded.
///
/// Visibility: **private**.
#[doc = "Implements the inside rounded operation. Visibility: private/internal."]
fn inside_rounded(
    r: Rect,
    rad: crate::widgets::style::ResolvedBorderRadius,
    x: i32,
    y: i32
) -> bool {
    let fx = (x as f32) + 0.5;
    let fy = (y as f32) + 0.5;
    let left = r.x as f32;
    let top = r.y as f32;
    let right = left + (r.width as f32);
    let bottom = top + (r.height as f32);
    /// Implements inside.
    ///
    /// Visibility: **private**.
    #[doc = "Implements the inside operation. Visibility: private/internal."]
    fn inside(cx: f32, cy: f32, rx: f32, ry: f32, x: f32, y: f32) -> bool {
        if rx <= 0.0 || ry <= 0.0 {
            return true;
        }
        let dx = (x - cx) / rx;
        let dy = (y - cy) / ry;
        dx * dx + dy * dy <= 1.0
    }
    if fx < left + rad.top_left.x && fy < top + rad.top_left.y {
        return inside(
            left + rad.top_left.x,
            top + rad.top_left.y,
            rad.top_left.x,
            rad.top_left.y,
            fx,
            fy
        );
    }
    if fx > right - rad.top_right.x && fy < top + rad.top_right.y {
        return inside(
            right - rad.top_right.x,
            top + rad.top_right.y,
            rad.top_right.x,
            rad.top_right.y,
            fx,
            fy
        );
    }
    if fx > right - rad.bottom_right.x && fy > bottom - rad.bottom_right.y {
        return inside(
            right - rad.bottom_right.x,
            bottom - rad.bottom_right.y,
            rad.bottom_right.x,
            rad.bottom_right.y,
            fx,
            fy
        );
    }
    if fx < left + rad.bottom_left.x && fy > bottom - rad.bottom_left.y {
        return inside(
            left + rad.bottom_left.x,
            bottom - rad.bottom_left.y,
            rad.bottom_left.x,
            rad.bottom_left.y,
            fx,
            fy
        );
    }
    true
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


#[cfg(test)]
mod tests {
    use super::{ImageData, rgba_to_matawin};
    use mataimage::{Image as DecodedImage, ImageFrame};

    #[test]
    fn converts_new_mataimage_primary_frame_in_place() {
        let decoded = DecodedImage {
            width: 2,
            height: 1,
            pixels: vec![0x1122_3344, 0xaabb_ccdd],
            // New mataimage semantics: these are additional animation frames,
            // not the primary frame MataWin should display by default.
            frames: vec![ImageFrame { pixels: vec![0x0102_0304, 0x0506_0708] }],
        };

        let image = ImageData::from_decoded(decoded).expect("conversion should succeed");
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.pixels.as_slice(), &[0x4411_2233, 0xddaa_bbcc]);
    }

    #[test]
    fn rgba_to_matawin_rotates_rgba_to_argb() {
        assert_eq!(rgba_to_matawin(0x1122_3344), 0x4411_2233);
        assert_eq!(rgba_to_matawin(0x0000_00ff), 0xff00_0000);
    }
}
