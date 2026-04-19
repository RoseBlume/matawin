use crate::ImageError;

pub struct Wbmp {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA
}

impl Wbmp {
    pub fn new(bytes: &[u8]) -> Result<Self, ImageError> {
        if bytes.len() < 4 {
            return Err(ImageError::new("File too small for WBMP"));
        }

        let mut offset = 0;

        let type_field = bytes[offset];
        offset += 1;

        let fix_header = bytes[offset];
        offset += 1;

        if type_field != 0 || fix_header != 0 {
            return Err(ImageError::new("Unsupported WBMP type (only Type 0 supported)"));
        }

        let width = Self::read_mbint(bytes, &mut offset)?;
        let height = Self::read_mbint(bytes, &mut offset)?;

        if width == 0 || height == 0 {
            return Err(ImageError::new("Invalid WBMP dimensions"));
        }

        let row_bytes = ((width + 7) / 8) as usize;
        let expected_size = row_bytes * height as usize;

        if offset + expected_size > bytes.len() {
            return Err(ImageError::new("WBMP data truncated"));
        }

        let mut data = vec![0u8; (width * height * 4) as usize];

        for y in 0..height {
            let row_start = offset + y as usize * row_bytes;

            for x in 0..width {
                let byte = bytes[row_start + (x / 8) as usize];
                let bit = 7 - (x % 8);
                let value = (byte >> bit) & 1;

                let color = if value == 0 {
                    [255, 255, 255, 255] // white
                } else {
                    [0, 0, 0, 255] // black
                };

                let idx = ((y * width + x) * 4) as usize;
                data[idx..idx + 4].copy_from_slice(&color);
            }
        }

        Ok(Self { width, height, data })
    }

    // Multi-byte integer (7 bits per byte, MSB = continuation)
    fn read_mbint(bytes: &[u8], offset: &mut usize) -> Result<u32, ImageError> {
        let mut result = 0u32;

        loop {
            if *offset >= bytes.len() {
                return Err(ImageError::new("Unexpected EOF in WBMP header"));
            }

            let byte = bytes[*offset];
            *offset += 1;

            result = (result << 7) | (byte & 0x7F) as u32;

            if byte & 0x80 == 0 {
                break;
            }
        }

        Ok(result)
    }
}

impl super::Decoder for Wbmp {
    fn width(&self) -> u32 { self.width }
    fn height(&self) -> u32 { self.height }

    fn as_bytes(&self) -> Vec<u8> {
        self.data.clone()
    }

    fn pixels(&self) -> Vec<u32> {
        let mut pixels = Vec::with_capacity((self.width * self.height) as usize);

        for chunk in self.data.chunks_exact(4) {
            let r = chunk[0] as u32;
            let g = chunk[1] as u32;
            let b = chunk[2] as u32;
            let a = chunk[3] as u32;

            pixels.push((r << 24) | (g << 16) | (b << 8) | a);
        }

        pixels
    }
}