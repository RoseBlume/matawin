use crate::ImageError;

pub struct Tga {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA
}

impl Tga {
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        if bytes.len() < 18 {
            return Err(ImageError::new("File too small for TGA header"));
        }

        let id_length = bytes[0] as usize;
        let color_map_type = bytes[1];
        let image_type = bytes[2];

        // We only support truecolor (no palette)
        if color_map_type != 0 {
            return Err(ImageError::new("Color-mapped TGA not supported"));
        }

        let width = u16::from_le_bytes(bytes[12..14].try_into().unwrap()) as u32;
        let height = u16::from_le_bytes(bytes[14..16].try_into().unwrap()) as u32;
        let bpp = bytes[16];
        let descriptor = bytes[17];

        let origin_top = (descriptor & 0x20) != 0;

        let pixel_size = match bpp {
            24 => 3,
            32 => 4,
            _ => return Err(ImageError::new("Only 24/32-bit TGA supported")),
        };

        let offset = 18 + id_length;

        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| ImageError::new("Image size overflow"))?;

        let mut data = vec![0u8; pixel_count * 4];

        match image_type {
            2 => {
                // --- Uncompressed ---
                Self::decode_raw(
                    bytes,
                    offset,
                    width,
                    height,
                    pixel_size,
                    origin_top,
                    &mut data,
                )?;
            }
            10 => {
                // --- RLE ---
                Self::decode_rle(
                    bytes,
                    offset,
                    width,
                    height,
                    pixel_size,
                    origin_top,
                    &mut data,
                )?;
            }
            _ => {
                return Err(ImageError::new(format!(
                    "Unsupported TGA type {}",
                    image_type
                )));
            }
        }

        Ok(Self {
            width,
            height,
            data,
        })
    }

    fn write_pixel(out: &mut [u8], idx: usize, pixel: &[u8]) {
        let (r, g, b, a) = match pixel.len() {
            3 => (pixel[2], pixel[1], pixel[0], 255),
            4 => (pixel[2], pixel[1], pixel[0], pixel[3]),
            _ => (0, 0, 0, 255),
        };

        out[idx..idx + 4].copy_from_slice(&[r, g, b, a]);
    }

    fn decode_raw(
        bytes: &[u8],
        mut offset: usize,
        width: u32,
        height: u32,
        pixel_size: usize,
        origin_top: bool,
        out: &mut [u8],
    ) -> Result<(), ImageError> {
        for y in 0..height {
            let dst_y = if origin_top { y } else { height - 1 - y };

            for x in 0..width {
                if offset + pixel_size > bytes.len() {
                    return Err(ImageError::new("Unexpected EOF in TGA data"));
                }

                let pixel = &bytes[offset..offset + pixel_size];
                offset += pixel_size;

                let o = ((dst_y * width + x) * 4) as usize;
                Self::write_pixel(out, o, pixel);
            }
        }

        Ok(())
    }

    fn decode_rle(
        bytes: &[u8],
        mut offset: usize,
        width: u32,
        height: u32,
        pixel_size: usize,
        origin_top: bool,
        out: &mut [u8],
    ) -> Result<(), ImageError> {
        let mut x = 0;
        let mut y = 0;

        while y < height {
            if offset >= bytes.len() {
                return Err(ImageError::new("Unexpected EOF in RLE data"));
            }

            let header = bytes[offset];
            offset += 1;

            let count = (header & 0x7F) + 1;

            if (header & 0x80) != 0 {
                // RLE packet
                if offset + pixel_size > bytes.len() {
                    return Err(ImageError::new("Unexpected EOF in RLE pixel"));
                }

                let pixel = &bytes[offset..offset + pixel_size];
                offset += pixel_size;

                for _ in 0..count {
                    let dst_y = if origin_top { y } else { height - 1 - y };
                    let o = ((dst_y * width + x) * 4) as usize;

                    Self::write_pixel(out, o, pixel);

                    x += 1;
                    if x >= width {
                        x = 0;
                        y += 1;
                        if y >= height {
                            break;
                        }
                    }
                }
            } else {
                // Raw packet
                for _ in 0..count {
                    if offset + pixel_size > bytes.len() {
                        return Err(ImageError::new("Unexpected EOF in raw packet"));
                    }

                    let pixel = &bytes[offset..offset + pixel_size];
                    offset += pixel_size;

                    let dst_y = if origin_top { y } else { height - 1 - y };
                    let o = ((dst_y * width + x) * 4) as usize;

                    Self::write_pixel(out, o, pixel);

                    x += 1;
                    if x >= width {
                        x = 0;
                        y += 1;
                        if y >= height {
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

impl super::Decoder for Tga {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn as_bytes(&self) -> Vec<u8> {
        self.data.clone()
    }
    fn pixels(&self) -> Vec<u32> {
        let mut pixels = Vec::with_capacity((self.width * self.height) as usize);
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