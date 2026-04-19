use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
pub struct Framebuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

// pub enum Adjustment {
//     Fill,
//     Position(i32, i32),
//     None
// }

impl Framebuffer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height) as usize],
        }
    }

    pub fn set_pixels(&mut self, pixels: Vec<u32>) {
        self.pixels = pixels;
    }

    pub fn from_color(width: u32, height: u32, color: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![color; (width * height) as usize]
        }
    }

    pub fn from_bmp(path: &str, width: u32, height: u32) -> Self {

        let mut file = File::open(path).expect("Failed to open BMP file");

        // --- BMP FILE HEADER (14 bytes) ---
        let mut file_header = [0u8; 14];
        file.read_exact(&mut file_header).unwrap();

        // Check "BM"
        if &file_header[0..2] != b"BM" {
            panic!("Not a BMP file");
        }

        let data_offset = u32::from_le_bytes([
            file_header[10],
            file_header[11],
            file_header[12],
            file_header[13],
        ]);

        // --- DIB HEADER (assume BITMAPINFOHEADER = 40 bytes) ---
        let mut dib_header = [0u8; 40];
        file.read_exact(&mut dib_header).unwrap();

        let bmp_width = i32::from_le_bytes([dib_header[4], dib_header[5], dib_header[6], dib_header[7]]);
        let bmp_height = i32::from_le_bytes([dib_header[8], dib_header[9], dib_header[10], dib_header[11]]);
        let bpp = u16::from_le_bytes([dib_header[14], dib_header[15]]);

        if bpp != 24 && bpp != 32 {
            panic!("Only 24-bit and 32-bit BMP supported");
        }

        let top_down = bmp_height < 0;
        let bmp_height_abs = bmp_height.abs() as usize;
        let bmp_width_abs = bmp_width as usize;

        // Move to pixel data
        file.seek(SeekFrom::Start(data_offset as u64)).unwrap();

        let bytes_per_pixel = (bpp / 8) as usize;

        // Rows are padded to 4 bytes
        let row_stride = ((bmp_width_abs * bytes_per_pixel + 3) / 4) * 4;

        let mut pixels = vec![0u32; (width * height) as usize];

        let mut row = vec![0u8; row_stride];

        for y in 0..bmp_height_abs {
            file.read_exact(&mut row).unwrap();

            let dst_y = if top_down {
                y
            } else {
                bmp_height_abs - 1 - y
            };

            if dst_y >= height as usize {
                continue;
            }

            for x in 0..bmp_width_abs {
                if x >= width as usize {
                    continue;
                }

                let i = x * bytes_per_pixel;

                let b = row[i] as u32;
                let g = row[i + 1] as u32;
                let r = row[i + 2] as u32;

                let a = if bpp == 32 {
                    row[i + 3] as u32
                } else {
                    0xFF
                };

                let pixel = (a << 24) | (r << 16) | (g << 8) | b;

                pixels[dst_y * width as usize + x] = pixel;
            }
        }

        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        self.width = new_width;
        self.height = new_height;
        self.pixels.resize((new_width * new_height) as usize, 0);
    }

    pub fn clear(&mut self, color: u32) {
        self.pixels.fill(color);
    }

    pub fn as_ptr(&self) -> *const u32 {
        self.pixels.as_ptr()
    }
}