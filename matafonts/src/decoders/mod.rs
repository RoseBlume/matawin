pub mod ttf;

use crate::error::FontError;

pub trait Decoder {
    fn characters(
        &self,
        pt: u16,
        chars: impl Into<String>,
    ) -> Result<Vec<Character>, FontError>;

    fn character(
        &self,
        pt: u16,
        character: char,
    ) -> Result<Character, FontError>;
}

/// A rasterized font glyph together with the metrics required to lay it out.
///
/// `pixels` are stored in `0xRRGGBBAA` format. The bitmap contains only the
/// rasterized glyph area; layout must use `advance_width`, `bearing_x`,
/// `bearing_y`, and `baseline` rather than treating `width` as the advance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Character {
    /// Width of the rasterized bitmap in pixels.
    pub width: u32,
    /// Height of the rasterized bitmap in pixels.
    pub height: u32,
    /// Rasterized pixels in `0xRRGGBBAA` format.
    pub pixels: Vec<u32>,
    /// Horizontal pen advance after drawing this glyph.
    pub advance_width: u32,
    /// Horizontal offset from the pen origin to the bitmap's left edge.
    pub bearing_x: i32,
    /// Distance from the baseline upward to the bitmap's top edge.
    pub bearing_y: i32,
    /// Recommended line height for this point size.
    pub line_height: u32,
    /// Distance from the top of a line box to the baseline.
    pub baseline: u32,
}
