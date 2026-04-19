#[cfg(feature = "bmp")]
pub mod bmp;
#[cfg(feature = "pgm")]
pub mod pgm;
#[cfg(feature = "tga")]
pub mod tga;
#[cfg(feature = "png")]
pub mod png;
#[cfg(feature = "wbmp")]
pub mod wbmp;
// mod webp; // EXPERIMENTAL

// pub use webp::WebP; // EXPERIMENTAL
pub trait Decoder {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn as_bytes(&self) -> Vec<u8>;
    fn pixels(&self) -> Vec<u32>;
}