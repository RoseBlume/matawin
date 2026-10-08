//! Implements the `buffer` module.
//!
//! This module documents both public API items and internal implementation details.

use std::{ fs::File, io::{ BufReader, Read, Seek, SeekFrom } };

/// Contiguous 32-bit software framebuffer.
///
/// Pixels use matawin's native `0xAARRGGBB` integer representation. On the
/// little-endian desktop platforms targeted by matawin this is B, G, R, A/X in
/// memory, matching the fast Windows, Wayland, and common X11 upload paths.
#[derive(Debug, Clone)]
pub struct Framebuffer {
    /// Stores width.
    ///
    /// Visibility: **public**.
    pub width: u32,
    /// Stores height.
    ///
    /// Visibility: **public**.
    pub height: u32,
    /// Stores pixels.
    ///
    /// Visibility: **public**.
    pub pixels: Vec<u32>,
}

impl Framebuffer {
    #[inline]
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; pixel_count(width, height)],
        }
    }

    #[inline]
    /// Performs the from color operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the from color operation. Visibility: public."]
    pub fn from_color(width: u32, height: u32, color: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![color; pixel_count(width, height)],
        }
    }

    /// Replaces the backing pixel vector without copying it.
    #[inline]
    #[doc = "Sets pixels. Visibility: public."]
    pub fn set_pixels(&mut self, pixels: Vec<u32>) {
        debug_assert_eq!(pixels.len(), pixel_count(self.width, self.height));
        self.pixels = pixels;
    }

    /// Loads an uncompressed 24/32-bit BMP into a framebuffer of `width` x
    /// `height`. Pixels outside the BMP remain zero; BMP content outside the
    /// requested framebuffer is skipped.
    #[doc = "Implements the from bmp operation. Visibility: public."]
    pub fn from_bmp(path: &str, width: u32, height: u32) -> Self {
        let file = File::open(path).expect("Failed to open BMP file");
        let mut file = BufReader::with_capacity(64 * 1024, file);

        let mut file_header = [0u8; 14];
        file.read_exact(&mut file_header).unwrap();
        if &file_header[0..2] != b"BM" {
            panic!("Not a BMP file");
        }

        let data_offset = u32::from_le_bytes(file_header[10..14].try_into().unwrap());

        let mut dib_header = [0u8; 40];
        file.read_exact(&mut dib_header).unwrap();
        let bmp_width = i32::from_le_bytes(dib_header[4..8].try_into().unwrap());
        let bmp_height = i32::from_le_bytes(dib_header[8..12].try_into().unwrap());
        let bpp = u16::from_le_bytes(dib_header[14..16].try_into().unwrap());

        if bmp_width <= 0 || bmp_height == i32::MIN {
            panic!("Unsupported BMP dimensions");
        }
        if bpp != 24 && bpp != 32 {
            panic!("Only 24-bit and 32-bit BMP supported");
        }

        let top_down = bmp_height < 0;
        let src_height = bmp_height.unsigned_abs() as usize;
        let src_width = bmp_width as usize;
        let bytes_per_pixel = (bpp / 8) as usize;
        let row_stride = (src_width * bytes_per_pixel + 3) & !3;
        let copy_width = src_width.min(width as usize);
        let dst_width = width as usize;
        let dst_height = height as usize;

        file.seek(SeekFrom::Start(data_offset as u64)).unwrap();

        let mut pixels = vec![0u32; pixel_count(width, height)];
        let mut row = vec![0u8; row_stride];

        for source_y in 0..src_height {
            file.read_exact(&mut row).unwrap();

            let dst_y = if top_down { source_y } else { src_height - 1 - source_y };
            if dst_y >= dst_height {
                continue;
            }

            let dst = &mut pixels[dst_y * dst_width..dst_y * dst_width + copy_width];
            if bpp == 32 {
                for (pixel, bgra) in dst.iter_mut().zip(row.chunks_exact(4)) {
                    *pixel = u32::from_le_bytes([bgra[0], bgra[1], bgra[2], bgra[3]]);
                }
            } else {
                for (pixel, bgr) in dst.iter_mut().zip(row.chunks_exact(3)) {
                    *pixel =
                        0xff00_0000 |
                        ((bgr[2] as u32) << 16) |
                        ((bgr[1] as u32) << 8) |
                        (bgr[0] as u32);
                }
            }
        }

        Self {
            width,
            height,
            pixels,
        }
    }

    #[inline]
    /// Performs the resize operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the resize operation. Visibility: public."]
    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        self.width = new_width;
        self.height = new_height;
        self.pixels.resize(pixel_count(new_width, new_height), 0);
    }

    /// Resizes the framebuffer and invalidates every old pixel.
    ///
    /// This is useful for UI/window resizes where the previous rasterized frame
    /// must never be reused at a different resolution. The next render starts
    /// from a completely initialized surface at the new native pixel size.
    #[inline]
    #[doc = "Resizes and clear. Visibility: public."]
    pub fn resize_and_clear(&mut self, new_width: u32, new_height: u32, color: u32) {
        self.width = new_width;
        self.height = new_height;
        let len = pixel_count(new_width, new_height);
        if self.pixels.len() != len {
            // Vec::resize keeps the largest capacity ever reached. A UI that is
            // maximized once would therefore retain the large native-resolution
            // backing store forever. Release it when the new surface is less
            // than half of the retained allocation; otherwise reuse capacity to
            // avoid resize-drag allocation churn.
            if len.saturating_mul(2) < self.pixels.capacity() {
                self.pixels = vec![color; len];
                return;
            }
            self.pixels.resize(len, color);
        }
        self.pixels.fill(color);
    }

    #[inline]
    /// Clears the stored state.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the clear operation. Visibility: public."]
    pub fn clear(&mut self, color: u32) {
        self.pixels.fill(color);
    }

    #[inline]
    /// Performs the as ptr operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the as ptr operation. Visibility: public."]
    pub fn as_ptr(&self) -> *const u32 {
        self.pixels.as_ptr()
    }

    #[inline]
    /// Performs the as mut ptr operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the as mut ptr operation. Visibility: public."]
    pub fn as_mut_ptr(&mut self) -> *mut u32 {
        self.pixels.as_mut_ptr()
    }

    #[inline]
    /// Performs the as slice operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the as slice operation. Visibility: public."]
    pub fn as_slice(&self) -> &[u32] {
        &self.pixels
    }

    #[inline]
    /// Performs the as mut slice operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the as mut slice operation. Visibility: public."]
    pub fn as_mut_slice(&mut self) -> &mut [u32] {
        &mut self.pixels
    }
}

#[inline]
/// Implements pixel count.
///
/// Visibility: **private**.
#[doc = "Implements the pixel count operation. Visibility: private/internal."]
fn pixel_count(width: u32, height: u32) -> usize {
    (width as usize).checked_mul(height as usize).expect("framebuffer dimensions overflow usize")
}
