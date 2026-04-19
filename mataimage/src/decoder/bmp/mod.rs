#[cfg(feature = "simd")]
use crate::simd;
use crate::ImageError;
use std::convert::TryInto;

pub struct Bmp {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA
}

impl Bmp {
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        if bytes.len() < 54 {
            return Err(ImageError::new("File too small for BMP header"));
        }

        if &bytes[0..2] != b"BM" {
            return Err(ImageError::new("Not a BMP file"));
        }

        let pixel_offset = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
        let width = u32::from_le_bytes(bytes[18..22].try_into().unwrap());
        let height = i32::from_le_bytes(bytes[22..26].try_into().unwrap());
        let bpp = u16::from_le_bytes(bytes[28..30].try_into().unwrap());
        let compression = u32::from_le_bytes(bytes[30..34].try_into().unwrap());

        let height_abs = height.abs() as u32;
        let origin_top = height < 0;
        let mut data = vec![0u8; (width * height_abs * 4) as usize];

        match bpp {
            32 => Self::decode_32bit(bytes, pixel_offset, width, height_abs, origin_top, &mut data)?,
            24 => Self::decode_24bit(bytes, pixel_offset, width, height_abs, origin_top, &mut data)?,
            8 | 4 => {
                let palette_size = if bpp == 8 { 256 } else { 16 };
                let mut palette = Vec::with_capacity(palette_size);
                let palette_start = 54;
                for i in 0..palette_size {
                    let idx = palette_start + i * 4;
                    if idx + 3 >= bytes.len() {
                        return Err(ImageError::new("Palette truncated"));
                    }
                    let b = bytes[idx];
                    let g = bytes[idx + 1];
                    let r = bytes[idx + 2];
                    palette.push([r, g, b, 255]);
                }

                match compression {
                    0 => Self::decode_paletted_raw(bytes, pixel_offset, width, height_abs, origin_top, bpp, &palette, &mut data)?,
                    1 | 2 => Self::decode_paletted_rle(bytes, pixel_offset, width, height_abs, origin_top, bpp, &palette, &mut data)?,
                    _ => return Err(ImageError::new("Unsupported BMP compression")),
                }
            }
            _ => return Err(ImageError::new("Only 32, 24, 8, 4-bit BMPs supported")),
        }

