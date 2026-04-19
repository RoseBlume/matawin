// #![cfg_attr(all(feature = "simd", nightly, any(target_arch = "riscv32", target_arch = "riscv64")), feature(riscv_ext_intrinsics))]
mod decoder;
#[cfg(feature = "simd")]
mod simd;
pub mod error;
pub use error::ImageError;

// Formats:
// Bmp
// Pgm
// Tga
// Png
// WebP // EXPERIMENTAL
pub use decoder::{
    Decoder,

};

#[cfg(feature = "bmp")]
pub use decoder::bmp::Bmp;
#[cfg(feature = "pgm")]
pub use decoder::pgm::Pgm;
#[cfg(feature = "tga")]
pub use decoder::tga::Tga;
#[cfg(feature = "png")]
pub use decoder::png::Png;
#[cfg(feature = "wbmp")]
pub use decoder::wbmp::Wbmp;

// pub use pixels::{
//     PixelFormat,
//     Pixel,
//     Pixels
// };

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub pixels: Vec<u32>,
}

impl Image {
    pub fn load(path: &str) -> Result<Self, ImageError> {
    let bytes = std::fs::read(path)
        .map_err(|e| ImageError::new(format!("Failed to read file: {}", e)))?;

    let decoder: Box<dyn Decoder> =
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            #[cfg(feature = "png")] {
                Box::new(Png::new(&bytes)?)
            }
            #[cfg(not(feature = "png"))]
            return Err(ImageError::new(&format!("{} is feature gated out", "png")));
        }

        // JPEG (JFIF / EXIF both start with FF D8 FF)
        else if bytes.len() >= 3 && bytes[0..3] == [0xFF, 0xD8, 0xFF] {
            // Box::new(Jpeg::new(&bytes)?)
            return Err(ImageError::new("JPEG Images not currently supported"));
        }

        // WebP: RIFF....WEBP (NOT just RIFF)
        else if bytes.len() >= 12
            && &bytes[0..4] == b"RIFF"
            && &bytes[8..12] == b"WEBP"
        {
            return Err(ImageError::new("WebP Images not currently supported"));
        }

        // Other RIFF-based formats (WAV, AVI, etc.)
        // else if bytes.starts_with(b"RIFF") {
        //     return Err(ImageError::new("RIFF container detected, but not a WebP image"));
        // }

        // BMP
        else if bytes.starts_with(b"BM") {
            #[cfg(feature = "bmp")] {
                Box::new(Bmp::new(&bytes)?)
            }
            #[cfg(not(feature = "bmp"))]
            return Err(ImageError::new(&format!("{} is feature gated out", "bmp")));
        }

        // GIF (GIF87a / GIF89a)
        else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            #[cfg(feature = "gif")] {
                Box::new(Gif::new(&bytes)?)
            }
            #[cfg(not(feature = "gif"))]
            return Err(ImageError::new(&format!("{} is feature gated out", "gif")));
        }

        // TIFF (little + big endian)
        else if bytes.len() >= 4
            && (bytes[0..4] == [0x49, 0x49, 0x2A, 0x00] // II*
                || bytes[0..4] == [0x4D, 0x4D, 0x00, 0x2A]) // MM*
        {
            #[cfg(feature = "tiff")]
            {
                Box::new(Tiff::new(&bytes)?)
            }
            #[cfg(not(feature = "tiff"))]
            return Err(ImageError::new(&format!("{} is feature gated out", "tiff")));
        }

        // Custom pixmap (your format)
        // else if bytes.len() >= 8 && bytes[0..8] == [0; 8] {
        //     Box::new(Pixmap::new(&bytes)?)
        // }

        // PGM (P2 = ASCII, P5 = binary)
        else if bytes.len() >= 2
            && (bytes[0..2] == [b'P', b'2'] || bytes[0..2] == [b'P', b'5'])
        {
            #[cfg(feature = "pgm")] {
                Box::new(Pgm::new(&bytes)?)
            }
            #[cfg(not(feature = "pgm"))]
            return Err(ImageError::new(&format!("{} is feature gated out", "pgm")));
        }

        // TGA (very loose heuristic)
        else if bytes.len() >= 18
            && (bytes[1..3] == [0, 2] || bytes[1..3] == [0, 10])
        {
            #[cfg(feature = "tga")] {
                Box::new(Tga::new(&bytes)?)
            }
            #[cfg(not(feature = "tga"))]
            return Err(ImageError::new(&format!("{} is feature gated out", "tga")));
        }
        else {
            return Err(ImageError::new("Unsupported image format"));
        };

        Ok(Image {
            width: decoder.width(),
            height: decoder.height(),
            data: decoder.as_bytes(),
            pixels: decoder.pixels(),
        })
    }
}