        Ok(Self { width, height: height_abs, data })
    }

    fn decode_32bit(
        bytes: &[u8],
        pixel_offset: usize,
        width: u32,
        height: u32,
        origin_top: bool,
        out: &mut [u8],
    ) -> Result<(), ImageError> {
        for y in 0..height {
            let bmp_y = if origin_top { y } else { height - 1 - y };
            let row_start = pixel_offset + (bmp_y * width * 4) as usize;

            let mut x = 0;

            // -------- SIMD path --------
            #[cfg(feature = "simd")]
            unsafe {
                while x + 4 <= width {
                    let src = bytes.as_ptr().add(row_start + (x * 4) as usize);
                    let dst = out.as_mut_ptr().add(((y * width + x) * 4) as usize);

                    if !simd::bgra_to_rgba_16(src, dst) {
                        break; // SIMD not supported → fallback
                    }

                    x += 4;
                }
            }

            // -------- Scalar fallback --------
            for x2 in x..width {
                let idx = row_start + (x2 * 4) as usize;
                if idx + 3 >= bytes.len() {
                    return Err(ImageError::new("Unexpected EOF in BMP 32-bit data"));
                }

                let b = bytes[idx];
                let g = bytes[idx + 1];
                let r = bytes[idx + 2];
                let a = bytes[idx + 3];

                let out_idx = ((y * width + x2) * 4) as usize;
                out[out_idx..out_idx + 4].copy_from_slice(&[r, g, b, a]);
            }
        }

        Ok(())
    }
    

    fn decode_24bit(
        bytes: &[u8],
        pixel_offset: usize,
        width: u32,
        height: u32,
        origin_top: bool,
        out: &mut [u8],
    ) -> Result<(), ImageError> {
        let row_size = ((24 * width + 31) / 32) * 4;

        for y in 0..height {
            let bmp_y = if origin_top { y } else { height - 1 - y };
            let row_start = pixel_offset + (bmp_y * row_size) as usize;

            let mut x = 0;

            // -------- Unrolled loop (SIMD-friendly) --------
            while x + 4 <= width {
                for i in 0..4 {
                    let idx = row_start + ((x + i) * 3) as usize;
                    let out_idx = ((y * width + x + i) * 4) as usize;

                    let b = bytes[idx];
                    let g = bytes[idx + 1];
                    let r = bytes[idx + 2];

                    out[out_idx..out_idx + 4].copy_from_slice(&[r, g, b, 255]);
                }
                x += 4;
            }

            // -------- Scalar tail --------
            for x2 in x..width {
                let idx = row_start + (x2 * 3) as usize;
                if idx + 2 >= bytes.len() {
                    return Err(ImageError::new("Unexpected EOF in BMP 24-bit data"));
                }

                let b = bytes[idx];
                let g = bytes[idx + 1];
                let r = bytes[idx + 2];

                let out_idx = ((y * width + x2) * 4) as usize;
                out[out_idx..out_idx + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }

        Ok(())
    }

    fn decode_paletted_raw(
        bytes: &[u8],
        pixel_offset: usize,
        width: u32,
        height: u32,
        origin_top: bool,
        bpp: u16,
        palette: &[[u8; 4]],
        out: &mut [u8],
    ) -> Result<(), ImageError> {
        let row_size = ((bpp as u32 * width + 31) / 32) * 4;
        for y in 0..height {
            let bmp_y = if origin_top { y } else { height - 1 - y };
            let row_start = pixel_offset + (bmp_y * row_size) as usize;
            let mut x = 0;
            while x < width {
                let byte = bytes[row_start + (x as usize * bpp as usize / 8)];
                if bpp == 8 {
                    let color = palette[byte as usize];
                    let out_idx = ((y * width + x) * 4) as usize;
                    out[out_idx..out_idx + 4].copy_from_slice(&color);
                    x += 1;
                } else {
                    let high = byte >> 4;
                    let low = byte & 0x0F;
                    let out_idx1 = ((y * width + x) * 4) as usize;
                    out[out_idx1..out_idx1 + 4].copy_from_slice(&palette[high as usize]);
                    x += 1;
                    if x < width {
                        let out_idx2 = ((y * width + x) * 4) as usize;
                        out[out_idx2..out_idx2 + 4].copy_from_slice(&palette[low as usize]);
                        x += 1;
                    }
                }
            }
        }
        Ok(())
    }

    fn decode_paletted_rle(
        bytes: &[u8],
        pixel_offset: usize,
        width: u32,
        height: u32,
        _origin_top: bool,
        bpp: u16,
        palette: &[[u8; 4]],
        out: &mut [u8],
    ) -> Result<(), ImageError> {
        let mut x = 0;
        let mut y = 0;
        let mut offset = pixel_offset;

        while y < height {
            if offset >= bytes.len() {
                return Err(ImageError::new("Unexpected EOF in RLE BMP data"));
            }

            let header = bytes[offset];
            offset += 1;
            let count = (header & 0x7F) + 1;

            if header & 0x80 != 0 {
                // RLE packet
                let byte = bytes[offset];
                offset += 1;
                for _ in 0..count {
                    let value = if bpp == 8 { byte } else {
                        if x % 2 == 0 { byte >> 4 } else { byte & 0x0F }
                    };
                    let out_idx = ((y * width + x) * 4) as usize;
                    out[out_idx..out_idx + 4].copy_from_slice(&palette[value as usize]);
                    x += 1;
                    if x >= width { x = 0; y += 1; if y >= height { break; } }
                }
            } else {
                // Raw packet
                for _ in 0..count {
                    let byte = bytes[offset];
                    offset += 1;
                    let values = if bpp == 8 { vec![byte] } else { vec![byte >> 4, byte & 0x0F] };
                    for value in values {
                        let out_idx = ((y * width + x) * 4) as usize;
                        out[out_idx..out_idx + 4].copy_from_slice(&palette[value as usize]);
                        x += 1;
                        if x >= width { x = 0; y += 1; if y >= height { break; } }
                    }
                }
            }
        }

        Ok(())
    }
}

impl super::Decoder for Bmp {
    fn width(&self) -> u32 { self.width }
    fn height(&self) -> u32 { self.height }
    fn as_bytes(&self) -> Vec<u8> { self.data.clone() }
    fn pixels(&self) -> Vec<u32> {
        let mut pixels = Vec::with_capacity((self.width * self.height) as usize);

        #[cfg(feature = "simd")]
        {
            // Faster chunk processing
            for chunk in self.data.chunks_exact(4) {
                let val = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                pixels.push(val);
            }
            return pixels;
        }

        // fallback
        #[cfg(not(feature = "simd"))] {
            for chunk in self.data.chunks(4) {
                let r = chunk[0] as u32;
                let g = chunk[1] as u32;
                let b = chunk[2] as u32;
                let a = chunk[3] as u32;
                pixels.push((r << 24) | (g << 16) | (b << 8) | a);
            }

            pixels
        }
    }

}